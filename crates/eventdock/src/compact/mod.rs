mod binary;
mod store;

pub use store::{CompactEventStore, CompactEventView, CompactStoreError};

use crate::{EventId, FieldValue, LogLevel, Timestamp};
use binary::{
    read_bool, read_f64, read_i64, read_string, read_u8, read_u32, read_u64,
    write_bool, write_f64, write_i64, write_string_unchecked, write_u8,
    write_u32, write_u64,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactEncodingError {
    StringTooLong,
    TooManyFields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactDecodingError {
    UnexpectedEnd,
    InvalidUtf8,
    InvalidFieldTag,
    InvalidBoolean,
    TrailingData,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EventMetadata {
    pub id: EventId,
    pub occurred_at: Timestamp,
    pub received_at: Timestamp,
    pub level: LogLevel,
    pub payload_offset: usize,
    pub payload_len: usize,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldTag {
    Null = 0,
    Bool = 1,
    I64 = 2,
    U64 = 3,
    F64 = 4,
    String = 5,
}

impl TryFrom<u8> for FieldTag {
    type Error = CompactDecodingError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::Null as u8 => Ok(Self::Null),
            x if x == Self::Bool as u8 => Ok(Self::Bool),
            x if x == Self::I64 as u8 => Ok(Self::I64),
            x if x == Self::U64 as u8 => Ok(Self::U64),
            x if x == Self::F64 as u8 => Ok(Self::F64),
            x if x == Self::String as u8 => Ok(Self::String),
            _ => Err(CompactDecodingError::InvalidFieldTag),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PayloadView<'a> {
    source: &'a str,
    message: &'a str,
    fields: &'a [u8],
    field_count: u32,
}

impl<'a> PayloadView<'a> {
    fn fields(&self) -> FieldIter<'a> {
        FieldIter {
            remaining: self.fields,
            count: self.field_count,
        }
    }
}

pub struct FieldIter<'a> {
    remaining: &'a [u8],
    count: u32,
}

impl<'a> Iterator for FieldIter<'a> {
    type Item = Result<FieldView<'a>, CompactDecodingError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return None;
        }

        match read_field(&mut self.remaining) {
            Ok(field) => {
                self.count -= 1;
                Some(Ok(field))
            }
            Err(error) => {
                self.count = 0;
                Some(Err(error))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldValueView<'a> {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldView<'a> {
    pub name: &'a str,
    pub value: FieldValueView<'a>,
}

fn encode_payload(
    buffer: &mut Vec<u8>,
    source: &str,
    message: &str,
    fields: &crate::Fields,
) -> Result<(), CompactEncodingError> {
    // Validate the entire payload before writing any bytes.
    validate_string(source)?;
    validate_string(message)?;
    let count = u32::try_from(fields.len())
        .map_err(|_| CompactEncodingError::TooManyFields)?;

    for (name, value) in fields.iter() {
        validate_field(name, value)?;
    }

    write_string_unchecked(buffer, source);
    write_string_unchecked(buffer, message);
    write_u32(buffer, count);

    for (name, value) in fields.iter() {
        write_field_unchecked(buffer, name, value);
    }

    Ok(())
}

fn validate_string(value: &str) -> Result<(), CompactEncodingError> {
    u32::try_from(value.len())
        .map(|_| ())
        .map_err(|_| CompactEncodingError::StringTooLong)
}

fn validate_field(
    name: &str,
    value: &FieldValue,
) -> Result<(), CompactEncodingError> {
    validate_string(name)?;
    if let FieldValue::String(value) = value {
        validate_string(value)?;
    }
    Ok(())
}

#[cfg(test)]
fn write_field(
    buffer: &mut Vec<u8>,
    name: &str,
    value: &FieldValue,
) -> Result<(), CompactEncodingError> {
    validate_field(name, value)?;
    write_field_unchecked(buffer, name, value);
    Ok(())
}

fn write_field_unchecked(
    buffer: &mut Vec<u8>,
    name: &str,
    value: &FieldValue,
) {
    write_string_unchecked(buffer, name);
    match value {
        FieldValue::Null => write_u8(buffer, FieldTag::Null as u8),
        FieldValue::Bool(value) => {
            write_u8(buffer, FieldTag::Bool as u8);
            write_bool(buffer, *value);
        }
        FieldValue::I64(value) => {
            write_u8(buffer, FieldTag::I64 as u8);
            write_i64(buffer, *value);
        }
        FieldValue::U64(value) => {
            write_u8(buffer, FieldTag::U64 as u8);
            write_u64(buffer, *value);
        }
        FieldValue::F64(value) => {
            write_u8(buffer, FieldTag::F64 as u8);
            write_f64(buffer, *value);
        }
        FieldValue::String(value) => {
            write_u8(buffer, FieldTag::String as u8);
            write_string_unchecked(buffer, value);
        }
    }
}
fn read_field<'a>(
    input: &mut &'a [u8]
) -> Result<FieldView<'a>, CompactDecodingError> {
    let mut remaining = *input;
    let name = read_string(&mut remaining)?;
    let tag = FieldTag::try_from(read_u8(&mut remaining)?)?;
    let value = match tag {
        FieldTag::Null => FieldValueView::Null,
        FieldTag::Bool => FieldValueView::Bool(read_bool(&mut remaining)?),
        FieldTag::I64 => FieldValueView::I64(read_i64(&mut remaining)?),
        FieldTag::U64 => FieldValueView::U64(read_u64(&mut remaining)?),
        FieldTag::F64 => FieldValueView::F64(read_f64(&mut remaining)?),
        FieldTag::String => {
            FieldValueView::String(read_string(&mut remaining)?)
        }
    };
    *input = remaining;
    Ok(FieldView { name, value })
}

// Used only for payloads created by encode_payload and retained privately by the store.
// The header remains bounds-checked and UTF-8-checked. Fields are decoded lazily.
fn decode_trusted_payload(
    mut input: &[u8]
) -> Result<PayloadView<'_>, CompactDecodingError> {
    let source = read_string(&mut input)?;
    let message = read_string(&mut input)?;
    let field_count = read_u32(&mut input)?;
    Ok(PayloadView {
        source,
        message,
        fields: input,
        field_count,
    })
}

#[cfg(test)]
fn decode_payload(
    input: &[u8]
) -> Result<PayloadView<'_>, CompactDecodingError> {
    let view = decode_trusted_payload(input)?;
    let mut remaining = view.fields;
    for _ in 0..view.field_count {
        read_field(&mut remaining)?;
    }
    if !remaining.is_empty() {
        return Err(CompactDecodingError::TrailingData);
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::binary::write_string;
    use super::*;
    use crate::Fields;

    fn sample_fields() -> Fields {
        let mut fields = Fields::new();
        fields.insert("bool", true);
        fields.insert("float", -0.0_f64);
        fields.insert("integer", -42_i64);
        fields.insert("null", FieldValue::Null);
        fields.insert("string", "café 猫");
        fields.insert("unsigned", u64::MAX);
        fields
    }

    #[test]
    fn complete_payload_round_trip() {
        let fields = sample_fields();
        let mut buffer = Vec::new();
        encode_payload(&mut buffer, "source", "message", &fields).unwrap();

        let view = decode_payload(&buffer).unwrap();
        assert_eq!(view.source, "source");
        assert_eq!(view.message, "message");
        assert_eq!(view.field_count, fields.len() as u32);

        let decoded: Vec<_> = view
            .fields()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(decoded.len(), fields.len());
        assert_eq!(decoded[0].name, "bool");
        assert_eq!(decoded[0].value, FieldValueView::Bool(true));
        assert_eq!(decoded[1].value, FieldValueView::F64(-0.0));
        if let FieldValueView::F64(value) = decoded[1].value {
            assert_eq!(value.to_bits(), (-0.0_f64).to_bits());
        }
        assert_eq!(decoded[2].value, FieldValueView::I64(-42));
        assert_eq!(decoded[3].value, FieldValueView::Null);
        assert_eq!(decoded[4].value, FieldValueView::String("café 猫"));
        assert_eq!(decoded[5].value, FieldValueView::U64(u64::MAX));
    }

    #[test]
    fn empty_fields_round_trip() {
        let mut buffer = Vec::new();
        encode_payload(&mut buffer, "", "", &Fields::new()).unwrap();
        let view = decode_payload(&buffer).unwrap();
        assert_eq!(view.source, "");
        assert_eq!(view.message, "");
        assert_eq!(view.fields().count(), 0);
    }

    #[test]
    fn empty_field_name_and_value_round_trip() {
        let mut fields = Fields::new();
        fields.insert("", "");
        let mut buffer = Vec::new();
        encode_payload(&mut buffer, "", "", &fields).unwrap();
        let view = decode_payload(&buffer).unwrap();
        let field = view.fields().next().unwrap().unwrap();
        assert_eq!(field.name, "");
        assert_eq!(field.value, FieldValueView::String(""));
    }

    #[test]
    fn payloads_append_without_overlap() {
        let mut buffer = Vec::new();
        encode_payload(&mut buffer, "first", "one", &Fields::new()).unwrap();
        let split = buffer.len();
        encode_payload(&mut buffer, "second", "two", &sample_fields()).unwrap();
        assert_eq!(
            decode_payload(&buffer[..split])
                .unwrap()
                .source,
            "first"
        );
        assert_eq!(
            decode_payload(&buffer[split..])
                .unwrap()
                .message,
            "two"
        );
    }

    #[test]
    fn rejects_truncated_field_count() {
        let mut buffer = Vec::new();
        write_string(&mut buffer, "source").unwrap();
        write_string(&mut buffer, "message").unwrap();
        buffer.extend_from_slice(&[1, 0]);
        assert_eq!(
            decode_payload(&buffer),
            Err(CompactDecodingError::UnexpectedEnd)
        );
    }

    #[test]
    fn trusted_header_defers_field_validation() {
        let mut buffer = Vec::new();
        encode_payload(&mut buffer, "source", "message", &sample_fields())
            .unwrap();
        buffer.pop();
        let view = decode_trusted_payload(&buffer).unwrap();
        assert_eq!(view.message, "message");
        assert_eq!(
            decode_payload(&buffer),
            Err(CompactDecodingError::UnexpectedEnd)
        );
        assert!(
            view.fields()
                .any(|field| field.is_err())
        );
    }

    #[test]
    fn rejects_truncated_field() {
        let mut buffer = Vec::new();
        encode_payload(&mut buffer, "source", "message", &sample_fields())
            .unwrap();
        buffer.pop();
        assert_eq!(
            decode_payload(&buffer),
            Err(CompactDecodingError::UnexpectedEnd)
        );
    }

    #[test]
    fn rejects_invalid_field_tag() {
        let mut buffer = Vec::new();
        write_string(&mut buffer, "").unwrap();
        write_string(&mut buffer, "").unwrap();
        write_u32(&mut buffer, 1);
        write_string(&mut buffer, "key").unwrap();
        write_u8(&mut buffer, 255);
        assert_eq!(
            decode_payload(&buffer),
            Err(CompactDecodingError::InvalidFieldTag)
        );
    }

    #[test]
    fn rejects_invalid_boolean() {
        let mut buffer = Vec::new();
        write_string(&mut buffer, "").unwrap();
        write_string(&mut buffer, "").unwrap();
        write_u32(&mut buffer, 1);
        write_string(&mut buffer, "key").unwrap();
        write_u8(&mut buffer, FieldTag::Bool as u8);
        write_u8(&mut buffer, 2);
        assert_eq!(
            decode_payload(&buffer),
            Err(CompactDecodingError::InvalidBoolean)
        );
    }

    #[test]
    fn rejects_invalid_utf8() {
        let mut buffer = Vec::new();
        write_string(&mut buffer, "").unwrap();
        write_string(&mut buffer, "").unwrap();
        write_u32(&mut buffer, 1);
        write_u32(&mut buffer, 1);
        write_u8(&mut buffer, 0xff);
        write_u8(&mut buffer, FieldTag::Null as u8);
        assert_eq!(
            decode_payload(&buffer),
            Err(CompactDecodingError::InvalidUtf8)
        );
    }

    #[test]
    fn rejects_trailing_data() {
        let mut buffer = Vec::new();
        encode_payload(&mut buffer, "", "", &Fields::new()).unwrap();
        buffer.push(0);
        assert_eq!(
            decode_payload(&buffer),
            Err(CompactDecodingError::TrailingData)
        );
    }

    #[test]
    fn field_reader_preserves_cursor_on_error() {
        let bytes = [0, 0, 0, 0, FieldTag::Bool as u8, 2];
        let mut input = bytes.as_slice();
        assert_eq!(
            read_field(&mut input),
            Err(CompactDecodingError::InvalidBoolean)
        );
        assert_eq!(input, bytes.as_slice());
    }

    #[test]
    fn field_iterator_advances_in_order() {
        let mut buffer = Vec::new();
        write_field(&mut buffer, "first", &FieldValue::I64(10)).unwrap();
        write_field(&mut buffer, "second", &FieldValue::Bool(true)).unwrap();
        let mut iter = FieldIter {
            remaining: &buffer,
            count: 2,
        };
        assert_eq!(iter.next().unwrap().unwrap().name, "first");
        assert_eq!(iter.next().unwrap().unwrap().name, "second");
        assert!(iter.next().is_none());
    }

    #[test]
    fn field_iterator_reports_error_once() {
        let bytes = [0, 0, 0, 0, FieldTag::Bool as u8, 2];
        let mut iter = FieldIter {
            remaining: &bytes,
            count: 1,
        };
        assert_eq!(
            iter.next(),
            Some(Err(CompactDecodingError::InvalidBoolean))
        );
        assert!(iter.next().is_none());
    }
}
