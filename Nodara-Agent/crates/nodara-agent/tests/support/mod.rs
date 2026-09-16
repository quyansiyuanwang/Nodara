//! A scripted stand-in for the runtime's HTTP surface.
//!
//! The agent is only allowed to reach a system through the runtime API, so that
//! API is what its integration tests must exercise. This server implements the
//! same routes for real, over a real socket, which is a stronger check than
//! mocking the client: request shapes, status codes, error bodies and JSON
//! round-tripping all have to be right.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

/// What the fake runtime has been asked to do.
#[derive(Debug, Default)]
pub struct Recorded {
    pub sessions_created: usize,
    pub plans_published: usize,
    pub messages_appended: usize,
    pub runs_started: usize,
    pub session_ids_on_runs: Vec<Option<String>>,
    pub start_paused_on_runs: Vec<bool>,
    pub approval_on_runs: Vec<Option<String>>,
    pub statuses_set: Vec<String>,
    pub approvals_decided: usize,
}

/// How a run behaves when it is started.
#[derive(Debug, Clone)]
pub enum RunScript {
    /// Finish immediately with this status and optional failure code.
    Immediate {
        status: String,
        code: Option<String>,
    },
    /// Stay running until something steers it.
    Hanging,
}

struct State {
    /// Script per attempt (1-based); the last entry repeats.
    scripts: Vec<RunScript>,
    attempts: usize,
    runs: Vec<Value>,
    session: Value,
}

/// A running fake runtime.
pub struct FakeRuntime {
    base: String,
    recorded: Arc<Mutex<Recorded>>,
    state: Arc<Mutex<State>>,
    shutdown: Arc<Mutex<bool>>,
}

impl FakeRuntime {
    /// Start the server on an ephemeral port.
    pub fn start(scripts: Vec<RunScript>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let recorded = Arc::new(Mutex::new(Recorded::default()));
        let state = Arc::new(Mutex::new(State {
            scripts: if scripts.is_empty() {
                vec![RunScript::Immediate {
                    status: "completed".to_string(),
                    code: None,
                }]
            } else {
                scripts
            },
            attempts: 0,
            runs: Vec::new(),
            session: json!({}),
        }));
        let shutdown = Arc::new(Mutex::new(false));

        {
            let recorded = recorded.clone();
            let state = state.clone();
            let shutdown = shutdown.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if *shutdown.lock().expect("shutdown lock") {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let recorded = recorded.clone();
                    let state = state.clone();
                    std::thread::spawn(move || {
                        let _ = serve(stream, &recorded, &state);
                    });
                }
            });
        }

        Self {
            base: format!("http://{address}"),
            recorded,
            state,
            shutdown,
        }
    }

    /// Base URL to hand to the agent.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// What the runtime was asked to do.
    pub fn recorded(&self) -> Recorded {
        let recorded = self.recorded.lock().expect("recorded lock");
        Recorded {
            sessions_created: recorded.sessions_created,
            plans_published: recorded.plans_published,
            messages_appended: recorded.messages_appended,
            runs_started: recorded.runs_started,
            session_ids_on_runs: recorded.session_ids_on_runs.clone(),
            start_paused_on_runs: recorded.start_paused_on_runs.clone(),
            approval_on_runs: recorded.approval_on_runs.clone(),
            statuses_set: recorded.statuses_set.clone(),
            approvals_decided: recorded.approvals_decided,
        }
    }

    /// The stored session, for asserting what the Studio would render.
    pub fn session(&self) -> Value {
        self.state.lock().expect("state lock").session.clone()
    }
}

impl Drop for FakeRuntime {
    fn drop(&mut self) {
        *self.shutdown.lock().expect("shutdown lock") = true;
        // Nudge the accept loop so it notices the flag.
        let _ = std::net::TcpStream::connect(self.base.trim_start_matches("http://"));
    }
}

