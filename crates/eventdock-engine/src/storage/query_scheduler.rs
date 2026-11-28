use crate::{CompactEventStore, backend::QueryRequest, query::QueryExecution};
use flume::Receiver;

pub(super) struct QueryScheduler {
    requests: Receiver<QueryRequest>,
    active: Option<(QueryRequest, QueryExecution)>,
    scan_budget: usize,
}

impl QueryScheduler {
    pub(super) fn new(
        requests: Receiver<QueryRequest>,
        scan_budget: usize,
    ) -> Self {
        Self {
            requests,
            active: None,
            scan_budget,
        }
    }

    pub(super) fn advance(
        &mut self,
        store: &CompactEventStore,
    ) {
        self.accept_pending(store);
        self.advance_active(store);
    }

    fn accept_pending(
        &mut self,
        store: &CompactEventStore,
    ) {
        if self.active.is_none()
            && let Ok(request) = self.requests.try_recv()
            && !request.reply.is_disconnected()
        {
            let execution = QueryExecution::new(request.query, store);
            self.active = Some((request, execution));
        }
    }

    fn advance_active(
        &mut self,
        store: &CompactEventStore,
    ) {
        let Some((request, execution)) = self.active.as_mut() else {
            return;
        };

        if request.reply.is_disconnected() {
            self.active = None;
        } else if execution.advance(store, self.scan_budget) {
            self.finish_active(store);
        }
    }

    fn finish_active(
        &mut self,
        store: &CompactEventStore,
    ) {
        let (request, execution) = self
            .active
            .take()
            .expect("active query");
        if !request.reply.is_disconnected() {
            let _ = request
                .reply
                .send(execution.finish(store));
        }
    }
}
