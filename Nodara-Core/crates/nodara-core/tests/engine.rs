//! End-to-end tests for the execution engine.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nodara_core::{
    register_builtins, AllowAllPolicy, CapabilityRegistry, CollectingEventSink, DenyAllPolicy,
    EngineOptions, ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, RunControl,
    RunRequest, WorkflowEngine,
};
use nodara_schema::{
    Edge, EdgeBranch, ExecutionEvent, Node, NodeDescriptor, PortDescriptor, PortKind, RunStatus,
    ValueType, Variable, Workflow,
};

fn registry() -> Arc<CapabilityRegistry> {
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    assert!(registry.can_execute("core.Log"));
    assert!(registry.can_execute("system.Delay"));
    Arc::new(registry)
}

/// Captures the timeout the engine passed into the executor.
#[derive(Debug)]
struct TimeoutCaptureExecutor {
    seen_ms: Arc<AtomicU64>,
}

impl NodeExecutor for TimeoutCaptureExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor::new("test.TimeoutCapture", "Timeout Capture", "Test")
    }

    fn execute(
        &self,
        input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> nodara_core::NodeResult<NodeOutput> {
        self.seen_ms
            .store(input.timeout_ms.unwrap_or_default(), Ordering::SeqCst);
        Ok(NodeOutput::new())
    }
}

/// Captures configuration after engine-level template resolution.
#[derive(Debug)]
struct ConfigCaptureExecutor {
    seen: Arc<Mutex<serde_json::Value>>,
}

impl NodeExecutor for ConfigCaptureExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer" },
                    "y": { "type": "integer" },
                    "threshold": { "type": "number" },
                    "enabled": { "type": "boolean" },
                    "label": { "type": "string" },
                    "exact_label": { "type": "string" }
                }
            }),
            ..NodeDescriptor::new("test.ConfigCapture", "Config Capture", "Test")
        }
    }

    fn execute(
        &self,
        input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> nodara_core::NodeResult<NodeOutput> {
        *self.seen.lock().expect("config capture") = serde_json::json!({
            "x": input.config_i64("x"),
            "y": input.config_i64("y"),
            "threshold": input.config_f64("threshold"),
            "enabled": input.config_bool("enabled"),
            "label": input.config_str("label"),
            "exact_label": input.config_str("exact_label")
        });
        Ok(NodeOutput::new())
    }
}

/// Test executor that fails a fixed number of times before succeeding.
#[derive(Debug)]
struct FlakyExecutor {
    calls: Arc<AtomicUsize>,
    failures: usize,
}

impl NodeExecutor for FlakyExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor::new("test.Flaky", "Flaky", "Test")
    }

    fn execute(
        &self,
        _input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> nodara_core::NodeResult<NodeOutput> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call < self.failures {
            Err(NodeError::Execution(format!(
                "temporary failure {}",
                call + 1
            )))
        } else {
            Ok(NodeOutput::new().with_output("out", serde_json::json!(true)))
        }
    }
}

fn linear_workflow() -> Workflow {
    let mut workflow = Workflow::new("wf.linear");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(
        Node::new("log", "core.Log").with_config(serde_json::json!({ "message": "hello" })),
    );
    workflow.add_node(
        Node::new("calc", "core.Calculate")
            .with_config(serde_json::json!({ "expression": "2 + 2 * 3", "output_var": "result" })),
    );
    workflow.add_node(
        Node::new("log2", "core.Log")
            .with_config(serde_json::json!({ "message": "result is {{result}}" })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "log"));
    workflow.add_edge(Edge::new("e2", "log", "calc"));
    workflow.add_edge(Edge::new("e3", "calc", "log2"));
    workflow.add_edge(Edge::new("e4", "log2", "end"));
    workflow
}

