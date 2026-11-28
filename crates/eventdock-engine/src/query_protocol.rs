//! Typed, newline-delimited JSON query protocol shared by clients and the server.
use crate::{Backend, EventQuery};
use eventdock_protocol::WireResponse;
pub use eventdock_protocol::{QueryRequest, QueryResponse};
use serde::Serialize;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self},
    time::{Duration, Instant},
};

const MAX_REQUEST_BYTES: u64 = 4096;
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);

fn validate_request(
    request: QueryRequest,
    max_query_events: usize,
) -> io::Result<EventQuery> {
    if request.limit == 0 || request.limit > max_query_events {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("query limit must be between 1 and {max_query_events}"),
        ));
    }
    Ok(EventQuery {
        before_id: request.before_id,
        limit: request.limit,
        level: request.level,
    })
}

/// One request and one response per connection. No backend instance is needed by clients.
pub fn query_remote(
    address: &str,
    request: QueryRequest,
) -> io::Result<QueryResponse> {
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    write_json_line(&mut stream, &request)?;

    let mut bytes = Vec::new();
    let count = BufReader::new(stream)
        .take(MAX_RESPONSE_BYTES + 1)
        .read_until(b'\n', &mut bytes)?;
    if count as u64 > MAX_RESPONSE_BYTES || !bytes.ends_with(b"\n") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid query response framing",
        ));
    }
    match serde_json::from_slice::<WireResponse>(&bytes)
        .map_err(io::Error::other)?
    {
        WireResponse::Success(response) => Ok(response),
        WireResponse::Failure { error } => Err(io::Error::other(error)),
    }
}

fn write_json_line<T: Serialize>(
    writer: &mut impl Write,
    value: &T,
) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, value).map_err(io::Error::other)?;
    writer.write_all(b"\n")
}

// Check cancellation and the total connection deadline before each I/O operation.
struct ConnectionIo<'a> {
    stream: TcpStream,
    running: &'a AtomicBool,
    deadline: Instant,
}

impl ConnectionIo<'_> {
    fn remaining(&self) -> io::Result<Duration> {
        if !self.running.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "query server stopped",
            ));
        }
        let remaining = self
            .deadline
            .saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "query deadline exceeded",
            ));
        }
        Ok(remaining.min(ACCEPT_POLL_INTERVAL))
    }
}

impl Read for ConnectionIo<'_> {
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> io::Result<usize> {
        loop {
            self.stream
                .set_read_timeout(Some(self.remaining()?))?;
            match self.stream.read(bytes) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }
}

impl Write for ConnectionIo<'_> {
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> io::Result<usize> {
        loop {
            self.stream
                .set_write_timeout(Some(self.remaining()?))?;
            match self.stream.write(bytes) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.remaining()?;
        self.stream.flush()
    }
}

fn handle_connection(
    stream: TcpStream,
    backend: &Backend,
    max_query_events: usize,
    running: &AtomicBool,
    deadline: Instant,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    let mut stream = ConnectionIo {
        stream,
        running,
        deadline,
    };
    let mut bytes = Vec::new();
    let count = BufReader::new(&mut stream)
        .take(MAX_REQUEST_BYTES + 1)
        .read_until(b'\n', &mut bytes)?;

    let response =
        if count as u64 > MAX_REQUEST_BYTES || !bytes.ends_with(b"\n") {
            WireResponse::Failure {
                error: "query request exceeds limit or lacks newline".into(),
            }
        } else {
            match execute_request(&bytes, backend, max_query_events, &stream) {
                Ok(result) => WireResponse::Success(result),
                Err(error) => WireResponse::Failure {
                    error: error.to_string(),
                },
            }
        };
    write_json_line(&mut stream, &response)
}

