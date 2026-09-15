//! The agent session.
//!
//! The loop the architecture document asks for:
//!
//! ```text
//! discover capabilities -> select tools -> plan -> guardrails -> publish preview
//!        ^                                                       |
//!        |                                                       v
//!   re-plan from the failure            run (bound to the session)
//!        ^                                                       |
//!        |                                                       v
//!        +--------- observe events (REST) ---- report ----------+
//! ```
//!
//! Re-planning is bounded by the repair budget. A run that failed for a reason
//! the model can fix — a bad configuration, a node that errored — is fed back as
//! another planning turn. A run the *runtime* refused (policy, cancellation, a
//! plugin crash) is not: no amount of re-planning changes the runtime's answer.

use std::path::PathBuf;
use std::time::Duration;

use nodara_schema::{
    ExecutionEvent, NodeDescriptor, PlanPreview, SessionStatus, ValidationReport, Workflow,
};
use serde_json::Value;

use crate::audit::{AuditTrace, TraceEntry, TraceStep};
use crate::error::{AgentError, AgentResult};
use crate::planner::{PlanRequest, Planner};
use crate::policy::{Budget, BudgetTracker, GuardrailPolicy};
use crate::provider::LlmProvider;
use crate::report::ExecutionReport;
use crate::runtime_client::RuntimeClient;
use crate::selector::ToolSelector;

/// How many run attempts the agent will make before giving up.
const MAX_RUN_ATTEMPTS: u32 = 2;

/// Session configuration.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Runtime base URL.
    pub runtime_url: String,
    /// How many repair rounds each planning attempt may use.
    pub max_repairs: u32,
    /// Session budgets.
    pub budget: Budget,
    /// Local guardrails.
    pub guardrails: GuardrailPolicy,
    /// Where to write the decision trace.
    pub trace_path: Option<PathBuf>,
    /// Start a run once a workflow is accepted.
    pub auto_run: bool,
    /// How long to wait for a run to finish.
    pub run_timeout: Duration,
    /// Variables passed to the run.
    pub variables: Value,
    /// Publish the session and plan preview to the runtime so the Studio can
    /// watch the work.
    pub publish_session: bool,
    /// Plan without contacting the runtime: no capability discovery, no
    /// session publication, structure-only local validation.
    pub offline: bool,
    /// How the tool catalogue is narrowed before prompting.
    pub tool_selector: ToolSelector,
    /// When present, `plan` and `plan_and_run` modify this document instead of
    /// authoring one from nothing.
    pub base_workflow: Option<Workflow>,
    /// Reuse an existing runtime session instead of creating a new one.
    pub session_id: Option<String>,
    /// Per-run capability approval strategy.
    pub approval_mode: RunApprovalMode,
    /// Start an automatic run paused so an operator can review it first.
    pub start_paused: bool,
}

/// How gated capabilities are approved for a run started by the agent.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunApprovalMode {
    /// Approve automatically while recording each decision.
    #[default]
    Auto,
    /// Ask the owning runtime session and wait for an operator.
    Session,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            runtime_url: "http://127.0.0.1:8710".to_string(),
            max_repairs: 3,
            budget: Budget::default(),
            guardrails: GuardrailPolicy::permissive(),
            trace_path: None,
            auto_run: false,
            run_timeout: Duration::from_secs(300),
            variables: Value::Object(serde_json::Map::new()),
            publish_session: true,
            offline: false,
            tool_selector: ToolSelector::default(),
            base_workflow: None,
            session_id: None,
            approval_mode: RunApprovalMode::Auto,
            start_paused: false,
        }
    }
}

/// What to explain.
#[derive(Debug, Clone, Default)]
pub struct ExplainTarget {
    /// A workflow document to account for.
    pub workflow: Option<Workflow>,
    /// A run to account for.
    pub run_id: Option<String>,
}

impl ExplainTarget {
    /// Explain a workflow.
    pub fn workflow(workflow: Workflow) -> Self {
        Self {
            workflow: Some(workflow),
            run_id: None,
        }
    }

    /// Explain a run.
    pub fn run(run_id: impl Into<String>) -> Self {
        Self {
            workflow: None,
            run_id: Some(run_id.into()),
        }
    }

    /// Builder-style run id.
    #[must_use]
    pub fn with_run(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }
}

