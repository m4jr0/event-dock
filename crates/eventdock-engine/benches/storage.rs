use eventdock_core::{IncomingEvent, IncomingEventRef, LogLevel};
use eventdock_engine::{CompactEventStore, EventQuery, QueryExecution};
use std::{hint::black_box, time::Instant};

const MIB: usize = 1024 * 1024;
const ITERATIONS: usize = 10_000;
const RUNS: usize = 7;

struct Sample {
    elapsed: std::time::Duration,
    retained: usize,
    payload_bytes: usize,
    allocated_bytes: usize,
    queries: usize,
}

fn events(sizes: &[usize]) -> Vec<IncomingEvent> {
    sizes
        .iter()
        .map(|&size| {
            IncomingEvent::new("benchmark", LogLevel::Info, "x".repeat(size))
        })
        .collect()
}

fn insert(
    store: &mut CompactEventStore,
    inputs: &[IncomingEvent],
    index: usize,
) {
    black_box(
        store
            .push(IncomingEventRef::from(&inputs[index % inputs.len()]))
            .unwrap(),
    );
}

fn measure(
    mut store: CompactEventStore,
    inputs: &[IncomingEvent],
    warmup: usize,
    query: bool,
) -> Sample {
    for i in 0..warmup {
        insert(&mut store, inputs, i);
    }
    let mut execution = None;
    let mut completed = 0;
    let start = Instant::now();
    for i in 0..ITERATIONS {
        insert(&mut store, inputs, warmup + i);
        if query {
            let active = execution.get_or_insert_with(|| {
                QueryExecution::new(
                    EventQuery {
                        before_id: None,
                        limit: 512,
                        level: None,
                    },
                    &store,
                )
            });
            if active.advance(&store, 256) {
                black_box(execution.take().unwrap().finish(&store));
                completed += 1;
            }
        }
    }
    let elapsed = start.elapsed();
    Sample {
        elapsed,
        retained: store.len(),
        payload_bytes: store.payload_bytes(),
        allocated_bytes: store.allocated_bytes(),
        queries: completed,
    }
}

struct Workload {
    name: &'static str,
    limits: Option<(usize, usize)>,
    warmup: usize,
    variable: bool,
    query: bool,
}

impl Workload {
    fn run(
        &self,
        fixed: &[IncomingEvent],
        variable: &[IncomingEvent],
    ) -> Sample {
        let store = match self.limits {
            Some((count, bytes)) => {
                CompactEventStore::with_limits(count, bytes)
            }
            None => CompactEventStore::new(),
        };
        let inputs = if self.variable { variable } else { fixed };
        measure(store, inputs, self.warmup, self.query)
    }
}

fn report(
    name: &str,
    samples: &mut [Sample],
) {
    samples.sort_by_key(|sample| sample.elapsed);
    let median = &samples[samples.len() / 2];
    let rate =
        |sample: &Sample| ITERATIONS as f64 / sample.elapsed.as_secs_f64();
    println!(
        "{name}: median={:.0} events/s, min={:.0}, max={:.0}, {:.1} ns/event, retained={}, buffer_bytes={}, allocated_bytes={}, completed_queries={}",
        rate(median),
        rate(samples.last().unwrap()),
        rate(&samples[0]),
        median.elapsed.as_secs_f64() * 1e9 / ITERATIONS as f64,
        median.retained,
        median.payload_bytes,
        median.allocated_bytes,
        median.queries,
    );
}

fn main() {
    let fixed = events(&[1024]);
    let variable = events(&[64, 256, 1024, 4096]);
    let workloads = [
        Workload {
            name: "empty",
            limits: None,
            warmup: 0,
            variable: false,
            query: false,
        },
        Workload {
            name: "count-limited",
            limits: Some((4096, 64 * MIB)),
            warmup: 4096,
            variable: false,
            query: false,
        },
        Workload {
            name: "byte-limited-4MiB",
            limits: Some((100_000, 4 * MIB)),
            warmup: 4 * MIB / 1024 + 1,
            variable: false,
            query: false,
        },
        Workload {
            name: "byte-limited-16MiB",
            limits: Some((100_000, 16 * MIB)),
            warmup: 16 * MIB / 1024 + 1,
            variable: false,
            query: false,
        },
        Workload {
            name: "byte-limited-64MiB",
            limits: Some((100_000, 64 * MIB)),
            warmup: 64 * MIB / 1024 + 1,
            variable: false,
            query: false,
        },
        Workload {
            name: "variable-16MiB",
            limits: Some((100_000, 16 * MIB)),
            warmup: 20_000,
            variable: true,
            query: false,
        },
        Workload {
            name: "mixed-query-16MiB",
            limits: Some((100_000, 16 * MIB)),
            warmup: 16 * MIB / 1024 + 1,
            variable: false,
            query: true,
        },
    ];
    // Warm up the process without changing the initial state of measured stores.
    for workload in &workloads {
        black_box(workload.run(&fixed, &variable));
    }
    let mut samples: Vec<Vec<Sample>> = (0..workloads.len())
        .map(|_| Vec::with_capacity(RUNS))
        .collect();
    // Rotate workload order to reduce systematic first-run effects.
    for run in 0..RUNS {
        for offset in 0..workloads.len() {
            let index = (run + offset) % workloads.len();
            let sample = workloads[index].run(&fixed, &variable);
            samples[index].push(sample);
        }
    }
    println!(
        "{RUNS} measured runs per workload, {ITERATIONS} insertions per run"
    );
    for (workload, samples) in workloads.iter().zip(&mut samples) {
        report(workload.name, samples);
    }
}