#[test]
fn runs_a_linear_workflow_and_publishes_variables() {
    let sink = Arc::new(CollectingEventSink::new());
    let engine = WorkflowEngine::new(registry());
    let control = RunControl::new();
    let request = RunRequest::new(linear_workflow()).with_event_sink(sink.clone());

    let outcome = engine.run(request, &control);

    assert_eq!(
        outcome.status,
        RunStatus::Completed,
        "{:?}",
        outcome.failure
    );
    assert_eq!(outcome.nodes_executed, 5);
    assert_eq!(outcome.variables.get("result"), Some(&serde_json::json!(8)));

    let events = sink.snapshot();
    assert!(matches!(
        events.first().map(|e| &e.event),
        Some(ExecutionEvent::RunStarted { .. })
    ));
    assert!(matches!(
        events.last().map(|e| &e.event),
        Some(ExecutionEvent::RunCompleted { .. })
    ));
    let logs: Vec<String> = events
        .iter()
        .filter_map(|envelope| match &envelope.event {
            ExecutionEvent::Log { message, .. } => Some(message.clone()),
            _ => None,
        })
        .collect();
    assert!(logs.contains(&"hello".to_string()));
    assert!(logs.contains(&"result is 8".to_string()));
}

#[test]
fn a_disabled_node_is_skipped_and_passes_through() {
    let mut workflow = Workflow::new("wf.disabled");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut log = Node::new("log", "core.Log");
    log.enabled = false;
    log.config = serde_json::json!({ "message": "must not run" });
    workflow.add_node(log);
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "log"));
    workflow.add_edge(Edge::new("e2", "log", "end"));

    let sink = Arc::new(CollectingEventSink::new());
    let outcome = WorkflowEngine::new(registry()).run(
        RunRequest::new(workflow).with_event_sink(sink.clone()),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(outcome.nodes_executed, 2);
    assert!(!sink.snapshot().iter().any(|envelope| matches!(
        &envelope.event,
        ExecutionEvent::Log { message, .. } if message == "must not run"
    )));
}

#[test]
fn continue_on_error_takes_outgoing_branches() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(FlakyExecutor {
        calls: calls.clone(),
        failures: 10,
    });

    let mut workflow = Workflow::new("wf.continue");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut flaky = Node::new("flaky", "test.Flaky");
    flaky.retry = 1;
    flaky.continue_on_error = true;
    workflow.add_node(flaky);
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "flaky"));
    workflow.add_edge(Edge::new("e2", "flaky", "end"));

    let sink = Arc::new(CollectingEventSink::new());
    let outcome = WorkflowEngine::new(Arc::new(registry)).run(
        RunRequest::new(workflow).with_event_sink(sink.clone()),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(outcome.nodes_executed, 2);
    assert!(sink.snapshot().iter().any(|envelope| matches!(
        &envelope.event,
        ExecutionEvent::NodeFailed { node_id, .. } if node_id == "flaky"
    )));
}

#[test]
fn a_failure_branch_recovers_without_continue_on_error() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(FlakyExecutor {
        calls,
        failures: 10,
    });

    let mut workflow = Workflow::new("wf.failure-branch");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("flaky", "test.Flaky"));
    workflow.add_node(
        Node::new("healthy", "core.Log")
            .with_config(serde_json::json!({ "message": "unexpected success" })),
    );
    workflow.add_node(
        Node::new("recover", "core.Log").with_config(serde_json::json!({
            "message": "recovered {{last_error.code}} at {{last_error.node_id}}"
        })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "flaky"));
    let mut success_edge = Edge::new("e2", "flaky", "healthy");
    success_edge.branch = EdgeBranch::Success;
    let mut failure_edge = Edge::new("e3", "flaky", "recover");
    failure_edge.branch = EdgeBranch::Failure;
    workflow.add_edge(success_edge);
    workflow.add_edge(failure_edge);
    workflow.add_edge(Edge::new("e4", "healthy", "end"));
    workflow.add_edge(Edge::new("e5", "recover", "end"));

    let sink = Arc::new(CollectingEventSink::new());
    let outcome = WorkflowEngine::new(Arc::new(registry)).run(
        RunRequest::new(workflow).with_event_sink(sink.clone()),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    // The failed node is reported as a failure event, not a successful execution.
    assert_eq!(outcome.nodes_executed, 3);
    let logs: Vec<String> = sink
        .snapshot()
        .iter()
        .filter_map(|envelope| match &envelope.event {
            ExecutionEvent::Log { message, .. } => Some(message.clone()),
            _ => None,
        })
        .collect();
    assert!(logs.contains(&"recovered E_EXECUTION at flaky".to_string()));
    assert!(!logs.contains(&"unexpected success".to_string()));
}

