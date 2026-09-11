//! Capability authorisation.
//!
//! Permission is enforced in the runtime, not in a prompt. Every capability
//! invocation is turned into a [`CapabilityRequest`], handed to a
//! [`CapabilityPolicy`], and the resulting [`Decision`] is written to the audit
//! log. An agent therefore cannot grant itself authority by describing what it
//! intends to do.

use std::collections::HashSet;
use std::sync::Arc;

/// Outcome of a policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Perform the call.
    Allow,
    /// Refuse the call outright.
    Deny {
        /// Why the call was refused.
        reason: String,
    },
    /// Perform the call only if a human (or delegated approver) consents.
    RequireApproval {
        /// Why approval is required.
        reason: String,
    },
}

impl Decision {
    /// Lower-case name used in events and audit records.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny { .. } => "deny",
            Self::RequireApproval { .. } => "require_approval",
        }
    }
}

/// A single capability invocation under evaluation.
#[derive(Debug, Clone)]
pub struct CapabilityRequest {
    /// Run that triggered the call.
    pub run_id: String,
    /// Node that triggered the call.
    pub node_id: String,
    /// Node type that triggered the call.
    pub node_type: String,
    /// Capability identifier, conventionally the node type.
    pub capability: String,
    /// Permissions the node declares.
    pub permissions: Vec<String>,
    /// Whether the node is marked as performing a side effect.
    pub dangerous: bool,
    /// Resolved node configuration, for context-sensitive policies.
    pub input: serde_json::Value,
}

/// Decides whether a capability call may proceed.
pub trait CapabilityPolicy: Send + Sync {
    /// Evaluate one request.
    fn decide(&self, request: &CapabilityRequest) -> Decision;
}

/// Allows everything. Intended for tests and fully trusted embedded hosts.
#[derive(Debug, Default)]
pub struct AllowAllPolicy;

impl CapabilityPolicy for AllowAllPolicy {
    fn decide(&self, _request: &CapabilityRequest) -> Decision {
        Decision::Allow
    }
}

/// Denies everything. Useful as a safe default for untrusted contexts.
#[derive(Debug, Default)]
pub struct DenyAllPolicy;

impl CapabilityPolicy for DenyAllPolicy {
    fn decide(&self, _request: &CapabilityRequest) -> Decision {
        Decision::Deny {
            reason: "no capabilities are permitted in this context".to_string(),
        }
    }
}

/// Allows safe nodes, requires approval for nodes marked dangerous.
///
/// This mirrors the architecture document's rule that destructive capabilities
/// are gated by approval by default.
#[derive(Debug, Default)]
pub struct DefaultPolicy;

impl CapabilityPolicy for DefaultPolicy {
    fn decide(&self, request: &CapabilityRequest) -> Decision {
        if request.dangerous || !request.permissions.is_empty() {
            Decision::RequireApproval {
                reason: format!(
                    "`{}` declares privileged capability {:?}",
                    request.node_type, request.permissions
                ),
            }
        } else {
            Decision::Allow
        }
    }
}

/// Allows an explicit set of capabilities and denies everything else.
#[derive(Debug, Clone)]
pub struct AllowlistPolicy {
    allowed: HashSet<String>,
    require_approval_for_dangerous: bool,
}

impl AllowlistPolicy {
    /// Build an allowlist from any iterable of capability identifiers.
    pub fn new<I, S>(allowed: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed: allowed.into_iter().map(Into::into).collect(),
            require_approval_for_dangerous: true,
        }
    }

    /// Control whether dangerous-but-allowed nodes still need approval.
    #[must_use]
    pub fn with_dangerous_approval(mut self, required: bool) -> Self {
        self.require_approval_for_dangerous = required;
        self
    }
}

impl CapabilityPolicy for AllowlistPolicy {
    fn decide(&self, request: &CapabilityRequest) -> Decision {
        let listed = self.allowed.contains(&request.capability)
            || request
                .permissions
                .iter()
                .any(|permission| self.allowed.contains(permission));
        if !listed {
            return Decision::Deny {
                reason: format!(
                    "capability `{}` is not on the allowlist",
                    request.capability
                ),
            };
        }
        if request.dangerous && self.require_approval_for_dangerous {
            Decision::RequireApproval {
                reason: format!("capability `{}` is dangerous", request.capability),
            }
        } else {
            Decision::Allow
        }
    }
}

