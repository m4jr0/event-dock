use crate::{
    EventDockError, Fields, IncomingEvent, IncomingEventRef, LogLevel,
};

static EMPTY_FIELDS: Fields = Fields::new();

/// Destination to which a Logger submits events.
///
/// Submission may complete synchronously or enqueue work, depending on the sink.
pub trait LogSink {
    fn submit(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<(), EventDockError>;

    /// Submit an owned event, allowing queue-backed sinks to avoid cloning.
    fn submit_owned(
        &mut self,
        event: IncomingEvent,
    ) -> Result<(), EventDockError> {
        self.submit(IncomingEventRef::from(&event))
    }

    /// Flush buffered output. Non-buffering sinks can use this default.
    fn flush(&mut self) -> Result<(), EventDockError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct Logger<S> {
    source: String,
    sink: S,
}

impl<S> Logger<S>
where
    S: LogSink,
{
    pub fn new(
        source: impl Into<String>,
        sink: S,
    ) -> Self {
        Self {
            source: source.into(),
            sink,
        }
    }

    pub fn log(
        &mut self,
        level: LogLevel,
        message: impl AsRef<str>,
    ) -> Result<(), EventDockError> {
        self.log_with_fields(level, message, &EMPTY_FIELDS)
    }

    pub fn log_with_fields(
        &mut self,
        level: LogLevel,
        message: impl AsRef<str>,
        fields: &Fields,
    ) -> Result<(), EventDockError> {
        let event = IncomingEventRef::new(
            &self.source,
            level,
            message.as_ref(),
            fields,
        );

        self.sink.submit(event)
    }

    pub fn log_event(
        &mut self,
        event: IncomingEvent,
    ) -> Result<(), EventDockError> {
        self.sink.submit_owned(event)
    }

    pub fn trace(
        &mut self,
        message: impl AsRef<str>,
    ) -> Result<(), EventDockError> {
        self.log(LogLevel::Trace, message)
    }

    pub fn debug(
        &mut self,
        message: impl AsRef<str>,
    ) -> Result<(), EventDockError> {
        self.log(LogLevel::Debug, message)
    }

    pub fn info(
        &mut self,
        message: impl AsRef<str>,
    ) -> Result<(), EventDockError> {
        self.log(LogLevel::Info, message)
    }

    pub fn warning(
        &mut self,
        message: impl AsRef<str>,
    ) -> Result<(), EventDockError> {
        self.log(LogLevel::Warning, message)
    }

    pub fn error(
        &mut self,
        message: impl AsRef<str>,
    ) -> Result<(), EventDockError> {
        self.log(LogLevel::Error, message)
    }

    pub fn critical(
        &mut self,
        message: impl AsRef<str>,
    ) -> Result<(), EventDockError> {
        self.log(LogLevel::Critical, message)
    }

    /// Flush pending output in the underlying sink.
    pub fn flush(&mut self) -> Result<(), EventDockError> {
        self.sink.flush()
    }

    pub fn sink(&self) -> &S {
        &self.sink
    }

    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    pub fn into_sink(self) -> S {
        self.sink
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FieldValue;

    #[derive(Default)]
    struct RecordingSink {
        events: Vec<IncomingEvent>,
    }

    impl LogSink for RecordingSink {
        fn submit(
            &mut self,
            event: IncomingEventRef<'_>,
        ) -> Result<(), EventDockError> {
            self.events.push(IncomingEvent {
                timestamp: event.timestamp,
                source: event.source.to_owned(),
                level: event.level,
                message: event.message.to_owned(),
                fields: event.fields.clone(),
            });

            Ok(())
        }
    }

    #[test]
    fn log_uses_logger_source() {
        let mut logger = Logger::new("foo", RecordingSink::default());

        logger.info("bar").unwrap();

        let sink = logger.into_sink();

        assert_eq!(sink.events.len(), 1);
        assert_eq!(sink.events[0].source, "foo");
        assert_eq!(sink.events[0].level, LogLevel::Info);
        assert_eq!(sink.events[0].message, "bar");
    }

    #[test]
    fn log_event_preserves_event_source_and_fields() {
        let mut logger = Logger::new("foo", RecordingSink::default());

        let event = IncomingEvent::new("baz", LogLevel::Warning, "bar")
            .with_field("key", 42);

        let timestamp = event.timestamp;

        logger.log_event(event).unwrap();

        let sink = logger.into_sink();
        let recorded = &sink.events[0];

        assert_eq!(recorded.timestamp, timestamp);
        assert_eq!(recorded.source, "baz");
        assert_eq!(recorded.level, LogLevel::Warning);
        assert_eq!(recorded.message, "bar");
        assert_eq!(recorded.fields.get("key"), Some(&FieldValue::I64(42)));
    }

    #[test]
    fn log_accepts_borrowed_string() {
        let mut logger = Logger::new("foo", RecordingSink::default());
        let message = String::from("bar");

        logger.info(&message).unwrap();

        assert_eq!(message, "bar");
        assert_eq!(logger.sink().events[0].message, "bar",);
    }

    #[test]
    fn log_with_fields_preserves_fields() {
        let mut logger = Logger::new("foo", RecordingSink::default());

        let mut fields = Fields::new();
        fields.insert("key", 42);
        fields.insert("name", "baz");

        logger
            .log_with_fields(LogLevel::Warning, "bar", &fields)
            .unwrap();

        let sink = logger.into_sink();
        assert_eq!(sink.events.len(), 1);

        let event = &sink.events[0];
        assert_eq!(event.source, "foo");
        assert_eq!(event.level, LogLevel::Warning);
        assert_eq!(event.message, "bar");
        assert_eq!(event.fields, fields);
    }

    #[test]
    fn log_with_fields_borrows_fields() {
        let mut logger = Logger::new("foo", RecordingSink::default());

        let mut fields = Fields::new();
        fields.insert("key", 42);

        logger
            .log_with_fields(LogLevel::Info, "bar", &fields)
            .unwrap();

        fields.insert("key", 43);
        assert_eq!(fields.get("key"), Some(&FieldValue::I64(43)));

        let sink = logger.into_sink();
        assert_eq!(
            sink.events[0].fields.get("key"),
            Some(&FieldValue::I64(42))
        );
    }
}
