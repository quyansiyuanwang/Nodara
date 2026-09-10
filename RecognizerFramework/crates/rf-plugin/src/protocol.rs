//! The runtime <-> plugin method contract.
//!
//! Every request and response is a plain data structure deriving Serde, so the
//! exact wire shape is documented by the types themselves and published as JSON
//! Schema from `rf-schema`.

use std::collections::BTreeMap;

use rf_schema::{NodeDescriptor, ValueType};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wire protocol version spoken by this build.
pub const PROTOCOL_VERSION: &str = rf_schema::PROTOCOL_VERSION;

/// Canonical method names.
pub mod methods {
    /// Handshake and version negotiation.
    pub const INITIALIZE: &str = "initialize";
    /// Enumerate provided node types and their schemas.
    pub const DESCRIBE: &str = "describe";
    /// Execute one node.
    pub const EXECUTE: &str = "execute";
    /// Ask a running node to stop.
    pub const CANCEL: &str = "cancel";
    /// Release resources and exit.
    pub const SHUTDOWN: &str = "shutdown";
    /// Liveness probe.
    pub const HEALTH: &str = "health";
    /// Plugin -> runtime: incremental progress.
    pub const PROGRESS: &str = "progress";
    /// Plugin -> runtime: structured log line.
    pub const LOG: &str = "log";
}

/// Identifying information about a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Manifest id.
    pub id: String,
    /// Human-facing name.
    pub name: String,
    /// Plugin version.
    pub version: String,
}

/// `initialize` parameters, sent by the runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    /// Protocol version the runtime speaks.
    pub protocol_version: String,
    /// Runtime build version, for diagnostics.
    #[serde(default)]
    pub runtime_version: String,
    /// Permissions the runtime is willing to grant.
    #[serde(default)]
    pub granted_permissions: Vec<String>,
}

/// `initialize` result, returned by the plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Protocol version the plugin speaks.
    pub protocol_version: String,
    /// Plugin identity.
    pub plugin: PluginInfo,
    /// Capability identifiers the plugin provides.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// `describe` parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DescribeParams {
    /// Restrict the answer to these node types; empty means "all".
    #[serde(default)]
    pub node_types: Vec<String>,
}

/// `describe` result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DescribeResult {
    /// Descriptors for every requested node type.
    #[serde(default)]
    pub nodes: Vec<NodeDescriptor>,
}

/// `execute` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteParams {
    /// Run the node belongs to.
    pub run_id: String,
    /// Node being executed.
    pub node_id: String,
    /// Node type being executed.
    pub node_type: String,
    /// Fully interpolated configuration.
    #[serde(default)]
    pub config: Value,
    /// Values arriving on input ports.
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    /// Read-only snapshot of the run scope.
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
    /// Per-call deadline in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// `execute` result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// Values emitted on output ports.
    #[serde(default)]
    pub outputs: BTreeMap<String, Value>,
    /// Variables published to the run scope.
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
}

/// `cancel` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelParams {
    /// Run to cancel.
    pub run_id: String,
    /// Optional node scope; omitted cancels the whole run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
}

/// `progress` notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNotification {
    /// Run the progress belongs to.
    pub run_id: String,
    /// Node reporting progress.
    pub node_id: String,
    /// Fraction in `0.0..=1.0`, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Human-readable status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// `log` notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogNotification {
    /// Run the record belongs to.
    pub run_id: String,
    /// Node that produced the record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Severity: `debug`, `info`, `warn` or `error`.
    pub level: String,
    /// Message body.
    pub message: String,
}

/// Map a plugin node type default value type, used when descriptors omit ports.
pub fn default_value_type() -> ValueType {
    ValueType::Any
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_params_round_trip() {
        let params = ExecuteParams {
            run_id: "run".into(),
            node_id: "n1".into(),
            node_type: "windows.Input.Keyboard".into(),
            config: serde_json::json!({"keys": "hello"}),
            inputs: BTreeMap::new(),
            variables: BTreeMap::new(),
            timeout_ms: Some(1000),
        };
        let json = serde_json::to_string(&params).unwrap();
        let back: ExecuteParams = serde_json::from_str(&json).unwrap();
        assert_eq!(back.node_type, "windows.Input.Keyboard");
        assert_eq!(back.timeout_ms, Some(1000));
    }

    #[test]
    fn describe_params_default_to_all() {
        let params: DescribeParams = serde_json::from_str("{}").unwrap();
        assert!(params.node_types.is_empty());
    }
}
