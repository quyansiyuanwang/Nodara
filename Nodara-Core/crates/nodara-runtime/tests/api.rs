//! End-to-end tests for the public HTTP API.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nodara_core::{
    ExecutionContext, ExtensionDescriptor, ExtensionKind, NodeExecutor, NodeInput, NodeOutput,
};
use nodara_runtime::{RuntimeBuilder, RuntimeConfig, RuntimeState};
use nodara_schema::NodeDescriptor;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn state() -> Arc<RuntimeState> {
    RuntimeBuilder::new(RuntimeConfig::default())
        .build()
        .expect("runtime builds")
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
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request builds")
}

#[derive(Debug, Default)]
struct ArtifactExecutor;

impl NodeExecutor for ArtifactExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor::new("test.Artifact", "Artifact", "Test")
    }

    fn execute(
        &self,
        _input: NodeInput,
        context: &mut ExecutionContext,
    ) -> nodara_core::NodeResult<NodeOutput> {
        let meta = context
            .artifacts()
            .put("preview", "image/png", vec![1, 2, 3, 4]);
        Ok(NodeOutput::new()
            .with_output("artifact", serde_json::to_value(meta).unwrap_or_default()))
    }
}

fn valid_workflow() -> Value {
    json!({
        "schema_version": "2.0",
        "id": "wf.api",
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "calc", "type": "core.Calculate",
              "config": { "expression": "6 * 7", "output_var": "answer" } },
            { "id": "log", "type": "core.Log",
              "config": { "message": "answer is {{answer}}" } },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "source": "start", "target": "calc" },
            { "id": "e2", "source": "calc", "target": "log" },
            { "id": "e3", "source": "log", "target": "end" }
        ]
    })
}

