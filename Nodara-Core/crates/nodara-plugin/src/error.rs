//! Error types for plugin hosting and the wire protocol.

use thiserror::Error;

/// Convenience alias for plugin results.
pub type PluginResult<T> = Result<T, PluginError>;

/// Failures that can occur while discovering, launching or talking to a plugin.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PluginError {
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON encoding or decoding failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The plugin manifest is missing or invalid.
    #[error("invalid manifest: {0}")]
    Manifest(String),

    /// The plugin executable could not be launched.
    #[error("failed to launch plugin `{id}`: {source}")]
    Launch {
        /// Plugin id.
        id: String,
        /// Underlying failure.
        #[source]
        source: std::io::Error,
    },

    /// The handshake failed, usually because of a version mismatch.
    #[error("handshake failed: {0}")]
    Handshake(String),

    /// A framed message was not valid JSON-RPC 2.0.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// The plugin returned a JSON-RPC error for a request.
    #[error("plugin returned error {code}: {message}")]
    Remote {
        /// JSON-RPC error code.
        code: i64,
        /// Human-readable message.
        message: String,
        /// Optional structured payload.
        data: Option<serde_json::Value>,
    },

    /// No response arrived before the deadline.
    #[error("plugin request `{method}` timed out after {timeout_ms}ms")]
    Timeout {
        /// Method that timed out.
        method: String,
        /// Configured timeout.
        timeout_ms: u64,
    },

    /// The plugin process exited unexpectedly.
    #[error("plugin process closed the connection")]
    Disconnected,

    /// The plugin was asked to do something it does not implement.
    #[error("unsupported by plugin: {0}")]
    Unsupported(String),
}

pub use nodara_schema::SchemaError;
