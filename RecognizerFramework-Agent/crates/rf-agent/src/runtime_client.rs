//! A thin client for the runtime's public API.
//!
//! The agent talks to the runtime exactly as the Studio does. It has no access
//! to the engine, so it cannot execute anything without the runtime's policy
//! layer seeing it first.

use std::time::Duration;

use rf_schema::{
    AgentSession, ApprovalRequest, EventEnvelope, MessageRole, NodeDescriptor, PlanPreview,
    SessionStatus, ValidationReport, Workflow,
};
use serde_json::Value;

use crate::error::{AgentError, AgentResult};

/// A REST client for one runtime instance.
#[derive(Debug, Clone)]
pub struct RuntimeClient {
    base: String,
    timeout: Duration,
}

impl RuntimeClient {
    /// Create a client for `base`, e.g. `http://127.0.0.1:8710`.
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            timeout: Duration::from_secs(60),
        }
    }

    /// Read the base URL from `RF_RUNTIME_URL`, defaulting to localhost.
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("RF_RUNTIME_URL").unwrap_or_else(|_| "http://127.0.0.1:8710".to_string()),
        )
    }

    /// The configured base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    fn get(&self, path: &str) -> AgentResult<Value> {
        let response = ureq::get(&format!("{}{path}", self.base))
            .timeout(self.timeout)
            .call();
        Self::decode(response)
    }

    fn post(&self, path: &str, body: Value) -> AgentResult<Value> {
        let response = ureq::post(&format!("{}{path}", self.base))
            .timeout(self.timeout)
            .set("content-type", "application/json")
            .send_json(body);
        Self::decode(response)
    }

    fn decode(response: Result<ureq::Response, ureq::Error>) -> AgentResult<Value> {
        match response {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|error| AgentError::Transport(error.to_string())),
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_json::<Value>().unwrap_or(Value::Null);
                let message = body
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("runtime returned an error")
                    .to_string();
                Err(AgentError::Runtime { status, message })
            }
            Err(error) => Err(AgentError::Transport(error.to_string())),
        }
    }

    /// Liveness and capability counts.
    pub fn health(&self) -> AgentResult<Value> {
        self.get("/api/v1/health")
    }

    /// Every node type the runtime can execute.
    pub fn node_types(&self) -> AgentResult<Vec<NodeDescriptor>> {
        let payload = self.get("/api/v1/node-types")?;
        let descriptors = payload
            .get("node_types")
            .ok_or_else(|| AgentError::Transport("missing `node_types` in response".to_string()))?;
        Ok(serde_json::from_value(descriptors.clone())?)
    }

    /// Installed plugins.
    pub fn plugins(&self) -> AgentResult<Value> {
        self.get("/api/v1/plugins")
    }

    /// Validate a workflow document.
    pub fn validate(&self, workflow: &Workflow) -> AgentResult<ValidationReport> {
        let payload = self.post(
            "/api/v1/workflows/validate",
            serde_json::json!({ "workflow": workflow }),
        )?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Start a run.
    pub fn start_run(
        &self,
        workflow: &Workflow,
        variables: serde_json::Value,
    ) -> AgentResult<Value> {
        self.post(
            "/api/v1/runs",
            serde_json::json!({ "workflow": workflow, "variables": variables }),
        )
    }

    /// Read a run snapshot.
    pub fn get_run(&self, run_id: &str) -> AgentResult<Value> {
        self.get(&format!("/api/v1/runs/{run_id}"))
    }

    /// The buffered event sequence for a run.
    ///
    /// The Studio streams over the WebSocket; the agent reads the same sequence
    /// here. Observing a run therefore does not require carrying a WebSocket
    /// stack in a command-line tool whose other calls are already REST.
    pub fn event_log(&self, run_id: &str) -> AgentResult<Vec<EventEnvelope>> {
        let payload = self.get(&format!("/api/v1/runs/{run_id}/event-log"))?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Create an agent session so the Studio can watch the work.
    pub fn create_session(&self, goal: &str, provider: &str) -> AgentResult<AgentSession> {
        let payload = self.post(
            "/api/v1/agent/sessions",
            serde_json::json!({ "goal": goal, "provider": provider }),
        )?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Read a session.
    pub fn get_session(&self, session_id: &str) -> AgentResult<AgentSession> {
        let payload = self.get(&format!("/api/v1/agent/sessions/{session_id}"))?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Append a conversation turn.
    pub fn append_message(
        &self,
        session_id: &str,
        role: MessageRole,
        text: &str,
    ) -> AgentResult<AgentSession> {
        let payload = self.post(
            &format!("/api/v1/agent/sessions/{session_id}/messages"),
            serde_json::json!({ "role": role, "text": text }),
        )?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Publish the plan preview the Studio renders.
    pub fn publish_plan(
        &self,
        session_id: &str,
        preview: &PlanPreview,
    ) -> AgentResult<AgentSession> {
        let payload = self.post(
            &format!("/api/v1/agent/sessions/{session_id}/plan"),
            serde_json::to_value(preview)?,
        )?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Mark a session finished, failed or cancelled.
    pub fn set_session_status(
        &self,
        session_id: &str,
        status: SessionStatus,
    ) -> AgentResult<AgentSession> {
        let payload = self.post(
            &format!("/api/v1/agent/sessions/{session_id}/status"),
            serde_json::json!({ "status": status }),
        )?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Approvals waiting for an operator, across every session.
    pub fn pending_approvals(&self) -> AgentResult<Vec<ApprovalRequest>> {
        let payload = self.get("/api/v1/agent/approvals")?;
        let approvals = payload.as_array().cloned().unwrap_or_default();
        Ok(approvals
            .into_iter()
            .filter_map(|entry| serde_json::from_value(entry.get("approval")?.clone()).ok())
            .collect())
    }

    /// Record an operator decision on a pending approval.
    pub fn decide_approval(
        &self,
        session_id: &str,
        approval_id: &str,
        decision: rf_schema::ApprovalDecision,
        decided_by: &str,
    ) -> AgentResult<AgentSession> {
        let payload = self.post(
            &format!("/api/v1/agent/sessions/{session_id}/approvals/{approval_id}"),
            serde_json::json!({ "decision": decision, "decided_by": decided_by }),
        )?;
        Ok(serde_json::from_value(payload)?)
    }

    /// Every session the runtime knows about, with the raw pending approvals.
    pub fn sessions(&self) -> AgentResult<Value> {
        self.get("/api/v1/agent/sessions")
    }

    /// The audit log, optionally narrowed to one run.
    pub fn audit(&self, run_id: Option<&str>, limit: Option<usize>) -> AgentResult<Vec<Value>> {
        let mut query = Vec::new();
        if let Some(run_id) = run_id {
            query.push(format!("run_id={run_id}"));
        }
        if let Some(limit) = limit {
            query.push(format!("limit={limit}"));
        }
        let suffix = if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query.join("&"))
        };
        let payload = self.get(&format!("/api/v1/audit{suffix}"))?;
        Ok(payload.as_array().cloned().unwrap_or_default())
    }

    /// Start a run bound to a session, so gated nodes can ask for approval.
    pub fn start_run_for_session(
        &self,
        session_id: &str,
        workflow: &Workflow,
        variables: serde_json::Value,
    ) -> AgentResult<Value> {
        self.post(
            "/api/v1/runs",
            serde_json::json!({
                "workflow": workflow,
                "variables": variables,
                "session_id": session_id,
            }),
        )
    }

    /// Pause a run.
    pub fn pause(&self, run_id: &str) -> AgentResult<Value> {
        self.post(
            &format!("/api/v1/runs/{run_id}/pause"),
            serde_json::json!({}),
        )
    }

    /// Resume a run.
    pub fn resume(&self, run_id: &str) -> AgentResult<Value> {
        self.post(
            &format!("/api/v1/runs/{run_id}/resume"),
            serde_json::json!({}),
        )
    }

    /// Cancel a run.
    pub fn cancel(&self, run_id: &str) -> AgentResult<Value> {
        self.post(
            &format!("/api/v1/runs/{run_id}/cancel"),
            serde_json::json!({}),
        )
    }

    /// Poll a run until it reaches a terminal state, or the deadline passes.
    pub fn wait_for_run(&self, run_id: &str, timeout: Duration) -> AgentResult<Value> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let snapshot = self.get_run(run_id)?;
            let status = snapshot
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if matches!(status, "completed" | "failed" | "cancelled") {
                return Ok(snapshot);
            }
            if std::time::Instant::now() >= deadline {
                return Ok(snapshot);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
