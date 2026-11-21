#!/usr/bin/env python3
"""Build once, run isolated EventDock benchmarks, save raw and median CSV."""
import argparse
import csv
import json
import statistics
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ('insert', 'scan', 'get', 'get_random', 'fields')
STORES = ('owned', 'compact')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--events', type=int, nargs='+', default=[10000, 100000])
    parser.add_argument('--fields', type=int, nargs='+', default=[0, 4, 12])
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--out', type=Path, default=ROOT / 'benchmarks' / 'results')
    args = parser.parse_args()
    if args.repeats < 1 or any(n < 1 for n in args.events) or any(n < 0 for n in args.fields):
        parser.error('repeats/events must be positive and fields must be nonnegative')

    subprocess.run(['cargo', 'build', '--release', '-p', 'eventdock', '--bin', 'store_bench'], cwd=ROOT, check=True)
    executable = ROOT / 'target' / 'release' / 'store_bench'
    args.out.mkdir(parents=True, exist_ok=True)
    records = []
    for n in args.events:
        for f in args.fields:
            for scenario in SCENARIOS:
                for repeat in range(args.repeats):
                    checksums = {}
                    for store in (STORES if repeat % 2 == 0 else STORES[::-1]):
                        command = [str(executable), '--store', store, '--scenario', scenario, '--events', str(n), '--fields', str(f)]
                        result = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True)
                        record = json.loads(result.stdout.strip())
                        record['repeat'] = repeat
                        records.append(record)
                        checksums[store] = record['checksum']
                        print(f'{n:>8} events {f:>2} fields {scenario:<6} {store:<7} {record["elapsed_ns"] / 1e6:>10.3f} ms')
                    if checksums['owned'] != checksums['compact']:
                        raise RuntimeError(f'checksum mismatch: {n=} {f=} {scenario=} {checksums=}')

    raw = args.out / 'raw.csv'
    with raw.open('w', newline='') as file:
        writer = csv.DictWriter(file, fieldnames=list(records[0]))
        writer.writeheader()
        writer.writerows(records)

    summaries = []
    for n in args.events:
        for f in args.fields:
            for scenario in SCENARIOS:
                for store in STORES:
                    group = [r for r in records if (r['events'], r['fields'], r['scenario'], r['store']) == (n, f, scenario, store)]
                    rss = [r['rss_delta_kib'] for r in group if r['rss_delta_kib'] is not None]
                    summaries.append({
                        'events': n, 'fields': f, 'scenario': scenario, 'store': store,
                        'median_elapsed_ms': statistics.median(r['elapsed_ns'] for r in group) / 1e6,
                        'median_rss_delta_kib': statistics.median(rss) if rss else '',
                        'median_compact_allocated_bytes': statistics.median(r['compact_allocated_bytes'] for r in group) if store == 'compact' else '',
                    })
    summary = args.out / 'summary.csv'
    with summary.open('w', newline='') as file:
        writer = csv.DictWriter(file, fieldnames=list(summaries[0]))
        writer.writeheader()
        writer.writerows(summaries)
    print(f'\nSaved {raw} and {summary}')


if __name__ == '__main__':
    main()
