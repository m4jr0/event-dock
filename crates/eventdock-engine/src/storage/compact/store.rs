use super::payload::PayloadBuffer;
use super::{
    CompactDecodingError, EventMetadata, FieldIter, PayloadView,
    decode_retained_header, encode_payload,
};
use crate::{
    EventId, EventStorage, EventSummary, EventViewAccess, FieldView,
    IncomingEventRef, LogLevel, StorageError, Timestamp,
};
use std::collections::VecDeque;

/// Compatibility name for the shared storage error.
pub type CompactStoreError = crate::StorageError;

#[derive(Debug)]
pub struct CompactEventStore {
    metadata: VecDeque<EventMetadata>,
    payloads: PayloadBuffer,
    next_id: EventId,
    max_events: Option<usize>,
    max_payload_bytes: Option<usize>,
    // Reused across insertions; not part of retained payload storage.
    encoding_scratch: Vec<u8>,
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

fn event_summary(metadata: &EventMetadata) -> EventSummary {
    EventSummary {
        id: metadata.id,
        occurred_at: metadata.occurred_at,
        received_at: metadata.received_at,
        level: metadata.level,
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
            metadata: VecDeque::new(),
            payloads: PayloadBuffer::new(0),
            next_id: 0,
            max_events: None,
            max_payload_bytes: None,
            encoding_scratch: Vec::new(),
        }
    }

    /// Bounded by event count and retained payload bytes.
    /// Rejects a single event larger than the byte limit without evicting anything.
    #[must_use]
    pub fn with_limits(
        max_events: usize,
        max_payload_bytes: usize,
    ) -> Self {
        assert!(max_events > 0, "event capacity must be positive");
        assert!(max_payload_bytes > 0, "payload capacity must be positive");

        Self {
            max_events: Some(max_events),
            max_payload_bytes: Some(max_payload_bytes),
            ..Self::new()
        }
    }

    #[must_use]
    pub fn capacity(&self) -> Option<usize> {
        self.max_events
    }

    #[must_use]
    pub fn payload_limit(&self) -> Option<usize> {
        self.max_payload_bytes
    }

    fn evict_oldest(&mut self) {
        let oldest = self
            .metadata
            .pop_front()
            .expect("oldest event");
        self.payloads.release(
            oldest.payload_offset,
            oldest.payload_len,
            self.metadata
                .front()
                .map(|metadata| metadata.payload_offset),
        );
    }

    fn ensure_id_available(&self) -> Result<(), StorageError> {
        if self.next_id == EventId::MAX {
            return Err(StorageError::IdExhausted);
        }
        Ok(())
    }