/// What one agent session produced.
#[derive(Debug, Clone)]
pub struct AgentOutcome {
    /// True when the runtime accepted the workflow.
    pub accepted: bool,
    /// The accepted (or last) workflow document.
    pub workflow: Option<Workflow>,
    /// The run snapshot, when a run was started.
    pub run: Option<Value>,
    /// The decision trace.
    pub trace: Vec<TraceEntry>,
    /// Total tokens consumed.
    pub tokens_used: u64,
    /// Session published to the runtime, when publishing was enabled.
    pub session_id: Option<String>,
    /// The execution report.
    pub report: ExecutionReport,
}

impl AgentOutcome {
    /// True when a run finished successfully.
    pub fn run_succeeded(&self) -> bool {
        self.report.status == "completed"
    }
}

/// One agent, one provider, one runtime.
pub struct Agent<'a> {
    provider: &'a dyn LlmProvider,
    client: RuntimeClient,
    config: AgentConfig,
}

impl std::fmt::Debug for Agent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("provider", &self.provider.name())
            .field("runtime", &self.client.base())
            .finish()
    }
}

impl<'a> Agent<'a> {
    /// Create a session.
    pub fn new(provider: &'a dyn LlmProvider, config: AgentConfig) -> Self {
        Self {
            provider,
            client: RuntimeClient::new(config.runtime_url.clone()),
            config,
        }
    }

    /// The runtime client, for callers that need extra API access.
    pub fn client(&self) -> &RuntimeClient {
        &self.client
    }

    /// Node types the runtime can execute, which is also the tool catalogue.
    pub fn capabilities(&self) -> AgentResult<Vec<NodeDescriptor>> {
        self.client.node_types()
    }

    /// Pause a run this agent started.
    pub fn pause_run(&self, run_id: &str) -> AgentResult<Value> {
        self.client.pause(run_id)
    }

    /// Resume a paused run.
    pub fn resume_run(&self, run_id: &str) -> AgentResult<Value> {
        self.client.resume(run_id)
    }

    /// Stop a run.
    pub fn cancel_run(&self, run_id: &str) -> AgentResult<Value> {
        self.client.cancel(run_id)
    }

    /// Explain a workflow, a run, or both, in prose.
    ///
    /// The plan requires the agent to "解释节点和执行错误". This asks the model to
    /// account for what a workflow does and why a run ended the way it did, with
    /// the runtime's own diagnostics and events as the only evidence.
    pub fn explain(&self, target: &ExplainTarget) -> AgentResult<String> {
        let workflow_json = match &target.workflow {
            Some(workflow) => Some(serde_json::to_string_pretty(workflow)?),
            None => None,
        };

        let diagnostics = match &target.workflow {
            Some(workflow) => self
                .client
                .validate(workflow)
                .ok()
                .and_then(|report| serde_json::to_value(report).ok()),
            None => None,
        };

        let (snapshot, events) = match &target.run_id {
            Some(run_id) => {
                let snapshot = self.client.get_run(run_id).ok();
                let events = self
                    .client
                    .event_log(run_id)
                    .ok()
                    .and_then(|events| serde_json::to_value(events).ok())
                    .unwrap_or(Value::Null);
                (snapshot, events)
            }
            None => (None, Value::Null),
        };

        let prompt = crate::prompt::explain_prompt(
            workflow_json.as_deref(),
            diagnostics.as_ref(),
            snapshot.as_ref(),
            &events,
        );
        let mut request = crate::model::ChatRequest::new(vec![
            crate::model::ChatMessage::system(
                "You explain automation workflows and their runs to the operator.",
            ),
            crate::model::ChatMessage::user(prompt),
        ]);
        // An explanation is prose, not a document.
        request.json_mode = false;
        request.temperature = 0.2;
        Ok(self.provider.complete(&request)?.content)
    }

    /// Plan a workflow without running it.
    pub fn plan(&self, goal: &str, constraints: &[String]) -> AgentResult<AgentOutcome> {
        self.session(goal, constraints, false)
    }

    /// Plan a workflow and run it.
    pub fn plan_and_run(&self, goal: &str, constraints: &[String]) -> AgentResult<AgentOutcome> {
        self.session(goal, constraints, true)
    }

