//! Error types for execution and node implementations.

use rf_schema::{SchemaError, ValidationReport};
use thiserror::Error;

/// Errors reported by an individual node executor.
///
/// Every variant maps to a stable machine-readable code so that the Studio, the
/// CLI and the agent can branch on failures without string matching.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum NodeError {
    /// The supplied configuration is missing or malformed.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Policy refused the capability required by this node.
    #[error("permission denied for capability `{capability}`: {reason}")]
    PermissionDenied {
        /// Capability that was refused.
        capability: String,
        /// Why it was refused.
        reason: String,
    },

    /// The run was cancelled while the node was executing.
    #[error("node execution cancelled")]
    Cancelled,

    /// The node exceeded its allotted time.
    #[error("node execution timed out")]
    Timeout,

    /// A generic runtime failure inside the node.
    #[error("execution failed: {0}")]
    Execution(String),

    /// Underlying I/O failure, already rendered to a string.
    #[error("io error: {0}")]
    Io(String),

    /// The node does not support the requested operation.
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

impl NodeError {
    /// Stable error code for protocol and audit purposes.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig(_) => "E_INVALID_CONFIG",
            Self::PermissionDenied { .. } => "E_PERMISSION_DENIED",
            Self::Cancelled => "E_CANCELLED",
            Self::Timeout => "E_TIMEOUT",
            Self::Execution(_) => "E_EXECUTION",
            Self::Io(_) => "E_IO",
            Self::Unsupported(_) => "E_UNSUPPORTED",
        }
    }

    /// Whether retrying the same node could plausibly succeed.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Timeout | Self::Io(_))
    }

    /// Construct a permission-denied error.
    pub fn denied(capability: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::PermissionDenied {
            capability: capability.into(),
            reason: reason.into(),
        }
    }
}

/// Convenience alias for node results.
pub type NodeResult<T> = Result<T, NodeError>;

/// Errors reported by the engine for a whole run.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExecutionError {
    /// Static validation rejected the workflow before execution.
    #[error("workflow validation failed with {errors} error(s) and {warnings} warning(s)")]
    Validation {
        /// Number of blocking errors.
        errors: usize,
        /// Number of warnings.
        warnings: usize,
        /// Full report for callers that want to render it.
        report: Box<ValidationReport>,
    },

    /// The workflow document itself could not be parsed.
    #[error(transparent)]
    Schema(#[from] SchemaError),

    /// No node could serve as the entry point.
    #[error("workflow has no entry point")]
    NoEntryPoint,

    /// The graph contains a cycle and cannot be scheduled.
    #[error(transparent)]
    Graph(#[from] rf_schema::GraphError),

    /// A node referenced a type no registered executor provides.
    #[error("unknown node type `{node_type}` for node `{node_id}`")]
    UnknownNodeType {
        /// Offending node.
        node_id: String,
        /// Missing node type.
        node_type: String,
    },

    /// A node failed during execution.
    #[error("node `{node_id}` ({node_type}) failed: {source}")]
    NodeFailed {
        /// Offending node.
        node_id: String,
        /// Node type.
        node_type: String,
        /// Underlying node error.
        #[source]
        source: NodeError,
    },

    /// The run was cancelled.
    #[error("run cancelled")]
    Cancelled,
}

impl ExecutionError {
    /// Stable error code for protocol and audit purposes.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "E_VALIDATION",
            Self::Schema(_) => "E_SCHEMA",
            Self::NoEntryPoint => "E_NO_ENTRY_POINT",
            Self::Graph(_) => "E_GRAPH",
            Self::UnknownNodeType { .. } => "E_UNKNOWN_NODE_TYPE",
            Self::NodeFailed { source, .. } => source.code(),
            Self::Cancelled => "E_CANCELLED",
        }
    }
}

/// Convenience alias for execution results.
pub type ExecutionResult<T> = Result<T, ExecutionError>;
