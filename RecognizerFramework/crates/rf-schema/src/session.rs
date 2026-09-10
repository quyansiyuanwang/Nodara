//! Agent-session contract.
//!
//! The architecture document is explicit that the Studio and the agent must not
//! call each other directly; they cooperate through the runtime's session and
//! event API. These types are that API's payloads.
//!
//! The runtime stores sessions as data. It contains no model logic, no prompt
//! handling and no planning — the agent process produces those and publishes
//! them here, which is what keeps the core free of agent concerns while still
//! giving the Studio something to render.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::Diagnostic;
use crate::workflow::Workflow;

/// Lifecycle of an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Created, nothing planned yet.
    #[default]
    Draft,
    /// The agent is asking the model.
    Planning,
    /// A plan exists but a capability needs operator consent.
    AwaitingApproval,
    /// A plan exists and policy has cleared it.
    Ready,
    /// The planned workflow is executing.
    Running,
    /// The run finished successfully.
    Completed,
    /// The run failed, or planning did.
    Failed,
    /// An operator stopped it.
    Cancelled,
}

impl SessionStatus {
    /// True when the session will not change again without new input.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Who authored a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// The human operator.
    Operator,
    /// The agent.
    Agent,
    /// The runtime.
    Runtime,
}

/// One turn in the conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionMessage {
    /// Monotonic sequence number within the session.
    pub seq: u64,
    /// Unix epoch milliseconds.
    pub at_ms: u64,
    /// Author.
    pub role: MessageRole,
    /// Body text.
    pub text: String,
}

/// An operator decision on a gated capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    /// Allow the capability to run.
    Approved,
    /// Refuse it.
    Denied,
}

/// One pending or resolved approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalRequest {
    /// Identifier used to decide it.
    pub id: String,
    /// Unix epoch milliseconds the request was raised.
    pub requested_at_ms: u64,
    /// Run that is waiting.
    pub run_id: String,
    /// Node that is waiting.
    pub node_id: String,
    /// Node type that is waiting.
    pub node_type: String,
    /// Capability under evaluation.
    pub capability: String,
    /// Permissions the node declares.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Why policy asked for approval, and what the node would do.
    pub reason: String,
    /// The input the capability would receive, so the operator can judge it.
    #[serde(default)]
    pub input: serde_json::Value,
    /// Decision, once taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<ApprovalDecision>,
    /// When the decision was taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at_ms: Option<u64>,
    /// Who decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
}

impl ApprovalRequest {
    /// True while the run is still blocked on this request.
    pub fn is_pending(&self) -> bool {
        self.decision.is_none()
    }
}

/// The plan the agent proposed, with the runtime's verdict on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanPreview {
    /// Unix epoch milliseconds the preview was published.
    pub at_ms: u64,
    /// The proposed workflow.
    pub workflow: Workflow,
    /// Whether the runtime accepted it.
    pub valid: bool,
    /// Blocking error count.
    pub errors: usize,
    /// Warning count.
    pub warnings: usize,
    /// Full diagnostics, so the editor can underline the problems.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// A complete agent session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentSession {
    /// Identifier.
    pub id: String,
    /// What the operator asked for.
    pub goal: String,
    /// Provider the agent used, for display.
    #[serde(default)]
    pub provider: String,
    /// Lifecycle state.
    pub status: SessionStatus,
    /// Creation time.
    pub created_at_ms: u64,
    /// Last mutation time.
    pub updated_at_ms: u64,
    /// Conversation, oldest first.
    #[serde(default)]
    pub messages: Vec<SessionMessage>,
    /// Latest plan preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanPreview>,
    /// Approvals raised during the session.
    #[serde(default)]
    pub approvals: Vec<ApprovalRequest>,
    /// Run started from this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Tokens the agent has spent.
    #[serde(default)]
    pub tokens_used: u64,
}

impl AgentSession {
    /// Create an empty session.
    pub fn new(id: impl Into<String>, goal: impl Into<String>) -> Self {
        let now = crate::event::now_ms();
        Self {
            id: id.into(),
            goal: goal.into(),
            provider: String::new(),
            status: SessionStatus::Draft,
            created_at_ms: now,
            updated_at_ms: now,
            messages: Vec::new(),
            plan: None,
            approvals: Vec::new(),
            run_id: None,
            tokens_used: 0,
        }
    }

    /// Approvals still waiting for a decision.
    pub fn pending_approvals(&self) -> impl Iterator<Item = &ApprovalRequest> {
        self.approvals
            .iter()
            .filter(|approval| approval.is_pending())
    }

    /// True when at least one capability is blocked on the operator.
    pub fn needs_attention(&self) -> bool {
        self.pending_approvals().next().is_some()
    }
}

/// A request to create a session, or to append a turn to one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionRequest {
    /// What the operator wants.
    pub goal: String,
    /// Provider name, for display.
    #[serde(default)]
    pub provider: String,
}

/// A message to append to a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionMessageRequest {
    /// Author.
    pub role: MessageRole,
    /// Body text.
    pub text: String,
}

/// An operator decision on one approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalDecisionRequest {
    /// The decision.
    pub decision: ApprovalDecision,
    /// Who took it.
    #[serde(default)]
    pub decided_by: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_pending_approvals() {
        let mut session = AgentSession::new("s1", "do a thing");
        assert!(!session.needs_attention());
        session.approvals.push(ApprovalRequest {
            id: "a1".into(),
            requested_at_ms: 0,
            run_id: "r1".into(),
            node_id: "n1".into(),
            node_type: "windows.Input.Keyboard".into(),
            capability: "windows.Input.Keyboard".into(),
            permissions: vec!["input.control".into()],
            reason: "privileged".into(),
            input: serde_json::json!({ "keys": "a" }),
            decision: None,
            decided_at_ms: None,
            decided_by: None,
        });
        assert!(session.needs_attention());
        assert_eq!(session.pending_approvals().count(), 1);
    }

    #[test]
    fn sessions_round_trip_through_json() {
        let mut session = AgentSession::new("s1", "goal");
        session.messages.push(SessionMessage {
            seq: 0,
            at_ms: 1,
            role: MessageRole::Operator,
            text: "hello".into(),
        });
        let json = serde_json::to_string(&session).unwrap();
        let back: AgentSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back, session);
    }
}
