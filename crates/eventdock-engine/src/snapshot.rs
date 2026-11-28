#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub active_connections: usize,
    pub refused_connections: u64,
    pub normal_disconnects: u64,
    pub io_errors: u64,
    pub oversized_frames: u64,
    pub invalid_encoding: u64,
    pub malformed_events: u64,
    pub task_failures: u64,
}

/// Container allocations owned by a compact event store, excluding allocator overhead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageMemoryUsage {
    pub live_payload_bytes: usize,
    pub payload_allocated_bytes: usize,
    pub live_metadata_bytes: usize,
    pub metadata_allocated_bytes: usize,
    pub encoding_scratch_allocated_bytes: usize,
}

impl StorageMemoryUsage {
    pub fn allocated_bytes(&self) -> usize {
        self.payload_allocated_bytes
            + self.metadata_allocated_bytes
            + self.encoding_scratch_allocated_bytes
    }

    pub fn live_bytes(&self) -> usize {
        self.live_payload_bytes + self.live_metadata_bytes
    }
}

/// Cumulative retention evictions, classified by the first enforced limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionStatistics {
    pub event_limit_evictions: u64,
    pub payload_limit_evictions: u64,
    pub evicted_payload_bytes: u64,
}

impl RetentionStatistics {
    pub fn evicted_total(&self) -> u64 {
        self.event_limit_evictions
            .saturating_add(self.payload_limit_evictions)
    }
}

/// Sample bounded queue occupancy, excluding blocked senders and completed replies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueUsage {
    pub queued: usize,
    pub capacity: usize,
}

/// Report pending work separately from retained storage memory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingWorkUsage {
    pub ingestion: QueueUsage,
    pub queries: QueueUsage,
    pub active_query: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackendSnapshot {
    pub stored_total: u64,
    pub rejected_total: u64,
    pub retained_events: usize,
    /// Storage usage at publication time, before any subsequent destruction.
    pub storage_memory: StorageMemoryUsage,
    pub retention: RetentionStatistics,
    pub pending_work: PendingWorkUsage,
    pub network: NetworkSnapshot,
}
