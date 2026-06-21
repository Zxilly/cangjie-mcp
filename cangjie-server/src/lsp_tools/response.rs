use serde::Serialize;
use serde_json::{json, Value};

use super::types::{LspOperation, LspResponse, LspResponseStatus, ResolvedTarget};

fn empty_data() -> Value {
    json!({})
}

pub(crate) fn status_from_count(count: usize) -> LspResponseStatus {
    if count == 0 {
        LspResponseStatus::Empty
    } else {
        LspResponseStatus::Ok
    }
}

fn serialize_response(response: &LspResponse) -> String {
    serde_json::to_string_pretty(response).unwrap_or_else(|e| {
        format!("{{\"status\":\"error\",\"message\":\"Serialization error: {e}\"}}")
    })
}

pub(crate) fn response_with_data<T: Serialize>(
    operation: LspOperation,
    status: LspResponseStatus,
    resolved_target: Option<ResolvedTarget>,
    data: &T,
    message: Option<String>,
) -> String {
    let data = serde_json::to_value(data).unwrap_or_else(|_| empty_data());
    serialize_response(&LspResponse {
        operation,
        status,
        resolved_target,
        data,
        message,
    })
}

fn status_response(
    operation: LspOperation,
    status: LspResponseStatus,
    message: impl Into<String>,
) -> String {
    serialize_response(&LspResponse {
        operation,
        status,
        resolved_target: None,
        data: empty_data(),
        message: Some(message.into()),
    })
}

pub(crate) fn error_response(operation: LspOperation, message: impl Into<String>) -> String {
    status_response(operation, LspResponseStatus::Error, message)
}

#[cfg(feature = "lsp")]
pub(crate) fn unsupported_response(operation: LspOperation, message: impl Into<String>) -> String {
    status_response(operation, LspResponseStatus::Unsupported, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_serialization_uses_timeout_status() {
        let serialized = response_with_data(
            LspOperation::Diagnostics,
            LspResponseStatus::Timeout,
            None,
            &json!({ "diagnostics": [] }),
            Some("timeout".to_string()),
        );
        let parsed: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["status"], "timeout");
    }
}
