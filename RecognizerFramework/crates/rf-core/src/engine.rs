//! Deterministic, pausable workflow execution.
//!
//! The engine walks the graph in topological order, activating nodes branch by
//! branch as guards evaluate true. It owns no I/O: every side effect happens
//! inside a [`NodeExecutor`], and every observation leaves through the event bus
//! and audit log. That is what lets the same engine drive the CLI, the Studio and
//! an autonomous agent without special cases.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use rf_schema::{
    validate_with, ExecutionEvent, RunStatus, ValidationOptions, Workflow, WorkflowGraph,
};

use crate::audit::{AuditCategory, AuditLog, AuditRecord, NullAuditLog};
use crate::context::{ArtifactStore, ExecutionContext};
use crate::control::RunControl;
use crate::error::{ExecutionError, NodeError};
use crate::events::{EventBus, EventSink, NullEventSink};
use crate::executor::NodeInput;
use crate::expr::evaluate_expression;
use crate::policy::{ApprovalHandler, AutoApprove, CapabilityPolicy, DefaultPolicy};
use crate::registry::CapabilityRegistry;

/// Tunables for a run.
#[derive(Debug, Clone)]
pub struct EngineOptions {
    /// Validate the workflow (including capability checks) before running.
    pub validate: bool,
    /// Validation strictness.
    pub validation: ValidationOptions,
    /// Hard ceiling on executed nodes, as a runaway-loop guard.
    pub max_nodes: usize,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            validate: true,
            validation: ValidationOptions::default(),
            max_nodes: 10_000,
        }
    }
}

/// A request to execute a workflow.
pub struct RunRequest {
    /// Workflow to execute.
    pub workflow: Workflow,
    /// Caller-supplied run id. A UUID is generated when omitted.
    pub run_id: Option<String>,
    /// Variables that override or extend the workflow's own defaults.
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Destination for execution events.
    pub event_sink: Arc<dyn EventSink>,
}

impl std::fmt::Debug for RunRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunRequest")
            .field("workflow_id", &self.workflow.id)
            .field("run_id", &self.run_id)
            .field("variables", &self.variables.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl RunRequest {
    /// A request with no extra variables and a discarding event sink.
    pub fn new(workflow: Workflow) -> Self {
        Self {
            workflow,
            run_id: None,
            variables: BTreeMap::new(),
            event_sink: Arc::new(NullEventSink),
        }
    }

    /// Builder-style event sink.
    #[must_use]
    pub fn with_event_sink(mut self, sink: Arc<dyn EventSink>) -> Self {
        self.event_sink = sink;
        self
    }

    /// Builder-style run id.
    #[must_use]
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Builder-style variable overrides.
    #[must_use]
    pub fn with_variables(mut self, variables: BTreeMap<String, serde_json::Value>) -> Self {
        self.variables = variables;
        self
    }
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RunFailure {
    /// Stable machine-readable code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// The result of a completed run.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// Run identifier.
    pub run_id: String,
    /// Terminal status.
    pub status: RunStatus,
    /// Number of nodes actually executed.
    pub nodes_executed: usize,
    /// Final variable scope.
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Failure details, when the run did not complete.
    pub failure: Option<RunFailure>,
}

impl RunOutcome {
    /// True when the run completed successfully.
    pub fn is_success(&self) -> bool {
        self.status == RunStatus::Completed
    }
}

/// A handle to a run started on its own thread.
pub struct RunningRun {
    run_id: String,
    control: RunControl,
    join: Option<std::thread::JoinHandle<RunOutcome>>,
}

impl std::fmt::Debug for RunningRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningRun")
            .field("run_id", &self.run_id)
            .finish()
    }
}

impl RunningRun {
    /// Run identifier.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Control handle for pause / resume / step / cancel.
    pub fn control(&self) -> &RunControl {
        &self.control
    }

    /// Block until the run finishes and return its outcome.
    pub fn join(mut self) -> RunOutcome {
        match self.join.take() {
            Some(handle) => handle.join().unwrap_or_else(|_| RunOutcome {
                run_id: self.run_id.clone(),
                status: RunStatus::Failed,
                nodes_executed: 0,
                variables: BTreeMap::new(),
                duration_ms: 0,
                failure: Some(RunFailure {
                    code: "E_PANIC".to_string(),
                    message: "run thread panicked".to_string(),
                }),
            }),
            None => RunOutcome {
                run_id: self.run_id.clone(),
                status: RunStatus::Failed,
                nodes_executed: 0,
                variables: BTreeMap::new(),
                duration_ms: 0,
                failure: Some(RunFailure {
                    code: "E_JOINED".to_string(),
                    message: "run was already joined".to_string(),
                }),
            },
        }
    }
}

