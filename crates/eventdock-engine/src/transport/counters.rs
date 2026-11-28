use crate::NetworkSnapshot;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

#[derive(Default)]
pub(crate) struct NetworkCounters {
    active_connections: AtomicUsize,
    refused_connections: AtomicU64,
    normal_disconnects: AtomicU64,
    io_errors: AtomicU64,
    oversized_frames: AtomicU64,
    invalid_encoding: AtomicU64,
    malformed_events: AtomicU64,
    task_failures: AtomicU64,
}

impl NetworkCounters {
    pub(crate) fn register_connection(&self) {
        self.active_connections
            .fetch_add(1, Relaxed);
    }

    pub(crate) fn register_disconnection(&self) {
        self.active_connections
            .fetch_sub(1, Relaxed);
    }

    pub(crate) fn register_refused_connection(&self) {
        self.refused_connections
            .fetch_add(1, Relaxed);
    }

    pub(crate) fn register_normal_disconnect(&self) {
        self.normal_disconnects
            .fetch_add(1, Relaxed);
    }

    pub(crate) fn register_io_error(&self) {
        self.io_errors.fetch_add(1, Relaxed);
    }

    pub(crate) fn register_oversized_frame(&self) {
        self.oversized_frames
            .fetch_add(1, Relaxed);
    }

    pub(crate) fn register_invalid_encoding(&self) {
        self.invalid_encoding
            .fetch_add(1, Relaxed);
    }

    pub(crate) fn register_malformed_event(&self) {
        self.malformed_events
            .fetch_add(1, Relaxed);
    }

    pub(crate) fn register_task_failure(&self) {
        self.task_failures.fetch_add(1, Relaxed);
    }

    pub(crate) fn snapshot(&self) -> NetworkSnapshot {
        NetworkSnapshot {
            active_connections: self.active_connections.load(Relaxed),
            refused_connections: self.refused_connections.load(Relaxed),
            normal_disconnects: self.normal_disconnects.load(Relaxed),
            io_errors: self.io_errors.load(Relaxed),
            oversized_frames: self.oversized_frames.load(Relaxed),
            invalid_encoding: self.invalid_encoding.load(Relaxed),
            malformed_events: self.malformed_events.load(Relaxed),
            task_failures: self.task_failures.load(Relaxed),
        }
    }
}
