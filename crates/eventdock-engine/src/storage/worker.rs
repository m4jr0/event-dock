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
        receiver: Some(receiver),
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
    receiver: Option<Receiver<IncomingEvent>>,
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
        if let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(event) => {
                    self.ingest(&event);
                    return true;
                }
                Err(TryRecvError::Disconnected) => self.receiver = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        if self.receiver.is_none()
            && self
                .queries
                .requests()
                .is_disconnected()
        {
            return false;
        }
        if self.queries.has_work() {
            return true;
        }
        self.wait_for_work();
        true
    }

    fn wait_for_work(&mut self) {
        let selector = Selector::new();
        let selector = if let Some(receiver) = &self.receiver {
            selector.recv(receiver, Wake::Event)
        } else {
            selector
        };
        // Do not select a closed query channel: it would wake continuously.
        let selector = if self
            .queries
            .requests()
            .is_disconnected()
        {
            // Avoid waiting on an empty selector during shutdown.
            if self.receiver.is_none() {
                return;
            }
            selector
        } else {
            selector.recv(self.queries.requests(), Wake::Query)
        };
        let wake = selector.wait_timeout(self.snapshots.time_until_due());

        match wake {
            Ok(Wake::Event(Ok(event))) => self.ingest(&event),
            Ok(Wake::Event(Err(_))) => self.receiver = None,
            Ok(Wake::Query(Ok(request))) => {
                self.queries
                    .accept_request(request, &self.store);
            }
            Ok(Wake::Query(Err(_))) | Err(_) => {}
        }
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
