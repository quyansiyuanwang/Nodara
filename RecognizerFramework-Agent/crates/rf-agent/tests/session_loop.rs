//! The agent loop, against a scripted runtime.

mod support;

use std::time::Duration;

use rf_agent::{
    Agent, AgentConfig, AgentError, GuardrailPolicy, MockProvider, ToolPolicy, ToolSelector,
};
use serde_json::json;
use support::{FakeRuntime, RunScript};

fn draft(id: &str, message: &str) -> String {
    json!({
        "schema_version": "2.0",
        "id": id,
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "log", "type": "core.Log", "config": { "message": message } },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "source": "start", "target": "log" },
            { "id": "e2", "source": "log", "target": "end" }
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
        "schema_version": "2.0",
        "id": "wf.gated",
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "keys", "type": "windows.Input.Keyboard", "config": { "keys": "a" } },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "source": "start", "target": "keys" },
            { "id": "e2", "source": "keys", "target": "end" }
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
    let workflow: rf_schema::Workflow =
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
        "schema_version": "2.0",
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
