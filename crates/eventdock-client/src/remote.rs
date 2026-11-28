use crate::{
    PendingQuery, QueryReply, QueryRequest, QueryResponse, QueryTransport,
    query_reply_channel,
};
use eventdock_protocol::{
    MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, WireResponse, read_json_line,
    write_json_line,
};
use std::{
    io::{self, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

struct Job {
    request: QueryRequest,
    reply: QueryReply,
    deadline: Instant,
}

/// Drive remote requests on a bounded background worker.
pub struct RemoteTransport {
    jobs: Option<flume::Sender<Job>>,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl RemoteTransport {
    pub fn new(
        address: SocketAddr,
        capacity: usize,
    ) -> io::Result<Self> {
        if capacity == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "queue capacity must be positive",
            ));
        }
        let (jobs, receiver) = flume::bounded::<Job>(capacity);
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let worker = thread::Builder::new()
            .name("eventdock-client".into())
            .spawn(move || {
                run_worker(address, receiver, &worker_running, &runtime)
            })?;
        Ok(Self {
            jobs: Some(jobs),
            running,
            worker: Some(worker),
        })
    }
}

impl QueryTransport for RemoteTransport {
    fn submit(
        &self,
        request: QueryRequest,
    ) -> io::Result<PendingQuery> {
        let (reply, pending) = query_reply_channel();
        self.jobs
            .as_ref()
            .ok_or_else(stopped)?
            .try_send(Job {
                request,
                reply,
                deadline: Instant::now() + TIMEOUT,
            })
            .map_err(|error| match error {
                flume::TrySendError::Full(_) => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "remote query queue is full",
                ),
                flume::TrySendError::Disconnected(_) => io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "remote query worker stopped",
                ),
            })?;
        Ok(pending)
    }
}

fn stopped() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "remote query transport stopped")
}

impl Drop for RemoteTransport {
    fn drop(&mut self) {
        self.running
            .store(false, Ordering::Release);
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    address: SocketAddr,
    receiver: flume::Receiver<Job>,
    running: &AtomicBool,
    runtime: &tokio::runtime::Runtime,
) {
    while let Ok(job) = receiver.recv() {
        if !running.load(Ordering::Acquire) {
            break;
        }
        if !job.reply.is_cancelled() {
            let result = query(address, &job, running, runtime);
            job.reply.complete(result);
        }
    }
}

struct RequestIo<'a> {
    stream: TcpStream,
    job: &'a Job,
    running: &'a AtomicBool,
}

fn remaining(
    job: &Job,
    running: &AtomicBool,
) -> io::Result<Duration> {
    if !running.load(Ordering::Acquire) || job.reply.is_cancelled() {
        return Err(stopped());
    }
    let wait = job
        .deadline
        .saturating_duration_since(Instant::now());
    if wait.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "remote query deadline exceeded",
        ));
    }
    Ok(wait)
}

fn retryable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

impl Read for RequestIo<'_> {
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> io::Result<usize> {
        loop {
            self.stream.set_read_timeout(Some(
                remaining(self.job, self.running)?.min(POLL_INTERVAL),
            ))?;
            match self.stream.read(bytes) {
                Err(error) if retryable(&error) => continue,
                result => return result,
            }
        }
    }
}

impl Write for RequestIo<'_> {
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> io::Result<usize> {
        loop {
            self.stream.set_write_timeout(Some(
                remaining(self.job, self.running)?.min(POLL_INTERVAL),
            ))?;
            match self.stream.write(bytes) {
                Err(error) if retryable(&error) => continue,
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        remaining(self.job, self.running)?;
        self.stream.flush()
    }
}

// Keep the same connection attempt alive while checking cancellation.
async fn connect(
    address: SocketAddr,
    job: &Job,
    running: &AtomicBool,
) -> io::Result<TcpStream> {
    let mut attempt = std::pin::pin!(tokio::net::TcpStream::connect(address));
    loop {
        let wait = remaining(job, running)?.min(POLL_INTERVAL);
        if let Ok(result) = tokio::time::timeout(wait, attempt.as_mut()).await {
            remaining(job, running)?;
            let stream = result?.into_std()?;
            stream.set_nonblocking(false)?;
            return Ok(stream);
        }
    }
}

fn query(
    address: SocketAddr,
    job: &Job,
    running: &AtomicBool,
    runtime: &tokio::runtime::Runtime,
) -> io::Result<QueryResponse> {
    let stream = runtime.block_on(connect(address, job, running))?;
    let mut stream = RequestIo {
        stream,
        job,
        running,
    };
    write_json_line(&mut stream, &job.request, MAX_REQUEST_BYTES)?;
    read_json_line::<WireResponse>(BufReader::new(stream), MAX_RESPONSE_BYTES)?
        .into_result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::BufRead, net::TcpListener};

    #[test]
    fn dropping_transport_cancels_active_and_queued_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let transport =
            RemoteTransport::new(listener.local_addr().unwrap(), 1).unwrap();
        let request = QueryRequest {
            limit: 1,
            before_id: None,
            level: None,
        };
        let active = transport.submit(request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "connection stalled");
                    thread::sleep(POLL_INTERVAL);
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.ends_with('\n'));
        let queued = transport.submit(request).unwrap();
        let start = Instant::now();
        drop(transport);
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(reader.read(&mut [0]).unwrap(), 0);
        assert_eq!(
            active.recv().unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(
            queued.recv().unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn dropping_pending_releases_active_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let transport =
            RemoteTransport::new(listener.local_addr().unwrap(), 1).unwrap();
        let pending = transport
            .submit(QueryRequest {
                limit: 1,
                before_id: None,
                level: None,
            })
            .unwrap();
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "connection stalled");
                    thread::sleep(POLL_INTERVAL);
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut request = String::new();
        reader.read_line(&mut request).unwrap();
        assert!(request.ends_with('\n'));
        drop(pending);
        assert_eq!(reader.read(&mut [0]).unwrap(), 0);
    }
}
