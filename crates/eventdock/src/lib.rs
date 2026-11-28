pub use storage::compact::{
    CompactDecodingError, CompactEncodingError, CompactEventStore,
    CompactEventView, CompactStoreError, FieldIter, FieldValueView, FieldView,
};

mod error;
mod logger;
mod sinks;
mod storage;

pub use error::EventDockError;
pub use eventdock_core::{
    EventId, FieldValue, Fields, IncomingEvent, IncomingEventRef, LogLevel,
    Timestamp, identity,
};
pub use eventdock_core::IncomingEventRef as EventRef;
pub use logger::{LogSink, Logger};
pub use sinks::{BackgroundSink, BackgroundSinkConfig, ConsoleSink};
pub use sinks::{TcpSink, TcpSinkConfig, TcpSinkState};
pub use storage::EventStore;
pub use storage::StoredEvent;
pub use storage::{EventStorage, EventSummary, EventViewAccess, StorageError};

/// Version of this library package.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
