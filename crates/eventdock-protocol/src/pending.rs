use crate::QueryResponse;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Bound admitted queries until their results are consumed or discarded.
#[derive(Debug, Clone)]
pub struct QueryAdmission {
    available: Arc<AtomicUsize>,
}

impl QueryAdmission {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            available: Arc::new(AtomicUsize::new(capacity)),
        }
    }

    pub fn try_channel(&self) -> io::Result<(QueryReply, PendingQuery)> {
        self.available
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_sub(1)
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "outstanding query limit reached",
                )
            })?;
        Ok(reply_channel(Some(QueryPermit(Arc::clone(
            &self.available,
        )))))
    }
}

struct QueryPermit(Arc<AtomicUsize>);

impl Drop for QueryPermit {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Release);
    }
}

struct Completion {
    result: io::Result<QueryResponse>,
    _permit: Option<QueryPermit>,
}

pub struct PendingQuery {
    response: flume::Receiver<Completion>,
}

pub struct QueryReply {
    sender: flume::Sender<Completion>,
    permit: Option<QueryPermit>,
}

pub fn query_reply_channel() -> (QueryReply, PendingQuery) {
    reply_channel(None)
}

fn reply_channel(permit: Option<QueryPermit>) -> (QueryReply, PendingQuery) {
    let (sender, response) = flume::bounded(1);
    (QueryReply { sender, permit }, PendingQuery { response })
}

fn disconnected() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "query transport disconnected")
}

impl PendingQuery {
    /// Poll without blocking. Consume the response at most once.
    pub fn try_recv(&self) -> io::Result<Option<QueryResponse>> {
        match self.response.try_recv() {
            Ok(completion) => completion.result.map(Some),
            Err(flume::TryRecvError::Empty) => Ok(None),
            Err(flume::TryRecvError::Disconnected) => Err(disconnected()),
        }
    }

    pub fn recv(self) -> io::Result<QueryResponse> {
        self.response
            .recv()
            .map_err(|_| disconnected())?
            .result
    }

    pub async fn recv_async(self) -> io::Result<QueryResponse> {
        self.response
            .recv_async()
            .await
            .map_err(|_| disconnected())?
            .result
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
        self.sender
            .try_send(Completion {
                result,
                _permit: self.permit,
            })
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_tracks_completed_and_abandoned_queries() {
        let admission = QueryAdmission::new(1);
        let (reply, pending) = admission.try_channel().unwrap();
        assert!(admission.try_channel().is_err());
        assert!(reply.complete(Ok(QueryResponse {
            events: Vec::new(),
            next_cursor: None
        })));
        assert!(admission.try_channel().is_err());
        assert!(pending.try_recv().unwrap().is_some());
        let (reply, pending) = admission.try_channel().unwrap();
        drop(pending);
        assert!(admission.try_channel().is_err());
        assert!(!reply.complete(Err(io::Error::other("cancelled"))));
        let (reply, pending) = admission.try_channel().unwrap();
        assert!(reply.complete(Err(io::Error::other("failed"))));
        drop(pending);
        assert!(admission.try_channel().is_ok());
    }

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