fn serve(
    mut stream: TcpStream,
    recorded: &Arc<Mutex<Recorded>>,
    state: &Arc<Mutex<State>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        if header.trim().is_empty() {
            break;
        }
        if let Some(value) = header
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .map(str::trim)
            .and_then(|value| value.parse::<usize>().ok())
        {
            content_length = value;
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };

    // Artifact bodies are raw bytes in the real runtime. Keep that contract in
    // the fake so the Agent's multimodal path is exercised end to end.
    if method == "GET" && path.contains("/artifacts/") && !path.ends_with("/artifacts") {
        let bytes = [0x89, b'P', b'N', b'G', 1, 2, 3, 4];
        write!(
            stream,
            "HTTP/1.1 200 OK
content-type: image/png
content-length: {}
connection: close

",
            bytes.len()
        )?;
        stream.write_all(&bytes)?;
        return stream.flush();
    }

    let (status, payload) = route(&method, &path, &body, recorded, state);
    let encoded = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    write!(
        stream,
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        encoded.len()
    )?;
    stream.write_all(&encoded)?;
    stream.flush()
}

fn route(
    method: &str,
    path: &str,
    body: &Value,
    recorded: &Arc<Mutex<Recorded>>,
    state: &Arc<Mutex<State>>,
) -> (u16, Value) {
    let mut state = state.lock().expect("state lock");
    let mut recorded = recorded.lock().expect("recorded lock");

    match (method, path) {
        ("GET", "/api/v1/health") => (
            200,
            json!({ "status": "ok", "node_types": 6, "plugins": 0, "runs": 0 }),
        ),
        ("GET", "/api/v1/node-types") => (200, json!({ "node_types": catalogue() })),
        // The fake runs the *real* validator against its own catalogue, so the
        // agent's explain path sees the same diagnostics a live runtime produces.
        ("POST", "/api/v1/workflows/validate") => {
            let Ok(workflow) = serde_json::from_value::<nodara_schema::Workflow>(
                body.get("workflow").cloned().unwrap_or(Value::Null),
            ) else {
                return (
                    400,
                    json!({ "code": "E_BAD_REQUEST", "message": "not a workflow" }),
                );
            };
            let index = CatalogueIndex(catalogue_descriptors());
            let report = nodara_schema::validate_with(&workflow, &index, &Default::default());
            (200, serde_json::to_value(report).unwrap_or(Value::Null))
        }
        ("POST", "/api/v1/agent/sessions") => {
            recorded.sessions_created += 1;
            state.session = json!({
                "id": "session-1",
                "goal": body.get("goal").cloned().unwrap_or(Value::Null),
                "provider": body.get("provider").cloned().unwrap_or(Value::Null),
                "status": "draft",
                "created_at_ms": 1,
                "updated_at_ms": 1,
                "messages": [],
                "approvals": [],
                "tokens_used": 0
            });
            (201, state.session.clone())
        }
        ("GET", "/api/v1/agent/sessions") => (
            200,
            json!({ "sessions": [state.session.clone()], "pending_approvals": [] }),
        ),
        ("POST", "/api/v1/runs") => {
            recorded.runs_started += 1;
            recorded.start_paused_on_runs.push(
                body.get("start_paused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
            recorded.approval_on_runs.push(
                body.get("approval")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            );
            recorded.session_ids_on_runs.push(
                body.get("session_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            );
            state.attempts += 1;
            let script = state
                .scripts
                .get(state.attempts - 1)
                .or_else(|| state.scripts.last())
                .cloned()
                .unwrap_or(RunScript::Immediate {
                    status: "completed".to_string(),
                    code: None,
                });
            let (status, code) = if body.get("start_paused").and_then(Value::as_bool) == Some(true)
            {
                ("paused".to_string(), None)
            } else {
                match script {
                    RunScript::Immediate { status, code } => (status, code),
                    RunScript::Hanging => ("running".to_string(), None),
                }
            };
            let id = format!("run-{}", state.attempts);
            let snapshot = snapshot(&id, &status, code.as_deref(), state.attempts);
            state.session["run_id"] = json!(id);
            state.runs.push(snapshot.clone());
            (202, snapshot)
        }
        _ if path.starts_with("/api/v1/runs/") => {
            let rest = path.trim_start_matches("/api/v1/runs/");
            let mut segments = rest.split('/');
            let run_id = segments.next().unwrap_or_default().to_string();
            let action = segments.next();
            let index = state
                .runs
                .iter()
                .position(|run| run.get("id").and_then(Value::as_str) == Some(run_id.as_str()));
            let Some(index) = index else {
                return (
                    404,
                    json!({ "code": "E_RUN_NOT_FOUND", "message": "no such run" }),
                );
            };

            match (method, action) {
                ("GET", None) => (200, state.runs[index].clone()),
                ("GET", Some("event-log")) => {
                    let run = &state.runs[index];
                    (200, json!(events_for(run)))
                }
                ("GET", Some("artifacts")) => (
                    200,
                    json!({
                        "artifacts": [{
                            "id": "artifact-1",
                            "name": "desktop.png",
                            "content_type": "image/png",
                            "size": 8
                        }]
                    }),
                ),
                ("POST", Some(action @ ("pause" | "resume" | "cancel"))) => {
                    let status = match action {
                        "pause" => "paused",
                        "resume" => "running",
                        _ => "cancelled",
                    };
                    state.runs[index]["status"] = json!(status);
                    recorded.statuses_set.push(status.to_string());
                    (200, state.runs[index].clone())
                }
                _ => (404, json!({ "code": "E_NOT_FOUND", "message": path })),
            }
        }
        _ if path.starts_with("/api/v1/agent/sessions/") => {
            let rest = path.trim_start_matches("/api/v1/agent/sessions/");
            let mut segments = rest.split('/');
            let _session_id = segments.next().unwrap_or_default();
            match (method, segments.next()) {
                ("GET", None) => (200, state.session.clone()),
                ("POST", Some("messages")) => {
                    recorded.messages_appended += 1;
                    (200, state.session.clone())
                }
                ("POST", Some("plan")) => {
                    recorded.plans_published += 1;
                    state.session["plan"] = body.clone();
                    state.session["status"] = json!("ready");
                    (200, state.session.clone())
                }
                ("POST", Some("status")) => {
                    let status = body
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string();
                    recorded.statuses_set.push(status.clone());
                    state.session["status"] = json!(status);
                    (200, state.session.clone())
                }
                ("POST", Some("approvals")) => {
                    recorded.approvals_decided += 1;
                    (200, state.session.clone())
                }
                _ => (404, json!({ "code": "E_NOT_FOUND", "message": path })),
            }
        }
        _ => (404, json!({ "code": "E_NOT_FOUND", "message": path })),
    }
}

/// The node types this fake runtime can execute.
fn catalogue_descriptors() -> Vec<nodara_schema::NodeDescriptor> {
    [
        ("core.Start", "Start", "Core", false),
        ("core.End", "End", "Core", false),
        ("core.Log", "Log", "Core", false),
        ("core.Calculate", "Calculate", "Core", false),
        ("windows.Input.Keyboard", "Keyboard", "Input", true),
    ]
    .into_iter()
    .map(
        |(node_type, display, category, dangerous)| nodara_schema::NodeDescriptor {
            permissions: if dangerous {
                vec!["input.control".to_string()]
            } else {
                Vec::new()
            },
            dangerous,
            ..nodara_schema::NodeDescriptor::new(node_type, display, category)
        },
    )
    .collect()
}

fn catalogue() -> Vec<Value> {
    catalogue_descriptors()
        .into_iter()
        .filter_map(|descriptor| serde_json::to_value(descriptor).ok())
        .collect()
}

/// The subset of the registry interface validation needs.
struct CatalogueIndex(Vec<nodara_schema::NodeDescriptor>);

impl nodara_schema::NodeTypeIndex for CatalogueIndex {
    fn node_types(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|descriptor| descriptor.node_type.clone())
            .collect()
    }

    fn descriptor(&self, node_type: &str) -> Option<nodara_schema::NodeDescriptor> {
        self.0
            .iter()
            .find(|descriptor| descriptor.node_type == node_type)
            .cloned()
    }
}

fn snapshot(id: &str, status: &str, code: Option<&str>, attempt: usize) -> Value {
    let terminal = matches!(status, "completed" | "failed" | "cancelled");
    json!({
        "id": id,
        "workflow_id": "wf.test",
        "status": status,
        "started_at_ms": 1000,
        "finished_at_ms": if terminal { json!(1012) } else { Value::Null },
        "nodes_executed": if status == "completed" { 3 } else { 1 },
        "variables": if status == "completed" { json!({ "answer": 42 }) } else { json!({}) },
        "failure": code.map(|code| json!({ "code": code, "message": format!("attempt {attempt}") })),
        "event_count": 4
    })
}

fn events_for(run: &Value) -> Vec<Value> {
    let status = run
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut events = vec![json!({
        "run_id": run["id"], "seq": 0, "timestamp_ms": 1000,
        "event": { "type": "run_started", "workflow_id": "wf.test" }
    })];
    events.push(json!({
        "run_id": run["id"], "seq": 1, "timestamp_ms": 1001,
        "event": { "type": "log", "level": "info", "message": "hello from the fake runtime" }
    }));
    match status {
        "completed" => {
            events.push(json!({
                "run_id": run["id"], "seq": 2, "timestamp_ms": 1005,
                "event": {
                    "type": "node_started",
                    "node_id": "capture",
                    "node_type": "windows.Desktop.Capture",
                    "input": {
                        "config": { "x": 10, "y": 20, "output_var": "shot" },
                        "resolved_config": { "x": 10, "y": 20, "output_var": "shot" },
                        "inputs": { "in": "ready" },
                        "variables_before": { "screen": "desktop" }
                    }
                }
            }));
            events.push(json!({
                "run_id": run["id"], "seq": 3, "timestamp_ms": 1008,
                "event": {
                    "type": "data_transferred",
                    "edge_id": "capture-log",
                    "source": "capture",
                    "target": "log",
                    "source_port": "artifact",
                    "target_port": "in",
                    "value": "ready"
                }
            }));
            events.push(json!({
                "run_id": run["id"], "seq": 4, "timestamp_ms": 1010,
                "event": {
                    "type": "node_finished",
                    "node_id": "capture",
                    "outputs": {
                        "artifact": {
                            "id": "artifact-1",
                            "name": "desktop.png",
                            "content_type": "image/png",
                            "size": 8
                        }
                    },
                    "variables_after": { "shot": { "id": "artifact-1" } },
                    "duration_ms": 9
                }
            }));
            events.push(json!({
                "run_id": run["id"], "seq": 5, "timestamp_ms": 1012,
                "event": { "type": "run_completed", "nodes_executed": 3, "duration_ms": 12 }
            }));
        }
        "failed" => {
            events.push(json!({
                "run_id": run["id"], "seq": 2, "timestamp_ms": 1005,
                "event": { "type": "node_failed", "node_id": "calc", "code": "E_INVALID_CONFIG",
                           "message": "bad expression", "retryable": false }
            }));
            events.push(json!({
                "run_id": run["id"], "seq": 3, "timestamp_ms": 1006,
                "event": { "type": "run_failed", "code": "E_INVALID_CONFIG", "message": "bad expression" }
            }));
        }
        "cancelled" => {
            events.push(json!({
                "run_id": run["id"], "seq": 2, "timestamp_ms": 1005,
                "event": { "type": "run_cancelled", "reason": "operator" }
            }));
        }
        _ => {}
    }
    events
}