    fn encode_event(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<Vec<u8>, StorageError> {
        let mut encoded = std::mem::take(&mut self.encoding_scratch);
        encoded.clear();
        if let Err(error) = encode_payload(
            &mut encoded,
            event.source,
            event.message,
            event.fields,
        ) {
            encoded.clear();
            self.encoding_scratch = encoded;
            return Err(error.into());
        }
        Ok(encoded)
    }

    fn recycle_encoding_buffer(
        &mut self,
        mut encoded: Vec<u8>,
    ) {
        encoded.clear();
        self.encoding_scratch = encoded;
    }

    fn ensure_payload_fits(
        &self,
        length: usize,
    ) -> Result<(), StorageError> {
        if self
            .max_payload_bytes
            .is_some_and(|limit| length > limit)
        {
            return Err(StorageError::PayloadTooLarge);
        }
        Ok(())
    }

    fn evict_to_event_limit(&mut self) {
        if let Some(limit) = self.max_events {
            while self.metadata.len() >= limit {
                self.evict_oldest();
            }
        }
    }

    fn live_payload_bytes(&self) -> usize {
        self.payloads.live_bytes()
    }

    fn evict_to_payload_limit(
        &mut self,
        incoming: usize,
    ) {
        if let Some(limit) = self.max_payload_bytes {
            while !self.metadata.is_empty()
                && self.live_payload_bytes() > limit - incoming
            {
                self.evict_oldest();
            }
        }
    }

    fn make_room(
        &mut self,
        incoming: usize,
    ) {
        self.evict_to_event_limit();
        self.evict_to_payload_limit(incoming);
    }

    fn grow_payloads(
        &mut self,
        incoming: usize,
    ) {
        let required = self
            .live_payload_bytes()
            .checked_add(incoming)
            .expect("payload capacity overflow");
        // Allow physical slack for contiguous records without reducing retention.
        let ceiling = self
            .max_payload_bytes
            .map_or(usize::MAX, |limit| limit.saturating_mul(2));
        let capacity = self
            .payloads
            .capacity()
            .saturating_mul(2)
            .max(4096)
            .max(required)
            .min(ceiling);
        assert!(
            capacity > self.payloads.capacity(),
            "no contiguous payload space"
        );
        let mut replacement = PayloadBuffer::new(capacity);
        for metadata in &mut self.metadata {
            let bytes = self
                .payloads
                .get(metadata.payload_offset, metadata.payload_len)
                .expect("retained payload");
            metadata.payload_offset = replacement
                .push(bytes)
                .expect("grown payload space");
        }
        self.payloads = replacement;
    }

    fn allocate_payload(
        &mut self,
        encoded: &[u8],
    ) -> usize {
        if let Some(offset) = self.payloads.push(encoded) {
            return offset;
        }
        self.grow_payloads(encoded.len());
        self.payloads
            .push(encoded)
            .expect("grown payload space")
    }

    fn append_event(
        &mut self,
        event: IncomingEventRef<'_>,
        encoded: &[u8],
    ) -> EventId {
        let id = self.next_id;
        self.next_id += 1; // Checked by ensure_id_available before mutation.

        let offset = self.allocate_payload(encoded);
        self.metadata.push_back(EventMetadata {
            id,
            occurred_at: event.timestamp,
            received_at: Timestamp::now(),
            level: event.level,
            payload_offset: offset,
            payload_len: encoded.len(),
        });
        id
    }

    /// Insert atomically with respect to validation errors: rejected events never evict.
    pub fn push(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<EventId, StorageError> {
        self.ensure_id_available()?;
        let encoded = self.encode_event(event)?;
        if let Err(error) = self.ensure_payload_fits(encoded.len()) {
            self.recycle_encoding_buffer(encoded);
            return Err(error);
        }

        self.make_room(encoded.len());
        let id = self.append_event(event, &encoded);
        self.recycle_encoding_buffer(encoded);

        debug_assert!(
            self.metadata
                .back()
                .is_some_and(|last| last.id == id)
        );
        debug_assert!(
            self.max_events
                .is_none_or(|limit| self.metadata.len() <= limit)
        );
        debug_assert!(
            self.max_payload_bytes
                .is_none_or(|limit| self.live_payload_bytes() <= limit)
        );
        Ok(id)
    }

    #[cfg(test)]
    fn retention_invariants_hold(&self) -> bool {
        if self
            .max_events
            .is_some_and(|limit| self.metadata.len() > limit)
        {
            return false;
        }

        if self
            .max_payload_bytes
            .is_some_and(|limit| self.live_payload_bytes() > limit)
        {
            return false;
        }

        let mut ranges = Vec::new();
        for metadata in &self.metadata {
            let Some(end) = metadata
                .payload_offset
                .checked_add(metadata.payload_len)
            else {
                return false;
            };
            if end > self.payloads.capacity() {
                return false;
            }
            ranges.push((metadata.payload_offset, end));
        }
        ranges.sort_unstable();
        if ranges
            .windows(2)
            .any(|pair| pair[0].1 > pair[1].0)
        {
            return false;
        }
        if self
            .metadata
            .iter()
            .map(|metadata| metadata.payload_len)
            .sum::<usize>()
            != self.live_payload_bytes()
        {
            return false;
        }

        true
    }

    /// Look up only the compact metadata without decoding a payload.
    pub fn metadata_for(
        &self,
        id: EventId,
    ) -> Option<EventSummary> {
        let first_id = self.metadata.front()?.id;
        let index = usize::try_from(id.checked_sub(first_id)?).ok()?;
        let metadata = self.metadata.get(index)?;
        (metadata.id == id).then(|| event_summary(metadata))
    }

    pub fn get(
        &self,
        id: EventId,
    ) -> Option<CompactEventView<'_>> {
        let first_id = self.metadata.front()?.id;
        let index = usize::try_from(id.checked_sub(first_id)?).ok()?;
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
        let bytes = self
            .payloads
            .get(metadata.payload_offset, metadata.payload_len)
            .ok_or(CompactDecodingError::UnexpectedEnd)?;

        #[cfg(feature = "storage-validation")]
        super::decode_payload(bytes)?;
        let payload = decode_retained_header(bytes)?;
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

    /// Iterate the small metadata records without decoding event payloads.
    pub fn iter_metadata(
        &self
    ) -> impl DoubleEndedIterator<Item = EventSummary> + '_ {
        self.metadata.iter().map(event_summary)
    }

    pub fn iter_levels(
        &self
    ) -> impl DoubleEndedIterator<Item = LogLevel> + '_ {
        self.metadata
            .iter()
            .map(|metadata| metadata.level)
    }

    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    /// Return retained encoded bytes, excluding unused ring space.
    pub fn payload_bytes(&self) -> usize {
        self.live_payload_bytes()
    }

