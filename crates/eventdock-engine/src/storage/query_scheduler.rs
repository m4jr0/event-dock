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

    pub(super) fn has_work(&self) -> bool {
        self.active.is_some() || !self.requests.is_empty()
    }

    pub(super) fn requests(&self) -> &Receiver<QueryRequest> {
        &self.requests
    }

    pub(super) fn accept_request(
        &mut self,
        request: QueryRequest,
        store: &CompactEventStore,
    ) {
        debug_assert!(self.active.is_none());
        if !request.reply.is_disconnected() {
            let execution = QueryExecution::new(request.query, store);
            self.active = Some((request, execution));
        }
    }

    fn accept_pending(
        &mut self,
        store: &CompactEventStore,
    ) {
        if self.active.is_none()
            && let Ok(request) = self.requests.try_recv()
        {
            self.accept_request(request, store);
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
            request
                .reply
                .complete(execution.finish(store));
        }
    }
}
