use eventdock_client::{Client, QueryRequest, RemoteTransport};
use eventdock_engine::{Backend, BackendConfig, serve_queries};
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

fn request() -> QueryRequest {
    QueryRequest {
        limit: 1,
        before_id: None,
        level: None,
    }
}

#[test]
fn local_and_remote_queries_agree() {
    let backend = Arc::new(
        Backend::start(BackendConfig {
            address: "127.0.0.1:0".parse().unwrap(),
            ..BackendConfig::default()
        })
        .unwrap(),
    );
    let mut producer = TcpStream::connect(backend.address()).unwrap();
    let event = eventdock_core::IncomingEvent::new(
        "test",
        eventdock_core::LogLevel::Info,
        "hello",
    );
    serde_json::to_writer(&mut producer, &event).unwrap();
    producer.write_all(b"\n").unwrap();
    drop(producer);
    let deadline = Instant::now() + Duration::from_secs(5);
    while backend.snapshot().stored_total == 0 {
        assert!(Instant::now() < deadline, "ingestion stalled");
        thread::sleep(Duration::from_millis(5));
    }
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
    let local = Client::new(|request| backend.submit_client_query(request));
    let remote = Client::new(RemoteTransport::new(address, 8).unwrap());
    let expected = local
        .submit_query(request())
        .unwrap()
        .recv()
        .unwrap();
    let actual = remote
        .submit_query(request())
        .unwrap()
        .recv()
        .unwrap();
    let invalid = QueryRequest {
        limit: 0,
        ..request()
    };
    let local_error = local
        .submit_query(invalid)
        .err()
        .unwrap();
    let remote_error = remote
        .submit_query(invalid)
        .unwrap()
        .recv()
        .unwrap_err();
    running.store(false, Ordering::Release);
    server.join().unwrap().unwrap();
    assert_eq!(expected, actual);
    assert_eq!(actual.events.len(), 1);
    assert_eq!(actual.events[0].message, "hello");
    assert_eq!(local_error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(local_error.to_string(), remote_error.to_string());
    Arc::try_unwrap(backend)
        .ok()
        .unwrap()
        .shutdown()
        .unwrap();
}

#[test]
fn remote_rejects_truncated_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(&stream)
            .read_line(&mut line)
            .unwrap();
        stream
            .write_all(br#"{"events":[],"next_cursor":null}"#)
            .unwrap();
    });
    let client = Client::new(RemoteTransport::new(address, 1).unwrap());
    let error = client
        .submit_query(request())
        .unwrap()
        .recv()
        .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}
