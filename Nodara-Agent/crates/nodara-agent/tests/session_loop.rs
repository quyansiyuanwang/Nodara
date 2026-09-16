//! The agent loop, against a scripted runtime.

mod support;

use std::time::Duration;

use nodara_agent::{
    Agent, AgentConfig, AgentError, ExplainTarget, GuardrailPolicy, MockProvider, RunApprovalMode,
    ToolPolicy, ToolSelector,
};
use serde_json::json;
use support::{FakeRuntime, RunScript};

fn draft(id: &str, message: &str) -> String {
    json!({
        "schema_version": "2.1",
        "id": id,
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "log", "type": "core.Log", "config": { "message": message } },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "kind": "control", "source": "start", "target": "log" },
            { "id": "e2", "kind": "control", "source": "log", "target": "end" }
        ]
    })
    .to_string()
}

fn config(base: &str) -> AgentConfig {
    AgentConfig {
        runtime_url: base.to_string(),
        run_timeout: Duration::from_secs(5),
        auto_run: true,
        tool_selector: ToolSelector::default(),
        ..AgentConfig::default()
    }
}

#[test]
fn plans_publishes_and_runs_end_to_end() {
    let runtime = FakeRuntime::start(vec![RunScript::Immediate {
        status: "completed".to_string(),
        code: None,
    }]);
    let provider = MockProvider::new([draft("wf.one", "hello")]);
    let agent = Agent::new(&provider, config(runtime.base()));

    let outcome = agent
        .plan_and_run("log a greeting", &[])
        .expect("the session completes");

    assert!(outcome.accepted);
    assert_eq!(outcome.session_id.as_deref(), Some("session-1"));
    assert_eq!(outcome.report.status, "completed");
    assert_eq!(outcome.report.attempts, 1);
    assert_eq!(outcome.report.nodes_executed, 3);
    assert_eq!(outcome.report.run_id.as_deref(), Some("run-1"));
    assert!(outcome.report.events_observed >= 4);
    assert!(outcome
        .report
        .highlights
        .iter()
        .any(|line| line.contains("hello from the fake runtime")));

    // The runtime must have seen the session, the preview and the run.
    let recorded = runtime.recorded();
    assert_eq!(recorded.sessions_created, 1);
    assert_eq!(recorded.plans_published, 1);
    assert!(recorded.messages_appended >= 2);
    assert_eq!(recorded.runs_started, 1);
    assert_eq!(
        recorded.session_ids_on_runs,
        vec![Some("session-1".to_string())],
        "the run must be bound to the session so gated nodes can ask for approval"
    );
    assert!(recorded.statuses_set.contains(&"completed".to_string()));

    let session = runtime.session();
    assert_eq!(session["plan"]["valid"], true);
    assert_eq!(session["plan"]["workflow"]["id"], "wf.one");
}

#[test]
fn continued_session_receives_node_snapshots_and_screenshot_images() {
    let runtime = FakeRuntime::start(vec![RunScript::Immediate {
        status: "completed".to_string(),
        code: None,
    }]);
    let provider = MockProvider::new([
        draft("wf.capture", "capture"),
        draft("wf.inspect", "inspect"),
    ]);
    let first_agent = Agent::new(&provider, config(runtime.base()));
    let first = first_agent
        .plan_and_run("capture the desktop", &[])
        .expect("the first session completes");

    let mut second_config = config(runtime.base());
    second_config.session_id = first.session_id.clone();
    second_config.auto_run = false;
    let second_agent = Agent::new(&provider, second_config);
    second_agent
        .plan("find the button in the screenshot", &[])
        .expect("the continued session plans");

    let calls = provider.calls();
    let evidence = calls[1]
        .messages
        .iter()
        .find(|message| message.content.contains("[runtime evidence for run-1]"))
        .expect("the previous run evidence is included in the next turn");
    assert_eq!(
        evidence.images.len(),
        1,
        "the screenshot is a native image part"
    );
    assert_eq!(evidence.images[0].media_type, "image/png");
    assert!(evidence.content.contains("variables_before"));
    assert!(evidence.content.contains("variables_after"));
    assert!(evidence.content.contains("data_transferred"));
}

