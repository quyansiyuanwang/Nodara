//! Natural language to workflow, with repair.
//!
//! The planner is a *loop*, not a single call: it drafts, validates against the
//! real runtime, feeds the diagnostics back, and retries until the runtime
//! accepts the document or the attempt budget runs out. Because validation is
//! done by the runtime rather than by the planner, the agent cannot talk itself
//! into a workflow that the runtime would reject.

use nodara_schema::{validate_with, NodeDescriptor, ValidationReport, Workflow};
use serde_json::Value;

use crate::error::AgentResult;
use crate::model::{ChatMessage, ChatRequest};
use crate::policy::BudgetTracker;
use crate::prompt::{modify_prompt, repair_prompt, system_prompt, user_prompt};
use crate::provider::LlmProvider;

/// Everything needed to plan one workflow.
#[derive(Debug, Clone)]
pub struct PlanRequest {
    /// What the operator wants.
    pub goal: String,
    /// Extra constraints, appended to the user turn.
    pub constraints: Vec<String>,
    /// How many repair attempts to allow after the first draft.
    pub max_repairs: u32,
    /// When present, the model is asked to modify this document rather than
    /// author one from nothing.
    pub base: Option<Workflow>,
}

impl PlanRequest {
    /// A request with a sensible repair budget.
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            constraints: Vec::new(),
            max_repairs: 3,
            base: None,
        }
    }

    /// Builder-style base document to modify.
    #[must_use]
    pub fn with_base(mut self, base: Workflow) -> Self {
        self.base = Some(base);
        self
    }

    /// Builder-style constraint.
    #[must_use]
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }
}

/// The outcome of a planning attempt.
#[derive(Debug, Clone)]
pub struct PlanOutcome {
    /// The accepted workflow, or the last draft when nothing was accepted.
    pub workflow: Option<Workflow>,
    /// Raw text of the last draft, for auditing and for the operator to inspect.
    pub raw: String,
    /// Validation report for the final draft.
    pub report: ValidationReport,
    /// Number of repair rounds actually used.
    pub repairs: u32,
    /// Total tokens reported by the provider across every call.
    pub tokens_used: u64,
    /// True when the runtime accepted the document.
    pub accepted: bool,
}

/// Turns goals into workflows.
pub struct Planner<'a> {
    provider: &'a dyn LlmProvider,
    descriptors: Vec<NodeDescriptor>,
    /// Optional budget charged *before* each provider call, so `max_steps`
    /// actually caps model round trips instead of detecting overspend after
    /// the fact.
    budget: Option<&'a mut BudgetTracker>,
}

impl std::fmt::Debug for Planner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Planner")
            .field("provider", &self.provider.name())
            .field("node_types", &self.descriptors.len())
            .finish()
    }
}

impl<'a> Planner<'a> {
    /// Build a planner over the node types the runtime reports.
    pub fn new(provider: &'a dyn LlmProvider, descriptors: Vec<NodeDescriptor>) -> Self {
        Self {
            provider,
            descriptors,
            budget: None,
        }
    }

    /// Charge `budget` before every model call the planner makes.
    pub fn with_budget(mut self, budget: &'a mut BudgetTracker) -> Self {
        self.budget = Some(budget);
        self
    }

    fn charge_step(&mut self) -> AgentResult<()> {
        match &mut self.budget {
            Some(budget) => budget.charge_step(),
            None => Ok(()),
        }
    }

    /// Plan a workflow, repairing until the runtime accepts it.
    pub fn plan(&mut self, request: &PlanRequest) -> AgentResult<PlanOutcome> {
        let system = system_prompt(&self.descriptors);
        let opening = match &request.base {
            Some(base) => {
                let current = serde_json::to_string_pretty(base)?;
                modify_prompt(&current, &user_prompt(&request.goal, &request.constraints))
            }
            None => user_prompt(&request.goal, &request.constraints),
        };
        let mut messages = vec![ChatMessage::system(system), ChatMessage::user(opening)];

        self.charge_step()?;
        let first = self
            .provider
            .complete(&ChatRequest::new(messages.clone()))?;
        let mut raw = first.content;
        let mut repairs = 0;
        let mut tokens_used = u64::from(first.usage.total());

        loop {
            let (workflow, report) = self.evaluate(&raw);
            if report.is_valid() {
                return Ok(PlanOutcome {
                    workflow,
                    raw,
                    report,
                    repairs,
                    tokens_used,
                    accepted: true,
                });
            }
            if repairs >= request.max_repairs {
                return Ok(PlanOutcome {
                    workflow,
                    raw,
                    report,
                    repairs,
                    tokens_used,
                    accepted: false,
                });
            }
            repairs += 1;

            let diagnostics = serde_json::to_value(&report)?;
            messages.push(ChatMessage::assistant(raw.clone()));
            messages.push(ChatMessage::user(repair_prompt(&raw, &diagnostics)));
            self.charge_step()?;
            let response = self
                .provider
                .complete(&ChatRequest::new(messages.clone()))?;
            tokens_used += u64::from(response.usage.total());
            raw = response.content;
        }
    }

