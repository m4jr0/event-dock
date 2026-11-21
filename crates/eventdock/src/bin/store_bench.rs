//! Standalone, process-isolated comparison of the two EventDock stores.
use eventdock::{
    CompactEventStore, EventRef, EventStore, FieldValue, IncomingEvent,
    LogLevel, Timestamp,
};
use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy)]
enum Backend {
    Owned,
    Compact,
}
#[derive(Clone, Copy)]
enum Scenario {
    Insert,
    Scan,
    Get,
    GetRandom,
    Fields,
}

struct Args {
    backend: Backend,
    scenario: Scenario,
    events: usize,
    fields: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut backend = None;
    let mut scenario = None;
    let mut events = None;
    let mut fields = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--store" => {
                backend = Some(match value.as_str() {
                    "owned" => Backend::Owned,
                    "compact" => Backend::Compact,
                    _ => return Err(format!("invalid store: {value}")),
                })
            }
            "--scenario" => {
                scenario = Some(match value.as_str() {
                    "insert" => Scenario::Insert,
                    "scan" => Scenario::Scan,
                    "get" => Scenario::Get,
                    "get_random" => Scenario::GetRandom,
                    "fields" => Scenario::Fields,
                    _ => return Err(format!("invalid scenario: {value}")),
                })
            }
            "--events" => {
                events = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "invalid events count".to_owned())?,
                )
            }
            "--fields" => {
                fields = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "invalid fields count".to_owned())?,
                )
            }
            _ => return Err(format!("unknown option: {flag}")),
        }
    }
    let events = events.ok_or("missing --events")?;
    if events == 0 {
        return Err("--events must be positive".into());
    }
    Ok(Args {
        backend: backend.ok_or("missing --store")?,
        scenario: scenario.ok_or("missing --scenario")?,
        events,
        fields: fields.ok_or("missing --fields")?,
    })
}

fn generate(
    events: usize,
    fields: usize,
) -> Vec<IncomingEvent> {
    (0..events)
        .map(|index| {
            let level = match index % 4 {
                0 => LogLevel::Debug,
                1 => LogLevel::Info,
                2 => LogLevel::Warning,
                _ => LogLevel::Error,
            };
            let mut event = IncomingEvent::new(
                "bench-service",
                level,
                format!("event {index:08}: a reproducible log message"),
            );
            event.timestamp = Timestamp::from_unix_nanos(index as i64);
            for field in 0..fields {
                event.fields.insert(
                    format!("field_{field:02}"),
                    match field % 4 {
                        0 => FieldValue::I64(index as i64),
                        1 => FieldValue::Bool(index % 2 == 0),
                        2 => {
                            FieldValue::String(format!("value_{index}_{field}"))
                        }
                        _ => FieldValue::F64(index as f64 / 3.0),
                    },
                );
            }
            event
        })
        .collect()
}

fn permuted_index(
    index: usize,
    len: usize,
) -> usize {
    // The multiplier is coprime with every power-of-ten dataset size.
    ((index as u128 * 7919 + 104729) % len as u128) as usize
}

fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status
        .lines()
        .find(|line| line.starts_with("VmRSS:"))?;
    line.split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn run_owned(
    input: &[IncomingEvent],
    scenario: Scenario,
) -> (u128, u64, Option<u64>) {
    let mut store = EventStore::new(input.len());
    let start = Instant::now();
    for event in input {
        black_box(store.push(black_box(EventRef::from(event))));
    }
    let insert_ns = start.elapsed().as_nanos();
    let rss_after = rss_kib();
    let start = Instant::now();
    let checksum = match scenario {
        Scenario::Insert => store.len() as u64,
        Scenario::Scan => store
            .iter()
            .filter(|event| black_box(event.event.level) >= LogLevel::Info)
            .count() as u64,
        Scenario::Get => (0..input.len())
            .map(|id| {
                let event = store
                    .get(id as u64)
                    .expect("missing owned event");
                black_box(event.event.message.len() as u64)
            })
            .sum(),
        Scenario::GetRandom => (0..input.len())
            .map(|index| {
                let id = black_box(permuted_index(index, input.len()) as u64);
                let event = store
                    .get(id)
                    .expect("missing owned event");
                black_box(event.event.message.len() as u64)
            })
            .sum(),
        Scenario::Fields => store
            .iter()
            .map(|event| {
                event
                    .event
                    .fields
                    .iter()
                    .map(|(name, value)| {
                        black_box(
                            name.len() as u64
                                + match value {
                                    FieldValue::String(s) => s.len() as u64,
                                    FieldValue::Null => 0,
                                    _ => 1,
                                },
                        )
                    })
                    .sum::<u64>()
            })
            .sum(),
    };
    let query_ns = start.elapsed().as_nanos();
    black_box(&store);
    (
        if matches!(scenario, Scenario::Insert) {
            insert_ns
        } else {
            query_ns
        },
        checksum,
        rss_after,
    )
}

