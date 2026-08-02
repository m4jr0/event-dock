use crate::BackendSnapshot;
use std::sync::Mutex;

/// Publishes immutable copies of the latest backend snapshot.
#[derive(Default)]
pub(crate) struct SharedSnapshot {
    value: Mutex<BackendSnapshot>,
}

impl SharedSnapshot {
    pub(crate) fn read(&self) -> BackendSnapshot {
        self.lock().clone()
    }

    pub(crate) fn publish(
        &self,
        snapshot: BackendSnapshot,
    ) {
        *self.lock() = snapshot;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BackendSnapshot> {
        // Statistics are replaceable; recover the last value if a writer panicked.
        self.value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