#[tokio::test]
async fn health_reports_capabilities() {
    let state = state().await;
    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(body["node_types"].as_u64().unwrap() >= 6);
    assert!(body["extensions"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn node_types_include_builtins_with_schemas() {
    let state = state().await;
    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/node-types")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let types: Vec<String> = body["node_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|descriptor| descriptor["node_type"].as_str().unwrap().to_string())
        .collect();
    assert!(types.contains(&"core.Log".to_string()));
    assert!(types.contains(&"core.Calculate".to_string()));

    let log = body["node_types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|descriptor| descriptor["node_type"] == "core.Log")
        .unwrap();
    assert!(log["config_schema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("message")));
}

#[tokio::test]
async fn builders_can_register_unified_extension_metadata() {
    let mut builder = RuntimeBuilder::new(RuntimeConfig::default());
    builder.register_extension(ExtensionDescriptor {
        id: "test.ui.console".to_string(),
        name: "Test UI Console".to_string(),
        version: "1.0.0".to_string(),
        kind: ExtensionKind::Ui,
        source: "test-host".to_string(),
        description: Some("Registered by an embedding host.".to_string()),
        capabilities: vec!["Console".to_string()],
        permissions: Vec::new(),
        node_types: Vec::new(),
        loaded: true,
    });
    let state = builder.build().expect("runtime builds");
    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/extensions")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let registered = body["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|extension| extension["id"] == "test.ui.console")
        .expect("custom extension");
    assert_eq!(registered["kind"], "ui");
    assert_eq!(registered["source"], "test-host");
}

#[tokio::test]
async fn extensions_endpoint_lists_builtin_registration() {
    let state = state().await;
    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/extensions")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let builtin = body["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|extension| extension["id"] == "nodara.builtins")
        .expect("built-in extension");
    assert_eq!(builtin["kind"], "builtin");
    assert_eq!(builtin["loaded"], true);
    assert!(builtin["node_types"]
        .as_array()
        .unwrap()
        .contains(&json!("core.Log")));
}

#[tokio::test]
async fn workflow_schema_endpoint_publishes_the_installed_catalog() {
    let state = state().await;
    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/schema/workflow")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // The node types this runtime installed are the ones an editor completes.
    let enum_values = body["definitions"]["NodeType"]["enum"].as_array().unwrap();
    assert!(enum_values.contains(&json!("core.Log")));
    assert!(enum_values.contains(&json!("core.Calculate")));

    // Each type carries its own config schema, keyed on `type`.
    let log = &body["definitions"]["NodeConfig.core.Log"];
    assert_eq!(
        log["properties"]["message"]["description"],
        json!("Message template; supports `{{variable}}` interpolation.")
    );
    assert_eq!(
        log["properties"]["level"]["enum"],
        json!(["debug", "info", "warn", "error"])
    );
    assert_eq!(log["additionalProperties"], false);

    let branch = body["definitions"]["Node"]["allOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|branch| branch["if"]["properties"]["type"]["const"] == "core.Log")
        .expect("a core.Log branch");
    assert_eq!(
        branch["then"]["properties"]["config"]["$ref"],
        "#/definitions/NodeConfig.core.Log"
    );

    // The published file name resolves to the same document.
    let (status, aliased) = call(
        &state,
        Request::builder()
            .uri("/api/v1/schema/workflow.schema.json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(aliased, body);
}

#[tokio::test]
async fn schema_endpoint_serves_typed_documents_and_rejects_unknown_ones() {
    let state = state().await;
    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/schema/node-descriptor")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["properties"]["node_type"].is_object());

    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/schema/nope")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "E_SCHEMA_NOT_FOUND");
}

#[tokio::test]
async fn validation_endpoint_reports_diagnostics() {
    let state = state().await;
    let (status, body) = call(
        &state,
        json_request(
            "POST",
            "/api/v1/workflows/validate",
            json!({ "workflow": valid_workflow() }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["diagnostics"].as_array().is_some());
    assert!(!body["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["severity"] == "error"));

    let mut broken = valid_workflow();
    broken["edges"][0]["target"] = json!("ghost");
    let (status, body) = call(
        &state,
        json_request(
            "POST",
            "/api/v1/workflows/validate",
            json!({ "workflow": broken }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "WF113"));
}

#[tokio::test]
async fn invalid_workflows_are_rejected_before_running() {
    let state = state().await;
    let mut broken = valid_workflow();
    broken["edges"][0]["target"] = json!("ghost");
    let (status, body) = call(
        &state,
        json_request("POST", "/api/v1/runs", json!({ "workflow": broken })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "E_WORKFLOW_INVALID");
}

#[tokio::test]
async fn a_run_completes_and_reports_variables() {
    let state = state().await;
    let (status, created) = call(
        &state,
        json_request(
            "POST",
            "/api/v1/runs",
            json!({ "workflow": valid_workflow() }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{created}");
    let run_id = created["id"].as_str().unwrap().to_string();

    let mut snapshot = created;
    for _ in 0..100 {
        if snapshot["status"]
            .as_str()
            .is_some_and(|status| matches!(status, "completed" | "failed" | "cancelled"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        let (_, body) = call(
            &state,
            Request::builder()
                .uri(format!("/api/v1/runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        snapshot = body;
    }

    assert_eq!(snapshot["status"], "completed", "{snapshot}");
    assert_eq!(snapshot["nodes_executed"], 4);
    assert_eq!(snapshot["variables"]["answer"], 42);
    assert!(snapshot["event_count"].as_u64().unwrap() >= 9);
}

#[tokio::test]
async fn run_artifacts_are_listed_and_downloadable() {
    let mut builder = RuntimeBuilder::new(RuntimeConfig::default());
    builder.register_executor(ArtifactExecutor);
    let state = builder.build().expect("runtime builds");
    let workflow = json!({
        "schema_version": "2.0",
        "id": "wf.artifact",
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "artifact", "type": "test.Artifact" },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "source": "start", "target": "artifact" },
            { "id": "e2", "source": "artifact", "target": "end" }
        ]
    });
    let (status, created) = call(
        &state,
        json_request("POST", "/api/v1/runs", json!({ "workflow": workflow })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{created}");
    let run_id = created["id"].as_str().unwrap().to_string();

    let mut artifacts = Value::Null;
    for _ in 0..100 {
        let (_, body) = call(
            &state,
            Request::builder()
                .uri(format!("/api/v1/runs/{run_id}/artifacts"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        artifacts = body;
        if !artifacts["artifacts"].as_array().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let artifact = &artifacts["artifacts"][0];
    assert_eq!(artifact["content_type"], "image/png");
    assert_eq!(artifact["size"], 4);
    let artifact_id = artifact["id"].as_str().unwrap();

    let response = nodara_runtime::api::router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/runs/{run_id}/artifacts/{artifact_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("artifact response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(&bytes[..], &[1, 2, 3, 4]);
}

#[tokio::test]
async fn unknown_runs_produce_a_structured_404() {
    let state = state().await;
    let (status, body) = call(
        &state,
        Request::builder()
            .uri("/api/v1/runs/does-not-exist")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "E_RUN_NOT_FOUND");
}

#[tokio::test]
async fn runs_can_be_cancelled() {
    let state = state().await;
    let workflow = json!({
        "schema_version": "2.0",
        "id": "wf.cancel",
        "nodes": [
            { "id": "start", "type": "core.Start" },
            { "id": "wait", "type": "system.Delay",
              "config": { "duration_ms": 5000 } },
            { "id": "log", "type": "core.Log",
              "config": { "message": "never reached" } },
            { "id": "end", "type": "core.End" }
        ],
        "edges": [
            { "id": "e1", "source": "start", "target": "wait" },
            { "id": "e2", "source": "wait", "target": "log" },
            { "id": "e3", "source": "log", "target": "end" }
        ]
    });

    let (_, created) = call(
        &state,
        json_request("POST", "/api/v1/runs", json!({ "workflow": workflow })),
    )
    .await;
    let run_id = created["id"].as_str().unwrap().to_string();
    tokio::time::sleep(Duration::from_millis(60)).await;

    let (status, _) = call(
        &state,
        json_request("POST", &format!("/api/v1/runs/{run_id}/cancel"), json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for _ in 0..100 {
        let (_, body) = call(
            &state,
            Request::builder()
                .uri(format!("/api/v1/runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        if body["status"] == "cancelled" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("run was never cancelled");
}