    /// Parse and validate one draft.
    ///
    /// Parsing first, then validating with the *same* function the runtime uses,
    /// means a draft that passes here passes there. Without a catalogue
    /// (offline planning) only structure, graph and variable rules apply.
    fn evaluate(&self, raw: &str) -> (Option<Workflow>, ValidationReport) {
        match extract_workflow(raw) {
            Ok(workflow) => {
                let report = if self.descriptors.is_empty() {
                    nodara_schema::validate(&workflow)
                } else {
                    let index = DescriptorIndex(&self.descriptors);
                    validate_with(&workflow, &index, &Default::default())
                };
                (Some(workflow), report)
            }
            Err(error) => {
                let mut report = ValidationReport::default();
                report.push_payload_error(&error);
                (None, report)
            }
        }
    }
}

/// Models often wrap JSON in prose or a fenced block. Extract the object.
pub fn extract_workflow(raw: &str) -> Result<Workflow, String> {
    let trimmed = raw.trim();
    let candidate = if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after);
        match after.find("```") {
            Some(end) => after[..end].trim(),
            None => after.trim(),
        }
    } else if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        &trimmed[start..=end]
    } else {
        trimmed
    };

    let value: Value =
        serde_json::from_str(candidate).map_err(|error| format!("reply was not JSON: {error}"))?;
    serde_json::from_value(value)
        .map_err(|error| format!("reply was JSON but not a workflow document: {error}"))
}

/// Adapter that lets `nodara-schema`'s validation see the runtime's descriptors.
struct DescriptorIndex<'a>(&'a [NodeDescriptor]);

impl nodara_schema::NodeTypeIndex for DescriptorIndex<'_> {
    fn node_types(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|descriptor| descriptor.node_type.clone())
            .collect()
    }

    fn descriptor(&self, node_type: &str) -> Option<NodeDescriptor> {
        self.0
            .iter()
            .find(|descriptor| descriptor.node_type == node_type)
            .cloned()
    }
}

/// Small helper so a parse failure looks like any other diagnostic.
trait ReportPayloadError {
    fn push_payload_error(&mut self, message: &str);
}