    pub fn allocated_bytes(&self) -> usize {
        self.payloads.allocated_bytes()
            + self.metadata.capacity() * std::mem::size_of::<EventMetadata>()
            + self.encoding_scratch.capacity()
    }
}

type CompactFieldsIter<'a> = std::iter::Map<
    crate::FieldIter<'a>,
    fn(Result<FieldView<'a>, crate::CompactDecodingError>) -> FieldView<'a>,
>;

fn compact_field<'a>(
    field: Result<FieldView<'a>, crate::CompactDecodingError>
) -> FieldView<'a> {
    field.expect("invalid retained field")
}

impl EventViewAccess for crate::CompactEventView<'_> {
    type FieldsIter<'a>
        = CompactFieldsIter<'a>
    where
        Self: 'a;

    fn summary(&self) -> EventSummary {
        EventSummary {
            id: self.id(),
            occurred_at: self.occurred_at(),
            received_at: self.received_at(),
            level: self.level(),
        }
    }

    fn source(&self) -> &str {
        crate::CompactEventView::source(self)
    }

    fn message(&self) -> &str {
        crate::CompactEventView::message(self)
    }

    fn fields(&self) -> Self::FieldsIter<'_> {
        crate::CompactEventView::fields(self).map(compact_field)
    }
}

impl EventStorage for crate::CompactEventStore {
    type View<'a> = crate::CompactEventView<'a>;

    fn push(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<EventId, StorageError> {
        crate::CompactEventStore::push(self, event)
    }

    fn get(
        &self,
        id: EventId,
    ) -> Option<Self::View<'_>> {
        crate::CompactEventStore::get(self, id)
    }

