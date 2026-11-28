//! Shared query contracts, independent of engine execution and transport.
use eventdock_core::{EventId, LogLevel};
use serde::{Deserialize, Serialize};

/// A bounded page of events, ordered from newest to oldest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventQuery {
    /// Exclusive upper bound. `None` starts at the newest retained event.
    pub before_id: Option<EventId>,
    pub limit: usize,
    /// Optional exact severity match, checked against compact metadata.
    pub level: Option<LogLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueriedEvent {
    pub id: EventId,
    pub level: eventdock_core::LogLevel,
    pub occurred_at: eventdock_core::Timestamp,
    pub received_at: eventdock_core::Timestamp,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventQueryResult {
    pub events: Vec<QueriedEvent>,
    /// Pass this cursor as `before_id` to fetch older retained events.
    pub next_cursor: Option<EventId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    pub limit: usize,
    #[serde(default)]
    pub before_id: Option<EventId>,
    #[serde(default)]
    pub level: Option<LogLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResponse {
    pub events: Vec<QueriedEvent>,
    pub next_cursor: Option<EventId>,
}

impl From<EventQueryResult> for QueryResponse {
    fn from(result: EventQueryResult) -> Self {
        Self {
            events: result.events,
            next_cursor: result.next_cursor,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireResponse {
    Success(QueryResponse),
    Failure { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_request_defaults() {
        let request: QueryRequest =
            serde_json::from_str(r#"{"limit":20}"#).unwrap();
        assert_eq!(
            request,
            QueryRequest {
                limit: 20,
                before_id: None,
                level: None
            }
        );
    }

    #[test]
    fn legacy_response_shapes() {
        let success = WireResponse::Success(QueryResponse {
            events: Vec::new(),
            next_cursor: None,
        });
        assert_eq!(
            serde_json::to_value(success).unwrap(),
            serde_json::json!({"events": [], "next_cursor": null})
        );
        let failure = WireResponse::Failure {
            error: "busy".into(),
        };
        assert_eq!(
            serde_json::to_value(failure).unwrap(),
            serde_json::json!({"error": "busy"})
        );
    }
}