#[test]
fn an_inactive_failure_guard_does_not_hide_the_error() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(FlakyExecutor {
        calls,
        failures: 10,
    });

    let mut workflow = Workflow::new("wf.inactive-failure-branch");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("flaky", "test.Flaky"));
    workflow.add_node(
        Node::new("recover", "core.Log").with_config(serde_json::json!({ "message": "recovered" })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "flaky"));
    let mut failure_edge = Edge::new("e2", "flaky", "recover");
    failure_edge.branch = EdgeBranch::Failure;
    failure_edge.condition = Some("allow_recovery".to_string());
    workflow.add_edge(failure_edge);
    workflow.add_edge(Edge::new("e3", "recover", "end"));
    workflow.variables.insert(
        "allow_recovery".to_string(),
        Variable {
            value: serde_json::json!(false),
            ..Variable::default()
        },
    );

    let outcome =
        WorkflowEngine::new(Arc::new(registry)).run(RunRequest::new(workflow), &RunControl::new());

    assert_eq!(outcome.status, RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_EXECUTION");
}

#[test]
fn a_success_branch_ignores_the_failure_path() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(FlakyExecutor { calls, failures: 0 });

    let mut workflow = Workflow::new("wf.success-branch");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("flaky", "test.Flaky"));
    workflow.add_node(
        Node::new("healthy", "core.Log")
            .with_config(serde_json::json!({ "message": "healthy path" })),
    );
    workflow.add_node(
        Node::new("recover", "core.Log")
            .with_config(serde_json::json!({ "message": "unexpected recovery" })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "flaky"));
    let mut success_edge = Edge::new("e2", "flaky", "healthy");
    success_edge.branch = EdgeBranch::Success;
    let mut failure_edge = Edge::new("e3", "flaky", "recover");
    failure_edge.branch = EdgeBranch::Failure;
    workflow.add_edge(success_edge);
    workflow.add_edge(failure_edge);
    workflow.add_edge(Edge::new("e4", "healthy", "end"));
    workflow.add_edge(Edge::new("e5", "recover", "end"));

    let sink = Arc::new(CollectingEventSink::new());
    let outcome = WorkflowEngine::new(Arc::new(registry)).run(
        RunRequest::new(workflow).with_event_sink(sink.clone()),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(outcome.nodes_executed, 4);
    let logs: Vec<String> = sink
        .snapshot()
        .iter()
        .filter_map(|envelope| match &envelope.event {
            ExecutionEvent::Log { message, .. } => Some(message.clone()),
            _ => None,
        })
        .collect();
    assert!(logs.contains(&"healthy path".to_string()));
    assert!(!logs.contains(&"unexpected recovery".to_string()));
}

#[test]
fn a_false_node_condition_prunes_the_branch() {
    let mut workflow = Workflow::new("wf.condition");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut log = Node::new("log", "core.Log");
    log.condition = Some("allow".to_string());
    log.config = serde_json::json!({ "message": "must not run" });
    workflow.add_node(log);
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "log"));
    workflow.add_edge(Edge::new("e2", "log", "end"));
    workflow.variables.insert(
        "allow".to_string(),
        Variable {
            value: serde_json::json!(false),
            ..Variable::default()
        },
    );

    let sink = Arc::new(CollectingEventSink::new());
    let outcome = WorkflowEngine::new(registry()).run(
        RunRequest::new(workflow).with_event_sink(sink.clone()),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(outcome.nodes_executed, 1);
    assert!(!sink.snapshot().iter().any(|envelope| matches!(
        &envelope.event,
        ExecutionEvent::Log { message, .. } if message == "must not run"
    )));
}