#[test]
fn replans_after_a_fixable_run_failure() {
    let runtime = FakeRuntime::start(vec![
        RunScript::Immediate {
            status: "failed".to_string(),
            code: Some("E_INVALID_CONFIG".to_string()),
        },
        RunScript::Immediate {
            status: "completed".to_string(),
            code: None,
        },
    ]);
    let provider = MockProvider::new([draft("wf.first", "broken"), draft("wf.second", "fixed")]);
    let agent = Agent::new(&provider, config(runtime.base()));

    let outcome = agent
        .plan_and_run("log something", &[])
        .expect("the session completes after re-planning");

    assert_eq!(outcome.report.status, "completed");
    assert_eq!(outcome.report.attempts, 2, "the second plan must be used");
    assert_eq!(runtime.recorded().runs_started, 2);
    assert_eq!(
        outcome.workflow.as_ref().unwrap().id,
        "wf.second",
        "the agent must keep the workflow that actually worked"
    );
    // The failure must have reached the model as feedback.
    let calls = provider.calls();
    let second_turn = calls
        .last()
        .expect("a second planning call")
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        second_turn.contains("A previous attempt failed"),
        "the re-plan prompt must carry the failure"
    );
}

#[test]
fn a_policy_refusal_is_not_replanned() {
    let runtime = FakeRuntime::start(vec![RunScript::Immediate {
        status: "failed".to_string(),
        code: Some("E_PERMISSION_DENIED".to_string()),
    }]);
    let provider = MockProvider::new([draft("wf.denied", "blocked")]);
    let agent = Agent::new(&provider, config(runtime.base()));

    let outcome = agent
        .plan_and_run("press a key", &[])
        .expect("session runs");

    assert_eq!(outcome.report.status, "failed");
    assert_eq!(outcome.report.attempts, 1);
    assert_eq!(runtime.recorded().runs_started, 1);
    assert_eq!(provider.calls().len(), 1, "re-planning must not occur");
    let markdown = outcome.report.to_markdown();
    assert!(markdown.contains("E_PERMISSION_DENIED"));
}

#[test]
fn an_allowlist_refusal_stops_before_anything_runs() {
    let runtime = FakeRuntime::start(vec![RunScript::Immediate {
        status: "completed".to_string(),
        code: None,
    }]);
    let provider = MockProvider::new([json!({
        "schema_version": "2.1",
        "id": "wf.gated",
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "keys", "type": "windows.Input.Keyboard", "config": { "keys": "a" } },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "kind": "control", "source": "start", "target": "keys" },
            { "id": "e2", "kind": "control", "source": "keys", "target": "end" }
        ]
    })
    .to_string()]);

    let mut config = config(runtime.base());
    config.guardrails = GuardrailPolicy {
        tools: ToolPolicy::allow_only(["core.Start", "core.End", "core.Log"]),
    };
    let agent = Agent::new(&provider, config);

    let error = agent
        .plan_and_run("press a key", &[])
        .expect_err("the guardrail must refuse");
    assert!(matches!(error, AgentError::Refused(_)), "got {error:?}");
    assert_eq!(
        runtime.recorded().runs_started,
        0,
        "a refused plan must never reach the runtime"
    );
}

