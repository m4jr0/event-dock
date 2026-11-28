use super::{
    query_scheduler::QueryScheduler, snapshot_publisher::SnapshotPublisher,
};
use crate::{
    BackendConfig, CompactEventStore, backend::QueryRequest,
    shared_snapshot::SharedSnapshot, transport::Counters,
};
use eventdock_core::{IncomingEvent, IncomingEventRef};
use flume::{Receiver, RecvTimeoutError};
use std::{sync::Arc, time::Duration};

const MAX_INGESTION_WAIT: Duration = Duration::from_millis(2);

pub(crate) fn run_storage(
    config: BackendConfig,
    receiver: Receiver<IncomingEvent>,
    publisher: Arc<SharedSnapshot>,
    counters: Arc<Counters>,
    queries: Receiver<QueryRequest>,
) {
    let mut worker = StorageWorker {
        store: CompactEventStore::with_limits(
            config.event_capacity,
            config.payload_capacity,
        ),
        receiver,
        snapshots: SnapshotPublisher::new(
            publisher,
            counters,
            config.snapshot_interval,
        ),
        queries: QueryScheduler::new(queries, config.query_scan_budget),
    };
    worker.run();
}

struct StorageWorker {
    store: CompactEventStore,
    receiver: Receiver<IncomingEvent>,
    snapshots: SnapshotPublisher,
    queries: QueryScheduler,
}

impl StorageWorker {
    fn run(&mut self) {
        loop {
            self.queries.advance(&self.store);
            if !self.receive_event() {
                break;
            }
            self.snapshots
                .publish_if_due(self.store.len());
        }
        self.snapshots.publish(self.store.len());
    }

    fn receive_event(&mut self) -> bool {
        let wait = self
            .snapshots
            .time_until_due()
            .min(MAX_INGESTION_WAIT);
        match self.receiver.recv_timeout(wait) {
            Ok(event) => self.ingest(&event),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
        }
        true
    }

    fn ingest(
        &mut self,
        event: &IncomingEvent,
    ) {
        let accepted = self
            .store
            .push(IncomingEventRef::from(event))
            .is_ok();
        self.snapshots
            .record_ingestion(accepted);
    }
}
