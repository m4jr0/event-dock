//! Transport-independent, nonblocking query submission.
mod pending;

pub use eventdock_protocol::{QueryRequest, QueryResponse};
pub use pending::{PendingQuery, QueryReply, query_reply_channel};
use std::io;

/// Submit queries without waiting for transport I/O or queue capacity.
pub trait QueryTransport: Send + Sync {
    fn submit(
        &self,
        request: QueryRequest,
    ) -> io::Result<PendingQuery>;
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
