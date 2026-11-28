//! Shared query contracts, independent of engine execution and transport.
//!
//! The current wire contract uses one newline-delimited JSON request and response
//! per connection. Frame limits include the newline. Requests reject unknown
//! fields; responses permit additive fields. Missing error codes and unknown
//! error categories map to generic I/O errors. Incompatible request changes
//! require an explicit versioned protocol rather than silent reinterpretation.
//! Pagination is newest-first with an exclusive cursor, not snapshot isolation:
//! events evicted between pages may no longer be available.
mod framing;
mod pending;
use eventdock_core::{EventId, LogLevel};
pub use framing::{
    MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, encode_json_line, read_json_line,
    write_json_line,
};
pub use pending::{PendingQuery, QueryReply, query_reply_channel};
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
    Failure {
        error: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<QueryErrorCode>,
    },
}

/// Stable wire error categories. Unknown categories remain readable by older clients.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryErrorCode {
    InvalidInput,
    Busy,
    Unavailable,
    Timeout,
    ResponseTooLarge,
    #[serde(other)]
    Internal,
}

impl WireResponse {
    pub fn failure(error: std::io::Error) -> Self {
        use std::io::ErrorKind;
        let code = match error.kind() {
            ErrorKind::InvalidInput | ErrorKind::InvalidData => {
                QueryErrorCode::InvalidInput
            }
            ErrorKind::WouldBlock => QueryErrorCode::Busy,
            ErrorKind::BrokenPipe | ErrorKind::ConnectionAborted => {
                QueryErrorCode::Unavailable
            }
            ErrorKind::TimedOut => QueryErrorCode::Timeout,
            _ => QueryErrorCode::Internal,
        };
        Self::Failure {
            error: error.to_string(),
            code: Some(code),
        }
    }

    pub fn into_result(self) -> std::io::Result<QueryResponse> {
        use std::io::{Error, ErrorKind};
        match self {
            Self::Success(response) => Ok(response),
            Self::Failure { error, code } => {
                let kind = match code {
                    Some(QueryErrorCode::InvalidInput) => {
                        ErrorKind::InvalidInput
                    }
                    Some(QueryErrorCode::Busy) => ErrorKind::WouldBlock,
                    Some(QueryErrorCode::Unavailable) => ErrorKind::BrokenPipe,
                    Some(QueryErrorCode::Timeout) => ErrorKind::TimedOut,
                    Some(QueryErrorCode::ResponseTooLarge) => {
                        ErrorKind::InvalidData
                    }
                    _ => ErrorKind::Other,
                };
                Err(Error::new(kind, error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_errors_preserve_kind() {
        use std::io::{Error, ErrorKind};
        for kind in [
            ErrorKind::InvalidInput,
            ErrorKind::WouldBlock,
            ErrorKind::BrokenPipe,
            ErrorKind::TimedOut,
        ] {
            let response = WireResponse::failure(Error::new(kind, "test"));
            let bytes = serde_json::to_vec(&response).unwrap();
            let decoded: WireResponse = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                decoded
                    .into_result()
                    .unwrap_err()
                    .kind(),
                kind
            );
        }
    }

    #[test]
    fn legacy_and_future_error_codes_are_readable() {
        for json in [
            r#"{"error":"legacy"}"#,
            r#"{"error":"future","code":"future_category"}"#,
        ] {
            let response: WireResponse = serde_json::from_str(json).unwrap();
            assert_eq!(
                response
                    .into_result()
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::Other
            );
        }
    }

    #[test]
    fn additive_response_fields_are_readable() {
        let response: WireResponse = serde_json::from_str(
            r#"{"events":[],"next_cursor":null,"future_field":true}"#,
        )
        .unwrap();
        assert!(
            response
                .into_result()
                .unwrap()
                .events
                .is_empty()
        );
    }

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
            code: None,
        };
        assert_eq!(
            serde_json::to_value(failure).unwrap(),
            serde_json::json!({"error": "busy"})
        );
    }
}
