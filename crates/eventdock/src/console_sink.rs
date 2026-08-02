use crate::{EventDockError, EventRef, FieldValue, LogSink};
use std::io::{self, Write};

#[derive(Debug, Default)]
pub struct ConsoleSink;

impl LogSink for ConsoleSink {
    fn submit(
        &mut self,
        event: EventRef<'_>,
    ) -> Result<(), EventDockError> {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        write_event(&mut output, event)
    }
}

fn write_event(
    output: &mut impl Write,
    event: EventRef<'_>,
) -> Result<(), EventDockError> {
    write!(
        output,
        "[{}] [{}] {}",
        event.source, event.level, event.message
    )?;

    if !event.fields.is_empty() {
        write!(output, " (")?;

        for (index, (name, value)) in event.fields.iter().enumerate() {
            if index > 0 {
                write!(output, ", ")?;
            }

            write!(output, "{name}=")?;

            match value {
                FieldValue::String(text) => write!(output, "{text}")?,
                _ => serde_json::to_writer(&mut *output, value)?,
            }
        }

        write!(output, ")")?;
    }

    writeln!(output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Fields, LogLevel};

    #[test]
    fn writes_compact_lowercase_labels() {
        let fields = Fields::new();
        let event = EventRef::new("foo", LogLevel::Info, "foo", &fields);

        let mut output = Vec::new();
        write_event(&mut output, event).unwrap();

        assert_eq!(std::str::from_utf8(&output).unwrap(), "[foo] [info] foo\n");
    }

    #[test]
    fn writes_structured_fields() {
        let mut fields = Fields::new();
        fields.insert("backend", "qux");
        fields.insert("frame", 42);
        fields.insert("vsync", true);

        let event = EventRef::new("foo", LogLevel::Warning, "bar", &fields);

        let mut output = Vec::new();
        write_event(&mut output, event).unwrap();

        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            "[foo] [warning] bar (backend=qux, frame=42, vsync=true)\n"
        );
    }

    #[test]
    fn does_not_change_message_contents() {
        let fields = Fields::new();
        let event = EventRef::new("foo", LogLevel::Debug, "baz", &fields);

        let mut output = Vec::new();
        write_event(&mut output, event).unwrap();

        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            "[foo] [debug] baz\n"
        );
    }
}