impl ReportPayloadError for ValidationReport {
    fn push_payload_error(&mut self, message: &str) {
        self.diagnostics.push(nodara_schema::Diagnostic {
            severity: nodara_schema::Severity::Error,
            code: "AG100".to_string(),
            message: message.to_string(),
            path: "/".to_string(),
            node_id: None,
            edge_id: None,
            hint: Some("reply with a single workflow JSON object".to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AgentError;
    use crate::policy::{Budget, BudgetTracker};
    use crate::provider::MockProvider;
    use nodara_schema::NodeDescriptor;

    fn descriptors() -> Vec<NodeDescriptor> {
        vec![
            NodeDescriptor {
                outputs: Vec::new(),
                ..NodeDescriptor::new("core.Start", "Start", "Core")
            },
            NodeDescriptor {
                config_schema: serde_json::json!({
                    "type": "object",
                    "properties": { "message": { "type": "string" } },
                    "required": ["message"],
                    "additionalProperties": false
                }),
                allows_additional_config: false,
                ..NodeDescriptor::new("core.Log", "Log", "Core")
            },
            NodeDescriptor {
                ..NodeDescriptor::new("core.End", "End", "Core")
            },
        ]
    }

    #[test]
    fn extracts_json_from_a_fenced_block() {
        let raw = "Here you go:\n```json\n{\"schema_version\":\"2.0\",\"id\":\"wf\",\"nodes\":[],\"edges\":[]}\n```";
        let workflow = extract_workflow(raw).expect("parses");
        assert_eq!(workflow.id, "wf");
    }

    #[test]
    fn accepts_a_good_first_draft() {
        let draft = serde_json::json!({
            "schema_version": "2.0",
            "id": "wf.good",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                { "id": "log", "type": "core.Log", "config": { "message": "hi" } },
                { "id": "end", "type": "core.End" }
            ],
            "edges": [
                { "id": "e1", "source": "start", "target": "log" },
                { "id": "e2", "source": "log", "target": "end" }
            ]
        })
        .to_string();
        let provider = MockProvider::new([draft]);
        let mut planner = Planner::new(&provider, descriptors());
        let outcome = planner.plan(&PlanRequest::new("log something")).unwrap();
        assert!(outcome.accepted);
        assert_eq!(outcome.repairs, 0);
    }

    #[test]
    fn repairs_a_broken_draft_using_runtime_diagnostics() {
        let broken = serde_json::json!({
            "schema_version": "2.0",
            "id": "wf.broken",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                { "id": "log", "type": "core.Log", "config": {} },
                { "id": "end", "type": "core.End" }
            ],
            "edges": [
                { "id": "e1", "source": "start", "target": "log" },
                { "id": "e2", "source": "log", "target": "end" }
            ]
        })
        .to_string();
        let fixed = serde_json::json!({
            "schema_version": "2.0",
            "id": "wf.fixed",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                { "id": "log", "type": "core.Log", "config": { "message": "hi" } },
                { "id": "end", "type": "core.End" }
            ],
            "edges": [
                { "id": "e1", "source": "start", "target": "log" },
                { "id": "e2", "source": "log", "target": "end" }
            ]
        })
        .to_string();

        let provider = MockProvider::new([broken, fixed]);
        let mut planner = Planner::new(&provider, descriptors());
        let outcome = planner.plan(&PlanRequest::new("log something")).unwrap();

        assert!(outcome.accepted);
        assert_eq!(outcome.repairs, 1);
        // The repair turn must have carried the diagnostics.
        let calls = provider.calls();
        assert_eq!(calls.len(), 2);
        let repair_turn = &calls[1].messages.last().unwrap().content;
        assert!(repair_turn.contains("WF142"), "{repair_turn}");
    }

    #[test]
    fn the_budget_is_charged_before_each_model_call() {
        let broken = serde_json::json!({
            "schema_version": "2.0",
            "id": "wf.broken",
            "nodes": [{ "id": "start", "type": "core.Start" }],
            "edges": []
        })
        .to_string();
        let provider = MockProvider::new([broken.clone(), broken.clone(), broken]);
        let mut budget = BudgetTracker::new(Budget {
            max_steps: 1,
            ..Budget::default()
        });
        let mut planner = Planner::new(&provider, descriptors()).with_budget(&mut budget);
        let mut request = PlanRequest::new("impossible");
        request.max_repairs = 2;

        let error = planner.plan(&request).expect_err("budget must be refused");
        assert!(matches!(error, AgentError::BudgetExhausted(_)));
        assert_eq!(
            provider.calls().len(),
            1,
            "the second model call must be refused before it happens"
        );
    }

    #[test]
    fn gives_up_after_the_repair_budget() {
        let broken = serde_json::json!({
            "schema_version": "2.0",
            "id": "wf.broken",
            "nodes": [{ "id": "start", "type": "core.Start" }],
            "edges": []
        })
        .to_string();
        let provider = MockProvider::new([broken.clone(), broken.clone(), broken]);
        let mut planner = Planner::new(&provider, descriptors());
        let mut request = PlanRequest::new("impossible");
        request.max_repairs = 2;
        let outcome = planner.plan(&request).unwrap();
        assert!(!outcome.accepted);
        assert_eq!(outcome.repairs, 2);
        assert!(!outcome.report.is_valid());
    }

    #[test]
    fn reports_prose_instead_of_json_as_a_diagnostic() {
        let provider = MockProvider::new(["I am sorry, I cannot do that."]);
        let mut planner = Planner::new(&provider, descriptors());
        let outcome = planner
            .plan(&PlanRequest {
                goal: "x".into(),
                constraints: Vec::new(),
                max_repairs: 0,
                base: None,
            })
            .unwrap();
        assert!(!outcome.accepted);
        assert!(outcome
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "AG100"));
    }

    #[test]
    fn a_base_document_is_sent_for_modification_not_generation() {
        let mut base = Workflow::new("wf.existing");
        base.add_node(nodara_schema::Node::new("start", "core.Start"));
        base.add_node(nodara_schema::Node::new("log", "core.Log"));
        base.add_node(nodara_schema::Node::new("end", "core.End"));

        let provider = MockProvider::new([serde_json::json!({
            "schema_version": "2.0",
            "id": "wf.existing",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                { "id": "log", "type": "core.Log", "config": { "message": "added" } },
                { "id": "end", "type": "core.End" }
            ],
            "edges": [
                { "id": "e1", "source": "start", "target": "log" },
                { "id": "e2", "source": "log", "target": "end" }
            ]
        })
        .to_string()]);

        let mut planner = Planner::new(&provider, descriptors());
        let outcome = planner
            .plan(&PlanRequest::new("add a log line").with_base(base))
            .unwrap();
        assert!(outcome.accepted);

        let first_turn = &provider.calls()[0].messages[1].content;
        assert!(first_turn.contains("workflow the operator is currently editing"));
        assert!(first_turn.contains("wf.existing"));
        assert!(first_turn.contains("Return the whole document, not a patch"));
    }
}