    fn session(&self, goal: &str, constraints: &[String], run: bool) -> AgentResult<AgentOutcome> {
        let mut trace = match &self.config.trace_path {
            Some(path) => AuditTrace::open(path)?,
            None => AuditTrace::in_memory(),
        };
        let mut budget = BudgetTracker::new(self.config.budget.clone());

        trace.record(
            TraceStep::Goal,
            goal.to_string(),
            serde_json::json!({
                "constraints": constraints,
                "runtime": self.client.base(),
                "provider": self.provider.name(),
                "guardrails": self.config.guardrails.tools.describe(),
            }),
        );

        // Publish the session first: the Studio can then watch from the start,
        // and a gated node later has somewhere to ask for approval.
        let (session_id, conversation) = if self.config.offline {
            trace.record(
                TraceStep::Note,
                "offline planning: the runtime is not contacted".to_string(),
                serde_json::json!({ "offline": true }),
            );
            (None, Vec::new())
        } else {
            self.begin_session(goal, &mut trace)
        };

        // Capability discovery is the agent's only source of truth about what it
        // may plan with, so it always asks the runtime rather than assuming.
        // Offline planning deliberately skips it and validates structure only.
        let (descriptors, selected) = if self.config.offline {
            (Vec::new(), Vec::new())
        } else {
            let descriptors = match self.client.node_types() {
                Ok(descriptors) => descriptors,
                Err(error) => return self.fail_session(&session_id, error),
            };
            let selected = self.config.tool_selector.select(goal, &descriptors);
            trace.record(
                TraceStep::Note,
                format!(
                    "selected {} of {} node types",
                    selected.len(),
                    descriptors.len()
                ),
                serde_json::json!({
                    "selected": selected.iter().map(|d| &d.node_type).collect::<Vec<_>>()
                }),
            );
            (descriptors, selected)
        };

        let mut tokens_used = 0u64;
        let mut attempts = 0u32;
        let mut feedback = String::new();

        loop {
            attempts += 1;
            // The planner charges the budget before each model call, so the
            // step cap prevents the round trips it counts instead of
            // reporting overspend afterwards.
            let mut planner =
                Planner::new(self.provider, selected.clone()).with_budget(&mut budget);
            let mut request = PlanRequest {
                goal: goal.to_string(),
                constraints: constraints.to_vec(),
                max_repairs: self.config.max_repairs,
                base: self.config.base_workflow.clone(),
                history: conversation.clone(),
            };
            if !feedback.is_empty() {
                request
                    .constraints
                    .push(format!("A previous attempt failed: {feedback}"));
            }

            let outcome = match planner.plan(&request) {
                Ok(outcome) => outcome,
                Err(error) => return self.fail_session(&session_id, error),
            };
            if let Err(error) = budget.charge_tokens(outcome.tokens_used) {
                return self.fail_session(&session_id, error);
            }
            tokens_used += outcome.tokens_used;

            trace.record(
                TraceStep::ModelCall,
                format!(
                    "attempt {attempts}: planned in {} call(s), {} token(s)",
                    outcome.repairs + 1,
                    outcome.tokens_used
                ),
                serde_json::json!({ "raw": outcome.raw }),
            );
            let report_json = match serde_json::to_value(&outcome.report) {
                Ok(value) => value,
                Err(error) => return self.fail_session(&session_id, error.into()),
            };
            trace.record(
                TraceStep::Validation,
                if outcome.accepted {
                    "runtime accepted the workflow".to_string()
                } else {
                    format!(
                        "runtime rejected the workflow ({} error(s))",
                        outcome.report.error_count()
                    )
                },
                report_json,
            );

            let Some(workflow) = outcome.workflow.clone() else {
                return Ok(self.finish_without_plan(
                    goal,
                    session_id,
                    outcome.accepted,
                    attempts,
                    tokens_used,
                    trace,
                ));
            };

            // Guardrails run after planning so a refusal names the offending
            // node, and before running so nothing is executed.
            match self.config.guardrails.tools.check(&workflow, &descriptors) {
                Ok(()) => trace.record(
                    TraceStep::Guardrail,
                    "guardrails passed",
                    serde_json::json!({ "policy": self.config.guardrails.tools.describe() }),
                ),
                Err(error) => {
                    trace.record(
                        TraceStep::Guardrail,
                        format!("refused: {error}"),
                        serde_json::json!({ "policy": self.config.guardrails.tools.describe() }),
                    );
                    self.finish_session(&session_id, SessionStatus::Failed);
                    return Err(error);
                }
            }

            self.publish_plan(&session_id, &workflow, &outcome.report);

            if !(run || self.config.auto_run) {
                let report = self.build_report(
                    goal,
                    &session_id,
                    &Value::Null,
                    outcome.accepted,
                    attempts,
                    Vec::new(),
                    Vec::new(),
                    tokens_used,
                    trace.entries().to_vec(),
                );
                self.finish_session(&session_id, SessionStatus::Ready);
                return Ok(AgentOutcome {
                    accepted: outcome.accepted,
                    workflow: Some(workflow),
                    run: None,
                    trace: trace.entries().to_vec(),
                    tokens_used,
                    session_id,
                    report,
                });
            }

            match self.run_once(&session_id, &workflow, &mut trace) {
                RunAttempt::Succeeded {
                    snapshot,
                    events,
                    highlights,
                } => {
                    let report = self.build_report(
                        goal,
                        &session_id,
                        &snapshot,
                        outcome.accepted,
                        attempts,
                        events,
                        highlights,
                        tokens_used,
                        trace.entries().to_vec(),
                    );
                    self.finish_session(&session_id, SessionStatus::Completed);
                    return Ok(AgentOutcome {
                        accepted: true,
                        workflow: Some(workflow),
                        run: Some(snapshot),
                        trace: trace.entries().to_vec(),
                        tokens_used,
                        session_id,
                        report,
                    });
                }
                RunAttempt::Replannable {
                    reason,
                    snapshot,
                    events,
                    highlights,
                } => {
                    trace.record(
                        TraceStep::Note,
                        format!("re-planning after failure: {reason}"),
                        snapshot.clone().unwrap_or(Value::Null),
                    );
                    if attempts >= MAX_RUN_ATTEMPTS {
                        let snapshot = snapshot.unwrap_or(Value::Null);
                        let report = self.build_report(
                            goal,
                            &session_id,
                            &snapshot,
                            outcome.accepted,
                            attempts,
                            events,
                            highlights,
                            tokens_used,
                            trace.entries().to_vec(),
                        );
                        self.finish_session(&session_id, SessionStatus::Failed);
                        return Ok(AgentOutcome {
                            accepted: true,
                            workflow: Some(workflow),
                            run: Some(snapshot),
                            trace: trace.entries().to_vec(),
                            tokens_used,
                            session_id,
                            report,
                        });
                    }
                    feedback = reason;
                }
                RunAttempt::Terminal {
                    snapshot,
                    events,
                    highlights,
                } => {
                    let report = self.build_report(
                        goal,
                        &session_id,
                        &snapshot,
                        outcome.accepted,
                        attempts,
                        events,
                        highlights,
                        tokens_used,
                        trace.entries().to_vec(),
                    );
                    let status = match report.status.as_str() {
                        "cancelled" => SessionStatus::Cancelled,
                        "paused" | "running" => SessionStatus::Running,
                        _ => SessionStatus::Failed,
                    };
                    self.finish_session(&session_id, status);
                    return Ok(AgentOutcome {
                        accepted: true,
                        workflow: Some(workflow),
                        run: Some(snapshot),
                        trace: trace.entries().to_vec(),
                        tokens_used,
                        session_id,
                        report,
                    });
                }
            }
        }
    }