#[test]
fn interrupt_and_resume_reach_the_runtime() {
    let runtime = FakeRuntime::start(vec![RunScript::Hanging]);
    let provider = MockProvider::new([draft("wf.long", "waiting")]);
    let agent = Agent::new(&provider, config(runtime.base()));

    // Start a run directly through the client so it stays running.
    let workflow: nodara_schema::Workflow =
        serde_json::from_str(&draft("wf.long", "waiting")).expect("draft parses");
    let started = agent
        .client()
        .start_run(&workflow, json!({}))
        .expect("the run starts");
    let run_id = started["id"].as_str().unwrap().to_string();
    assert_eq!(started["status"], "running");

    let paused = agent.pause_run(&run_id).expect("pause is accepted");
    assert_eq!(paused["status"], "paused");
    let resumed = agent.resume_run(&run_id).expect("resume is accepted");
    assert_eq!(resumed["status"], "running");
    let cancelled = agent.cancel_run(&run_id).expect("cancel is accepted");
    assert_eq!(cancelled["status"], "cancelled");

    let statuses = runtime.recorded().statuses_set;
    assert_eq!(
        statuses,
        vec![
            "paused".to_string(),
            "running".to_string(),
            "cancelled".to_string()
        ]
    );
}

#[test]
fn a_rejected_plan_produces_a_report_without_running() {
    let runtime = FakeRuntime::start(vec![]);
    // Missing `core.End`: the runtime would reject this, and our local
    // capability-aware validation agrees.
    let provider = MockProvider::new([json!({
        "schema_version": "2.1",
        "id": "wf.invalid",
        "nodes": [{ "id": "start", "type": "core.Start" }],
        "edges": []
    })
    .to_string()]);
    let mut config = config(runtime.base());
    config.auto_run = false;
    // One attempt only, so a single scripted reply is enough.
    config.max_repairs = 0;
    let agent = Agent::new(&provider, config);

    let outcome = agent.plan("do nothing", &[]).expect("planning returns");
    assert!(!outcome.accepted);
    assert_eq!(outcome.report.status, "rejected");
    assert_eq!(runtime.recorded().runs_started, 0);
    assert!(outcome
        .report
        .to_markdown()
        .contains("did not accept a plan"));
}

#[test]
fn the_session_is_published_for_the_studio_to_render() {
    let runtime = FakeRuntime::start(vec![]);
    let provider = MockProvider::new([draft("wf.preview", "hi")]);
    let mut config = config(runtime.base());
    config.auto_run = false;
    let agent = Agent::new(&provider, config);

    let outcome = agent.plan("log something", &[]).expect("planning returns");
    assert!(outcome.accepted);
    assert_eq!(outcome.session_id.as_deref(), Some("session-1"));

    let recorded = runtime.recorded();
    assert_eq!(recorded.sessions_created, 1);
    assert_eq!(recorded.plans_published, 1);
    assert!(recorded.messages_appended >= 1);

    let session = runtime.session();
    assert_eq!(session["plan"]["workflow"]["id"], "wf.preview");
    assert_eq!(session["plan"]["errors"], 0);
    assert_eq!(session["status"], "ready");
}

#[test]
fn a_base_document_is_modified_rather_than_regenerated() {
    let runtime = FakeRuntime::start(vec![]);
    let provider = MockProvider::new([draft("wf.existing", "added a line")]);
    let mut config = config(runtime.base());
    config.auto_run = false;
    config.base_workflow = Some(
        serde_json::from_str(&draft("wf.existing", "original")).expect("the base workflow parses"),
    );
    let agent = Agent::new(&provider, config);

    let outcome = agent.plan("add a log line", &[]).expect("planning returns");
    assert!(outcome.accepted);

    let first_turn = &provider.calls()[0].messages[1].content;
    assert!(first_turn.contains("workflow the operator is currently editing"));
    assert!(first_turn.contains("wf.existing"));
}

