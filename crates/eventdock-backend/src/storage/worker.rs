use crate::{
    BackendConfig, BackendSnapshot, backend::QueryRequest,
    query::QueryExecution, shared_snapshot::SharedSnapshot,
    transport::Counters,
};
use eventdock::{CompactEventStore, IncomingEvent, IncomingEventRef};
use flume::{Receiver, RecvTimeoutError};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) fn run_storage(
    config: BackendConfig,
    receiver: Receiver<IncomingEvent>,
    publisher: Arc<SharedSnapshot>,
    counters: Arc<Counters>,
    queries: Receiver<QueryRequest>,
) {
    let mut store = CompactEventStore::with_limits(
        config.event_capacity,
        config.payload_capacity,
    );

    let mut snapshot = BackendSnapshot::default();
    let interval = config.snapshot_interval;
    let mut deadline = Instant::now() + interval;
    let mut active_query: Option<(QueryRequest, QueryExecution)> = None;

    loop {
        // Only one query executes at a time; the bounded channel queues others.
        if active_query.is_none()
            && let Ok(request) = queries.try_recv()
            && !request.reply.is_disconnected()
        {
            let execution = QueryExecution::new(request.query, &store);
            active_query = Some((request, execution));
        }

        // A query cannot monopolize the storage worker: scan a bounded slice,
        // then return to ingestion and snapshot publication.
        if let Some((request, execution)) = active_query.as_mut() {
            if request.reply.is_disconnected() {
                active_query = None;
            } else if execution.advance(&store, config.query_scan_budget) {
                let (request, execution) = active_query.take().unwrap();
                if !request.reply.is_disconnected() {
                    let _ = request
                        .reply
                        .send(execution.finish(&store));
                }
            }
        }

        let wait = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(2));

        match receiver.recv_timeout(wait) {
            Ok(event) => process_event(&mut store, &mut snapshot, &event),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if Instant::now() >= deadline {
            publish(&publisher, &mut snapshot, &counters, store.len());
            deadline = Instant::now() + interval;
        }
    }

    publish(&publisher, &mut snapshot, &counters, store.len());
}

fn process_event(
    store: &mut CompactEventStore,
    snapshot: &mut BackendSnapshot,
    event: &IncomingEvent,
) {
    let result = store.push(IncomingEventRef::from(event));

    if result.is_err() {
        snapshot.rejected_total += 1;
        return;
    }

    snapshot.stored_total += 1;
}

fn publish(
    publisher: &SharedSnapshot,
    snapshot: &mut BackendSnapshot,
    counters: &Counters,
    retained: usize,
) {
    snapshot.retained_events = retained;
    snapshot.network = counters.snapshot();
    publisher.publish(snapshot.clone());
}