#[test]
fn an_invalid_node_condition_fails_with_config_error() {
    let mut workflow = Workflow::new("wf.bad-condition");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut log = Node::new("log", "core.Log");
    log.condition = Some("1 +".to_string());
    log.config = serde_json::json!({ "message": "conditional" });
    workflow.add_node(log);
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "log"));
    workflow.add_edge(Edge::new("e2", "log", "end"));

    let outcome =
        WorkflowEngine::new(registry()).run(RunRequest::new(workflow), &RunControl::new());
    assert_eq!(outcome.status, RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_INVALID_CONFIG");
}

#[test]
fn node_timeout_is_passed_to_the_executor() {
    let seen = Arc::new(AtomicU64::new(0));
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(TimeoutCaptureExecutor {
        seen_ms: seen.clone(),
    });

    let mut workflow = Workflow::new("wf.timeout");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut timed = Node::new("timed", "test.TimeoutCapture");
    timed.timeout_ms = Some(250);
    workflow.add_node(timed);
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "timed"));
    workflow.add_edge(Edge::new("e2", "timed", "end"));

    let outcome =
        WorkflowEngine::new(Arc::new(registry)).run(RunRequest::new(workflow), &RunControl::new());
    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(seen.load(Ordering::SeqCst), 250);
}

#[test]
fn node_level_delays_are_applied() {
    let mut workflow = Workflow::new("wf.node-delay");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut log = Node::new("log", "core.Log");
    log.delay_before_ms = 20;
    log.delay_after_ms = 20;
    log.config = serde_json::json!({ "message": "delayed" });
    workflow.add_node(log);
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "log"));
    workflow.add_edge(Edge::new("e2", "log", "end"));

    let started = std::time::Instant::now();
    let outcome =
        WorkflowEngine::new(registry()).run(RunRequest::new(workflow), &RunControl::new());

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert!(
        started.elapsed() >= Duration::from_millis(30),
        "pre/post delays should be reflected in execution time"
    );
}

#[test]
fn exact_templates_preserve_types_expected_by_the_config_schema() {
    let seen = Arc::new(Mutex::new(serde_json::Value::Null));
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(ConfigCaptureExecutor { seen: seen.clone() });

    let mut workflow = Workflow::new("wf.typed-template");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(
        Node::new("capture", "test.ConfigCapture").with_config(serde_json::json!({
            "x": "{{point.x}}",
            "y": "{{point.y}}",
            "threshold": "{{point.threshold}}",
            "enabled": "{{point.enabled}}",
            "label": "point {{point.x}},{{point.y}}",
            "exact_label": "{{point.x}}"
        })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "capture"));
    workflow.add_edge(Edge::new("e2", "capture", "end"));

    let mut variables = BTreeMap::new();
    variables.insert(
        "point".to_string(),
        serde_json::json!({
            "x": 320,
            "y": 240,
            "threshold": 0.75,
            "enabled": true
        }),
    );
    let engine = WorkflowEngine::new(Arc::new(registry));
    let outcome = engine.run(
        RunRequest::new(workflow).with_variables(variables),
        &RunControl::new(),
    );
    assert!(outcome.is_success(), "{:?}", outcome.failure);

    let seen = seen.lock().expect("config capture");
    assert_eq!(seen["x"], 320);
    assert_eq!(seen["y"], 240);
    assert_eq!(seen["threshold"], 0.75);
    assert_eq!(seen["enabled"], true);
    assert_eq!(seen["label"], "point 320,240");
    assert_eq!(seen["exact_label"], "320");
}

#[test]
fn calculate_many_evaluates_named_expressions_in_order() {
    let mut workflow = Workflow::new("wf.calculate-many");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(
        Node::new("calc", "core.CalculateMany").with_config(serde_json::json!({
            "variables": { "offset": 1 },
            "expressions": [
                { "name": "base", "expression": "2 + 3" },
                { "name": "answer", "expression": "base * 4 + offset" }
            ],
            "output_var": "calculation"
        })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "calc"));
    workflow.add_edge(Edge::new("e2", "calc", "end"));

    let engine = WorkflowEngine::new(registry());
    let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());
    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(outcome.variables.get("base"), Some(&serde_json::json!(5)));
    assert_eq!(
        outcome.variables.get("answer"),
        Some(&serde_json::json!(21))
    );
    assert_eq!(
        outcome.variables.get("calculation"),
        Some(&serde_json::json!({ "base": 5, "answer": 21 }))
    );
}

