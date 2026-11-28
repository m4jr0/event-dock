//! Shared foundational types for EventDock.

mod event;
mod field;
pub mod identity;
mod level;
mod timestamp;

pub use event::{EventId, IncomingEvent, IncomingEventRef};
pub use field::{FieldValue, Fields};
pub use level::LogLevel;
pub use timestamp::Timestamp;