    fn iter_metadata(
        &self
    ) -> impl DoubleEndedIterator<Item = EventSummary> + '_ {
        crate::CompactEventStore::iter_metadata(self)
    }

    fn len(&self) -> usize {
        crate::CompactEventStore::len(self)
    }
    fn capacity(&self) -> Option<usize> {
        crate::CompactEventStore::capacity(self)
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
    fn iter_levels_preserves_order() {
        let mut store = CompactEventStore::new();
        let first = event("first");
        let mut second = event("second");
        second.level = LogLevel::Error;

        store
            .push(IncomingEventRef::from(&first))
            .unwrap();

        store
            .push(IncomingEventRef::from(&second))
            .unwrap();

        assert_eq!(
            store.iter_levels().collect::<Vec<_>>(),
            [LogLevel::Warning, LogLevel::Error]
        );

        assert_eq!(
            store
                .iter_levels()
                .rev()
                .collect::<Vec<_>>(),
            [LogLevel::Error, LogLevel::Warning]
        );
    }

    #[test]
    fn bounded_store_reclaims_and_keeps_stable_ids() {
        let mut store = CompactEventStore::with_limits(3, 512);

        for i in 0..1000 {
            let item = event(&format!("message {i}"));

            store
                .push(IncomingEventRef::from(&item))
                .unwrap();

            assert!(store.len() <= 3);
            assert!(store.payload_bytes() <= 512);
            assert!(store.retention_invariants_hold());
            assert_eq!(store.get(i).unwrap().message(), format!("message {i}"));

            if i >= 3 {
                assert!(store.get(i - 3).is_none());
            }
        }

        assert_eq!(
            store
                .iter_metadata()
                .map(|m| m.id)
                .collect::<Vec<_>>(),
            [997, 998, 999]
        );
    }

    #[test]
    fn oversized_payload_rejection_preserves_events() {
        let mut store = CompactEventStore::with_limits(2, 100);
        let first = event("ok");

        store
            .push(IncomingEventRef::from(&first))
            .unwrap();

        let large = event(&"x".repeat(1000));

        assert_eq!(
            store.push(IncomingEventRef::from(&large)),
            Err(CompactStoreError::PayloadTooLarge)
        );

        assert_eq!(store.get(0).unwrap().message(), "ok");
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn variable_payload_retention_matches_logical_budget() {
        use std::collections::VecDeque;
        let mut store = CompactEventStore::with_limits(31, 1024);
        let mut expected = VecDeque::new();
        let mut live = 0;
        let mut state = 12345_u32;
        for id in 0..10000 {
            state = state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            let message = format!("{id}:{}", "x".repeat(state as usize % 800));
            let input = event(&message);
            let mut encoded = Vec::new();
            encode_payload(
                &mut encoded,
                &input.source,
                &input.message,
                &input.fields,
            )
            .unwrap();
            let len = encoded.len();
            while expected.len() >= 31 || live + len > 1024 {
                let (_, _, old_len) = expected.pop_front().unwrap();
                live -= old_len;
            }
            assert_eq!(
                store
                    .push(IncomingEventRef::from(&input))
                    .unwrap(),
                id
            );
            expected.push_back((id, message, len));
            live += len;
            assert_eq!(store.len(), expected.len());
            assert_eq!(store.payload_bytes(), live);
            assert!(store.retention_invariants_hold());
            for (view, (expected_id, message, _)) in store.iter().zip(&expected)
            {
                assert_eq!(view.id(), *expected_id);
                assert_eq!(view.message(), message.as_str());
            }
        }
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
            .push(IncomingEventRef::from(&input))
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
                    .push(IncomingEventRef::from(&input))
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
            .push(IncomingEventRef::from(&input))
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
            .push(IncomingEventRef::from(&input))
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
            store.push(IncomingEventRef::from(&input)),
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
            .push(IncomingEventRef::from(&input))
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
            .push(IncomingEventRef::from(&input))
            .unwrap();

        let end =
            store.metadata[0].payload_offset + store.metadata[0].payload_len;
        store
            .payloads
            .truncate_for_test(end - 1);

        assert_eq!(
            store.view(&store.metadata[0]).err(),
            Some(CompactDecodingError::UnexpectedEnd)
        );
    }

    #[test]
    fn view_validates_truncated_fields_according_to_mode() {
        let mut store = CompactEventStore::new();
        let input = event("data");

        store
            .push(IncomingEventRef::from(&input))
            .unwrap();

        let end =
            store.metadata[0].payload_offset + store.metadata[0].payload_len;

        store
            .payloads
            .truncate_for_test(end - 1);
        store.metadata[0].payload_len -= 1;

        let result = store.view(&store.metadata[0]);

        #[cfg(feature = "storage-validation")]
        assert_eq!(result.err(), Some(CompactDecodingError::UnexpectedEnd));

        #[cfg(not(feature = "storage-validation"))]
        {
            let view = result.unwrap();

            assert_eq!(view.message(), "data");
            assert!(view.fields().any(|field| {
                matches!(field, Err(CompactDecodingError::UnexpectedEnd))
            }));
        }
    }

    #[test]
    fn borrowed_event_ref_is_supported() {
        let mut store = CompactEventStore::new();
        let fields = Fields::new();

        let input = IncomingEventRef {
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

    #[test]
    fn encoding_scratch_is_reused_and_rejection_preserves_history() {
        let mut store = CompactEventStore::with_limits(2, 128);
        let first = event("first");

        store
            .push(IncomingEventRef::from(&first))
            .unwrap();

        let capacity = store.encoding_scratch.capacity();
        assert!(capacity > 0);

        let before = store
            .iter_metadata()
            .collect::<Vec<_>>();

        let too_large = event(&"x".repeat(1000));

        assert_eq!(
            store.push(IncomingEventRef::from(&too_large)),
            Err(StorageError::PayloadTooLarge)
        );

        assert_eq!(
            store
                .iter_metadata()
                .collect::<Vec<_>>(),
            before
        );

        assert!(store.encoding_scratch.capacity() >= capacity);
        assert!(store.retention_invariants_hold());
    }
}
