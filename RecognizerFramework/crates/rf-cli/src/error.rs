//! CLI error type.

use thiserror::Error;

/// Convenience alias.
pub type CliResult<T> = Result<T, CliError>;

/// Everything that can go wrong while running a CLI command.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CliError {
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Workflow document failure.
    #[error("schema error: {0}")]
    Schema(#[from] rf_schema::SchemaError),

    /// Plugin subsystem failure.
    #[error("plugin error: {0}")]
    Plugin(#[from] rf_plugin::PluginError),

    /// Runtime failure.
    #[error("runtime error: {0}")]
    Runtime(#[from] rf_runtime::RuntimeError),

    /// A `key=value` argument was malformed.
    #[error("invalid argument `{0}`: expected `key=value`")]
    InvalidVariable(String),

    /// The workflow parsed but is invalid.
    #[error("workflow is invalid: {0} error(s)")]
    Invalid(usize),

    /// The requested command could not complete.
    #[error("{0}")]
    Failed(String),
}
