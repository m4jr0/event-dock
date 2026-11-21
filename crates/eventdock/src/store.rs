use crate::{EventId, IncomingEvent, StoredEvent, Timestamp};
use std::collections::VecDeque;

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

    fn allocate_id(&mut self) -> EventId {
        let id = self.next_id;

        self.next_id = id
            .checked_add(1)
            .expect("Event ID counter overflowed");

        id
    }

    pub fn push(
        &mut self,
        event: IncomingEvent,
    ) -> EventId {
        let id = self.allocate_id();

        if self.events.len() == self.capacity {
            self.events.pop_front();
        }

        self.events.push_back(StoredEvent {
            id,
            received_at: Timestamp::now(),
            event,
        });

        id
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LogLevel;

    fn make_event(message: &str) -> IncomingEvent {
        IncomingEvent::new("foo", LogLevel::Info, message)
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

        assert_eq!(store.push(make_event("foo")), 0);
        assert_eq!(store.push(make_event("bar")), 1);
        assert_eq!(store.push(make_event("baz")), 2);
    }

    #[test]
    fn preserves_insertion_order() {
        let mut store = EventStore::new(3);

        store.push(make_event("foo"));
        store.push(make_event("bar"));
        store.push(make_event("baz"));

        let messages: Vec<_> = store
            .iter()
            .map(|stored| stored.event.message.as_str())
            .collect();

        assert_eq!(messages, ["foo", "bar", "baz"]);
    }

    #[test]
    fn evicts_oldest_event_when_full() {
        let mut store = EventStore::new(3);

        store.push(make_event("foo"));
        store.push(make_event("bar"));
        store.push(make_event("baz"));
        store.push(make_event("qux"));

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

        store.push(make_event("foo"));
        store.push(make_event("bar"));
        store.push(make_event("baz"));

        assert!(store.get(0).is_none());
        assert_eq!(store.get(1).unwrap().event.message, "bar");
        assert_eq!(store.get(2).unwrap().event.message, "baz");
        assert!(store.get(3).is_none());
        assert!(store.get(u64::MAX).is_none());
    }

    #[test]
    fn reverse_iteration_returns_newest_first() {
        let mut store = EventStore::new(3);

        store.push(make_event("foo"));
        store.push(make_event("bar"));
        store.push(make_event("baz"));

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
