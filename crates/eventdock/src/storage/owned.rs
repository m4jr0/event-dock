use crate::{
    EventId, EventStorage, EventSummary, EventViewAccess, FieldView,
    IncomingEvent, IncomingEventRef, StorageError, Timestamp,
};
use std::collections::VecDeque;

/// An owned event retained by `EventStore`, with assigned storage metadata.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: EventId,
    pub received_at: Timestamp,
    pub event: IncomingEvent,
}

/// Bounded, in-memory event history.
/// Once full, pushing a new event removes the oldest event.
#[derive(Debug)]
pub struct EventStore {
    next_id: EventId,
    events: VecDeque<StoredEvent>,
    capacity: usize,
}

impl EventStore {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "EventStore capacity must be greater than zero"
        );

        Self {
            next_id: 0,
            events: VecDeque::new(),
            capacity,
        }
    }

    /// Store a borrowed incoming event. Failed insertion leaves the store unchanged.
    pub fn push(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<EventId, StorageError> {
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(StorageError::IdExhausted)?;
        let event = IncomingEvent::from(event);
        let id = self.next_id;
        self.next_id = next_id;

        if self.events.len() == self.capacity {
            self.events.pop_front();
        }

        self.events.push_back(StoredEvent {
            id,
            received_at: Timestamp::now(),
            event,
        });

        Ok(id)
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &StoredEvent> {
        self.events.iter()
    }

    #[must_use]
    pub fn get(
        &self,
        id: EventId,
    ) -> Option<&StoredEvent> {
        let oldest_id = self.events.front()?.id;
        let offset = id.checked_sub(oldest_id)?;
        let index = usize::try_from(offset).ok()?;

        self.events.get(index)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

type OwnedFieldsIter<'a> = std::iter::Map<
    std::collections::btree_map::Iter<'a, String, crate::FieldValue>,
    fn((&'a String, &'a crate::FieldValue)) -> FieldView<'a>,
>;

fn owned_field<'a>(
    (name, value): (&'a String, &'a crate::FieldValue)
) -> FieldView<'a> {
    let value = match value {
        crate::FieldValue::Null => crate::FieldValueView::Null,
        crate::FieldValue::Bool(value) => crate::FieldValueView::Bool(*value),
        crate::FieldValue::I64(value) => crate::FieldValueView::I64(*value),
        crate::FieldValue::U64(value) => crate::FieldValueView::U64(*value),
        crate::FieldValue::F64(value) => crate::FieldValueView::F64(*value),
        crate::FieldValue::String(value) => {
            crate::FieldValueView::String(value)
        }
    };

    FieldView { name, value }
}

impl EventViewAccess for &crate::StoredEvent {
    type FieldsIter<'a>
        = OwnedFieldsIter<'a>
    where
        Self: 'a;

    fn summary(&self) -> EventSummary {
        EventSummary {
            id: self.id,
            occurred_at: self.event.timestamp,
            received_at: self.received_at,
            level: self.event.level,
        }
    }

    fn source(&self) -> &str {
        &self.event.source
    }

    fn message(&self) -> &str {
        &self.event.message
    }

    fn fields(&self) -> Self::FieldsIter<'_> {
        self.event
            .fields
            .iter()
            .map(owned_field)
    }
}

fn owned_summary(event: &crate::StoredEvent) -> EventSummary {
    EventSummary {
        id: event.id,
        occurred_at: event.event.timestamp,
        received_at: event.received_at,
        level: event.event.level,
    }
}

impl EventStorage for crate::EventStore {
    type View<'a> = &'a crate::StoredEvent;

    fn push(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<EventId, StorageError> {
        crate::EventStore::push(self, event)
    }

    fn get(
        &self,
        id: EventId,
    ) -> Option<Self::View<'_>> {
        crate::EventStore::get(self, id)
    }

    fn iter_metadata(
        &self
    ) -> impl DoubleEndedIterator<Item = EventSummary> + '_ {
        crate::EventStore::iter(self).map(owned_summary)
    }

    fn len(&self) -> usize {
        crate::EventStore::len(self)
    }

    fn capacity(&self) -> Option<usize> {
        Some(crate::EventStore::capacity(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogLevel;

    fn make_event(message: &str) -> IncomingEvent {
        IncomingEvent::new("foo", LogLevel::Info, message)
    }

    #[test]
    fn accepts_borrowed_event_and_owns_its_data() {
        let mut store = EventStore::new(2);

        {
            let event = IncomingEvent::new("source", LogLevel::Info, "message")
                .with_field("count", 42);
            store
                .push(crate::IncomingEventRef::from(&event))
                .unwrap();
        }

        let stored = store.get(0).unwrap();
        assert_eq!(stored.event.source, "source");
        assert_eq!(stored.event.message, "message");
        assert_eq!(stored.event.fields.len(), 1);
    }

    #[test]
    fn new_store_is_empty() {
        let store = EventStore::new(3);

        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert_eq!(store.capacity(), 3);
        assert!(store.get(0).is_none());
    }

    #[test]
    fn assigns_sequential_ids() {
        let mut store = EventStore::new(3);

        assert_eq!(
            store
                .push(IncomingEventRef::from(&make_event("foo")))
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .push(IncomingEventRef::from(&make_event("bar")))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .push(IncomingEventRef::from(&make_event("baz")))
                .unwrap(),
            2
        );
    }

    #[test]
    fn preserves_insertion_order() {
        let mut store = EventStore::new(3);

        store
            .push(IncomingEventRef::from(&make_event("foo")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("bar")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("baz")))
            .unwrap();

        let messages: Vec<_> = store
            .iter()
            .map(|stored| stored.event.message.as_str())
            .collect();

        assert_eq!(messages, ["foo", "bar", "baz"]);
    }

    #[test]
    fn evicts_oldest_event_when_full() {
        let mut store = EventStore::new(3);

        store
            .push(IncomingEventRef::from(&make_event("foo")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("bar")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("baz")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("qux")))
            .unwrap();

        assert_eq!(store.len(), 3);
        assert!(store.get(0).is_none());

        let ids: Vec<_> = store
            .iter()
            .map(|stored| stored.id)
            .collect();

        assert_eq!(ids, [1, 2, 3]);
    }

    #[test]
    fn lookup_handles_evicted_and_future_ids() {
        let mut store = EventStore::new(2);

        store
            .push(IncomingEventRef::from(&make_event("foo")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("bar")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("baz")))
            .unwrap();

        assert!(store.get(0).is_none());
        assert_eq!(store.get(1).unwrap().event.message, "bar");
        assert_eq!(store.get(2).unwrap().event.message, "baz");
        assert!(store.get(3).is_none());
        assert!(store.get(u64::MAX).is_none());
    }

    #[test]
    fn reverse_iteration_returns_newest_first() {
        let mut store = EventStore::new(3);

        store
            .push(IncomingEventRef::from(&make_event("foo")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("bar")))
            .unwrap();
        store
            .push(IncomingEventRef::from(&make_event("baz")))
            .unwrap();

        let ids: Vec<_> = store
            .iter()
            .rev()
            .map(|stored| stored.id)
            .collect();

        assert_eq!(ids, [2, 1, 0]);
    }

    #[test]
    #[should_panic(expected = "capacity must be greater than zero")]
    fn zero_capacity_is_rejected() {
        let _ = EventStore::new(0);
    }
}
