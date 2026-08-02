use std::{io, net::SocketAddr, time::Duration};

#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub address: SocketAddr,
    pub event_capacity: usize,
    pub payload_capacity: usize,
    pub queue_capacity: usize,
    pub query_queue_capacity: usize,
    pub max_query_events: usize,
    pub query_scan_budget: usize,
    pub max_event_bytes: usize,
    pub max_connections: usize,
    pub snapshot_interval: Duration,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            address: SocketAddr::from(([127, 0, 0, 1], 7878)),
            event_capacity: 100_000,
            payload_capacity: 64 * 1024 * 1024,
            queue_capacity: 4096,
            query_queue_capacity: 64,
            max_query_events: 512,
            query_scan_budget: 256,
            max_event_bytes: 1024 * 1024,
            max_connections: 64,
            snapshot_interval: Duration::from_millis(33),
        }
    }
}

impl BackendConfig {
    pub(crate) fn validate(&self) -> io::Result<()> {
        if self.event_capacity == 0
            || self.payload_capacity == 0
            || self.queue_capacity == 0
            || self.query_queue_capacity == 0
            || self.max_query_events == 0
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

        Ok(())
    }
}
