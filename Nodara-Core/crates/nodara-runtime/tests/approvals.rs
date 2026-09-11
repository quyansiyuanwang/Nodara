//! The operator-approval path, end to end over HTTP.
//!
//! This is the behaviour the architecture document is most insistent about: a
//! gated capability must not run until policy — and, when the policy says so, a
//! human — has authorised it, and the decision must be recorded.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nodara_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_runtime::{RuntimeBuilder, RuntimeConfig, RuntimeState};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};
use serde_json::{json, Value};
use tower::ServiceExt;

/// A deliberately gated node: it declares a permission and a side effect.
#[derive(Debug, Default)]
struct GatedExecutor;

impl NodeExecutor for GatedExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![PortDescriptor::new(
                "in",
                "In",
                PortKind::Input,
                ValueType::Any,
            )],
            outputs: vec![PortDescriptor::new(
                "out",
                "Out",
                PortKind::Output,
                ValueType::Any,
            )],
            dangerous: true,
            permissions: vec!["test.sideeffect".to_string()],
            allows_additional_config: true,
            ..NodeDescriptor::new("test.Gated", "Gated", "Test")
        }
    }

    fn execute(&self, _input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        context.log(nodara_schema::LogLevel::Info, "gated node ran");
        Ok(NodeOutput::new().with_variable("gated_ran", json!(true)))
    }
}

#[derive(Debug, Default)]
struct BoomExecutor;

impl NodeExecutor for BoomExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            dangerous: true,
            permissions: vec!["test.sideeffect".to_string()],
            ..NodeDescriptor::new("test.Boom", "Boom", "Test")
        }
    }

    fn execute(
        &self,
        _input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> NodeResult<NodeOutput> {
        Err(NodeError::Execution("never permitted".to_string()))
    }
}

async fn state(auto_approve: bool, timeout: Duration) -> Arc<RuntimeState> {
    let config = RuntimeConfig {
        auto_approve,
        approval_timeout: timeout,
        ..RuntimeConfig::default()
    };
    let mut builder = RuntimeBuilder::new(config);
    builder.register_executor(GatedExecutor);
    builder.register_executor(BoomExecutor);
    builder.build().expect("runtime builds")
}

async fn call(state: &Arc<RuntimeState>, request: Request<Body>) -> (StatusCode, Value) {
    let response = nodara_runtime::api::router(state.clone())
        .oneshot(request)
        .await
        .expect("router responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        },
    )
}

fn post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request builds")
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request builds")
}

/// `Start -> Gated -> End`, attached to a session.
fn gated_workflow() -> Value {
    json!({
        "schema_version": "2.0",
        "id": "wf.gated",
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "gated", "type": "test.Gated" },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "source": "start", "target": "gated" },
            { "id": "e2", "source": "gated", "target": "end" }
        ]
    })
}

