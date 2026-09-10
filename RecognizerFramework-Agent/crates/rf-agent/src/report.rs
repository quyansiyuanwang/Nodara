//! The execution report.
//!
//! The plan requires the agent to "生成执行报告". A report is not a log dump: it
//! states what was asked for, what was planned, what actually ran, and what the
//! operator should look at — in enough detail to paste into a ticket.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audit::TraceEntry;

/// What one agent session did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    /// The operator's goal.
    pub goal: String,
    /// Session this ran under, when the runtime was reachable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Run that was started, when one was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// True when the runtime accepted the workflow.
    pub accepted: bool,
    /// How many plan attempts were needed (1 means the first draft passed).
    pub attempts: u32,
    /// Terminal run status, or `planned` when nothing ran.
    pub status: String,
    /// Nodes executed.
    pub nodes_executed: usize,
    /// Wall-clock duration of the run.
    pub duration_ms: u64,
    /// Final variable scope.
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
    /// Failure detail, when the run did not succeed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<Value>,
    /// Number of events observed for the run.
    pub events_observed: usize,
    /// Log lines worth surfacing, newest last.
    #[serde(default)]
    pub highlights: Vec<String>,
    /// Tokens the agent spent.
    pub tokens_used: u64,
    /// The decision trace, so the report is self-contained for replay.
    #[serde(default)]
    pub trace: Vec<TraceEntry>,
}

impl ExecutionReport {
    /// Render as Markdown, for a ticket or a chat message.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Agent report\n\n");
        out.push_str(&format!("**Goal.** {}\n\n", self.goal));
        if self.accepted {
            out.push_str(&format!(
                "**Status.** {} (planned in {} attempt(s))\n\n",
                self.status, self.attempts
            ));
        } else {
            out.push_str(&format!(
                "**Status.** {} — the runtime did not accept a plan\n\n",
                self.status
            ));
        }

        if let Some(session_id) = &self.session_id {
            out.push_str(&format!("- session: `{session_id}`\n"));
        }
        if let Some(run_id) = &self.run_id {
            out.push_str(&format!(
                "- run: `{run_id}` — {} node(s) in {}ms, {} event(s)\n",
                self.nodes_executed, self.duration_ms, self.events_observed
            ));
        }
        out.push_str(&format!("- tokens: {}\n\n", self.tokens_used));

        if let Some(failure) = &self.failure {
            let code = failure
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let message = failure
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no message");
            out.push_str("## Failure\n\n");
            out.push_str(&format!("`{code}` — {message}\n\n"));
        }

        if !self.highlights.is_empty() {
            out.push_str("## Log\n\n");
            for line in &self.highlights {
                out.push_str(&format!("- {line}\n"));
            }
            out.push('\n');
        }

        if !self.variables.is_empty() {
            out.push_str("## Resulting variables\n\n");
            for (name, value) in &self.variables {
                out.push_str(&format!("- `{name}` = `{value}`\n"));
            }
            out.push('\n');
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(status: &str, failure: Option<Value>) -> ExecutionReport {
        ExecutionReport {
            goal: "log hello".into(),
            session_id: Some("s1".into()),
            run_id: Some("r1".into()),
            accepted: true,
            attempts: 2,
            status: status.into(),
            nodes_executed: 3,
            duration_ms: 12,
            variables: BTreeMap::from([("answer".to_string(), serde_json::json!(42))]),
            failure,
            events_observed: 9,
            highlights: vec!["hello".into()],
            tokens_used: 120,
            trace: Vec::new(),
        }
    }

    #[test]
    fn renders_a_readable_report() {
        let markdown = report("completed", None).to_markdown();
        assert!(markdown.contains("**Status.** completed (planned in 2 attempt(s))"));
        assert!(markdown.contains("`answer` = `42`"));
        assert!(markdown.contains("- hello"));
        assert!(markdown.contains("session: `s1`"));
    }

    #[test]
    fn a_failure_is_called_out() {
        let markdown = report(
            "failed",
            Some(serde_json::json!({
                "code": "E_PERMISSION_DENIED",
                "message": "refused"
            })),
        )
        .to_markdown();
        assert!(markdown.contains("`E_PERMISSION_DENIED` — refused"));
    }

    #[test]
    fn a_rejected_plan_says_so() {
        let mut report = report("failed", None);
        report.accepted = false;
        report.run_id = None;
        let markdown = report.to_markdown();
        assert!(markdown.contains("did not accept a plan"));
    }
}
