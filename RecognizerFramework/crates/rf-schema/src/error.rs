//! Error types for schema, validation and migration operations.

use thiserror::Error;

/// Convenience alias for schema results.
pub type SchemaResult<T> = Result<T, SchemaError>;

/// Errors produced while parsing, validating or migrating documents.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// Underlying `serde_json` failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Filesystem failure while reading a document.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The document declares a workflow format this build cannot handle.
    #[error("unsupported schema version `{found}` (supported: `{supported}`)")]
    UnsupportedSchemaVersion {
        /// Version found in the document.
        found: String,
        /// Version supported by this build.
        supported: String,
    },

    /// Structural problem that prevented building a [`crate::Workflow`].
    #[error("invalid workflow: {0}")]
    InvalidWorkflow(String),

    /// Structural problem in a plugin manifest.
    #[error("invalid plugin manifest: {0}")]
    InvalidManifest(String),

    /// Plugin speaks a wire protocol this build does not support.
    #[error("unsupported protocol version `{0}`")]
    UnsupportedProtocolVersion(String),

    /// A legacy document could not be upgraded.
    #[error("migration failed: {0}")]
    Migration(String),

    /// A graph algorithm could not produce a result (for example a cycle).
    #[error("graph error: {0}")]
    Graph(String),
}