/// Combines several policies with deny-wins, then approval, then allow.
#[derive(Default)]
pub struct PolicyChain {
    policies: Vec<Arc<dyn CapabilityPolicy>>,
}

impl std::fmt::Debug for PolicyChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyChain")
            .field("policies", &self.policies.len())
            .finish()
    }
}

impl PolicyChain {
    /// Create an empty chain (which allows nothing until a policy is added).
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a policy.
    #[must_use]
    pub fn with(mut self, policy: Arc<dyn CapabilityPolicy>) -> Self {
        self.policies.push(policy);
        self
    }
}

impl CapabilityPolicy for PolicyChain {
    fn decide(&self, request: &CapabilityRequest) -> Decision {
        if self.policies.is_empty() {
            return Decision::Deny {
                reason: "policy chain is empty".to_string(),
            };
        }
        let mut approval: Option<String> = None;
        for policy in &self.policies {
            match policy.decide(request) {
                Decision::Deny { reason } => return Decision::Deny { reason },
                Decision::RequireApproval { reason } => {
                    approval.get_or_insert(reason);
                }
                Decision::Allow => {}
            }
        }
        match approval {
            Some(reason) => Decision::RequireApproval { reason },
            None => Decision::Allow,
        }
    }
}

/// Answers an approval request when policy asks for one.
pub trait ApprovalHandler: Send + Sync {
    /// Return `true` to permit the call.
    fn approve(&self, request: &CapabilityRequest) -> bool;
}

/// Approves every request. This is the documented "phase one" default where
/// autonomous runs are permitted, but every decision is still audited.
#[derive(Debug, Default)]
pub struct AutoApprove;

impl ApprovalHandler for AutoApprove {
    fn approve(&self, _request: &CapabilityRequest) -> bool {
        true
    }
}

/// Refuses every approval request. Useful for interactive hosts that have not
/// wired up a consent UI yet.
#[derive(Debug, Default)]
pub struct AutoDeny;

impl ApprovalHandler for AutoDeny {
    fn approve(&self, _request: &CapabilityRequest) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(dangerous: bool, permissions: &[&str]) -> CapabilityRequest {
        CapabilityRequest {
            run_id: "run".into(),
            node_id: "n1".into(),
            node_type: "windows.Input.Keyboard".into(),
            capability: "windows.Input.Keyboard".into(),
            permissions: permissions.iter().map(|s| (*s).to_string()).collect(),
            dangerous,
            input: serde_json::json!({}),
        }
    }

    #[test]
    fn default_policy_gates_dangerous_nodes() {
        assert_eq!(DefaultPolicy.decide(&request(false, &[])), Decision::Allow);
        assert!(matches!(
            DefaultPolicy.decide(&request(true, &["input.control"])),
            Decision::RequireApproval { .. }
        ));
    }

    #[test]
    fn allowlist_denies_unlisted_capabilities() {
        let policy = AllowlistPolicy::new(["core.Log"]);
        assert!(matches!(
            policy.decide(&request(false, &[])),
            Decision::Deny { .. }
        ));
    }

    #[test]
    fn allowlist_can_be_keyed_on_permissions() {
        let policy = AllowlistPolicy::new(["input.control"]).with_dangerous_approval(false);
        assert_eq!(
            policy.decide(&request(true, &["input.control"])),
            Decision::Allow
        );
    }

    #[test]
    fn chain_lets_deny_win() {
        let chain = PolicyChain::new()
            .with(Arc::new(AllowAllPolicy))
            .with(Arc::new(DenyAllPolicy));
        assert!(matches!(
            chain.decide(&request(false, &[])),
            Decision::Deny { .. }
        ));
    }

    #[test]
    fn empty_chain_denies() {
        let chain = PolicyChain::new();
        assert!(matches!(
            chain.decide(&request(false, &[])),
            Decision::Deny { .. }
        ));
    }
}