    fn finish_without_plan(
        &self,
        goal: &str,
        session_id: Option<String>,
        accepted: bool,
        attempts: u32,
        tokens_used: u64,
        trace: AuditTrace,
    ) -> AgentOutcome {
        self.finish_session(&session_id, SessionStatus::Failed);
        let report = self.build_report(
            goal,
            &session_id,
            &Value::Null,
            accepted,
            attempts,
            Vec::new(),
            Vec::new(),
            tokens_used,
            trace.entries().to_vec(),
        );
        AgentOutcome {
            accepted,
            workflow: None,
            run: None,
            trace: trace.entries().to_vec(),
            tokens_used,
            session_id,
            report,
        }
    }

    fn begin_session(
        &self,
        goal: &str,
        trace: &mut AuditTrace,
    ) -> (Option<String>, Vec<crate::model::ChatMessage>) {
        if !self.config.publish_session {
            return (None, Vec::new());
        }
        if let Some(session_id) = &self.config.session_id {
            let session = match self.client.get_session(session_id) {
                Ok(session) => session,
                Err(error) => {
                    trace.record(
                        TraceStep::Note,
                        format!("session `{session_id}` could not be loaded: {error}"),
                        Value::Null,
                    );
                    return (None, Vec::new());
                }
            };
            let history = session_messages_to_chat(&session.messages);
            let _ =
                self.client
                    .append_message(session_id, nodara_schema::MessageRole::Operator, goal);
            let _ = self
                .client
                .set_session_status(session_id, SessionStatus::Planning);
            trace.record(
                TraceStep::Note,
                format!("continued session {session_id}"),
                serde_json::json!({ "session_id": session_id }),
            );
            return (Some(session_id.clone()), history);
        }

        match self.client.create_session(goal, self.provider.name()) {
            Ok(session) => {
                trace.record(
                    TraceStep::Note,
                    format!("session {} created", session.id),
                    serde_json::json!({ "session_id": session.id }),
                );
                (Some(session.id), Vec::new())
            }
            Err(error) => {
                trace.record(
                    TraceStep::Note,
                    format!("session could not be published: {error}"),
                    Value::Null,
                );
                (None, Vec::new())
            }
        }
    }