/// Executes workflows against a fixed capability registry, policy and audit log.
#[derive(Clone)]
pub struct WorkflowEngine {
    registry: Arc<CapabilityRegistry>,
    policy: Arc<dyn CapabilityPolicy>,
    approval: Arc<dyn ApprovalHandler>,
    audit: Arc<dyn AuditLog>,
    options: EngineOptions,
}

impl std::fmt::Debug for WorkflowEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkflowEngine")
            .field("registry", &self.registry)
            .field("options", &self.options)
            .finish()
    }
}

impl WorkflowEngine {
    /// Create an engine with the default policy, auto-approval and no audit sink.
    pub fn new(registry: Arc<CapabilityRegistry>) -> Self {
        Self {
            registry,
            policy: Arc::new(DefaultPolicy),
            approval: Arc::new(AutoApprove),
            audit: Arc::new(NullAuditLog),
            options: EngineOptions::default(),
        }
    }

    /// Replace the capability policy.
    #[must_use]
    pub fn with_policy(mut self, policy: Arc<dyn CapabilityPolicy>) -> Self {
        self.policy = policy;
        self
    }

    /// Replace the approval handler.
    #[must_use]
    pub fn with_approval(mut self, approval: Arc<dyn ApprovalHandler>) -> Self {
        self.approval = approval;
        self
    }

    /// Replace the audit log.
    #[must_use]
    pub fn with_audit(mut self, audit: Arc<dyn AuditLog>) -> Self {
        self.audit = audit;
        self
    }

    /// Replace engine options.
    #[must_use]
    pub fn with_options(mut self, options: EngineOptions) -> Self {
        self.options = options;
        self
    }

    /// The capability registry in use.
    pub fn registry(&self) -> &CapabilityRegistry {
        &self.registry
    }

    /// The options in use.
    pub fn options(&self) -> &EngineOptions {
        &self.options
    }

    /// Start a run on a dedicated thread.
    pub fn spawn(&self, mut request: RunRequest) -> RunningRun {
        let run_id = request
            .run_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        request.run_id = Some(run_id.clone());
        let control = RunControl::new();
        let engine = self.clone();
        let thread_control = control.clone();
        let thread_run_id = run_id.clone();
        let join = std::thread::Builder::new()
            .name(format!("rf-run-{thread_run_id}"))
            .spawn(move || engine.run(request, &thread_control))
            .expect("spawning a run thread");
        RunningRun {
            run_id,
            control,
            join: Some(join),
        }
    }

