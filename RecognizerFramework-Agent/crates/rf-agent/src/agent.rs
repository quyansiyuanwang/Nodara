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

use rf_schema::{
    ExecutionEvent, NodeDescriptor, PlanPreview, SessionStatus, ValidationReport, Workflow,
};
use serde_json::Value;

use crate::audit::{AuditTrace, TraceEntry, TraceStep};
use crate::error::AgentResult;
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
    /// How the tool catalogue is narrowed before prompting.
    pub tool_selector: ToolSelector,
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
            tool_selector: ToolSelector::default(),
        }
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
        let session_id = self.publish_session(goal, &mut trace);

        // Capability discovery is the agent's only source of truth about what it
        // may plan with, so it always asks the runtime rather than assuming.
        let descriptors = self.client.node_types()?;
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

        let mut tokens_used = 0u64;
        let mut attempts = 0u32;
        let mut feedback = String::new();

        loop {
            attempts += 1;
            let planner = Planner::new(self.provider, selected.clone());
            let mut request = PlanRequest {
                goal: goal.to_string(),
                constraints: constraints.to_vec(),
                max_repairs: self.config.max_repairs,
            };
            if !feedback.is_empty() {
                request
                    .constraints
                    .push(format!("A previous attempt failed: {feedback}"));
            }

            let outcome = planner.plan(&request)?;
            budget.charge_step()?;
            for _ in 0..outcome.repairs {
                budget.charge_step()?;
            }
            budget.charge_tokens(outcome.tokens_used)?;
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
                serde_json::to_value(&outcome.report)?,
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
                    let status = if report.status == "cancelled" {
                        SessionStatus::Cancelled
                    } else {
                        SessionStatus::Failed
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

    fn publish_session(&self, goal: &str, trace: &mut AuditTrace) -> Option<String> {
        if !self.config.publish_session {
            return None;
        }
        match self.client.create_session(goal, self.provider.name()) {
            Ok(session) => {
                trace.record(
                    TraceStep::Note,
                    format!("session {} created", session.id),
                    serde_json::json!({ "session_id": session.id }),
                );
                Some(session.id)
            }
            Err(error) => {
                trace.record(
                    TraceStep::Note,
                    format!("session could not be published: {error}"),
                    Value::Null,
                );
                None
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
            at_ms: rf_schema::event::now_ms(),
            workflow: workflow.clone(),
            valid: report.is_valid(),
            errors: report.error_count(),
            warnings: report.warning_count(),
            diagnostics: report.diagnostics.clone(),
        };
        let _ = self.client.publish_plan(session_id, &preview);
        let _ = self.client.append_message(
            session_id,
            rf_schema::MessageRole::Agent,
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

    /// Start the run, then watch it to completion.
    fn run_once(
        &self,
        session_id: &Option<String>,
        workflow: &Workflow,
        trace: &mut AuditTrace,
    ) -> RunAttempt {
        let started = match session_id {
            Some(session_id) => self.client.start_run_for_session(
                session_id,
                workflow,
                self.config.variables.clone(),
            ),
            None => self
                .client
                .start_run(workflow, self.config.variables.clone()),
        };
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
            started,
        );
        if let Some(session_id) = session_id {
            let _ = self.client.append_message(
                session_id,
                rf_schema::MessageRole::Agent,
                &format!("Running `{run_id}`."),
            );
        }

        let snapshot = match self.client.wait_for_run(&run_id, self.config.run_timeout) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                trace.record(
                    TraceStep::Note,
                    format!("lost contact with run {run_id}: {error}"),
                    Value::Null,
                );
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
                rf_schema::MessageRole::Runtime,
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
        events: Vec<rf_schema::EventEnvelope>,
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
        events: Vec<rf_schema::EventEnvelope>,
        highlights: Vec<String>,
    },
    /// The run failed in a way the model might be able to fix.
    Replannable {
        reason: String,
        snapshot: Option<Value>,
        events: Vec<rf_schema::EventEnvelope>,
        highlights: Vec<String>,
    },
    /// The run failed for a reason re-planning cannot change.
    Terminal {
        snapshot: Value,
        events: Vec<rf_schema::EventEnvelope>,
        highlights: Vec<String>,
    },
}

/// Turn an event sequence into the handful of lines worth reporting.
fn summarise(events: &[rf_schema::EventEnvelope]) -> Vec<String> {
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
