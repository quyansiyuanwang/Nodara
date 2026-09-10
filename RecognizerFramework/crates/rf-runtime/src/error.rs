//! Runtime error types and their HTTP representation.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use thiserror::Error;

/// Convenience alias.
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Failures that can occur while configuring or operating the runtime.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RuntimeError {
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Workflow document failure.
    #[error("schema error: {0}")]
    Schema(#[from] rf_schema::SchemaError),

    /// Plugin subsystem failure.
    #[error("plugin error: {0}")]
    Plugin(#[from] rf_plugin::PluginError),

    /// The runtime could not bind its listener.
    #[error("failed to bind {address}: {source}")]
    Bind {
        /// Address the runtime tried to bind.
        address: String,
        /// Underlying failure.
        #[source]
        source: std::io::Error,
    },
}

/// A structured API error.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiErrorBody {
    /// Stable machine-readable code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Optional structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

/// An error that can be returned from an HTTP handler.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    body: ApiErrorBody,
}

impl ApiError {
    /// Build an error from a status and message.
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                detail: None,
            },
        }
    }

    /// Attach structured detail.
    #[must_use]
    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.body.detail = Some(detail);
        self
    }

    /// HTTP status this error maps to.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The body that will be serialized.
    pub fn body(&self) -> &ApiErrorBody {
        &self.body
    }

    /// 400 Bad Request.
    pub fn bad_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    /// 404 Not Found.
    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    /// 409 Conflict.
    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    /// 422 Unprocessable Entity.
    pub fn unprocessable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}