    /// Execute a workflow synchronously on the calling thread.
    pub fn run(&self, request: RunRequest, control: &RunControl) -> RunOutcome {
        let started = Instant::now();
        let run_id = request
            .run_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let bus = EventBus::new(run_id.clone(), request.event_sink.clone());
        let workflow = request.workflow;
        let seeded = seeded_variables(&workflow, &request.variables);

        // --- static validation ----------------------------------------------
        if self.options.validate {
            let report = validate_with(&workflow, self.registry.as_ref(), &self.options.validation);
            if !report.is_valid() {
                let message = format!(
                    "workflow `{}` failed validation with {} error(s)",
                    workflow.id,
                    report.error_count()
                );
                self.audit.record(
                    AuditRecord::new(run_id.clone(), AuditCategory::RunFinished, message.clone())
                        .detail(serde_json::to_value(&report).unwrap_or_default()),
                );
                return fail_fast(
                    run_id,
                    started,
                    &bus,
                    control,
                    seeded,
                    "E_VALIDATION",
                    message,
                );
            }
        }

        // --- scheduling plan -------------------------------------------------
        let graph = WorkflowGraph::new(&workflow);
        let secrets: HashSet<String> = workflow
            .variables
            .iter()
            .filter(|(_, variable)| variable.secret)
            .map(|(name, _)| name.clone())
            .collect();

        let Some(entry) = graph.entry_node() else {
            return fail_fast(
                run_id,
                started,
                &bus,
                control,
                seeded,
                "E_NO_ENTRY_POINT",
                "workflow has no entry point".to_string(),
            );
        };
        let entry_id = entry.id.clone();

        let order: Vec<String> = match graph.topological_order() {
            Ok(order) => order.into_iter().map(str::to_string).collect(),
            Err(error) => {
                return fail_fast(
                    run_id,
                    started,
                    &bus,
                    control,
                    seeded,
                    "E_GRAPH",
                    error.to_string(),
                );
            }
        };

        let mut context = ExecutionContext::new(
            run_id.clone(),
            workflow.id.clone(),
            seeded,
            secrets,
            control.clone(),
            bus.clone(),
            self.policy.clone(),
            self.approval.clone(),
            self.audit.clone(),
            Arc::new(ArtifactStore::new()),
        );

        bus.emit(ExecutionEvent::RunStarted {
            workflow_id: workflow.id.clone(),
        });
        self.audit.record(AuditRecord::new(
            run_id.clone(),
            AuditCategory::RunStarted,
            format!("run started for workflow `{}`", workflow.id),
        ));

        // --- execution -------------------------------------------------------
        let mut activated: HashSet<String> = HashSet::from([entry_id]);
        let mut node_outputs: HashMap<String, BTreeMap<String, serde_json::Value>> = HashMap::new();
        let mut executed = 0usize;
        let mut status = RunStatus::Completed;
        let mut failure: Option<RunFailure> = None;

        for node_id in order {
            if control.is_cancelled() {
                status = RunStatus::Cancelled;
                failure = Some(cancelled_failure());
                break;
            }
            if !activated.contains(&node_id) {
                continue;
            }
            if executed >= self.options.max_nodes {
                status = RunStatus::Failed;
                failure = Some(RunFailure {
                    code: "E_NODE_LIMIT".to_string(),
                    message: format!("execution exceeded {} nodes", self.options.max_nodes),
                });
                break;
            }
            if let Err(NodeError::Cancelled) = control.await_permission() {
                status = RunStatus::Cancelled;
                failure = Some(cancelled_failure());
                break;
            }

            let Some(node) = graph.node(&node_id).cloned() else {
                continue;
            };
            let Some(executor) = self.registry.get(&node.node_type) else {
                let error = ExecutionError::UnknownNodeType {
                    node_id: node.id.clone(),
                    node_type: node.node_type.clone(),
                };
                status = RunStatus::Failed;
                failure = Some(RunFailure {
                    code: error.code().to_string(),
                    message: error.to_string(),
                });
                break;
            };
            let descriptor = executor.descriptor();
            context.set_node(Some(node.id.clone()));

            if let Err(error) = context.authorize(
                &node.node_type,
                &descriptor.permissions,
                descriptor.dangerous,
                &node.config,
            ) {
                let message = error.to_string();
                bus.emit(ExecutionEvent::NodeFailed {
                    node_id: node.id.clone(),
                    code: error.code().to_string(),
                    message: message.clone(),
                    retryable: error.retryable(),
                });
                self.audit.record(
                    AuditRecord::new(run_id.clone(), AuditCategory::NodeFailed, message.clone())
                        .node(node.id.clone(), node.node_type.clone()),
                );
                status = if matches!(error, NodeError::Cancelled) {
                    RunStatus::Cancelled
                } else {
                    RunStatus::Failed
                };
                failure = Some(RunFailure {
                    code: error.code().to_string(),
                    message,
                });
                break;
            }

            let resolved_config = context.resolve_value(&node.config);
            let mut inputs = BTreeMap::new();
            for edge in graph.edges_to(&node.id) {
                let port = edge.source_port.as_deref().unwrap_or("out");
                let value = node_outputs
                    .get(&edge.source)
                    .and_then(|outputs| outputs.get(port))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let target_port = edge.target_port.clone().unwrap_or_else(|| "in".to_string());
                inputs.insert(target_port, value);
            }

            let input = NodeInput {
                node_id: node.id.clone(),
                node_type: node.node_type.clone(),
                config: node.config.clone(),
                resolved_config,
                inputs,
            };

            bus.emit(ExecutionEvent::NodeStarted {
                node_id: node.id.clone(),
                node_type: node.node_type.clone(),
            });
            self.audit.record(
                AuditRecord::new(
                    run_id.clone(),
                    AuditCategory::NodeStarted,
                    format!("node `{}` started", node.id),
                )
                .node(node.id.clone(), node.node_type.clone()),
            );

            let node_started = Instant::now();
            match executor.execute(input, &mut context) {
                Ok(output) => {
                    executed += 1;
                    let duration_ms = node_started.elapsed().as_millis() as u64;
                    for (name, value) in &output.variables {
                        context.set_variable(name.clone(), value.clone());
                    }
                    node_outputs.insert(node.id.clone(), output.outputs.clone());

                    bus.emit(ExecutionEvent::NodeFinished {
                        node_id: node.id.clone(),
                        outputs: output.outputs.clone(),
                        duration_ms,
                    });
                    self.audit.record(
                        AuditRecord::new(
                            run_id.clone(),
                            AuditCategory::NodeFinished,
                            format!("node `{}` finished in {duration_ms}ms", node.id),
                        )
                        .node(node.id.clone(), node.node_type.clone()),
                    );

                    for edge in graph.edges_from(&node.id) {
                        if edge_is_taken(edge, &context, &bus) {
                            activated.insert(edge.target.clone());
                        }
                    }
                    if node.node_type == "core.End" {
                        break;
                    }
                }
                Err(error) => {
                    let duration_ms = node_started.elapsed().as_millis() as u64;
                    bus.emit(ExecutionEvent::NodeFailed {
                        node_id: node.id.clone(),
                        code: error.code().to_string(),
                        message: error.to_string(),
                        retryable: error.retryable(),
                    });
                    self.audit.record(
                        AuditRecord::new(
                            run_id.clone(),
                            AuditCategory::NodeFailed,
                            format!("node `{}` failed: {error}", node.id),
                        )
                        .node(node.id.clone(), node.node_type.clone())
                        .detail(serde_json::json!({ "duration_ms": duration_ms })),
                    );
                    status = if matches!(error, NodeError::Cancelled) {
                        RunStatus::Cancelled
                    } else {
                        RunStatus::Failed
                    };
                    failure = Some(RunFailure {
                        code: error.code().to_string(),
                        message: error.to_string(),
                    });
                    break;
                }
            }
        }

        context.set_node(None);
        let variables = context.variables().clone();
        let duration_ms = started.elapsed().as_millis() as u64;

        match status {
            RunStatus::Completed => {
                bus.emit(ExecutionEvent::RunCompleted {
                    nodes_executed: executed,
                    duration_ms,
                });
            }
            RunStatus::Cancelled => {
                bus.emit(ExecutionEvent::RunCancelled {
                    reason: failure.as_ref().map(|f| f.message.clone()),
                });
            }
            _ => {
                let reported = failure.clone().unwrap_or(RunFailure {
                    code: "E_UNKNOWN".to_string(),
                    message: "unknown failure".to_string(),
                });
                bus.emit(ExecutionEvent::RunFailed {
                    code: reported.code,
                    message: reported.message,
                });
            }
        }
        self.audit.record(
            AuditRecord::new(
                run_id.clone(),
                AuditCategory::RunFinished,
                format!("run finished with status {status:?}"),
            )
            .detail(serde_json::json!({
                "nodes_executed": executed,
                "duration_ms": duration_ms,
                "failure": failure,
            })),
        );
        control.finish();

        RunOutcome {
            run_id,
            status,
            nodes_executed: executed,
            variables,
            duration_ms,
            failure,
        }
    }
}

