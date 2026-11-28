use eventdock_core::{IncomingEvent, IncomingEventRef, LogLevel};
use eventdock_engine::{CompactEventStore, EventQuery, QueryExecution};
use std::{hint::black_box, time::Instant};

const MIB: usize = 1024 * 1024;
const ITERATIONS: usize = 10_000;

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

fn report(
    name: &str,
    store: &CompactEventStore,
    elapsed: std::time::Duration,
    queries: usize,
) {
    println!(
        "{name}: {:.0} events/s, {:.1} ns/event, retained={}, buffer_bytes={}, allocated_bytes={}, completed_queries={queries}",
        ITERATIONS as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1e9 / ITERATIONS as f64,
        store.len(),
        store.payload_bytes(),
        store.allocated_bytes()
    );
}

fn measure(
    name: &str,
    mut store: CompactEventStore,
    inputs: &[IncomingEvent],
    warmup: usize,
    query: bool,
) {
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
    report(name, &store, start.elapsed(), completed);
}

fn main() {
    let fixed = events(&[1024]);
    let variable = events(&[64, 256, 1024, 4096]);
    for run in 1..=3 {
        println!("Run {run}");
        measure("empty", CompactEventStore::new(), &fixed, 0, false);
        measure(
            "count-limited",
            CompactEventStore::with_limits(4096, 64 * MIB),
            &fixed,
            4096,
            false,
        );
        for budget in [4, 16, 64] {
            let warmup = budget * MIB / 1024 + 1;
            measure(
                &format!("byte-limited-{budget}MiB"),
                CompactEventStore::with_limits(100_000, budget * MIB),
                &fixed,
                warmup,
                false,
            );
        }
        measure(
            "variable-16MiB",
            CompactEventStore::with_limits(100_000, 16 * MIB),
            &variable,
            20_000,
            false,
        );
        measure(
            "mixed-query-16MiB",
            CompactEventStore::with_limits(100_000, 16 * MIB),
            &fixed,
            16 * MIB / 1024 + 1,
            true,
        );
    }
}