#[test]
fn a_node_retries_until_it_succeeds() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(FlakyExecutor {
        calls: calls.clone(),
        failures: 2,
    });

    let mut workflow = Workflow::new("wf.retry");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut flaky = Node::new("flaky", "test.Flaky");
    flaky.retry = 2;
    flaky.retry_delay_ms = 5;
    workflow.add_node(flaky);
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "flaky"));
    workflow.add_edge(Edge::new("e2", "flaky", "end"));

    let outcome =
        WorkflowEngine::new(Arc::new(registry)).run(RunRequest::new(workflow), &RunControl::new());

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(outcome.nodes_executed, 3);
}

#[test]
fn common_result_mapping_publishes_a_selected_output() {
    let mut workflow = Workflow::new("wf.result-mapping");
    workflow.add_node(Node::new("start", "core.Start"));
    let mut calc = Node::new("calc", "core.Calculate");
    calc.config = serde_json::json!({
        "expression": "2 + 3 * 2",
        "output_var": "calculation_local"
    });
    calc.result_var = Some("answer".to_string());
    calc.result_port = Some("result".to_string());
    workflow.add_node(calc);
    workflow.add_node(
        Node::new("log", "core.Log")
            .with_config(serde_json::json!({ "message": "answer={{answer}}" })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "calc"));
    workflow.add_edge(Edge::new("e2", "calc", "log"));
    workflow.add_edge(Edge::new("e3", "log", "end"));

    let sink = Arc::new(CollectingEventSink::new());
    let outcome = WorkflowEngine::new(registry()).run(
        RunRequest::new(workflow).with_event_sink(sink.clone()),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(outcome.variables.get("answer"), Some(&serde_json::json!(8)));
    assert_eq!(
        outcome.variables.get("calculation_local"),
        Some(&serde_json::json!(8))
    );
    assert!(sink.snapshot().iter().any(|envelope| matches!(
        &envelope.event,
        ExecutionEvent::Log { message, .. } if message == "answer=8"
    )));
}

#[test]
fn seeds_and_overrides_variables() {
    let mut workflow = linear_workflow();
    workflow.variables.insert(
        "name".to_string(),
        Variable {
            value: serde_json::json!("default"),
            ..Variable::default()
        },
    );

    let mut overrides = BTreeMap::new();
    overrides.insert("name".to_string(), serde_json::json!("override"));

    let engine = WorkflowEngine::new(registry());
    let outcome = engine.run(
        RunRequest::new(workflow).with_variables(overrides),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(
        outcome.variables.get("name"),
        Some(&serde_json::json!("override"))
    );
}

#[test]
fn static_validation_rejects_a_broken_workflow() {
    let mut workflow = Workflow::new("wf.broken");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "missing"));

    let engine = WorkflowEngine::new(registry());
    let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());

    assert_eq!(outcome.status, RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_VALIDATION");
}

#[test]
fn unknown_node_type_fails_capability_validation() {
    let mut workflow = Workflow::new("wf.unknown");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("ghost", "does.Not.Exist"));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "ghost"));
    workflow.add_edge(Edge::new("e2", "ghost", "end"));

    let engine = WorkflowEngine::new(registry());
    let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());

    assert_eq!(outcome.status, RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_VALIDATION");
}

#[test]
fn validation_can_be_disabled_to_reach_the_engine_error() {
    let mut workflow = Workflow::new("wf.unknown");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("ghost", "does.Not.Exist"));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "ghost"));
    workflow.add_edge(Edge::new("e2", "ghost", "end"));

    let engine = WorkflowEngine::new(registry()).with_options(EngineOptions {
        validate: false,
        ..EngineOptions::default()
    });
    let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());

    assert_eq!(outcome.status, RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_UNKNOWN_NODE_TYPE");
}