fn cancelled_failure() -> RunFailure {
    RunFailure {
        code: "E_CANCELLED".to_string(),
        message: "run cancelled".to_string(),
    }
}

/// Emit a failure terminal event, release any waiters and produce an outcome.
fn fail_fast(
    run_id: String,
    started: Instant,
    bus: &EventBus,
    control: &RunControl,
    variables: BTreeMap<String, serde_json::Value>,
    code: &str,
    message: String,
) -> RunOutcome {
    bus.emit(ExecutionEvent::RunFailed {
        code: code.to_string(),
        message: message.clone(),
    });
    control.finish();
    RunOutcome {
        run_id,
        status: RunStatus::Failed,
        nodes_executed: 0,
        variables,
        duration_ms: started.elapsed().as_millis() as u64,
        failure: Some(RunFailure {
            code: code.to_string(),
            message,
        }),
    }
}

fn seeded_variables(
    workflow: &Workflow,
    overrides: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, serde_json::Value> {
    let mut variables: BTreeMap<String, serde_json::Value> = workflow
        .variables
        .iter()
        .map(|(name, variable)| (name.clone(), variable.value.clone()))
        .collect();
    for (name, value) in overrides {
        variables.insert(name.clone(), value.clone());
    }
    variables
}

fn edge_is_taken(edge: &rf_schema::Edge, context: &ExecutionContext, bus: &EventBus) -> bool {
    let Some(condition) = &edge.condition else {
        return true;
    };
    match evaluate_expression(condition, context.variables()) {
        Ok(value) => value != 0.0 && !value.is_nan(),
        Err(error) => {
            bus.emit(ExecutionEvent::Log {
                level: rf_schema::LogLevel::Warn,
                message: format!(
                    "edge `{}` guard `{condition}` could not be evaluated: {error}; treating as false",
                    edge.id
                ),
                node_id: None,
            });
            false
        }
    }
}
