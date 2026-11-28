//! Transport-independent, nonblocking query submission.
mod remote;
pub use remote::RemoteTransport;

pub use eventdock_protocol::{PendingQuery, QueryReply, query_reply_channel};
pub use eventdock_protocol::{QueryRequest, QueryResponse};
use std::io;

/// Submit queries without waiting for transport I/O or queue capacity.
pub trait QueryTransport: Send + Sync {
    fn submit(
        &self,
        request: QueryRequest,
    ) -> io::Result<PendingQuery>;
}

impl<F> QueryTransport for F
where
    F: Fn(QueryRequest) -> io::Result<PendingQuery> + Send + Sync,
{
    fn submit(
        &self,
        request: QueryRequest,
    ) -> io::Result<PendingQuery> {
        self(request)
    }
}

pub struct Client<T> {
    transport: T,
}

impl<T: QueryTransport> Client<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn submit_query(
        &self,
        request: QueryRequest,
    ) -> io::Result<PendingQuery> {
        self.transport.submit(request)
    }
}