    fn publish_plan(
        &self,
        session_id: &Option<String>,
        workflow: &Workflow,
        report: &ValidationReport,
    ) {
        let Some(session_id) = session_id else {
            return;
        };
        let preview = PlanPreview {
            at_ms: nodara_schema::event::now_ms(),
            workflow: workflow.clone(),
            valid: report.is_valid(),
            errors: report.error_count(),
            warnings: report.warning_count(),
            diagnostics: report.diagnostics.clone(),
        };
        let _ = self.client.publish_plan(session_id, &preview);
        let _ = self.client.append_message(
            session_id,
            nodara_schema::MessageRole::Agent,
            &format!(
                "Proposed `{}` with {} node(s) and {} edge(s).",
                workflow.id,
                workflow.nodes.len(),
                workflow.edges.len()
            ),
        );
    }

    fn finish_session(&self, session_id: &Option<String>, status: SessionStatus) {
        if let Some(session_id) = session_id {
            let _ = self.client.set_session_status(session_id, status);
        }
    }

    /// Fail a published session and return the error.
    ///
    /// Every fallible step after [`Self::publish_session`] must go through
    /// here: a bare `?` would leave the session stuck in `planning` forever,
    /// and the Studio would keep showing it as in progress.
    fn fail_session<T>(&self, session_id: &Option<String>, error: AgentError) -> AgentResult<T> {
        self.finish_session(session_id, SessionStatus::Failed);
        Err(error)
    }