#[tokio::test]
async fn a_gated_node_waits_for_an_operator_and_then_runs() {
    let state = state(false, Duration::from_secs(10)).await;

    let (status, session) = call(
        &state,
        post(
            "/api/v1/agent/sessions",
            json!({ "goal": "do the gated thing", "provider": "test" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = session["id"].as_str().unwrap().to_string();
    assert_eq!(session["status"], "draft");

    let (status, run) = call(
        &state,
        post(
            "/api/v1/runs",
            json!({ "workflow": gated_workflow(), "session_id": session_id }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{run}");
    let run_id = run["id"].as_str().unwrap().to_string();
    assert_eq!(
        state.sessions.get(&session_id).unwrap().run_id.as_deref(),
        Some(run_id.as_str())
    );

    // The run must now be blocked on an approval that names the node.
    let approval = loop {
        let (_, pending) = call(&state, get("/api/v1/agent/approvals")).await;
        if let Some(entry) = pending.as_array().and_then(|list| list.first()) {
            break entry.clone();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let approval_id = approval["approval"]["id"].as_str().unwrap().to_string();
    assert_eq!(approval["session_id"], session_id);
    assert_eq!(approval["approval"]["node_type"], "test.Gated");
    assert_eq!(approval["approval"]["permissions"][0], "test.sideeffect");
    assert_eq!(
        state.sessions.get(&session_id).unwrap().status,
        nodara_schema::SessionStatus::AwaitingApproval
    );

    // The run is genuinely parked: nothing has executed past the gate.
    let snapshot = state.runs.get(&run_id).unwrap().snapshot();
    assert!(!snapshot.status.is_terminal());

    // Approve, and the run must complete.
    let (status, decided) = call(
        &state,
        post(
            &format!("/api/v1/agent/sessions/{session_id}/approvals/{approval_id}"),
            json!({ "decision": "approved", "decided_by": "tester" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decided["approvals"][0]["decision"], "approved");
    assert_eq!(decided["approvals"][0]["decided_by"], "tester");

    let outcome = loop {
        let snapshot = state.runs.get(&run_id).unwrap().snapshot();
        if snapshot.status.is_terminal() {
            break snapshot;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(
        outcome.status,
        nodara_schema::RunStatus::Completed,
        "{outcome:?}"
    );
    assert_eq!(outcome.variables.get("gated_ran"), Some(&json!(true)));
}

#[tokio::test]
async fn a_refused_approval_stops_the_node() {
    let state = state(false, Duration::from_secs(10)).await;

    let (_, session) = call(
        &state,
        post("/api/v1/agent/sessions", json!({ "goal": "nope" })),
    )
    .await;
    let session_id = session["id"].as_str().unwrap().to_string();
    let (_, run) = call(
        &state,
        post(
            "/api/v1/runs",
            json!({ "workflow": gated_workflow(), "session_id": session_id }),
        ),
    )
    .await;
    let run_id = run["id"].as_str().unwrap().to_string();

    let approval_id = loop {
        let (_, pending) = call(&state, get("/api/v1/agent/approvals")).await;
        if let Some(entry) = pending.as_array().and_then(|list| list.first()) {
            break entry["approval"]["id"].as_str().unwrap().to_string();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };

    call(
        &state,
        post(
            &format!("/api/v1/agent/sessions/{session_id}/approvals/{approval_id}"),
            json!({ "decision": "denied", "decided_by": "tester" }),
        ),
    )
    .await;

    let outcome = loop {
        let snapshot = state.runs.get(&run_id).unwrap().snapshot();
        if snapshot.status.is_terminal() {
            break snapshot;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(outcome.status, nodara_schema::RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_PERMISSION_DENIED");
}

#[tokio::test]
async fn an_unanswered_request_is_refused_rather_than_allowed() {
    // A 40ms budget: nobody answers, so the gate must close.
    let state = state(false, Duration::from_millis(40)).await;
    let (_, session) = call(
        &state,
        post("/api/v1/agent/sessions", json!({ "goal": "timeout" })),
    )
    .await;
    let session_id = session["id"].as_str().unwrap().to_string();
    let (_, run) = call(
        &state,
        post(
            "/api/v1/runs",
            json!({ "workflow": gated_workflow(), "session_id": session_id }),
        ),
    )
    .await;
    let run_id = run["id"].as_str().unwrap().to_string();

    let outcome = loop {
        let snapshot = state.runs.get(&run_id).unwrap().snapshot();
        if snapshot.status.is_terminal() {
            break snapshot;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(outcome.status, nodara_schema::RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_PERMISSION_DENIED");
}

#[tokio::test]
async fn a_run_without_a_session_is_refused_when_approval_is_required() {
    let state = state(false, Duration::from_millis(40)).await;
    let (_, run) = call(
        &state,
        post("/api/v1/runs", json!({ "workflow": gated_workflow() })),
    )
    .await;
    let run_id = run["id"].as_str().unwrap().to_string();
    let outcome = loop {
        let snapshot = state.runs.get(&run_id).unwrap().snapshot();
        if snapshot.status.is_terminal() {
            break snapshot;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(outcome.status, nodara_schema::RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_PERMISSION_DENIED");
}

#[tokio::test]
async fn auto_approve_still_runs_the_gate_and_records_it() {
    let state = state(true, Duration::from_secs(5)).await;
    let (_, run) = call(
        &state,
        post("/api/v1/runs", json!({ "workflow": gated_workflow() })),
    )
    .await;
    let run_id = run["id"].as_str().unwrap().to_string();
    let outcome = loop {
        let snapshot = state.runs.get(&run_id).unwrap().snapshot();
        if snapshot.status.is_terminal() {
            break snapshot;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(outcome.status, nodara_schema::RunStatus::Completed);
    // The decision is still visible in the event stream.
    let events = state.runs.get(&run_id).unwrap().history();
    assert!(events.iter().any(|envelope| matches!(
        &envelope.event,
        nodara_schema::ExecutionEvent::CapabilityDecision { decision, .. }
            if decision == "require_approval"
    )));
}

#[tokio::test]
async fn the_event_log_exposes_the_full_sequence() {
    let state = state(true, Duration::from_secs(5)).await;
    let (_, run) = call(
        &state,
        post("/api/v1/runs", json!({ "workflow": gated_workflow() })),
    )
    .await;
    let run_id = run["id"].as_str().unwrap().to_string();
    loop {
        if state
            .runs
            .get(&run_id)
            .unwrap()
            .snapshot()
            .status
            .is_terminal()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let (status, log) = call(&state, get(&format!("/api/v1/runs/{run_id}/event-log"))).await;
    assert_eq!(status, StatusCode::OK);
    let events = log.as_array().expect("event log is an array");
    assert!(
        events.len() >= 8,
        "expected a full sequence, got {} event(s)",
        events.len()
    );
    // Sequence numbers must be strictly increasing for a reconnecting client.
    let mut previous = None;
    for event in events {
        let seq = event["seq"].as_u64().unwrap();
        if let Some(previous) = previous {
            assert!(
                seq > previous,
                "sequence went backwards: {seq} <= {previous}"
            );
        }
        previous = Some(seq);
    }
    assert_eq!(events.first().unwrap()["event"]["type"], "run_started");
    assert_eq!(events.last().unwrap()["event"]["type"], "run_completed");
}

#[tokio::test]
async fn the_audit_log_records_every_decision_and_can_be_filtered() {
    let state = state(true, Duration::from_secs(5)).await;
    let (_, run) = call(
        &state,
        post("/api/v1/runs", json!({ "workflow": gated_workflow() })),
    )
    .await;
    let run_id = run["id"].as_str().unwrap().to_string();
    loop {
        if state
            .runs
            .get(&run_id)
            .unwrap()
            .snapshot()
            .status
            .is_terminal()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let (status, records) = call(&state, get("/api/v1/audit")).await;
    assert_eq!(status, StatusCode::OK);
    let records = records.as_array().expect("an array of records");
    assert!(
        records.len() >= 8,
        "expected a full account, got {}",
        records.len()
    );

    // The decision an operator cares about must be present and explicit.
    let decision = records
        .iter()
        .find(|record| record["capability"] == "test.Gated");
    let decision = decision.expect("the gated capability is recorded");
    assert_eq!(decision["category"], "capability_evaluated");
    assert_eq!(decision["capability"], "test.Gated");
    assert_eq!(decision["decision"], "require_approval");
    assert_eq!(decision["node_id"], "gated");

    // Every record carries a monotonic sequence number and a timestamp.
    let mut previous = None;
    for record in records {
        let seq = record["seq"].as_u64().unwrap();
        if let Some(previous) = previous {
            assert!(seq > previous);
        }
        previous = Some(seq);
        assert!(record["timestamp_ms"].as_u64().unwrap() > 0);
    }

    // Filtering by run keeps only that run's records.
    let (status, filtered) = call(
        &state,
        get(&format!("/api/v1/audit?run_id={run_id}&limit=5")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let filtered = filtered.as_array().unwrap();
    assert_eq!(filtered.len(), 5, "the limit must apply");
    for record in filtered {
        assert_eq!(record["run_id"], run_id);
    }
}
