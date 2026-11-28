use crate::{
    BackendSnapshot, shared_snapshot::SharedSnapshot, transport::Counters,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct SnapshotPublisher {
    publisher: Arc<SharedSnapshot>,
    counters: Arc<Counters>,
    snapshot: BackendSnapshot,
    interval: Duration,
    deadline: Instant,
}

impl SnapshotPublisher {
    pub(super) fn new(
        publisher: Arc<SharedSnapshot>,
        counters: Arc<Counters>,
        interval: Duration,
    ) -> Self {
        Self {
            publisher,
            counters,
            snapshot: BackendSnapshot::default(),
            interval,
            deadline: Instant::now() + interval,
        }
    }

    pub(super) fn record_ingestion(
        &mut self,
        accepted: bool,
    ) {
        if accepted {
            self.snapshot.stored_total += 1;
        } else {
            self.snapshot.rejected_total += 1;
        }
    }

    pub(super) fn time_until_due(&self) -> Duration {
        self.deadline
            .saturating_duration_since(Instant::now())
    }

    pub(super) fn publish_if_due(
        &mut self,
        store: &crate::CompactEventStore,
    ) {
        if Instant::now() >= self.deadline {
            self.publish(store);
            self.deadline = Instant::now() + self.interval;
        }
    }

    pub(super) fn publish(
        &mut self,
        store: &crate::CompactEventStore,
    ) {
        self.snapshot.retained_events = store.len();
        self.snapshot.storage_memory = store.memory_usage();
        self.snapshot.network = self.counters.snapshot();
        self.publisher
            .publish(self.snapshot.clone());
    }
}
