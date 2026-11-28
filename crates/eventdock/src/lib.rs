mod error;
mod logger;
mod sinks;

pub use error::EventDockError;
pub use eventdock_core::IncomingEventRef as EventRef;
pub use eventdock_core::{
    EventId, FieldValue, Fields, IncomingEvent, IncomingEventRef, LogLevel,
    Timestamp, identity,
};
pub use logger::{LogSink, Logger};
pub use sinks::{BackgroundSink, BackgroundSinkConfig, ConsoleSink};
pub use sinks::{TcpSink, TcpSinkConfig, TcpSinkState};

/// Version of this library package.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
