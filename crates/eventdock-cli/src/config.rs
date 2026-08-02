//! CLI-specific defaults. Backend limits remain owned by BackendConfig.

pub const DEFAULT_ADDRESS: &str = "127.0.0.1:7878";
pub const DEFAULT_QUERY_ADDRESS: &str = "127.0.0.1:7879";
pub const DEFAULT_EVENT_CAPACITY: usize = 100_000;
pub const DEFAULT_QUERY_LIMIT: usize = 50;
pub const QUERY_WORKERS: usize = 4;
pub const QUERY_QUEUE_CAPACITY: usize = 32;
