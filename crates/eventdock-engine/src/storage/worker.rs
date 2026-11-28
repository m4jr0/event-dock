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

pub(super) fn create_store(
    config: BackendConfig,
    receiver: Receiver<IncomingEvent>,
    publisher: Arc<SharedSnapshot>,
    counters: Arc<Counters>,
    queries: Receiver<QueryRequest>,
) -> StoreTask {
    StoreTask::new(config, receiver, publisher, counters, queries)
}

enum Wake {
    Registration(Result<StoreTask, flume::RecvError>),
    Event(usize, Result<IncomingEvent, flume::RecvError>),
    Query(usize, Result<QueryRequest, flume::RecvError>),
}

pub(super) struct StorageWorker {
    stores: Vec<StoreTask>,
    registrations: Receiver<StoreTask>,
}

impl StorageWorker {
    pub(super) fn new(registrations: Receiver<StoreTask>) -> Self {
        Self {
            stores: Vec::new(),
            registrations,
        }
    }

    pub(super) fn run(&mut self) {
        loop {
            let registered = self.accept_registration();
            let worked = self.advance_stores();
            if self.stores.is_empty()
                && self.registrations.is_disconnected()
                && self.registrations.is_empty()
            {
                break;
            }
            if !worked && !registered {
                self.wait_for_work();
            }
        }
    }

    fn accept_registration(&mut self) -> bool {
        if let Ok(store) = self.registrations.try_recv() {
            self.stores.push(store);
            true
        } else {
            false
        }
    }

    fn advance_stores(&mut self) -> bool {
        let mut worked = false;
        self.stores.retain_mut(|store| {
            worked |= store.advance();
            if store.is_closed() {
                store
                    .snapshots
                    .publish(store.store.len());
                false
            } else {
                true
            }
        });
        worked
    }

    fn wait_for_work(&mut self) {
        let mut selector = Selector::new();
        if !self.registrations.is_disconnected()
            || !self.registrations.is_empty()
        {
            selector = selector.recv(&self.registrations, Wake::Registration);
        } else if self.stores.is_empty() {
            return;
        }
        for (index, store) in self.stores.iter().enumerate() {
            if let Some(receiver) = &store.receiver {
                selector = selector
                    .recv(receiver, move |result| Wake::Event(index, result));
            }
            // Register even a newly closed query channel so closure cannot be missed.
            if !store
                .queries
                .requests()
                .is_disconnected()
            {
                selector = selector
                    .recv(store.queries.requests(), move |result| {
                        Wake::Query(index, result)
                    });
            } else if store.receiver.is_none() {
                return;
            }
        }
        let wait = self
            .stores
            .iter()
            .map(|store| store.snapshots.time_until_due())
            .min();
        let wake = match wait {
            Some(wait) => selector.wait_timeout(wait).ok(),
            None => Some(selector.wait()),
        };
        match wake {
            Some(Wake::Registration(Ok(store))) => self.stores.push(store),
            Some(Wake::Registration(Err(_))) => {}
            Some(Wake::Event(index, Ok(event))) => {
                self.stores[index].ingest(&event)
            }
            Some(Wake::Event(index, Err(_))) => {
                self.stores[index].receiver = None
            }
            Some(Wake::Query(index, Ok(request))) => {
                let store = &mut self.stores[index];
                store
                    .queries
                    .accept_request(request, &store.store);
            }
            Some(Wake::Query(_, Err(_))) | None => {}
        }
    }
}

pub(super) struct StoreTask {
    store: CompactEventStore,
    receiver: Option<Receiver<IncomingEvent>>,
    snapshots: SnapshotPublisher,
    queries: QueryScheduler,
}

impl StoreTask {
    fn new(
        config: BackendConfig,
        receiver: Receiver<IncomingEvent>,
        publisher: Arc<SharedSnapshot>,
        counters: Arc<Counters>,
        queries: Receiver<QueryRequest>,
    ) -> Self {
        Self {
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
        }
    }

    fn advance(&mut self) -> bool {
        let had_query = self.queries.has_work();
        self.queries.advance(&self.store);
        let ingested = self.receive_event();
        self.snapshots
            .publish_if_due(self.store.len());
        had_query || ingested || self.queries.has_work()
    }

    fn is_closed(&self) -> bool {
        self.receiver.is_none()
            && self
                .queries
                .requests()
                .is_disconnected()
    }

    fn receive_event(&mut self) -> bool {
        let Some(receiver) = &self.receiver else {
            return false;
        };
        match receiver.try_recv() {
            Ok(event) => {
                self.ingest(&event);
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                false
            }
            Err(TryRecvError::Empty) => false,
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
