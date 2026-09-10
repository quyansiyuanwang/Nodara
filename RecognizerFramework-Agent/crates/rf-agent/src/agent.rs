//! The agent session.
//!
//! It composes the pieces: read the runtime's capabilities, plan a workflow,
//! apply guardrails, validate, and optionally run it and report the outcome.
//! Every step lands in the audit trace.

use std::path::PathBuf;
use std::time::Duration;

use rf_schema::{NodeDescriptor, Workflow};
use serde_json::Value;

use crate::audit::{AuditTrace, TraceEntry, TraceStep};
use crate::error::AgentResult;
use crate::planner::{PlanRequest, Planner};
use crate::policy::{Budget, BudgetTracker, GuardrailPolicy};
use crate::provider::LlmProvider;
use crate::runtime_client::RuntimeClient;

/// Session configuration.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Runtime base URL.
    pub runtime_url: String,
    /// How many repair rounds the planner may use.
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
}

impl AgentOutcome {
    /// True when a run finished successfully.
    pub fn run_succeeded(&self) -> bool {
        self.run
            .as_ref()
            .and_then(|snapshot| snapshot.get("status"))
            .and_then(Value::as_str)
            .is_some_and(|status| status == "completed")
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

        // Capability discovery is the agent's only source of truth about what it
        // can plan with, so it always asks the runtime rather than assuming.
        let descriptors = self.client.node_types()?;
        trace.record(
            TraceStep::Note,
            format!("discovered {} node types", descriptors.len()),
            serde_json::json!({
                "node_types": descriptors.iter().map(|d| &d.node_type).collect::<Vec<_>>()
            }),
        );

        let planner = Planner::new(self.provider, descriptors.clone());
        let outcome = planner.plan(&PlanRequest {
            goal: goal.to_string(),
            constraints: constraints.to_vec(),
            max_repairs: self.config.max_repairs,
        })?;

        budget.charge_step()?;
        for _ in 0..outcome.repairs {
            budget.charge_step()?;
        }
        budget.charge_tokens(outcome.tokens_used)?;

        trace.record(
            TraceStep::ModelCall,
            format!(
                "planned in {} call(s), {} token(s)",
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
            return Ok(AgentOutcome {
                accepted: false,
                workflow: None,
                run: None,
                trace: trace.entries().to_vec(),
                tokens_used: outcome.tokens_used,
            });
        };

        // Guardrails run *after* planning so a refusal is reported with the node
        // that caused it, and *before* running so nothing is executed.
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
                return Err(error);
            }
        }

        let mut run_snapshot = None;
        if (run || self.config.auto_run) && outcome.accepted {
            let started = self
                .client
                .start_run(&workflow, self.config.variables.clone())?;
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
            let finished = self.client.wait_for_run(&run_id, self.config.run_timeout)?;
            trace.record(
                TraceStep::RunFinished,
                format!(
                    "run {run_id} finished as {}",
                    finished
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                ),
                finished.clone(),
            );
            run_snapshot = Some(finished);
        }

        Ok(AgentOutcome {
            accepted: outcome.accepted,
            workflow: Some(workflow),
            run: run_snapshot,
            trace: trace.entries().to_vec(),
            tokens_used: outcome.tokens_used,
        })
    }
}
