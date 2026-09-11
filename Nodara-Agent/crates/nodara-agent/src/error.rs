//! Agent error type.

use thiserror::Error;

/// Convenience alias.
pub type AgentResult<T> = Result<T, AgentError>;

/// Everything that can go wrong while planning or operating a workflow.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AgentError {
    /// The model provider failed.
    #[error("provider error: {0}")]
    Provider(String),

    /// The runtime API failed.
    #[error("runtime error ({status}): {message}")]
    Runtime {
        /// HTTP status, or 0 for a transport failure.
        status: u16,
        /// Human-readable message.
        message: String,
    },

    /// A transport-level failure while talking to the runtime.
    #[error("transport error: {0}")]
    Transport(String),

    /// The model produced something that is not a usable workflow.
    #[error("the model did not produce a valid workflow: {0}")]
    InvalidPlan(String),

    /// A guardrail refused the action.
    #[error("refused: {0}")]
    Refused(String),

    /// A budget was exhausted.
    #[error("budget exhausted: {0}")]
    BudgetExhausted(String),

    /// JSON handling failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The workflow contract itself rejected the document.
    #[error("schema error: {0}")]
    Schema(#[from] nodara_schema::SchemaError),
}
