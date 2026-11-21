# EventDock store benchmarks (experimental)

Run from repository root:

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 benchmarks/run.py --events 10000 100000 --fields 0 4 12 --repeats 5
```

The runner builds `store_bench` once in release mode, then launches a separate process per sample, alternating store order. Results are saved in `benchmarks/results/raw.csv` and `summary.csv` (add `benchmarks/results/` to `.gitignore` if desired). No external Python packages required.

**Scenarios:** `insert` measures insertion of pre-generated events (owned store clones them, compact store borrows and encodes them); `scan` filters levels; `get` reads message lengths via ID; `fields` visits all field names/values. The harness checks matching checksums. Each process generates the same deterministic events before timing. The measured query phase begins only after insertion.

**Important limitations:**
- Insertion includes an owned clone for `EventStore`, but not for compact. This is an end-to-end API comparison, not a pure serialization microbenchmark.
- Compact `scan` currently decodes/validates payloads even though it only needs the level. This is real current behavior, not an optimized metadata-only scan.
- `EventStore` is bounded, but the test sets capacity to event count; compact is unbounded. There is no eviction in this benchmark.
- Linux `/proc/self/status` VmRSS difference is a rough process-memory indicator, not retained heap allocation. Allocator reuse, page granularity, and temporary allocations can skew it. The compact `allocated_bytes()` is only its two vector capacities. Neither is directly comparable to full owned-store heap usage.
- Process creation and dataset generation are outside the timed region, but CPU scheduling, frequency scaling, allocator behavior, and thermal state can affect results. Run on an otherwise idle machine. Treat small differences skeptically.
- The benchmark binary is a temporary development tool. The library implementation remains unchanged.
