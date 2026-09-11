//! Tool-call contract.
//!
//! A tool call is how a client — in practice the agent, but the shape is
//! general — expresses "I want this capability to run with this input".
//!
//! It is a *request*, never an authorisation: the runtime turns it into a
//! policy decision and records the outcome. Publishing the shape here means the
//! Studio can render an approval prompt from the same data structure the agent
//! produced, without either side depending on the other's types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A request to run one capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    /// Caller-chosen correlation id, echoed back in the result.
    pub call_id: String,
    /// Capability identifier, conventionally the node type.
    pub capability: String,
    /// Node type that would be executed.
    pub node_type: String,
    /// Run the call belongs to, when it is part of a workflow run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Why the caller wants it. Surfaced verbatim in approval prompts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Permissions the caller expects to need.
    #[serde(default)]
    pub requested_permissions: Vec<String>,
    /// The input the capability would receive.
    #[serde(default)]
    pub input: serde_json::Value,
}

impl ToolCall {
    /// Construct a call for a node type.
    pub fn new(
        call_id: impl Into<String>,
        node_type: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        let node_type = node_type.into();
        Self {
            call_id: call_id.into(),
            capability: node_type.clone(),
            node_type,
            run_id: None,
            reason: None,
            requested_permissions: Vec::new(),
            input,
        }
    }
}

/// The runtime's answer to a [`ToolCall`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ToolCallOutcome {
    /// The capability ran.
    Completed {
        /// Values emitted on output ports.
        #[serde(default)]
        outputs: std::collections::BTreeMap<String, serde_json::Value>,
    },
    /// Policy refused the call.
    Denied {
        /// Why it was refused.
        reason: String,
    },
    /// Policy asked for approval and no approver consented.
    AwaitingApproval {
        /// Identifier of the pending approval.
        approval_id: String,
    },
    /// The capability failed.
    Failed {
        /// Stable machine-readable code.
        code: String,
        /// Human-readable message.
        message: String,
    },
}
