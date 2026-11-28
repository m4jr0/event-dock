use super::{
    query_scheduler::QueryScheduler, snapshot_publisher::SnapshotPublisher,
};
use crate::{
    BackendConfig, CompactEventStore, backend::QueryRequest,
    shared_snapshot::SharedSnapshot, transport::Counters,
};
use eventdock_core::{IncomingEvent, IncomingEventRef};
use flume::{Receiver, Selector, TryRecvError};
use std::sync::Arc;

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

enum Wake {
    Event(Result<IncomingEvent, flume::RecvError>),
    Query(Result<QueryRequest, flume::RecvError>),
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
        match self.receiver.try_recv() {
            Ok(event) => self.ingest(&event),
            Err(TryRecvError::Disconnected) => return false,
            Err(TryRecvError::Empty) if !self.queries.has_work() => {
                return self.wait_for_work();
            }
            Err(TryRecvError::Empty) => {}
        }
        true
    }

    fn wait_for_work(&mut self) -> bool {
        let selector = Selector::new().recv(&self.receiver, Wake::Event);
        // Do not select a closed query channel: it would wake continuously.
        let selector = if self
            .queries
            .requests()
            .is_disconnected()
        {
            selector
        } else {
            selector.recv(self.queries.requests(), Wake::Query)
        };
        let wake = selector.wait_timeout(self.snapshots.time_until_due());

        match wake {
            Ok(Wake::Event(Ok(event))) => self.ingest(&event),
            Ok(Wake::Event(Err(_))) => return false,
            Ok(Wake::Query(Ok(request))) => {
                self.queries
                    .accept_request(request, &self.store);
            }
            Ok(Wake::Query(Err(_))) | Err(_) => {}
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
