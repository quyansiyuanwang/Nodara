//! Execution event stream contract.
//!
//! Events are the single observability surface shared by the CLI, the Studio
//! event viewer, the autonomous agent and the audit log. They are immutable,
//! monotonically sequenced records.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::workflow::EdgeBranch;

/// Lifecycle state of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Accepted but not yet started.
    Pending,
    /// Currently executing.
    Running,
    /// Suspended by an operator or policy.
    Paused,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
    /// Stopped by an operator.
    Cancelled,
}

impl RunStatus {
    /// True when no further progress is possible without a new run.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Severity of a log event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Verbose diagnostics.
    Debug,
    /// Normal progress information.
    Info,
    /// Recoverable problem.
    Warn,
    /// Serious problem.
    Error,
}

/// Complete input snapshot captured immediately before a node executes.
///
/// The runtime resolves secret variables to `***` before this snapshot is
/// emitted, so the same observability surface can be consumed by the Studio and
/// the Agent without exposing secret values.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct NodeInputSnapshot {
    /// Raw configuration exactly as authored in the workflow.
    #[serde(default)]
    pub config: serde_json::Value,
    /// Configuration after template and variable resolution.
    #[serde(default)]
    pub resolved_config: serde_json::Value,
    /// Values arriving on data input ports, keyed by port name.
    #[serde(default)]
    pub inputs: BTreeMap<String, serde_json::Value>,
    /// Redacted run-scope variables before the node starts.
    #[serde(default)]
    pub variables_before: BTreeMap<String, serde_json::Value>,
    /// Per-attempt timeout, when configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Payload of an execution event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionEvent {
    /// A run was accepted and is about to start.
    RunStarted {
        /// Workflow being executed.
        workflow_id: String,
    },
    /// A node began executing.
    NodeStarted {
        /// Node id.
        node_id: String,
        /// Node type.
        node_type: String,
        /// Complete input and variable state immediately before execution.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<NodeInputSnapshot>,
    },
    /// A node reported incremental progress.
    NodeProgress {
        /// Node id.
        node_id: String,
        /// Fraction in `0.0..=1.0`, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress: Option<f64>,
        /// Human-readable status.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// A node finished successfully.
    NodeFinished {
        /// Node id.
        node_id: String,
        /// Node outputs.
        #[serde(default)]
        outputs: BTreeMap<String, serde_json::Value>,
        /// Redacted run-scope variables after the node published its outputs.
        #[serde(default)]
        variables_after: BTreeMap<String, serde_json::Value>,
        /// Wall-clock duration in milliseconds.
        duration_ms: u64,
    },
    /// A node failed.
    NodeFailed {
        /// Node id.
        node_id: String,
        /// Machine-readable error code.
        code: String,
        /// Human-readable message.
        message: String,
        /// Whether the runtime will retry.
        #[serde(default)]
        retryable: bool,
        /// Redacted run-scope variables after the failure was recorded.
        #[serde(default)]
        variables_after: BTreeMap<String, serde_json::Value>,
    },
    /// A control edge activated a target node.
    EdgeActivated {
        /// Edge id.
        edge_id: String,
        /// Source node id.
        source: String,
        /// Target node id.
        target: String,
        /// Outcome branch that was followed.
        branch: EdgeBranch,
    },
    /// A data edge transferred a value into the target node input map.
    DataTransferred {
        /// Edge id.
        edge_id: String,
        /// Source node id.
        source: String,
        /// Target node id.
        target: String,
        /// Source output port.
        source_port: String,
        /// Target input port.
        target_port: String,
        /// Exact value transferred, for replay and model inspection.
        #[serde(default)]
        value: serde_json::Value,
    },
    /// A structured log record.
    Log {
        /// Severity.
        level: LogLevel,
        /// Message.
        message: String,
        /// Node that produced the record, when applicable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
    /// Execution was suspended.
    RunPaused,
    /// Execution resumed.
    RunResumed,
    /// Execution was cancelled.
    RunCancelled {
        /// Why the run was cancelled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Execution finished successfully.
    RunCompleted {
        /// Number of nodes executed.
        nodes_executed: usize,
        /// Total wall-clock duration in milliseconds.
        duration_ms: u64,
    },
    /// Execution failed.
    RunFailed {
        /// Machine-readable error code.
        code: String,
        /// Human-readable message.
        message: String,
    },
    /// A capability call was evaluated by policy.
    CapabilityDecision {
        /// Capability under evaluation.
        capability: String,
        /// Decision string: `allow`, `deny` or `require_approval`.
        decision: String,
        /// Node that triggered the check.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
}

/// A sequenced envelope around an [`ExecutionEvent`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct EventEnvelope {
    /// Run this event belongs to.
    pub run_id: String,
    /// Monotonically increasing sequence number within the run.
    pub seq: u64,
    /// Unix epoch milliseconds.
    pub timestamp_ms: u64,
    /// Event payload.
    pub event: ExecutionEvent,
}

impl EventEnvelope {
    /// Wrap an event with a sequence number and the current time.
    pub fn new(run_id: impl Into<String>, seq: u64, event: ExecutionEvent) -> Self {
        Self {
            run_id: run_id.into(),
            seq,
            timestamp_ms: now_ms(),
            event,
        }
    }
}

/// Current Unix time in milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states_are_detected() {
        assert!(RunStatus::Completed.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
        assert!(!RunStatus::Paused.is_terminal());
    }

    #[test]
    fn events_round_trip_through_json() {
        let envelope = EventEnvelope::new(
            "run-1",
            7,
            ExecutionEvent::NodeStarted {
                node_id: "log".into(),
                node_type: "core.Log".into(),
                input: Some(NodeInputSnapshot {
                    config: serde_json::json!({ "message": "{{greeting}}" }),
                    resolved_config: serde_json::json!({ "message": "hello" }),
                    inputs: BTreeMap::from([("in".into(), serde_json::json!("hello"))]),
                    variables_before: BTreeMap::from([(
                        "greeting".into(),
                        serde_json::json!("hello"),
                    )]),
                    timeout_ms: Some(1000),
                }),
            },
        );
        let json = serde_json::to_string(&envelope).unwrap();
        let back: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, envelope);
        assert!(json.contains("\"type\":\"node_started\""));
    }
}
