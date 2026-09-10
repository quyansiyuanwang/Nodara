//! Local guardrails and budgets.
//!
//! These are the agent's *own* limits, and they are deliberately not the last
//! line of defence. The runtime enforces capability policy independently; these
//! checks exist so the agent refuses early, with a clear reason, instead of
//! generating a workflow that is doomed to be blocked node by node.

use std::collections::HashSet;
use std::time::Duration;

use rf_schema::{NodeDescriptor, Workflow};

use crate::error::{AgentError, AgentResult};

/// Resource limits for one agent session.
#[derive(Debug, Clone)]
pub struct Budget {
    /// Maximum model round trips (drafts plus repairs).
    pub max_steps: u32,
    /// Maximum total tokens across all calls.
    pub max_tokens: u64,
    /// Maximum wall-clock time for the whole session.
    pub max_wall_time: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_steps: 8,
            max_tokens: 100_000,
            max_wall_time: Duration::from_secs(600),
        }
    }
}

/// Tracks consumption against a [`Budget`].
#[derive(Debug)]
pub struct BudgetTracker {
    budget: Budget,
    steps: u32,
    tokens: u64,
    started: std::time::Instant,
}

impl BudgetTracker {
    /// Start tracking.
    pub fn new(budget: Budget) -> Self {
        Self {
            budget,
            steps: 0,
            tokens: 0,
            started: std::time::Instant::now(),
        }
    }

    /// The configured budget.
    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    /// Steps consumed so far.
    pub fn steps(&self) -> u32 {
        self.steps
    }

    /// Tokens consumed so far.
    pub fn tokens(&self) -> u64 {
        self.tokens
    }

    /// Charge one model call, refusing when the budget is gone.
    pub fn charge_step(&mut self) -> AgentResult<()> {
        self.steps += 1;
        if self.steps > self.budget.max_steps {
            return Err(AgentError::BudgetExhausted(format!(
                "used {} of {} model steps",
                self.steps, self.budget.max_steps
            )));
        }
        self.check_time()
    }

    /// Charge token usage.
    pub fn charge_tokens(&mut self, tokens: u64) -> AgentResult<()> {
        self.tokens += tokens;
        if self.tokens > self.budget.max_tokens {
            return Err(AgentError::BudgetExhausted(format!(
                "used {} of {} tokens",
                self.tokens, self.budget.max_tokens
            )));
        }
        self.check_time()
    }

    /// Fail when the wall-clock budget is gone.
    pub fn check_time(&self) -> AgentResult<()> {
        if self.started.elapsed() > self.budget.max_wall_time {
            return Err(AgentError::BudgetExhausted(format!(
                "session exceeded {:?}",
                self.budget.max_wall_time
            )));
        }
        Ok(())
    }
}

/// Which node types the agent may put into a workflow.
#[derive(Debug, Clone, Default)]
pub struct ToolPolicy {
    /// When set, only these node types may appear.
    pub allowed_node_types: Option<Vec<String>>,
    /// Refuse any node whose descriptor is marked dangerous.
    pub deny_dangerous: bool,
    /// Refuse any node that declares permissions.
    pub deny_privileged: bool,
}

impl ToolPolicy {
    /// Allow everything the runtime exposes.
    pub fn unrestricted() -> Self {
        Self::default()
    }