fn execute_request(
    bytes: &[u8],
    backend: &Backend,
    max_query_events: usize,
    connection: &ConnectionIo<'_>,
) -> io::Result<QueryResponse> {
    let request: QueryRequest = serde_json::from_slice(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let query = validate_request(request, max_query_events)?;
    connection.remaining()?;
    let pending = backend.submit_query(query)?;
    loop {
        let wait = connection.remaining()?;
        if let Some(result) = pending.try_recv()? {
            return Ok(result.into());
        }
        thread::sleep(wait);
    }
}

/// A bounded set of query workers. The acceptor never waits for a slow client.
/// Requests beyond the bounded queue are rejected rather than spawning unlimited threads.
pub fn serve_queries(
    listener: TcpListener,
    backend: Arc<Backend>,
    running: Arc<AtomicBool>,
    max_query_events: usize,
    workers: usize,
    queue_capacity: usize,
) -> io::Result<()> {
    if workers == 0 || queue_capacity == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "query workers and queue capacity must be positive",
        ));
    }
    listener.set_nonblocking(true)?;
    let (sender, receiver) =
        mpsc::sync_channel::<(TcpStream, Instant)>(queue_capacity);
    let receiver = Arc::new(Mutex::new(receiver));
    let mut handles = Vec::with_capacity(workers);
    for index in 0..workers {
        let receiver = Arc::clone(&receiver);
        let backend = Arc::clone(&backend);
        let worker_running = Arc::clone(&running);
        let handle = thread::Builder::new()
            .name(format!("eventdock-query-{index}"))
            .spawn(move || {
                loop {
                    let stream = {
                        let guard = receiver
                            .lock()
                            .expect("query queue mutex poisoned");
                        guard.recv()
                    };
                    let Ok((stream, deadline)) = stream else {
                        break;
                    };
                    if !worker_running.load(Ordering::Acquire) {
                        break;
                    }
                    if Instant::now() >= deadline {
                        continue;
                    }
                    if let Err(error) = handle_connection(
                        stream,
                        &backend,
                        max_query_events,
                        &worker_running,
                        deadline,
                    ) && worker_running.load(Ordering::Acquire)
                    {
                        eprintln!("Query failed: {error}");
                    }
                }
            });
        match handle {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                running.store(false, Ordering::Release);
                drop(sender);
                let _ = join_workers(handles);
                return Err(error);
            }
        }
    }
    let result = accept_queries(&listener, &sender, &running);
    running.store(false, Ordering::Release);
    drop(sender);
    let joined = join_workers(handles);
    result.and(joined)
}

fn join_workers(handles: Vec<thread::JoinHandle<()>>) -> io::Result<()> {
    let mut failed = false;
    for handle in handles {
        failed |= handle.join().is_err();
    }
    if failed {
        Err(io::Error::other("query worker panicked"))
    } else {
        Ok(())
    }
}

fn accept_queries(
    listener: &TcpListener,
    sender: &SyncSender<(TcpStream, Instant)>,
    running: &AtomicBool,
) -> io::Result<()> {
    while running.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                match sender.try_send((stream, Instant::now() + TIMEOUT)) {
                    Ok(()) => {}
                    Err(TrySendError::Full((mut stream, _))) => {
                        // Attempt a best-effort busy response without blocking the acceptor.
                        if stream.set_nonblocking(true).is_ok() {
                            let _ = stream
                                .write(b"{\"error\":\"query server busy\"}\n");
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => {
                        return Err(io::Error::other("query workers stopped"));
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL)
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eventdock_core::LogLevel;

    #[test]
    fn connection_read_observes_total_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _client =
            TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let running = AtomicBool::new(true);
        let mut connection = ConnectionIo {
            stream,
            running: &running,
            deadline: Instant::now() + Duration::from_millis(50),
        };
        assert_eq!(
            connection
                .read(&mut [0])
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn connection_io_observes_shutdown() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _client =
            TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let running = AtomicBool::new(false);
        let mut connection = ConnectionIo {
            stream,
            running: &running,
            deadline: Instant::now() + TIMEOUT,
        };
        assert_eq!(
            connection
                .read(&mut [0])
                .unwrap_err()
                .kind(),
            io::ErrorKind::ConnectionAborted
        );
        assert_eq!(
            connection
                .write(b"response")
                .unwrap_err()
                .kind(),
            io::ErrorKind::ConnectionAborted
        );
    }

    #[test]
    fn request_round_trip() {
        let request = QueryRequest {
            limit: 20,
            before_id: Some(42),
            level: Some(LogLevel::Error),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<QueryRequest>(&json).unwrap(),
            request
        );
    }

    #[test]
    fn rejects_zero_limit() {
        let request = QueryRequest {
            limit: 0,
            before_id: None,
            level: None,
        };
        assert_eq!(
            validate_request(request, 512)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(
            serde_json::from_str::<QueryRequest>(
                r#"{"limit":1,"surprise":true}"#
            )
            .is_err()
        );
    }
}