fn run_compact(
    input: &[IncomingEvent],
    scenario: Scenario,
) -> (u128, u64, Option<u64>, usize) {
    let mut store = CompactEventStore::new();
    let start = Instant::now();
    for event in input {
        black_box(
            store
                .push(black_box(EventRef::from(event)))
                .expect("compact insertion failed"),
        );
    }
    let insert_ns = start.elapsed().as_nanos();
    let rss_after = rss_kib();
    let start = Instant::now();
    let checksum =
        match scenario {
            Scenario::Insert => store.len() as u64,
            Scenario::Scan => store
                .iter()
                .filter(|event| black_box(event.level()) >= LogLevel::Info)
                .count() as u64,
            Scenario::Get => (0..input.len())
                .map(|id| {
                    let event = store
                        .get(id as u64)
                        .expect("missing compact event");
                    black_box(event.message().len() as u64)
                })
                .sum(),
            Scenario::GetRandom => (0..input.len())
                .map(|index| {
                    let id =
                        black_box(permuted_index(index, input.len()) as u64);
                    let event = store
                        .get(id)
                        .expect("missing compact event");
                    black_box(event.message().len() as u64)
                })
                .sum(),
            Scenario::Fields => {
                store
                    .iter()
                    .map(|event| {
                        event.fields().map(|field| {
                let field = field.expect("invalid compact field");
                black_box(field.name.len() as u64 + match field.value {
                    eventdock::FieldValueView::String(s) => s.len() as u64,
                    eventdock::FieldValueView::Null => 0,
                    _ => 1,
                })
            }).sum::<u64>()
                    })
                    .sum()
            }
        };
    let query_ns = start.elapsed().as_nanos();
    let allocated = store.allocated_bytes();
    black_box(&store);
    (
        if matches!(scenario, Scenario::Insert) {
            insert_ns
        } else {
            query_ns
        },
        checksum,
        rss_after,
        allocated,
    )
}

fn main() {
    let args = parse_args().unwrap_or_else(|error| {
        eprintln!("{error}\nUsage: store_bench --store owned|compact --scenario insert|scan|get|get_random|fields --events N --fields N");
        std::process::exit(2);
    });
    let input = generate(args.events, args.fields);
    black_box(&input);
    let rss_before = rss_kib();
    let (elapsed_ns, checksum, rss_after, allocated) = match args.backend {
        Backend::Owned => {
            let (elapsed, checksum, rss) = run_owned(&input, args.scenario);
            (elapsed, checksum, rss, None)
        }
        Backend::Compact => {
            let (elapsed, checksum, rss, allocated) =
                run_compact(&input, args.scenario);
            (elapsed, checksum, rss, Some(allocated))
        }
    };
    let backend = if matches!(args.backend, Backend::Owned) {
        "owned"
    } else {
        "compact"
    };
    let scenario = match args.scenario {
        Scenario::Insert => "insert",
        Scenario::Scan => "scan",
        Scenario::Get => "get",
        Scenario::GetRandom => "get_random",
        Scenario::Fields => "fields",
    };
    let rss_delta = rss_after
        .zip(rss_before)
        .map(|(after, before)| after.saturating_sub(before));
    let number =
        |n: Option<u64>| n.map_or("null".to_owned(), |n| n.to_string());
    println!(
        "{{\"store\":\"{backend}\",\"scenario\":\"{scenario}\",\"events\":{},\"fields\":{},\"elapsed_ns\":{elapsed_ns},\"checksum\":{checksum},\"rss_before_kib\":{},\"rss_after_kib\":{},\"rss_delta_kib\":{},\"compact_allocated_bytes\":{}}}",
        args.events,
        args.fields,
        number(rss_before),
        number(rss_after),
        number(rss_delta),
        allocated.map_or("null".to_owned(), |n| n.to_string())
    );
}
