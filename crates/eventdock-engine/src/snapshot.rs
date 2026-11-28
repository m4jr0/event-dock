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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackendSnapshot {
    pub stored_total: u64,
    pub rejected_total: u64,
    pub retained_events: usize,
    pub network: NetworkSnapshot,
}
