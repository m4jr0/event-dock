use eventdock_backend::{Backend, BackendConfig, BackendSnapshot};
use std::{
    io::{self, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    thread,
    time::{Duration, Instant},
};

const EVENT: &[u8] =
    b"{\"timestamp\":42,\"source\":\"foo\",\"level\":\"info\",\"message\":\"bar\"}\n";
const TIMEOUT: Duration = Duration::from_secs(5);

fn config() -> BackendConfig {
    BackendConfig {
        address: SocketAddr::from(([127, 0, 0, 1], 0)),
        snapshot_interval: Duration::from_millis(5),
        ..BackendConfig::default()
    }
}

fn wait_until(
    backend: &Backend,
    predicate: impl Fn(&BackendSnapshot) -> bool,
) {
    let deadline = Instant::now() + TIMEOUT;

    loop {
        let snapshot = backend.snapshot();

        if predicate(&snapshot) {
            return;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for condition; last snapshot: {snapshot:?}"
        );

        thread::sleep(Duration::from_millis(5));
    }
}

fn connect(backend: &Backend) -> TcpStream {
    let stream = TcpStream::connect(backend.address()).unwrap();
    stream
        .set_write_timeout(Some(TIMEOUT))
        .unwrap();
    stream
}

#[test]
fn invalid_utf8_is_classified_and_disconnects_client() {
    let backend = Backend::start(config()).unwrap();
    let mut stream = connect(&backend);

    stream
        .write_all(&[0xff, b'\n'])
        .unwrap();

    wait_until(&backend, |s| s.network.invalid_encoding == 1);

    let snapshot = backend.shutdown().unwrap();
    assert_eq!(snapshot.network.invalid_encoding, 1);
    assert_eq!(snapshot.stored_total, 0);
    assert_eq!(snapshot.network.active_connections, 0);
}

#[test]
fn clean_eof_is_a_normal_disconnect() {
    let backend = Backend::start(config()).unwrap();
    let stream = connect(&backend);

    wait_until(&backend, |s| s.network.active_connections == 1);
    stream
        .shutdown(Shutdown::Write)
        .unwrap();
    drop(stream);

    wait_until(&backend, |s| {
        s.network.normal_disconnects == 1 && s.network.active_connections == 0
    });

    let snapshot = backend.shutdown().unwrap();
    assert_eq!(snapshot.network.normal_disconnects, 1);
    assert_eq!(snapshot.network.io_errors, 0);
}

#[test]
fn connection_limit_refuses_extra_client_and_recovers() {
    let backend = Backend::start(BackendConfig {
        max_connections: 1,
        ..config()
    })
    .unwrap();

    let first = connect(&backend);
    wait_until(&backend, |s| s.network.active_connections == 1);

    let _second = connect(&backend);
    wait_until(&backend, |s| s.network.refused_connections >= 1);

    drop(first);
    wait_until(&backend, |s| s.network.active_connections == 0);

    let mut third = connect(&backend);
    third.write_all(EVENT).unwrap();
    wait_until(&backend, |s| s.stored_total == 1);

    let snapshot = backend.shutdown().unwrap();
    assert_eq!(snapshot.network.refused_connections, 1);
    assert_eq!(snapshot.stored_total, 1);
}

#[test]
fn concurrent_clients_store_all_complete_events() {
    const CLIENTS: usize = 8;
    const EVENTS_PER_CLIENT: usize = 40;

    let backend = Backend::start(config()).unwrap();
    let address = backend.address();

    let workers: Vec<_> = (0..CLIENTS)
        .map(|_| {
            thread::spawn(move || {
                let mut stream = TcpStream::connect(address).unwrap();
                stream
                    .set_write_timeout(Some(TIMEOUT))
                    .unwrap();

                for _ in 0..EVENTS_PER_CLIENT {
                    stream.write_all(EVENT).unwrap();
                }

                stream
                    .shutdown(Shutdown::Write)
                    .unwrap();
            })
        })
        .collect();

    for worker in workers {
        worker.join().unwrap();
    }

    let expected = (CLIENTS * EVENTS_PER_CLIENT) as u64;
    wait_until(&backend, |s| s.stored_total == expected);

    let snapshot = backend.shutdown().unwrap();
    assert_eq!(snapshot.stored_total, expected);
    assert_eq!(snapshot.rejected_total, 0);
    assert_eq!(snapshot.network.active_connections, 0);
}

#[test]
fn malformed_json_does_not_poison_connection() {
    let backend = Backend::start(config()).unwrap();
    let mut stream = connect(&backend);

    stream.write_all(b"not json\n").unwrap();
    stream.write_all(EVENT).unwrap();

    wait_until(&backend, |s| {
        s.network.malformed_events == 1 && s.stored_total == 1
    });

    let snapshot = backend.shutdown().unwrap();
    assert_eq!(snapshot.network.malformed_events, 1);
    assert_eq!(snapshot.stored_total, 1);
}

#[test]
fn shutdown_completes_with_small_queue_and_active_producers() {
    let backend = Backend::start(BackendConfig {
        queue_capacity: 1,
        ..config()
    })
    .unwrap();

    // Keep connections alive and write a finite burst. This exercises shutdown
    // while intake may be waiting for queue capacity, without relying on sleeps.
    let mut streams: Vec<_> = (0..4)
        .map(|_| connect(&backend))
        .collect();

    for stream in &mut streams {
        stream.write_all(EVENT).unwrap();
    }

    wait_until(&backend, |s| s.network.active_connections == 4);

    let started = Instant::now();
    let snapshot = backend.shutdown().unwrap();

    assert!(started.elapsed() < TIMEOUT, "shutdown stalled");
    assert_eq!(snapshot.network.active_connections, 0);

    // Events already in sockets or awaiting queue admission are not guaranteed
    // to survive shutdown. Do not assert an exact stored_total here.
    drop(streams);
}

#[test]
fn ingests_evicts_and_counts_malformed() {
    let backend = Backend::start(BackendConfig {
        event_capacity: 2,
        ..config()
    })
    .unwrap();

    let mut stream = TcpStream::connect(backend.address()).unwrap();
    stream.write_all(b"bad json\n").unwrap();

    for _ in 0..3 {
        stream.write_all(b"{\"timestamp\":123,\"source\":\"test\",\"level\":\"info\",\"message\":\"hello\"}\n").unwrap();
    }

    let deadline = Instant::now() + Duration::from_secs(5);

    while backend.snapshot().stored_total != 3 {
        assert!(Instant::now() < deadline, "ingestion timed out");
        thread::sleep(Duration::from_millis(10));
    }

    let snapshot = backend.shutdown().unwrap();
    assert_eq!(snapshot.retained_events, 2);
    assert_eq!(snapshot.network.malformed_events, 1);
    assert_eq!(snapshot.network.active_connections, 0);
}

#[test]
fn rejects_zero_snapshot_interval() {
    let result = Backend::start(BackendConfig {
        snapshot_interval: Duration::ZERO,
        ..config()
    });

    assert!(
        matches!(result, Err(ref error) if error.kind() == io::ErrorKind::InvalidInput)
    );
}

#[test]
fn classifies_oversized_frames() {
    let backend = Backend::start(BackendConfig {
        max_event_bytes: 32,
        ..config()
    })
    .unwrap();

    let mut stream = TcpStream::connect(backend.address()).unwrap();
    stream.write_all(&[b'x'; 128]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);

    while backend
        .snapshot()
        .network
        .oversized_frames
        == 0
    {
        assert!(Instant::now() < deadline, "oversized frame not counted");
        thread::sleep(Duration::from_millis(10));
    }

    let snapshot = backend.shutdown().unwrap();
    assert_eq!(snapshot.network.oversized_frames, 1);
}