    /// Start the run, then watch it to completion.
    fn run_once(
        &self,
        session_id: &Option<String>,
        workflow: &Workflow,
        trace: &mut AuditTrace,
    ) -> RunAttempt {
        let started = self.client.start_run_with_options(
            workflow,
            self.config.variables.clone(),
            session_id.as_deref(),
            self.config.start_paused,
            self.config.approval_mode,
        );
        let started = match started {
            Ok(started) => started,
            Err(error) => {
                trace.record(
                    TraceStep::Note,
                    format!("the runtime refused to start the run: {error}"),
                    Value::Null,
                );
                return RunAttempt::Terminal {
                    snapshot: Value::Null,
                    events: Vec::new(),
                    highlights: Vec::new(),
                };
            }
        };
        let run_id = started
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        trace.record(
            TraceStep::RunStarted,
            format!("run {run_id} accepted"),
            started.clone(),
        );
        if let Some(session_id) = session_id {
            let _ = self.client.append_message(
                session_id,
                nodara_schema::MessageRole::Agent,
                &format!("Running `{run_id}`."),
            );
        }

        if self.config.start_paused
            && started.get("status").and_then(Value::as_str) == Some("paused")
        {
            if let Some(session_id) = session_id {
                let _ = self.client.append_message(
                    session_id,
                    nodara_schema::MessageRole::Runtime,
                    &format!("Run `{run_id}` is paused and ready for operator resume."),
                );
            }
            return RunAttempt::Terminal {
                snapshot: started,
                events: Vec::new(),
                highlights: vec![format!("Run {run_id} is paused")],
            };
        }

        let snapshot = match self.client.wait_for_run(&run_id, self.config.run_timeout) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                trace.record(
                    TraceStep::Note,
                    format!("lost contact with run {run_id}: {error}"),
                    Value::Null,
                );
                // Contact is gone but the run may still be executing: cancel
                // it best-effort so it cannot run on unattended.
                let _ = self.client.cancel(&run_id);
                return RunAttempt::Terminal {
                    snapshot: Value::Null,
                    events: Vec::new(),
                    highlights: Vec::new(),
                };
            }
        };

        // Observe the run. The agent reads the same event sequence the Studio
        // streams, so both see one truth.
        let events = self.client.event_log(&run_id).unwrap_or_default();
        let highlights = summarise(&events);
        let status = snapshot
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        trace.record(
            TraceStep::RunFinished,
            format!("run {run_id} finished as {status}"),
            snapshot.clone(),
        );
        if let Some(session_id) = session_id {
            let _ = self.client.append_message(
                session_id,
                nodara_schema::MessageRole::Runtime,
                &format!(
                    "Run finished as {status} after {} node(s).",
                    snapshot
                        .get("nodes_executed")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                ),
            );
        }

        if status == "completed" {
            return RunAttempt::Succeeded {
                snapshot,
                events,
                highlights,
            };
        }

        let code = snapshot
            .get("failure")
            .and_then(|failure| failure.get("code"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let message = snapshot
            .get("failure")
            .and_then(|failure| failure.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("no message")
            .to_string();

        // Only a failure the model could plausibly author its way out of is
        // replanned. Policy refusals, cancellations and plugin crashes are the
        // runtime's answer, and re-planning would not change it.
        if status == "failed" && matches!(code, "E_INVALID_CONFIG" | "E_EXECUTION") {
            RunAttempt::Replannable {
                reason: format!("run failed: [{code}] {message}"),
                snapshot: Some(snapshot),
                events,
                highlights,
            }
        } else {
            RunAttempt::Terminal {
                snapshot,
                events,
                highlights,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_report(
        &self,
        goal: &str,
        session_id: &Option<String>,
        snapshot: &Value,
        accepted: bool,
        attempts: u32,
        events: Vec<nodara_schema::EventEnvelope>,
        highlights: Vec<String>,
        tokens_used: u64,
        trace: Vec<TraceEntry>,
    ) -> ExecutionReport {
        let status = snapshot
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(if accepted { "planned" } else { "rejected" })
            .to_string();
        let duration_ms = snapshot
            .get("finished_at_ms")
            .and_then(Value::as_u64)
            .zip(snapshot.get("started_at_ms").and_then(Value::as_u64))
            .map_or(0, |(finished, started)| finished.saturating_sub(started));
        ExecutionReport {
            goal: goal.to_string(),
            session_id: session_id.clone(),
            run_id: snapshot
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string),
            accepted,
            attempts,
            status,
            nodes_executed: snapshot
                .get("nodes_executed")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            duration_ms,
            variables: snapshot
                .get("variables")
                .and_then(Value::as_object)
                .map(|object| object.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
            failure: snapshot.get("failure").cloned(),
            events_observed: events.len(),
            highlights,
            tokens_used,
            trace,
        }
    }
}

/// Classification of one run attempt.
enum RunAttempt {
    /// The run finished successfully.
    Succeeded {
        snapshot: Value,
        events: Vec<nodara_schema::EventEnvelope>,
        highlights: Vec<String>,
    },
    /// The run failed in a way the model might be able to fix.
    Replannable {
        reason: String,
        snapshot: Option<Value>,
        events: Vec<nodara_schema::EventEnvelope>,
        highlights: Vec<String>,
    },
    /// The run failed for a reason re-planning cannot change.
    Terminal {
        snapshot: Value,
        events: Vec<nodara_schema::EventEnvelope>,
        highlights: Vec<String>,
    },
}

/// Turn an event sequence into the handful of lines worth reporting.
fn summarise(events: &[nodara_schema::EventEnvelope]) -> Vec<String> {
    events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            ExecutionEvent::Log { message, .. } => Some(message.clone()),
            ExecutionEvent::NodeFailed {
                node_id,
                code,
                message,
                ..
            } => Some(format!("{node_id} failed [{code}]: {message}")),
            ExecutionEvent::CapabilityDecision {
                capability,
                decision,
                ..
            } if decision != "allow" => Some(format!("policy {capability}: {decision}")),
            _ => None,
        })
        .collect()
}

fn session_messages_to_chat(
    messages: &[nodara_schema::SessionMessage],
) -> Vec<crate::model::ChatMessage> {
    messages
        .iter()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| {
            let content = match message.role {
                nodara_schema::MessageRole::Operator => message.text.clone(),
                nodara_schema::MessageRole::Agent => message.text.clone(),
                nodara_schema::MessageRole::Runtime => format!("[runtime] {}", message.text),
            };
            match message.role {
                nodara_schema::MessageRole::Operator => crate::model::ChatMessage::user(content),
                nodara_schema::MessageRole::Agent | nodara_schema::MessageRole::Runtime => {
                    crate::model::ChatMessage::assistant(content)
                }
            }
        })
        .collect()
}
