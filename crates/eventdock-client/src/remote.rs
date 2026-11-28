use crate::{
    PendingQuery, QueryReply, QueryRequest, QueryResponse, QueryTransport,
    query_reply_channel,
};
use eventdock_protocol::WireResponse;
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    thread,
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

struct Job {
    request: QueryRequest,
    reply: QueryReply,
}

/// Drive remote requests on a bounded background worker.
pub struct RemoteTransport {
    jobs: flume::Sender<Job>,
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
        thread::Builder::new()
            .name("eventdock-client".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    if !job.reply.is_cancelled() {
                        job.reply
                            .complete(query(address, job.request));
                    }
                }
            })?;
        Ok(Self { jobs })
    }
}

impl QueryTransport for RemoteTransport {
    fn submit(
        &self,
        request: QueryRequest,
    ) -> io::Result<PendingQuery> {
        let (reply, pending) = query_reply_channel();
        self.jobs
            .try_send(Job { request, reply })
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

fn query(
    address: SocketAddr,
    request: QueryRequest,
) -> io::Result<QueryResponse> {
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    serde_json::to_writer(&mut stream, &request).map_err(io::Error::other)?;
    stream.write_all(b"\n")?;
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
