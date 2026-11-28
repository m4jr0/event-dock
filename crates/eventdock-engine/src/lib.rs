mod backend;
mod config;
mod ingestion;
mod query;
pub mod query_protocol;
mod resource_budget;
mod shared_snapshot;
mod snapshot;
mod storage;
mod transport;

pub use backend::{Backend, PendingQuery};
pub use config::BackendConfig;
pub use resource_budget::{ByteBudget, ByteReservation};
pub use snapshot::{
    BackendSnapshot, NetworkSnapshot, PendingWorkUsage, QueueUsage,
    RetentionStatistics, StorageMemoryUsage,
};

pub use eventdock_protocol::{EventQuery, EventQueryResult, QueriedEvent};
pub use query::{QueryExecution, execute_query};

pub use query_protocol::{
    QueryRequest, QueryResponse, query_remote, serve_queries,
};

/// Version of this library package.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use storage::compact::{
    CompactDecodingError, CompactEncodingError, CompactEventStore,
    CompactEventView, CompactStoreError, FieldIter, FieldValueView, FieldView,
};
pub use storage::{
    EventStorage, EventStore, EventSummary, EventViewAccess, StorageError,
    StoredEvent,
};

use eventdock_core::{
    EventId, FieldValue, Fields, IncomingEvent, IncomingEventRef, LogLevel,
    Timestamp,
};
