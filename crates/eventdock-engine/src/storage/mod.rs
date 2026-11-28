//! Backend-neutral access to retained event history.

pub mod compact;
mod pool;
mod query_scheduler;
mod snapshot_publisher;
mod worker;
pub(crate) use pool::StoragePool;
mod error;
mod owned;

pub use error::StorageError;
pub use owned::{EventStore, StoredEvent};

use crate::{EventId, FieldView, IncomingEventRef, LogLevel, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSummary {
    pub id: EventId,
    pub occurred_at: Timestamp,
    pub received_at: Timestamp,
    pub level: LogLevel,
}

/// Common borrowed access. Field iteration remains lazy for compact events.
pub trait EventViewAccess {
    fn summary(&self) -> EventSummary;
    fn source(&self) -> &str;
    fn message(&self) -> &str;

    type FieldsIter<'a>: Iterator<Item = FieldView<'a>>
    where
        Self: 'a;

    fn fields(&self) -> Self::FieldsIter<'_>;
}

/// Shared storage contract, using GATs for backend-specific borrowed views.
///
/// `push` consumes only a borrowed input for the duration of the call. A successful
/// insertion assigns a monotonically increasing ID and retains its own data.
/// Validation errors and ID exhaustion leave previously retained events intact.
/// `get` borrows from the store; metadata iteration follows insertion order and
/// supports reverse traversal. `capacity` is an optional event-count limit,
/// independent of any backend-specific byte limit.
pub trait EventStorage {
    type View<'a>: EventViewAccess
    where
        Self: 'a;

    fn push(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<EventId, StorageError>;

    fn get(
        &self,
        id: EventId,
    ) -> Option<Self::View<'_>>;

    fn iter_metadata(
        &self
    ) -> impl DoubleEndedIterator<Item = EventSummary> + '_;

    fn len(&self) -> usize;
    fn capacity(&self) -> Option<usize>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompactEventStore, EventStore, IncomingEvent};

    fn exercise<S: EventStorage>(mut store: S) {
        for i in 0..10 {
            let event = IncomingEvent::new(
                "source",
                LogLevel::Info,
                format!("event {i}"),
            )
            .with_field("index", i as i64);

            assert_eq!(
                store
                    .push(IncomingEventRef::from(&event))
                    .unwrap(),
                i
            );
        }

        assert_eq!(store.len(), 4);
        assert!(store.get(5).is_none());

        let ids: Vec<_> = store
            .iter_metadata()
            .map(|m| m.id)
            .collect();

        assert_eq!(ids, [6, 7, 8, 9]);

        let newest = store.get(9).unwrap();
        assert_eq!(newest.message(), "event 9");
        assert_eq!(newest.source(), "source");
        assert_eq!(newest.fields().count(), 1);
    }

    #[test]
    fn both_backends_agree_after_eviction() {
        exercise(EventStore::new(4));
        exercise(CompactEventStore::with_limits(4, 4096));
    }
}
