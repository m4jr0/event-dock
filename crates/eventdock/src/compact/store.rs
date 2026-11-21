use super::{
    CompactDecodingError, CompactEncodingError, EventMetadata, FieldIter,
    PayloadView, decode_payload, encode_payload,
};
use crate::{EventId, EventRef, LogLevel, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactStoreError {
    Encoding(CompactEncodingError),
    IdExhausted,
}

impl From<CompactEncodingError> for CompactStoreError {
    fn from(error: CompactEncodingError) -> Self {
        Self::Encoding(error)
    }
}

#[derive(Debug)]
pub struct CompactEventStore {
    metadata: Vec<EventMetadata>,
    payloads: Vec<u8>,
    next_id: EventId,
}

#[derive(Debug)]
pub struct CompactEventView<'a> {
    metadata: &'a EventMetadata,
    payload: PayloadView<'a>,
}

impl<'a> CompactEventView<'a> {
    pub fn id(&self) -> EventId {
        self.metadata.id
    }

    pub fn occurred_at(&self) -> Timestamp {
        self.metadata.occurred_at
    }

    pub fn received_at(&self) -> Timestamp {
        self.metadata.received_at
    }

    pub fn level(&self) -> LogLevel {
        self.metadata.level
    }

    pub fn source(&self) -> &'a str {
        self.payload.source
    }

    pub fn message(&self) -> &'a str {
        self.payload.message
    }

    pub fn fields(&self) -> FieldIter<'a> {
        self.payload.fields()
    }
}

impl Default for CompactEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CompactEventStore {
    pub fn new() -> Self {
        Self {
            metadata: Vec::new(),
            payloads: Vec::new(),
            next_id: 0,
        }
    }

    fn allocate_id(&mut self) -> Result<EventId, CompactStoreError> {
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or(CompactStoreError::IdExhausted)?;
        Ok(id)
    }

    pub fn push(
        &mut self,
        event: EventRef<'_>,
    ) -> Result<EventId, CompactStoreError> {
        // Reject ID exhaustion before changing the payload buffer.
        if self.next_id == EventId::MAX {
            return Err(CompactStoreError::IdExhausted);
        }

        let offset = self.payloads.len();
        encode_payload(
            &mut self.payloads,
            event.source,
            event.message,
            event.fields,
        )?;
        let len = self.payloads.len() - offset;

        // The check above guarantees allocation succeeds.
        let id = self.allocate_id()?;
        self.metadata.push(EventMetadata {
            id,
            occurred_at: event.timestamp,
            received_at: Timestamp::now(),
            level: event.level,
            payload_offset: offset,
            payload_len: len,
        });
        Ok(id)
    }