    /// Restrict the agent to an explicit capability set.
    pub fn allow_only<I, S>(node_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_node_types: Some(node_types.into_iter().map(Into::into).collect()),
            deny_dangerous: false,
            deny_privileged: false,
        }
    }

    /// Refuse side-effecting node types entirely.
    #[must_use]
    pub fn refusing_dangerous(mut self) -> Self {
        self.deny_dangerous = true;
        self
    }

    /// Refuse node types that declare any permission.
    #[must_use]
    pub fn refusing_privileged(mut self) -> Self {
        self.deny_privileged = true;
        self
    }

    /// Check a proposed workflow before it is sent to the runtime.
    pub fn check(&self, workflow: &Workflow, descriptors: &[NodeDescriptor]) -> AgentResult<()> {
        let allowed: Option<HashSet<&str>> = self
            .allowed_node_types
            .as_ref()
            .map(|list| list.iter().map(String::as_str).collect());

        for node in &workflow.nodes {
            if let Some(allowed) = &allowed {
                if !allowed.contains(node.node_type.as_str()) {
                    return Err(AgentError::Refused(format!(
                        "node `{}` uses `{}`, which is not on the agent allowlist",
                        node.id, node.node_type
                    )));
                }
            }
            let Some(descriptor) = descriptors
                .iter()
                .find(|descriptor| descriptor.node_type == node.node_type)
            else {
                // Unknown types are the runtime's job to reject, with a better
                // message than we could produce here.
                continue;
            };
            if self.deny_dangerous && descriptor.dangerous {
                return Err(AgentError::Refused(format!(
                    "node `{}` is `{}`, which this agent is not permitted to use",
                    node.id, node.node_type
                )));
            }
            if self.deny_privileged && !descriptor.permissions.is_empty() {
                return Err(AgentError::Refused(format!(
                    "node `{}` requires {:?}, which this agent may not request",
                    node.id, descriptor.permissions
                )));
            }
        }
        Ok(())
    }

    /// A human-readable summary for the audit trace.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(allowed) = &self.allowed_node_types {
            parts.push(format!("allowlist of {} node type(s)", allowed.len()));
        } else {
            parts.push("no node-type allowlist".to_string());
        }
        if self.deny_dangerous {
            parts.push("dangerous nodes refused".to_string());
        }
        if self.deny_privileged {
            parts.push("privileged nodes refused".to_string());
        }
        parts.join("; ")
    }
}

/// Guardrails applied to a whole session.
#[derive(Debug, Clone, Default)]
pub struct GuardrailPolicy {
    /// Which node types may be used.
    pub tools: ToolPolicy,
}

impl GuardrailPolicy {
    /// Allow everything.
    pub fn permissive() -> Self {
        Self::default()
    }

    /// A restrictive default: no dangerous nodes, no privileged nodes.
    pub fn safe() -> Self {
        Self {
            tools: ToolPolicy::default()
                .refusing_dangerous()
                .refusing_privileged(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_schema::Node;

    fn descriptor(node_type: &str, dangerous: bool, permissions: &[&str]) -> NodeDescriptor {
        NodeDescriptor {
            dangerous,
            permissions: permissions
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            ..NodeDescriptor::new(node_type, node_type, "Test")
        }
    }

    fn workflow_with(node_type: &str) -> Workflow {
        let mut workflow = Workflow::new("wf");
        workflow.add_node(Node::new("start", "core.Start"));
        workflow.add_node(Node::new("target", node_type));
        workflow
    }

    #[test]
    fn allowlist_refuses_unlisted_nodes() {
        let policy = ToolPolicy::allow_only(["core.Start", "core.Log"]);
        let workflow = workflow_with("windows.Input.Keyboard");
        assert!(policy
            .check(
                &workflow,
                &[descriptor(
                    "windows.Input.Keyboard",
                    true,
                    &["input.control"]
                )]
            )
            .is_err());
    }

    #[test]
    fn safe_policy_refuses_dangerous_nodes() {
        let policy = GuardrailPolicy::safe();
        let workflow = workflow_with("windows.Input.Keyboard");
        let descriptors = vec![descriptor(
            "windows.Input.Keyboard",
            true,
            &["input.control"],
        )];
        assert!(policy.tools.check(&workflow, &descriptors).is_err());
    }

    #[test]
    fn permissive_policy_accepts_safe_nodes() {
        let policy = GuardrailPolicy::permissive();
        let workflow = workflow_with("core.Log");
        let descriptors = vec![descriptor("core.Log", false, &[])];
        assert!(policy.tools.check(&workflow, &descriptors).is_ok());
    }

    #[test]
    fn budget_refuses_after_the_step_limit() {
        let mut tracker = BudgetTracker::new(Budget {
            max_steps: 2,
            ..Budget::default()
        });
        assert!(tracker.charge_step().is_ok());
        assert!(tracker.charge_step().is_ok());
        assert!(matches!(
            tracker.charge_step(),
            Err(AgentError::BudgetExhausted(_))
        ));
    }

    #[test]
    fn budget_refuses_after_the_token_limit() {
        let mut tracker = BudgetTracker::new(Budget {
            max_tokens: 100,
            ..Budget::default()
        });
        assert!(tracker.charge_tokens(60).is_ok());
        assert!(matches!(
            tracker.charge_tokens(60),
            Err(AgentError::BudgetExhausted(_))
        ));
    }
}
