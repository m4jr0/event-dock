use std::{io, net::SocketAddr, time::Duration};

#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub address: SocketAddr,
    pub storage_worker_count: usize,
    pub event_capacity: usize,
    pub payload_capacity: usize,
    pub queue_capacity: usize,
    /// Limit admitted ingestion by original JSON frame bytes.
    pub ingestion_byte_capacity: usize,
    pub query_queue_capacity: usize,
    pub max_query_events: usize,
    /// Target maximum source and message bytes per query page.
    pub max_query_text_bytes: usize,
    pub query_scan_budget: usize,
    pub max_event_bytes: usize,
    pub max_connections: usize,
    pub snapshot_interval: Duration,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            address: SocketAddr::from(([127, 0, 0, 1], 7878)),
            storage_worker_count: 1,
            event_capacity: 100_000,
            payload_capacity: 64 * 1024 * 1024,
            queue_capacity: 4096,
            ingestion_byte_capacity: 32 * 1024 * 1024,
            query_queue_capacity: 64,
            max_query_events: 512,
            max_query_text_bytes: 512 * 1024,
            query_scan_budget: 256,
            max_event_bytes: 1024 * 1024,
            max_connections: 64,
            snapshot_interval: Duration::from_millis(33),
        }
    }
}

impl BackendConfig {
    /// Describe configured allowances, not a process memory ceiling.
    pub fn resource_limits(&self) -> crate::ResourceLimits {
        crate::ResourceLimits {
            retained_payload_bytes: self.payload_capacity,
            retained_events: self.event_capacity,
            ingestion_frame_bytes: self.ingestion_byte_capacity,
            queued_events: self.queue_capacity,
            queued_queries: self.query_queue_capacity,
            producer_connections: self.max_connections,
        }
    }

    pub(crate) fn validate(&self) -> io::Result<()> {
        if self.storage_worker_count == 0
            || self.event_capacity == 0
            || self.payload_capacity == 0
            || self.queue_capacity == 0
            || self.query_queue_capacity == 0
            || self.max_query_events == 0
            || self.max_query_text_bytes == 0
            || self.query_scan_budget == 0
            || self.max_event_bytes == 0
            || self.max_connections == 0
            || self.snapshot_interval.is_zero()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "backend limits and snapshot interval must be positive",
            ));
        }

        if self.ingestion_byte_capacity < self.max_event_bytes
            || self.ingestion_byte_capacity > crate::ByteBudget::MAX_CAPACITY
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ingestion byte capacity must cover one maximum frame and fit the byte budget",
            ));
        }
        Ok(())
    }
}