#[test]
fn policy_denial_blocks_execution() {
    let engine = WorkflowEngine::new(registry()).with_policy(Arc::new(DenyAllPolicy));
    let outcome = engine.run(RunRequest::new(linear_workflow()), &RunControl::new());

    assert_eq!(outcome.status, RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_PERMISSION_DENIED");
    assert_eq!(outcome.nodes_executed, 0);
}

#[test]
fn allow_all_policy_permits_execution() {
    let engine = WorkflowEngine::new(registry()).with_policy(Arc::new(AllowAllPolicy));
    let outcome = engine.run(RunRequest::new(linear_workflow()), &RunControl::new());
    assert!(outcome.is_success(), "{:?}", outcome.failure);
}

#[test]
fn edge_guards_prune_branches() {
    let mut workflow = Workflow::new("wf.branch");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(
        Node::new("calc", "core.Calculate")
            .with_config(serde_json::json!({ "expression": "10", "output_var": "score" })),
    );
    workflow.add_node(
        Node::new("high", "core.Log").with_config(serde_json::json!({ "message": "high" })),
    );
    workflow.add_node(
        Node::new("low", "core.Log").with_config(serde_json::json!({ "message": "low" })),
    );
    workflow.add_node(Node::new("end", "core.End"));

    workflow.add_edge(Edge::new("e1", "start", "calc"));
    let mut to_high = Edge::new("e2", "calc", "high");
    to_high.condition = Some("score > 5".to_string());
    let mut to_low = Edge::new("e3", "calc", "low");
    to_low.condition = Some("score <= 5".to_string());
    workflow.add_edge(to_high);
    workflow.add_edge(to_low);
    workflow.add_edge(Edge::new("e4", "high", "end"));
    workflow.add_edge(Edge::new("e5", "low", "end"));

    let engine = WorkflowEngine::new(registry());
    let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    // start, calc, high, end = 4; `low` was pruned.
    assert_eq!(outcome.nodes_executed, 4);
}

#[test]
fn a_paused_run_steps_then_resumes() {
    let engine = WorkflowEngine::new(registry());
    let control = RunControl::new();
    control.pause();

    let workflow = linear_workflow();
    let thread_control = control.clone();
    let handle = {
        let engine = engine.clone();
        std::thread::spawn(move || engine.run(RunRequest::new(workflow), &thread_control))
    };

    // The run is parked on the first node boundary.
    std::thread::sleep(Duration::from_millis(50));
    assert!(!handle.is_finished());

    // A single step lets exactly one node through.
    control.step();
    std::thread::sleep(Duration::from_millis(50));
    assert!(!handle.is_finished(), "run should re-pause after one step");

    // Resuming lets the remainder finish.
    control.resume();
    let outcome = handle.join().expect("run thread");
    assert_eq!(
        outcome.status,
        RunStatus::Completed,
        "{:?}",
        outcome.failure
    );
    assert_eq!(outcome.nodes_executed, 5);
}

#[test]
fn a_node_breakpoint_pauses_before_execution_and_resumes() {
    let mut workflow = linear_workflow();
    workflow.node_mut("calc").unwrap().breakpoint = true;

    let sink = Arc::new(CollectingEventSink::new());
    let engine = WorkflowEngine::new(registry());
    let running = engine.spawn(RunRequest::new(workflow).with_event_sink(sink.clone()));

    let mut paused = false;
    for _ in 0..100 {
        if running.control().is_paused() {
            paused = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if !paused {
        running.control().cancel();
        let outcome = running.join();
        panic!("breakpoint did not pause the run: {:?}", outcome.failure);
    }

    running.control().resume();
    let outcome = running.join();
    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(outcome.nodes_executed, 5);

    let events = sink.snapshot();
    assert!(events
        .iter()
        .any(|envelope| matches!(envelope.event, ExecutionEvent::RunPaused)));
    assert!(events.iter().any(|envelope| matches!(
        &envelope.event,
        ExecutionEvent::Log { message, .. } if message.contains("breakpoint hit before node `calc`")
    )));
}

#[test]
fn stepping_a_breakpoint_node_executes_it_without_double_pausing() {
    let mut workflow = linear_workflow();
    workflow.node_mut("start").unwrap().breakpoint = true;

    let sink = Arc::new(CollectingEventSink::new());
    let engine = WorkflowEngine::new(registry());
    let running = engine.spawn(
        RunRequest::new(workflow)
            .with_start_paused(true)
            .with_event_sink(sink.clone()),
    );

    for _ in 0..100 {
        if running.control().is_paused() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(running.control().is_paused(), "run should start paused");
    running.control().step();

    let mut executed_start = false;
    for _ in 0..100 {
        executed_start = sink.snapshot().iter().any(|envelope| {
            matches!(
                &envelope.event,
                ExecutionEvent::NodeFinished { node_id, .. } if node_id == "start"
            )
        });
        if executed_start {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        executed_start,
        "Step must execute the breakpointed node once"
    );

    running.control().cancel();
    let outcome = running.join();
    assert_eq!(outcome.status, RunStatus::Cancelled);
}

#[test]
fn cancelling_a_paused_run_stops_it() {
    let engine = WorkflowEngine::new(registry());
    let control = RunControl::new();
    control.pause();

    let thread_control = control.clone();
    let handle = {
        let engine = engine.clone();
        std::thread::spawn(move || engine.run(RunRequest::new(linear_workflow()), &thread_control))
    };

    std::thread::sleep(Duration::from_millis(30));
    control.cancel();
    let outcome = handle.join().expect("run thread");

    assert_eq!(outcome.status, RunStatus::Cancelled);
    assert_eq!(outcome.failure.unwrap().code, "E_CANCELLED");
}

#[test]
fn spawned_runs_report_their_outcome() {
    let engine = WorkflowEngine::new(registry());
    let running = engine.spawn(RunRequest::new(linear_workflow()).with_run_id("run-fixed"));
    assert_eq!(running.run_id(), "run-fixed");
    let outcome = running.join();
    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(outcome.run_id, "run-fixed");
}

#[test]
fn delay_node_honours_cancellation() {
    let mut workflow = Workflow::new("wf.delay");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(
        Node::new("wait", "system.Delay").with_config(serde_json::json!({ "duration_ms": 5000 })),
    );
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "wait"));
    workflow.add_edge(Edge::new("e2", "wait", "end"));

    let engine = WorkflowEngine::new(registry());
    let control = RunControl::new();
    let thread_control = control.clone();
    let handle = std::thread::spawn(move || engine.run(RunRequest::new(workflow), &thread_control));

    std::thread::sleep(Duration::from_millis(50));
    control.cancel();
    let started = std::time::Instant::now();
    let outcome = handle.join().expect("run thread");

    assert_eq!(outcome.status, RunStatus::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancellation should interrupt the delay promptly"
    );
}

#[test]
fn secret_variables_are_redacted_in_the_outcome() {
    let mut workflow = linear_workflow();
    workflow.variables.insert(
        "api_key".to_string(),
        Variable {
            value: serde_json::json!("hunter2"),
            secret: true,
            ..Variable::default()
        },
    );

    let mut overrides = BTreeMap::new();
    overrides.insert("api_key".to_string(), serde_json::json!("s3cr3t"));

    let engine = WorkflowEngine::new(registry());
    let outcome = engine.run(
        RunRequest::new(workflow).with_variables(overrides),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(
        outcome.variables.get("api_key"),
        Some(&serde_json::json!("***")),
        "a secret variable must never reach run snapshots or reports"
    );
    assert_eq!(
        outcome.variables.get("result"),
        Some(&serde_json::json!(8)),
        "non-secret variables are unaffected"
    );
}

#[derive(Debug)]
struct DataProducer;

impl NodeExecutor for DataProducer {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            outputs: vec![PortDescriptor::new(
                "out",
                "Out",
                PortKind::Output,
                ValueType::Any,
            )],
            ..NodeDescriptor::new("test.DataProducer", "Data Producer", "Test")
        }
    }

    fn execute(
        &self,
        _input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> nodara_core::NodeResult<NodeOutput> {
        Ok(NodeOutput::new().with_output("out", serde_json::json!(7)))
    }
}

#[derive(Debug)]
struct DataConsumer {
    seen: Arc<Mutex<Option<serde_json::Value>>>,
}

impl NodeExecutor for DataConsumer {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![PortDescriptor::new(
                "in",
                "In",
                PortKind::Input,
                ValueType::Any,
            )],
            ..NodeDescriptor::new("test.DataConsumer", "Data Consumer", "Test")
        }
    }

    fn execute(
        &self,
        input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> nodara_core::NodeResult<NodeOutput> {
        *self.seen.lock().expect("consumer input") = input.input("in").cloned();
        Ok(NodeOutput::new())
    }
}

fn data_registry(consumer: Arc<Mutex<Option<serde_json::Value>>>) -> Arc<CapabilityRegistry> {
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    registry.register(DataProducer);
    registry.register(DataConsumer { seen: consumer });
    Arc::new(registry)
}

#[test]
fn data_edges_transfer_values_without_becoming_control_edges() {
    let seen = Arc::new(Mutex::new(None));
    let engine = WorkflowEngine::new(data_registry(seen.clone()));
    let sink = Arc::new(CollectingEventSink::new());
    let mut workflow = Workflow::new("wf.data");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("producer", "test.DataProducer"));
    workflow.add_node(Node::new("consumer", "test.DataConsumer"));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("start-producer", "start", "producer"));
    workflow.add_edge(Edge::new("producer-consumer", "producer", "consumer"));
    workflow.add_edge(Edge {
        id: "producer-value".to_string(),
        source: "producer".to_string(),
        target: "consumer".to_string(),
        kind: nodara_schema::workflow::EdgeKind::Data,
        source_port: Some("out".to_string()),
        target_port: Some("in".to_string()),
        ..Edge::new("ignored", "producer", "consumer")
    });
    workflow.add_edge(Edge::new("consumer-end", "consumer", "end"));

    let outcome = engine.run(
        RunRequest::new(workflow).with_event_sink(sink.clone()),
        &RunControl::new(),
    );

    assert_eq!(
        outcome.status,
        RunStatus::Completed,
        "{:?}",
        outcome.failure
    );
    assert_eq!(
        *seen.lock().expect("consumer input"),
        Some(serde_json::json!(7))
    );
    let events = sink.snapshot();
    assert!(events.iter().any(|envelope| matches!(
        &envelope.event,
        ExecutionEvent::DataTransferred { edge_id, .. } if edge_id == "producer-value"
    )));
    assert!(events.iter().any(|envelope| matches!(
        &envelope.event,
        ExecutionEvent::EdgeActivated { edge_id, .. } if edge_id == "producer-consumer"
    )));
}

