pub mod compact;
pub use compact::{
    CompactDecodingError, CompactEncodingError, CompactEventStore,
    CompactEventView, CompactStoreError, FieldIter, FieldValueView, FieldView,
};
mod console_sink;
mod error;
mod event;
mod field;
mod level;
mod logger;
mod store;
mod tcp_sink;
mod timestamp;

pub use console_sink::ConsoleSink;
pub use error::EventDockError;
pub use event::{EventId, EventRef, IncomingEvent, StoredEvent};
pub use field::{FieldValue, Fields};
pub use level::LogLevel;
pub use logger::{LogSink, Logger};
pub use store::EventStore;
pub use tcp_sink::{TcpSink, TcpSinkConfig, TcpSinkState};
pub use timestamp::Timestamp;
