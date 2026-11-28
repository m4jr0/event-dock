use eventdock_engine::{Backend, BackendConfig, BackendSnapshot};
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

#[test]
fn stopped_ingestion_preserves_history() {
    let mut backend = Backend::start(config()).unwrap();
    let mut stream = connect(&backend);
    stream.write_all(EVENT).unwrap();
    wait_until(&backend, |s| s.stored_total == 1);

    backend.stop_ingestion().unwrap();
    backend.stop_ingestion().unwrap();
    assert!(!backend.is_ingesting());
    assert!(backend.is_running());

    let page = backend
        .query(eventdock_engine::EventQuery {
            before_id: None,
            limit: 1,
            level: None,
        })
        .unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].message, "bar");
    assert_eq!(
        backend
            .shutdown()
            .unwrap()
            .retained_events,
        1
    );
}

#[test]
fn sustained_ingestion_preserves_resource_invariants() {
    const CLIENTS: usize = 4;
    const PER_WAVE: usize = 1000;
    const WAVES: usize = 4;
    const PAYLOAD: usize = 64 * 1024;
    const CAPACITY: usize = 128;
    const BUDGET: usize = 4096;
    let mut backend = Backend::start(BackendConfig {
        event_capacity: CAPACITY,
        payload_capacity: PAYLOAD,
        queue_capacity: 8,
        max_event_bytes: 2048,
        ingestion_byte_capacity: BUDGET,
        max_query_text_bytes: 4096,
        ..config()
    })
    .unwrap();
    let address = backend.address();
    let mut previous_allocated = 0;
    for wave in 0..WAVES {
        thread::scope(|scope| {
            for client in 0..CLIENTS {
                scope.spawn(move || {
                    let mut stream = TcpStream::connect(address).unwrap();
                    stream.set_write_timeout(Some(Duration::from_secs(30))).unwrap();
                    for index in 0..PER_WAVE {
                        let message = "x".repeat(32 + (index * 37 + client * 13) % 1200);
                        let frame = format!("{{\"timestamp\":42,\"source\":\"test\",\"level\":\"info\",\"message\":\"{message}\"}}\n");
                        stream.write_all(frame.as_bytes()).unwrap();
                    }
                    stream.shutdown(Shutdown::Write).unwrap();
                });
            }
            for _ in 0..32 {
                let page = backend
                    .query(eventdock_engine::EventQuery {
                        before_id: None,
                        limit: 128,
                        level: None,
                    })
                    .unwrap();
                assert!(
                    page.events
                        .windows(2)
                        .all(|pair| pair[0].id > pair[1].id)
                );
                assert!(
                    page.events
                        .iter()
                        .map(|event| event.source.len() + event.message.len())
                        .sum::<usize>()
                        <= 4096
                );
            }
        });
        let expected = ((wave + 1) * CLIENTS * PER_WAVE) as u64;
        let deadline = Instant::now() + Duration::from_secs(30);
        let snapshot = loop {
            let snapshot = backend.snapshot();
            assert!(
                snapshot
                    .ingestion_budget
                    .unavailable_bytes
                    <= BUDGET
            );
            if snapshot.stored_total == expected
                && snapshot
                    .ingestion_budget
                    .unavailable_bytes
                    == 0
                && snapshot.network.active_connections == 0
            {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "ingestion stalled: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(snapshot.rejected_total, 0);
        assert_eq!(snapshot.network.malformed_events, 0);
        assert!(snapshot.retained_events <= CAPACITY);
        assert!(
            snapshot
                .storage_memory
                .live_payload_bytes
                <= PAYLOAD
        );
        assert_eq!(
            snapshot.stored_total,
            snapshot.retained_events as u64
                + snapshot.retention.evicted_total()
        );
        assert!(
            snapshot
                .storage_memory
                .payload_allocated_bytes
                <= 2 * PAYLOAD
        );
        let allocated = snapshot
            .storage_memory
            .allocated_bytes();
        if wave >= 2 {
            assert_eq!(
                allocated, previous_allocated,
                "storage allocation did not stabilize"
            );
        }
        previous_allocated = allocated;
    }
    backend.stop_ingestion().unwrap();
    let mut cursor = None;
    let mut ids = Vec::new();
    loop {
        let page = backend
            .query(eventdock_engine::EventQuery {
                before_id: cursor,
                limit: 128,
                level: None,
            })
            .unwrap();
        ids.extend(page.events.iter().map(|event| event.id));
        match page.next_cursor {
            Some(next) => {
                assert!(cursor.is_none_or(|old| next < old));
                cursor = Some(next);
            }
            None => break,
        }
    }
    let snapshot = backend.shutdown().unwrap();
    assert_eq!(ids.len(), snapshot.retained_events);
    assert!(
        ids.windows(2)
            .all(|pair| pair[0] == pair[1] + 1)
    );
    assert_eq!(
        ids.first().copied(),
        Some((CLIENTS * PER_WAVE * WAVES - 1) as u64)
    );
    assert_eq!(
        snapshot
            .ingestion_budget
            .unavailable_bytes,
        0
    );
    assert_eq!(snapshot.pending_work.ingestion.queued, 0);
}
