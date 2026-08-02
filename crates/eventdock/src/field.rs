use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Structured value attached to an event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
}

/// Collection of named fields attached to an event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fields {
    // TODO(m4jr0): Benchmark BTreeMap against alternatives (contiguous, etc.).
    entries: BTreeMap<String, FieldValue>,
}

impl Fields {
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        name: impl Into<String>,
        value: impl Into<FieldValue>,
    ) -> Option<FieldValue> {
        self.entries.insert(name.into(), value.into())
    }

    pub fn get(
        &self,
        name: &str,
    ) -> Option<&FieldValue> {
        self.entries.get(name)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &FieldValue)> {
        self.entries.iter()
    }
}

impl<K, V> FromIterator<(K, V)> for Fields
where
    K: Into<String>,
    V: Into<FieldValue>,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self {
            entries: iter
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        }
    }
}

impl From<()> for FieldValue {
    fn from(_: ()) -> Self {
        Self::Null
    }
}

impl From<bool> for FieldValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i32> for FieldValue {
    fn from(value: i32) -> Self {
        Self::I64(i64::from(value))
    }
}

impl From<i64> for FieldValue {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

impl From<u32> for FieldValue {
    fn from(value: u32) -> Self {
        Self::U64(u64::from(value))
    }
}

impl From<u64> for FieldValue {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}

impl From<f32> for FieldValue {
    fn from(value: f32) -> Self {
        Self::F64(f64::from(value))
    }
}

impl From<f64> for FieldValue {
    fn from(value: f64) -> Self {
        Self::F64(value)
    }
}

impl From<String> for FieldValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for FieldValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_round_trip() {
        let json = serde_json::to_string(&FieldValue::Null).unwrap();
        assert_eq!(json, "null");

        let restored: FieldValue = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, FieldValue::Null);
    }

    #[test]
    fn integer_json_representation() {
        let signed = FieldValue::I64(42);
        let unsigned = FieldValue::U64(42);

        assert_eq!(serde_json::to_string(&signed).unwrap(), "42");
        assert_eq!(serde_json::to_string(&unsigned).unwrap(), "42");

        // JSON does not encode the original integer signedness.
        // Do not require an exact variant round trip here.
    }

    #[test]
    fn negative_integer_round_trip() {
        let original = FieldValue::I64(-42);
        let json = serde_json::to_string(&original).unwrap();
        let restored: FieldValue = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn large_unsigned_integer_round_trip() {
        let original = FieldValue::U64(u64::MAX);
        let json = serde_json::to_string(&original).unwrap();
        let restored: FieldValue = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn fields_serialize_as_json_object() {
        let mut fields = Fields::new();

        fields.insert("vsync", true);
        fields.insert("frame", 42);
        fields.insert("temperature", FieldValue::Null);

        let json = serde_json::to_string(&fields).unwrap();

        assert_eq!(json, r#"{"frame":42,"temperature":null,"vsync":true}"#);
    }

    #[test]
    fn fields_deserialize_from_json_object() {
        let json = r#"{"enabled":true,"name":"foo","missing":null}"#;

        let fields: Fields = serde_json::from_str(json).unwrap();

        assert_eq!(fields.len(), 3);
        assert_eq!(fields.get("enabled"), Some(&FieldValue::Bool(true)));
        assert_eq!(
            fields.get("name"),
            Some(&FieldValue::String("foo".to_owned()))
        );
        assert_eq!(fields.get("missing"), Some(&FieldValue::Null));
    }

    #[test]
    fn insert_replaces_existing_field() {
        let mut fields = Fields::new();

        assert_eq!(fields.insert("frame", 42), None);
        assert_eq!(fields.insert("frame", 43), Some(FieldValue::I64(42)));
        assert_eq!(fields.len(), 1);
    }

    #[test]
    fn collect_fields() {
        let fields: Fields =
            [("name", "foo"), ("key", "bar")].into_iter().collect();

        assert_eq!(fields.len(), 2);
        assert_eq!(
            fields.get("key"),
            Some(&FieldValue::String("bar".to_owned()))
        );
    }
}
