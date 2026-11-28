use crate::CompactEventStore;
use eventdock_core::EventId;
use eventdock_protocol::{EventQuery, EventQueryResult, QueriedEvent};

/// Scan borrowed event views and allocate only for the returned page.
/// A cursor is exclusive; evicted events are never resurrected.
pub fn execute_query(
    store: &CompactEventStore,
    query: EventQuery,
) -> EventQueryResult {
    let mut events: Vec<QueriedEvent> =
        Vec::with_capacity(query.limit.min(store.len()));

    let mut next_cursor = None;
    for summary in store.iter_metadata().rev() {
        if !query
            .before_id
            .is_none_or(|before| summary.id < before)
            || !query
                .level
                .is_none_or(|level| summary.level == level)
        {
            continue;
        }
        if events.len() == query.limit {
            next_cursor = events.last().map(|event| event.id);
            break;
        }
        if let Some(view) = store.get(summary.id) {
            events.push(QueriedEvent {
                id: summary.id,
                level: summary.level,
                occurred_at: summary.occurred_at,
                received_at: summary.received_at,
                source: view.source().to_owned(),
                message: view.message().to_owned(),
            });
        }
    }

    EventQueryResult {
        events,
        next_cursor,
    }
}

/// Incremental execution of a query against a retained event store.
///
/// Each `advance` call examines at most `scan_budget` event IDs. Event IDs
/// remain stable as the store evicts old events. Results are owned so they
/// remain valid after the storage worker resumes ingestion.
#[derive(Debug)]
pub struct QueryExecution {
    query: EventQuery,
    next_id: Option<EventId>,
    events: Vec<QueriedEvent>,
    finished: bool,
    has_more: bool,
}

impl QueryExecution {
    pub fn new(
        query: EventQuery,
        store: &CompactEventStore,
    ) -> Self {
        let newest = store
            .iter_metadata()
            .next_back()
            .map(|event| event.id);
        let next_id = match (newest, query.before_id) {
            (Some(newest), Some(before)) => {
                Some(newest.min(before.saturating_sub(1)))
            }
            (Some(newest), None) => Some(newest),
            (None, _) => None,
        };

        Self {
            query,
            next_id,
            events: Vec::new(),
            has_more: false,
            finished: query.limit == 0
                || next_id.is_none()
                || query.before_id == Some(0),
        }
    }

    /// Perform a bounded amount of scanning. Returns true when complete.
    pub fn advance(
        &mut self,
        store: &CompactEventStore,
        scan_budget: usize,
    ) -> bool {
        assert!(scan_budget > 0, "scan budget must be positive");

        if self.finished {
            return true;
        }

        let oldest = store
            .iter_metadata()
            .next()
            .map(|event| event.id);
        let mut examined = 0;

        while examined < scan_budget {
            let Some(id) = self.next_id else {
                self.finished = true;
                break;
            };

            if oldest.is_none_or(|first| id < first) {
                self.finished = true;
                break;
            }

            examined += 1;
            self.next_id = id.checked_sub(1);

            let Some(summary) = store.metadata_for(id) else {
                continue;
            };
            if !self
                .query
                .level
                .is_none_or(|level| summary.level == level)
            {
                continue;
            }
            if self.events.len() == self.query.limit {
                self.has_more = true;
                self.finished = true;
                break;
            }
            if let Some(view) = store.get(id) {
                self.events.push(QueriedEvent {
                    id: summary.id,
                    level: summary.level,
                    occurred_at: summary.occurred_at,
                    received_at: summary.received_at,
                    source: view.source().to_owned(),
                    message: view.message().to_owned(),
                });
            }
        }

        if self.next_id.is_none() {
            self.finished = true;
        }

        self.finished
    }

    pub fn finish(
        self,
        _store: &CompactEventStore,
    ) -> EventQueryResult {
        assert!(self.finished, "query execution is not complete");

        let next_cursor = self
            .has_more
            .then(|| self.events.last().map(|event| event.id))
            .flatten();

        EventQueryResult {
            events: self.events,
            next_cursor,
        }
    }
}
