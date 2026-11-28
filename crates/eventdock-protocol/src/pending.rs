use crate::QueryResponse;
use std::io;

pub struct PendingQuery {
    response: flume::Receiver<io::Result<QueryResponse>>,
}

pub struct QueryReply {
    sender: flume::Sender<io::Result<QueryResponse>>,
}

pub fn query_reply_channel() -> (QueryReply, PendingQuery) {
    let (sender, response) = flume::bounded(1);
    (QueryReply { sender }, PendingQuery { response })
}

fn disconnected() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "query transport disconnected")
}

impl PendingQuery {
    /// Poll without blocking. Consume the response at most once.
    pub fn try_recv(&self) -> io::Result<Option<QueryResponse>> {
        match self.response.try_recv() {
            Ok(result) => result.map(Some),
            Err(flume::TryRecvError::Empty) => Ok(None),
            Err(flume::TryRecvError::Disconnected) => Err(disconnected()),
        }
    }

    pub fn recv(self) -> io::Result<QueryResponse> {
        self.response
            .recv()
            .map_err(|_| disconnected())?
    }

    pub async fn recv_async(self) -> io::Result<QueryResponse> {
        self.response
            .recv_async()
            .await
            .map_err(|_| disconnected())?
    }
}

impl QueryReply {
    pub fn is_cancelled(&self) -> bool {
        self.sender.is_disconnected()
    }

    /// Deliver a single result without blocking. Return false if abandoned.
    pub fn complete(
        self,
        result: io::Result<QueryResponse>,
    ) -> bool {
        self.sender.try_send(result).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_response_can_be_polled() {
        let (reply, pending) = query_reply_channel();
        assert!(pending.try_recv().unwrap().is_none());
        assert!(reply.complete(Ok(QueryResponse {
            events: Vec::new(),
            next_cursor: None
        })));
        assert!(
            pending
                .try_recv()
                .unwrap()
                .unwrap()
                .events
                .is_empty()
        );
    }

    #[test]
    fn dropping_pending_cancels_reply() {
        let (reply, pending) = query_reply_channel();
        drop(pending);
        assert!(reply.is_cancelled());
        assert!(!reply.complete(Err(io::Error::other("cancelled"))));
    }

    #[test]
    fn dropping_reply_reports_disconnection() {
        let (reply, pending) = query_reply_channel();
        drop(reply);
        assert_eq!(
            pending.recv().unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn transport_errors_reach_client() {
        let (reply, pending) = query_reply_channel();
        assert!(
            reply.complete(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timeout"
            )))
        );
        assert_eq!(pending.recv().unwrap_err().kind(), io::ErrorKind::TimedOut);
    }
}
