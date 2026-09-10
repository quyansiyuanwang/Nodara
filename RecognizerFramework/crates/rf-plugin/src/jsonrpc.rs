//! JSON-RPC 2.0 framing.
//!
//! Only the subset the plugin protocol needs is implemented, but it is a
//! faithful implementation of the specification: requests, notifications,
//! responses and the reserved error codes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The only protocol revision this crate speaks.
pub const JSONRPC_VERSION: &str = "2.0";

/// Reserved and application-specific error codes.
pub mod codes {
    /// Invalid JSON was received.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON was valid but not a valid request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// The requested method does not exist.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i64 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// Node configuration was rejected by the plugin.
    pub const APP_INVALID_CONFIG: i64 = -32001;
    /// The plugin refused the capability on policy grounds.
    pub const APP_PERMISSION_DENIED: i64 = -32002;
    /// The node was cancelled.
    pub const APP_CANCELLED: i64 = -32003;
    /// The node exceeded its deadline.
    pub const APP_TIMEOUT: i64 = -32004;
    /// The plugin does not implement the operation.
    pub const APP_UNSUPPORTED: i64 = -32005;
    /// Generic execution failure inside the plugin.
    pub const APP_EXECUTION: i64 = -32006;
    /// I/O failure inside the plugin.
    pub const APP_IO: i64 = -32007;
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorObject {
    /// Numeric error code.
    pub code: i64,
    /// Short message.
    pub message: String,
    /// Optional structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ErrorObject {
    /// Construct an error object.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Attach structured detail.
    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// A JSON-RPC request expecting a response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Correlation identifier (number or string).
    pub id: Value,
    /// Method name.
    pub method: String,
    /// Method parameters.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

impl Request {
    /// Construct a request.
    pub fn new(id: Value, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC notification, which must not be answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Method parameters.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

impl Notification {
    /// Construct a notification.
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Correlation identifier; absent for parse errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    /// Successful result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Failure detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

impl Response {
    /// A successful response.
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }

    /// A failing response.
    pub fn failure(id: Option<Value>, error: ErrorObject) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }

    /// True when the response carries an error.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// Any JSON-RPC message received from the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A request expecting a reply.
    Request(Request),
    /// A notification that must not be answered.
    Notification(Notification),
    /// A response to an earlier request.
    Response(Response),
}

/// Decode one JSON value into a typed JSON-RPC message.
pub fn decode(value: Value) -> Result<Incoming, ErrorObject> {
    let object = value
        .as_object()
        .ok_or_else(|| ErrorObject::new(codes::INVALID_REQUEST, "message must be a JSON object"))?;

    match object.get("jsonrpc").and_then(Value::as_str) {
        Some(version) if version == JSONRPC_VERSION => {}
        Some(other) => {
            return Err(ErrorObject::new(
                codes::INVALID_REQUEST,
                format!("unsupported jsonrpc version `{other}`"),
            ));
        }
        None => {
            return Err(ErrorObject::new(
                codes::INVALID_REQUEST,
                "missing `jsonrpc` member",
            ));
        }
    }

    if object.contains_key("method") {
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| ErrorObject::new(codes::INVALID_REQUEST, "`method` must be a string"))?
            .to_string();
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        return match object.get("id") {
            Some(id) if !id.is_null() => Ok(Incoming::Request(Request {
                jsonrpc: JSONRPC_VERSION.to_string(),
                id: id.clone(),
                method,
                params,
            })),
            _ => Ok(Incoming::Notification(Notification {
                jsonrpc: JSONRPC_VERSION.to_string(),
                method,
                params,
            })),
        };
    }

    if object.contains_key("result") || object.contains_key("error") {
        let response: Response = serde_json::from_value(value)
            .map_err(|error| ErrorObject::new(codes::INVALID_REQUEST, error.to_string()))?;
        if response.result.is_some() && response.error.is_some() {
            return Err(ErrorObject::new(
                codes::INVALID_REQUEST,
                "response must not contain both `result` and `error`",
            ));
        }
        return Ok(Incoming::Response(response));
    }

    Err(ErrorObject::new(
        codes::INVALID_REQUEST,
        "message is neither a request, a notification nor a response",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_a_request() {
        let value = json!({"jsonrpc":"2.0","id":1,"method":"describe","params":{}});
        match decode(value).unwrap() {
            Incoming::Request(request) => {
                assert_eq!(request.method, "describe");
                assert_eq!(request.id, json!(1));
            }
            other => panic!("expected request, got {other:?}"),
        }
    }

    #[test]
    fn decodes_a_notification() {
        let value = json!({"jsonrpc":"2.0","method":"progress","params":{"run_id":"r"}});
        assert!(matches!(decode(value).unwrap(), Incoming::Notification(_)));
    }

    #[test]
    fn decodes_a_response() {
        let value = json!({"jsonrpc":"2.0","id":"abc","result":{"ok":true}});
        match decode(value).unwrap() {
            Incoming::Response(response) => {
                assert!(!response.is_error());
                assert_eq!(response.id, Some(json!("abc")));
            }
            other => panic!("expected response, got {other:?}"),
        }
    }

    #[test]
    fn rejects_wrong_version() {
        let value = json!({"jsonrpc":"1.0","id":1,"method":"x"});
        let error = decode(value).unwrap_err();
        assert_eq!(error.code, codes::INVALID_REQUEST);
    }

    #[test]
    fn rejects_response_with_both_result_and_error() {
        let value = json!({"jsonrpc":"2.0","id":1,"result":{},"error":{"code":1,"message":"x"}});
        assert!(decode(value).is_err());
    }

    #[test]
    fn responses_omit_null_members() {
        let json = serde_json::to_string(&Response::success(json!(1), json!({}))).unwrap();
        assert!(!json.contains("error"));
        let json = serde_json::to_string(&Response::failure(
            Some(json!(1)),
            ErrorObject::new(codes::METHOD_NOT_FOUND, "nope"),
        ))
        .unwrap();
        assert!(!json.contains("result"));
    }
}