#[test]
fn explain_asks_for_prose_and_includes_the_evidence() {
    let runtime = FakeRuntime::start(vec![]);
    let provider = MockProvider::new([
        "The workflow reads the clipboard and logs it. The gate is `system.Clipboard`.".to_string(),
    ]);
    let agent = Agent::new(&provider, config(runtime.base()));

    let workflow: nodara_schema::Workflow =
        serde_json::from_str(&draft("wf.explain", "hello")).expect("parses");
    let explanation = agent
        .explain(&ExplainTarget::workflow(workflow))
        .expect("explain returns");

    assert!(explanation.contains("system.Clipboard"));

    let request = &provider.calls()[0];
    assert!(!request.json_mode, "an explanation is prose, not JSON");
    let prompt = &request.messages[1].content;
    assert!(prompt.contains("wf.explain"), "the workflow must be quoted");
    assert!(
        prompt.contains("Validation diagnostics"),
        "the runtime's own diagnostics must be included"
    );
    assert!(prompt.contains("most useful next action"));
}

#[test]
fn explain_can_account_for_a_run() {
    let runtime = FakeRuntime::start(vec![RunScript::Immediate {
        status: "failed".to_string(),
        code: Some("E_INVALID_CONFIG".to_string()),
    }]);
    let provider =
        MockProvider::new(["The run failed because the expression was malformed.".to_string()]);
    let agent = Agent::new(&provider, config(runtime.base()));

    let workflow: nodara_schema::Workflow =
        serde_json::from_str(&draft("wf.run", "hello")).expect("parses");
    let started = agent
        .client()
        .start_run(&workflow, json!({}))
        .expect("the run starts");
    let run_id = started["id"].as_str().unwrap().to_string();

    let explanation = agent
        .explain(&ExplainTarget::run(&run_id).with_run(&run_id))
        .expect("explain returns");
    assert!(explanation.contains("failed"));

    let prompt = &provider.calls()[0].messages[1].content;
    assert!(prompt.contains("Run snapshot"));
    assert!(prompt.contains("Execution events"));
    assert!(prompt.contains("E_INVALID_CONFIG"));
}

#[test]
fn the_audit_endpoint_is_reachable_for_the_studio() {
    let runtime = FakeRuntime::start(vec![]);
    let provider = MockProvider::new([""]);
    let agent = Agent::new(&provider, config(runtime.base()));
    // The fake runtime in this suite does not implement /audit; the point here is
    // that a missing endpoint surfaces as a structured runtime error rather than
    // a panic, which is what the Studio's Audit tab relies on.
    let error = agent.client().audit(None, None).expect_err("no such route");
    assert!(matches!(error, AgentError::Runtime { status: 404, .. }));
}

#[test]
fn manual_mode_starts_paused_with_session_approval() {
    let runtime = FakeRuntime::start(vec![RunScript::Hanging]);
    let provider = MockProvider::new([draft("wf.manual", "manual")]);
    let mut config = config(runtime.base());
    config.start_paused = true;
    config.approval_mode = RunApprovalMode::Session;
    let agent = Agent::new(&provider, config);

    let outcome = agent.plan_and_run("run manually", &[]).expect("run starts");
    assert_eq!(outcome.report.status, "paused");
    let recorded = runtime.recorded();
    assert_eq!(recorded.start_paused_on_runs, vec![true]);
    assert_eq!(recorded.approval_on_runs, vec![Some("session".to_string())]);
}

#[test]
fn all_mode_uses_automatic_approval_without_pausing() {
    let runtime = FakeRuntime::start(vec![RunScript::Immediate {
        status: "completed".to_string(),
        code: None,
    }]);
    let provider = MockProvider::new([draft("wf.all", "automatic")]);
    let mut config = config(runtime.base());
    config.start_paused = false;
    config.approval_mode = RunApprovalMode::Auto;
    let agent = Agent::new(&provider, config);

    let outcome = agent
        .plan_and_run("run automatically", &[])
        .expect("run starts");
    assert_eq!(outcome.report.status, "completed");
    let recorded = runtime.recorded();
    assert_eq!(recorded.start_paused_on_runs, vec![false]);
    assert_eq!(recorded.approval_on_runs, vec![Some("auto".to_string())]);
}
