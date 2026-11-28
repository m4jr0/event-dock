use eventdock_client::{Client, QueryRequest, RemoteTransport};
use eventdock_core::{IncomingEvent, LogLevel};
use eventdock_engine::{
    Backend, BackendConfig, BackendSnapshot, serve_queries,
};
use std::{
    io::{self, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const EVENTS: u64 = 50_000;
const QUERIES: usize = 100;
const TIMEOUT: Duration = Duration::from_secs(10);

fn frame() -> Vec<u8> {
    let event =
        IncomingEvent::new("benchmark", LogLevel::Info, "x".repeat(256));
    let mut bytes = serde_json::to_vec(&event).unwrap();
    bytes.push(b'\n');
    bytes
}

fn produce(
    address: std::net::SocketAddr,
    bytes: &[u8],
) -> io::Result<()> {
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let deadline = Instant::now() + TIMEOUT;
    for _ in 0..EVENTS {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "producer deadline",
            ));
        }
        stream.write_all(bytes)?;
    }
    Ok(())
}

fn wait_for_storage(
    backend: &Backend,
    target: u64,
) -> BackendSnapshot {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let snapshot = backend.snapshot();
        if snapshot.stored_total >= target
            && snapshot
                .ingestion_budget
                .unavailable_bytes
                == 0
        {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "storage did not drain: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn request() -> QueryRequest {
    QueryRequest {
        limit: 128,
        before_id: None,
        level: None,
    }
}

fn measure_queries(
    mut submit: impl FnMut() -> io::Result<eventdock_protocol::PendingQuery>
) -> Vec<Duration> {
    (0..QUERIES)
        .map(|_| {
            let start = Instant::now();
            let response = submit()
                .unwrap()
                .recv_timeout(TIMEOUT)
                .unwrap();
            let elapsed = start.elapsed();
            assert_eq!(response.events.len(), 128);
            std::hint::black_box(response);
            elapsed
        })
        .collect()
}

fn report_queries(
    name: &str,
    mut samples: Vec<Duration>,
) {
    samples.sort_unstable();
    let percentile = |p: usize| {
        samples[(samples.len() * p).div_ceil(100) - 1].as_secs_f64() * 1000.0
    };
    println!(
        "{name}: p50={:.3} ms, p95={:.3} ms, p99={:.3} ms, max={:.3} ms",
        percentile(50),
        percentile(95),
        percentile(99),
        percentile(100)
    );
}

fn report_ingestion(
    name: &str,
    elapsed: Duration,
    snapshot: &BackendSnapshot,
) {
    println!(
        "{name}: {:.0} events/s, stored={}, rejected={}, retained={}, allocated_bytes={}, evicted={}",
        EVENTS as f64 / elapsed.as_secs_f64(),
        snapshot.stored_total,
        snapshot.rejected_total,
        snapshot.retained_events,
        snapshot
            .storage_memory
            .allocated_bytes(),
        snapshot.retention.evicted_total()
    );
    assert_eq!(snapshot.rejected_total, 0);
    assert_eq!(
        snapshot.stored_total,
        snapshot.retained_events as u64 + snapshot.retention.evicted_total()
    );
}

fn main() {
    // Avoid running the benchmark during cargo test --all-targets.
    if std::env::args().any(|arg| arg == "--test") {
        return;
    }
    let backend = Arc::new(
        Backend::start(BackendConfig {
            address: "127.0.0.1:0".parse().unwrap(),
            event_capacity: 4096,
            snapshot_interval: Duration::from_millis(5),
            ..BackendConfig::default()
        })
        .unwrap(),
    );
    let bytes = frame();
    println!("TCP ingestion: {EVENTS} events");
    let start = Instant::now();
    produce(backend.address(), &bytes).unwrap();
    let baseline = wait_for_storage(&backend, EVENTS);
    report_ingestion("TCP ingestion", start.elapsed(), &baseline);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let server = {
        let backend = Arc::clone(&backend);
        let running = Arc::clone(&running);
        thread::spawn(move || {
            serve_queries(listener, backend, running, 512, 1, 8)
        })
    };
    let remote = Client::new(RemoteTransport::new(address, 8).unwrap());
    println!("Idle queries: {QUERIES} per transport");
    report_queries(
        "local idle",
        measure_queries(|| backend.submit_client_query(request())),
    );
    report_queries(
        "remote idle",
        measure_queries(|| remote.submit_query(request())),
    );

    println!("Concurrent ingestion and local queries");
    let start = Instant::now();
    let producer = {
        let address = backend.address();
        thread::spawn(move || produce(address, &bytes))
    };
    let mut samples = Vec::new();
    while !producer.is_finished() && samples.len() < QUERIES {
        let start = Instant::now();
        let response = backend
            .submit_client_query(request())
            .unwrap()
            .recv_timeout(TIMEOUT)
            .unwrap();
        samples.push(start.elapsed());
        std::hint::black_box(response);
    }
    producer.join().unwrap().unwrap();
    let final_snapshot = wait_for_storage(&backend, 2 * EVENTS);
    report_ingestion("concurrent ingestion", start.elapsed(), &final_snapshot);
    if !samples.is_empty() {
        println!("Concurrent local query samples: {}", samples.len());
        report_queries("local during producer activity", samples);
    }
    println!(
        "Storage allocation: {} -> {} bytes",
        baseline
            .storage_memory
            .allocated_bytes(),
        final_snapshot
            .storage_memory
            .allocated_bytes()
    );
    drop(remote);
    running.store(false, Ordering::Release);
    server.join().unwrap().unwrap();
    Arc::try_unwrap(backend)
        .ok()
        .unwrap()
        .shutdown()
        .unwrap();
}