    pub fn get(
        &self,
        id: EventId,
    ) -> Option<CompactEventView<'_>> {
        // Valid only for the append-only prototype with sequential IDs.
        let index = usize::try_from(id).ok()?;
        let metadata = self.metadata.get(index)?;
        if metadata.id != id {
            return None;
        }
        Some(
            self.view(metadata)
                .expect("store contains an invalid payload"),
        )
    }

    fn view<'a>(
        &'a self,
        metadata: &'a EventMetadata,
    ) -> Result<CompactEventView<'a>, CompactDecodingError> {
        let end = metadata
            .payload_offset
            .checked_add(metadata.payload_len)
            .ok_or(CompactDecodingError::UnexpectedEnd)?;
        let bytes = self
            .payloads
            .get(metadata.payload_offset..end)
            .ok_or(CompactDecodingError::UnexpectedEnd)?;
        let payload = decode_payload(bytes)?;
        Ok(CompactEventView { metadata, payload })
    }

    pub fn iter(
        &self
    ) -> impl DoubleEndedIterator<Item = CompactEventView<'_>> + '_ {
        self.metadata.iter().map(|metadata| {
            self.view(metadata)
                .expect("store contains an invalid payload")
        })
    }

    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    pub fn payload_bytes(&self) -> usize {
        self.payloads.len()
    }

    pub fn allocated_bytes(&self) -> usize {
        self.payloads.capacity()
            + self.metadata.capacity() * std::mem::size_of::<EventMetadata>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldValue, Fields, IncomingEvent};

    fn event(message: &str) -> IncomingEvent {
        let mut event =
            IncomingEvent::new("source", LogLevel::Warning, message);
        event.timestamp = Timestamp::from_unix_nanos(123);
        event.fields.insert("count", 42_i64);
        event.fields.insert("enabled", true);
        event
    }

    #[test]
    fn new_store_is_empty() {
        let store = CompactEventStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert_eq!(store.payload_bytes(), 0);
        assert!(store.get(0).is_none());
    }

    #[test]
    fn push_and_get_borrowed_event() {
        let mut store = CompactEventStore::new();
        let input = event("message");
        let id = store
            .push(EventRef::from(&input))
            .unwrap();
        let view = store.get(id).unwrap();
        assert_eq!(id, 0);
        assert_eq!(view.id(), id);
        assert_eq!(view.occurred_at(), input.timestamp);
        assert_eq!(view.level(), input.level);
        assert_eq!(view.source(), "source");
        assert_eq!(view.message(), "message");
        assert_eq!(view.fields().count(), 2);
        assert!(view.received_at().as_unix_nanos() > 0);
    }

    #[test]
    fn preserves_ids_and_order() {
        let mut store = CompactEventStore::new();
        for i in 0..4 {
            let input = event(&format!("message-{i}"));
            assert_eq!(
                store
                    .push(EventRef::from(&input))
                    .unwrap(),
                i
            );
        }
        assert_eq!(store.len(), 4);
        assert_eq!(store.get(3).unwrap().message(), "message-3");
        assert!(store.get(4).is_none());
        assert!(store.get(u64::MAX).is_none());
        let ids: Vec<_> = store
            .iter()
            .rev()
            .map(|view| view.id())
            .collect();
        assert_eq!(ids, [3, 2, 1, 0]);
    }

    #[test]
    fn borrowed_fields_match_original_values() {
        let mut store = CompactEventStore::new();
        let mut input = event("data");
        input
            .fields
            .insert("negative_zero", -0.0_f64);
        input.fields.insert("empty", "");
        input
            .fields
            .insert("null", FieldValue::Null);
        store
            .push(EventRef::from(&input))
            .unwrap();
        let view = store.get(0).unwrap();
        let fields: Vec<_> = view
            .fields()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(fields.len(), input.fields.len());
        assert!(
            fields
                .iter()
                .any(|field| field.name == "empty")
        );
        assert!(
            fields
                .iter()
                .any(|field| field.name == "null")
        );
        assert!(
            fields
                .iter()
                .any(|field| field.name == "negative_zero")
        );
    }

    #[test]
    fn empty_fields_and_strings() {
        let mut store = CompactEventStore::new();
        let mut input = event("");
        input.source.clear();
        input.fields = Fields::new();
        store
            .push(EventRef::from(&input))
            .unwrap();
        let view = store.get(0).unwrap();
        assert_eq!(view.source(), "");
        assert_eq!(view.message(), "");
        assert_eq!(view.fields().count(), 0);
    }

    #[test]
    fn id_exhaustion_does_not_modify_store() {
        let mut store = CompactEventStore::new();
        store.next_id = u64::MAX;
        let input = event("data");
        assert_eq!(
            store.push(EventRef::from(&input)),
            Err(CompactStoreError::IdExhausted)
        );
        assert!(store.is_empty());
        assert_eq!(store.payload_bytes(), 0);
    }

    #[test]
    fn view_rejects_out_of_bounds_metadata() {
        let mut store = CompactEventStore::new();
        let input = event("data");
        store
            .push(EventRef::from(&input))
            .unwrap();
        store.metadata[0].payload_len = usize::MAX;
        assert_eq!(
            store.view(&store.metadata[0]).err(),
            Some(CompactDecodingError::UnexpectedEnd)
        );
    }

    #[test]
    fn view_rejects_truncated_payload() {
        let mut store = CompactEventStore::new();
        let input = event("data");
        store
            .push(EventRef::from(&input))
            .unwrap();
        store.payloads.pop();
        assert_eq!(
            store.view(&store.metadata[0]).err(),
            Some(CompactDecodingError::UnexpectedEnd)
        );
    }

    #[test]
    fn borrowed_event_ref_is_supported() {
        let mut store = CompactEventStore::new();
        let fields = Fields::new();
        let input = EventRef {
            timestamp: Timestamp::from_unix_nanos(456),
            source: "borrowed",
            level: LogLevel::Info,
            message: "message",
            fields: &fields,
        };
        store.push(input).unwrap();
        assert_eq!(store.get(0).unwrap().occurred_at(), input.timestamp);
        assert_eq!(store.get(0).unwrap().source(), "borrowed");
    }
}
