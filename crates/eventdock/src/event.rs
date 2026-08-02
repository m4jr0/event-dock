use crate::{FieldValue, Fields, LogLevel, Timestamp};
use serde::{Deserialize, Serialize};

pub type EventId = u64;

/// Event produced by an application and sent to EventDock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingEvent {
    pub timestamp: Timestamp,
    pub source: String,
    pub level: LogLevel,
    pub message: String,

    #[serde(default, skip_serializing_if = "Fields::is_empty")]
    pub fields: Fields,
}

impl IncomingEvent {
    pub fn new(
        source: impl Into<String>,
        level: LogLevel,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Timestamp::now(),
            source: source.into(),
            level,
            message: message.into(),
            fields: Fields::new(),
        }
    }

    #[must_use]
    pub fn with_field(
        mut self,
        name: impl Into<String>,
        value: impl Into<FieldValue>,
    ) -> Self {
        self.insert_field(name, value);
        self
    }

    pub fn insert_field(
        &mut self,
        name: impl Into<String>,
        value: impl Into<FieldValue>,
    ) -> Option<FieldValue> {
        self.fields.insert(name.into(), value.into())
    }
}

/// Borrowed view of an event.
///
/// Useful when an event only needs to be inspected or serialized
/// during the current call.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct EventRef<'a> {
    pub timestamp: Timestamp,
    pub source: &'a str,
    pub level: LogLevel,
    pub message: &'a str,

    #[serde(skip_serializing_if = "Fields::is_empty")]
    pub fields: &'a Fields,
}

impl<'a> EventRef<'a> {
    #[must_use]
    pub fn new(
        source: &'a str,
        level: LogLevel,
        message: &'a str,
        fields: &'a Fields,
    ) -> Self {
        Self {
            timestamp: Timestamp::now(),
            source,
            level,
            message,
            fields,
        }
    }
}

impl<'a> From<&'a IncomingEvent> for EventRef<'a> {
    fn from(event: &'a IncomingEvent) -> Self {
        Self {
            timestamp: event.timestamp,
            source: &event.source,
            level: event.level,
            message: &event.message,
            fields: &event.fields,
        }
    }
}

/// Event after EventDock has accepted and identified it.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub id: EventId,
    pub received_at: Timestamp,
    pub event: IncomingEvent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_event_has_expected_defaults() {
        let event = IncomingEvent::new("foo", LogLevel::Info, "bar");

        assert_eq!(event.source, "foo");
        assert_eq!(event.level, LogLevel::Info);
        assert_eq!(event.message, "bar");
        assert!(event.fields.is_empty());
    }

    #[test]
    fn with_field_builds_event() {
        let event = IncomingEvent::new("foo", LogLevel::Info, "bar")
            .with_field("foo", "baz")
            .with_field("bar", 42);

        assert_eq!(
            event.fields.get("foo"),
            Some(&FieldValue::String("baz".to_owned()))
        );
        assert_eq!(event.fields.get("bar"), Some(&FieldValue::I64(42)));
    }

    #[test]
    fn insert_field_replaces_existing_value() {
        let mut event = IncomingEvent::new("foo", LogLevel::Info, "bar");

        assert_eq!(event.insert_field("key", 42), None);
        assert_eq!(event.insert_field("key", 43), Some(FieldValue::I64(42)));
        assert_eq!(event.fields.get("key"), Some(&FieldValue::I64(43)));
    }

    #[test]
    fn empty_fields_are_omitted_from_json() {
        let event = IncomingEvent::new("foo", LogLevel::Info, "bar");
        let json = serde_json::to_value(&event).unwrap();

        assert!(json.get("fields").is_none());
    }

    #[test]
    fn nonempty_fields_are_serialized() {
        let event = IncomingEvent::new("foo", LogLevel::Info, "bar")
            .with_field("key", "baz");
        let json = serde_json::to_value(&event).unwrap();

        assert_eq!(json["fields"]["key"], "baz");
    }

    #[test]
    fn missing_fields_default_to_empty() {
        let json = json!({
            "timestamp": 123,
            "source": "foo",
            "level": "info",
            "message": "bar"
        });

        let event: IncomingEvent = serde_json::from_value(json).unwrap();

        assert_eq!(event.timestamp, Timestamp::from_unix_nanos(123));
        assert!(event.fields.is_empty());
    }

    #[test]
    fn event_json_round_trip() {
        let event = IncomingEvent::new("foo", LogLevel::Warning, "bar")
            .with_field("value", 25.5)
            .with_field("flag", true);

        let json = serde_json::to_string(&event).unwrap();
        let restored: IncomingEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.timestamp, event.timestamp);
        assert_eq!(restored.source, event.source);
        assert_eq!(restored.level, event.level);
        assert_eq!(restored.message, event.message);
        assert_eq!(restored.fields, event.fields);
    }

    #[test]
    fn event_ref_serializes_like_owned_event() {
        let event = IncomingEvent::new("foo", LogLevel::Warning, "bar")
            .with_field("value", 25.5)
            .with_field("flag", true);

        let owned_json = serde_json::to_string(&event).unwrap();
        let borrowed_json =
            serde_json::to_string(&EventRef::from(&event)).unwrap();

        assert_eq!(borrowed_json, owned_json);
    }

    #[test]
    fn event_ref_omits_empty_fields() {
        let fields = Fields::new();
        let event = EventRef::new("foo", LogLevel::Info, "bar", &fields);
        let json = serde_json::to_value(event).unwrap();

        assert!(json.get("fields").is_none());
        assert_eq!(json["source"], "foo");
        assert_eq!(json["message"], "bar");
    }

    #[test]
    fn event_ref_borrows_owned_event_data() {
        let event = IncomingEvent::new("foo", LogLevel::Info, "bar")
            .with_field("key", 42);
        let view = EventRef::from(&event);

        assert_eq!(view.timestamp, event.timestamp);
        assert_eq!(view.level, event.level);

        assert!(std::ptr::eq(view.source.as_ptr(), event.source.as_ptr(),));
        assert!(std::ptr::eq(view.message.as_ptr(), event.message.as_ptr(),));
        assert!(std::ptr::eq(view.fields, &event.fields));
    }
}
