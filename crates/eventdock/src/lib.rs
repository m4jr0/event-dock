pub use storage::compact::{
    CompactDecodingError, CompactEncodingError, CompactEventStore,
    CompactEventView, CompactStoreError, FieldIter, FieldValueView, FieldView,
};

mod error;
mod event;
mod field;
pub mod identity;
mod level;
mod logger;
mod sinks;
mod storage;
mod timestamp;

pub use error::EventDockError;
pub use event::IncomingEventRef as EventRef;
pub use event::{EventId, IncomingEvent, IncomingEventRef};
pub use field::{FieldValue, Fields};
pub use level::LogLevel;
pub use logger::{LogSink, Logger};
pub use sinks::{BackgroundSink, BackgroundSinkConfig, ConsoleSink};
pub use sinks::{TcpSink, TcpSinkConfig, TcpSinkState};
pub use storage::EventStore;
pub use storage::StoredEvent;
pub use storage::{EventStorage, EventSummary, EventViewAccess, StorageError};
pub use timestamp::Timestamp;

/// Version of this library package.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
