//! The Rust SDK surface: [`NodeExecutor`].
//!
//! This is the trait the architecture document specifies for Rust plugins,
//! platform adapters and embedded callers. It is intentionally synchronous: the
//! engine owns a dedicated thread per run, so a node may block on I/O (a plugin
//! round-trip, a Win32 call, a sleep) without stalling an async runtime.

use std::collections::BTreeMap;

use nodara_schema::NodeDescriptor;

use crate::context::ExecutionContext;
use crate::error::NodeResult;

/// Everything a node needs to execute.
#[derive(Debug, Clone)]
pub struct NodeInput {
    /// Id of the node being executed.
    pub node_id: String,
    /// Node type being executed.
    pub node_type: String,
    /// Raw configuration exactly as authored in the workflow.
    pub config: serde_json::Value,
    /// Configuration with `{{variable}}` placeholders resolved.
    pub resolved_config: serde_json::Value,
    /// Values arriving on input ports, keyed by port name.
    pub inputs: BTreeMap<String, serde_json::Value>,
}

impl NodeInput {
    /// Value arriving on a named input port.
    pub fn input(&self, port: &str) -> Option<&serde_json::Value> {
        self.inputs.get(port)
    }

    /// Read a string field from the resolved configuration.
    pub fn config_str(&self, key: &str) -> Option<&str> {
        self.resolved_config.get(key).and_then(|v| v.as_str())
    }

    /// Read an integer field from the resolved configuration.
    pub fn config_i64(&self, key: &str) -> Option<i64> {
        self.resolved_config
            .get(key)
            .and_then(serde_json::Value::as_i64)
    }

    /// Read a numeric field from the resolved configuration.
    pub fn config_f64(&self, key: &str) -> Option<f64> {
        self.resolved_config
            .get(key)
            .and_then(serde_json::Value::as_f64)
    }

    /// Read a boolean field from the resolved configuration.
    pub fn config_bool(&self, key: &str) -> Option<bool> {
        self.resolved_config
            .get(key)
            .and_then(serde_json::Value::as_bool)
    }

    /// Read a string field, failing with [`crate::NodeError::InvalidConfig`].
    pub fn require_str(&self, key: &str) -> NodeResult<String> {
        self.config_str(key)
            .map(str::to_string)
            .ok_or_else(|| crate::NodeError::InvalidConfig(format!("missing string field `{key}`")))
    }

    /// Read an integer field, failing with [`crate::NodeError::InvalidConfig`].
    pub fn require_i64(&self, key: &str) -> NodeResult<i64> {
        self.config_i64(key).ok_or_else(|| {
            crate::NodeError::InvalidConfig(format!("missing integer field `{key}`"))
        })
    }
}

/// What a node produced.
#[derive(Debug, Clone, Default)]
pub struct NodeOutput {
    /// Values emitted on output ports.
    pub outputs: BTreeMap<String, serde_json::Value>,
    /// Variables published to the run scope.
    pub variables: BTreeMap<String, serde_json::Value>,
}

impl NodeOutput {
    /// An empty output.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder-style output-port value.
    #[must_use]
    pub fn with_output(mut self, port: impl Into<String>, value: serde_json::Value) -> Self {
        self.outputs.insert(port.into(), value);
        self
    }

    /// Builder-style variable publication.
    #[must_use]
    pub fn with_variable(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        self.variables.insert(name.into(), value);
        self
    }

    /// Value on a named output port.
    pub fn output(&self, port: &str) -> Option<&serde_json::Value> {
        self.outputs.get(port)
    }
}

/// A single executable capability.
///
/// Implementors must be `Send + Sync` because the registry is shared across the
/// HTTP server, the run threads and (in future) a scheduler pool.
pub trait NodeExecutor: Send + Sync {
    /// Machine-readable description of the node type.
    fn descriptor(&self) -> NodeDescriptor;

    /// Execute the node.
    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput>;
}

impl std::fmt::Debug for dyn NodeExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeExecutor")
            .field("node_type", &self.descriptor().node_type)
            .finish()
    }
}