#[test]
fn a_data_edge_does_not_activate_its_target() {
    let seen = Arc::new(Mutex::new(None));
    let engine = WorkflowEngine::new(data_registry(seen.clone()));
    let mut workflow = Workflow::new("wf.data-only");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("producer", "test.DataProducer"));
    workflow.add_node(Node::new("consumer", "test.DataConsumer"));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("start-producer", "start", "producer"));
    workflow.add_edge(Edge::new("producer-end", "producer", "end"));
    workflow.add_edge(Edge {
        id: "producer-value".to_string(),
        source: "producer".to_string(),
        target: "consumer".to_string(),
        kind: nodara_schema::workflow::EdgeKind::Data,
        source_port: Some("out".to_string()),
        target_port: Some("in".to_string()),
        ..Edge::new("ignored", "producer", "consumer")
    });

    let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());

    assert_eq!(
        outcome.status,
        RunStatus::Completed,
        "{:?}",
        outcome.failure
    );
    assert!(seen.lock().expect("consumer input").is_none());
}

#[test]
fn a_control_edge_does_not_transfer_data() {
    let seen = Arc::new(Mutex::new(None));
    let engine = WorkflowEngine::new(data_registry(seen.clone()));
    let mut workflow = Workflow::new("wf.control-only");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("producer", "test.DataProducer"));
    workflow.add_node(Node::new("consumer", "test.DataConsumer"));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("start-producer", "start", "producer"));
    workflow.add_edge(Edge::new("producer-consumer", "producer", "consumer"));
    workflow.add_edge(Edge::new("consumer-end", "consumer", "end"));

    let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());

    assert_eq!(
        outcome.status,
        RunStatus::Completed,
        "{:?}",
        outcome.failure
    );
    assert!(seen.lock().expect("consumer input").is_none());
}
