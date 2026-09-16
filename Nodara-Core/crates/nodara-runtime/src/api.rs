//! The versioned HTTP and WebSocket API.
//!
//! Handlers are intentionally thin: they translate between JSON and the
//! [`crate::RuntimeState`], and every real decision lives in `nodara-core`,
//! `nodara-plugin` or [`crate::runs`].

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use nodara_core::{ApprovalHandler, AutoApprove};
use nodara_plugin::PluginSummary;
use nodara_schema::{
    validate_with, AgentSession, ApprovalDecisionRequest, NodeDescriptor, PlanPreview,
    SessionMessageRequest, SessionRequest, ValidationOptions, ValidationReport, Workflow,
};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::runs::RunSnapshot;
use crate::state::{PluginFailure, RuntimeState};

/// Build the complete API router.
pub fn router(state: Arc<RuntimeState>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/plugins", get(list_plugins))
        .route("/extensions", get(list_extensions))
        .route("/node-types", get(list_node_types))
        .route("/schema/{document}", get(get_schema))
        .route("/workflows/validate", post(validate_workflow))
        .route("/runs", get(list_runs).post(create_run))
        .route("/runs/{id}", get(get_run))
        .route("/runs/{id}/pause", post(pause_run))
        .route("/runs/{id}/resume", post(resume_run))
        .route("/runs/{id}/step", post(step_run))
        .route("/runs/{id}/cancel", post(cancel_run))
        // `events` is the WebSocket stream the plan specifies. The agent, which
        // already speaks REST, reads the same sequence over `event-log` rather
        // than carrying a WebSocket client of its own.
        .route("/runs/{id}/events", get(stream_run_events))
        .route("/runs/{id}/event-log", get(run_events))
        .route("/runs/{id}/workflow", get(run_workflow))
        .route("/runs/{id}/artifacts", get(list_run_artifacts))
        .route("/runs/{id}/artifacts/{artifact_id}", get(get_run_artifact))
        .route(
            "/agent/sessions",
            get(list_agent_sessions).post(create_agent_session),
        )
        .route("/agent/sessions/{id}", get(get_agent_session))
        .route("/agent/sessions/{id}/messages", post(append_agent_message))
        .route("/agent/sessions/{id}/plan", post(publish_agent_plan))
        .route("/agent/sessions/{id}/status", post(set_agent_status))
        .route(
            "/agent/sessions/{id}/approvals/{approval_id}",
            post(decide_agent_approval),
        )
        .route("/agent/approvals", get(list_pending_approvals))
        .route("/audit", get(list_audit_records));

    let mut router = Router::new()
        .route("/api/v1", get(root))
        .nest("/api/v1", api);

    if state.config.permissive_cors {
        router = router.layer(tower_http::cors::CorsLayer::permissive());
    }
    router.with_state(state)
}

#[derive(Debug, Serialize)]
struct ApiRoot {
    name: &'static str,
    api_version: &'static str,
    schema_version: &'static str,
    protocol_version: &'static str,
    /// JSON Schema documents this runtime publishes.
    schemas: &'static [&'static str],
}

async fn root() -> Json<ApiRoot> {
    Json(ApiRoot {
        name: "Nodara Runtime",
        api_version: nodara_schema::API_VERSION,
        schema_version: nodara_schema::SCHEMA_VERSION,
        protocol_version: nodara_schema::PROTOCOL_VERSION,
        schemas: SCHEMA_DOCUMENTS,
    })
}

/// JSON Schema documents the runtime can serve.
///
/// `workflow` is composed from the descriptors of the nodes this runtime
/// installed, so a workflow file that points its `$schema` at this endpoint
/// completes exactly the node types and configuration keys the deployment can
/// run. The others are derived from the Rust types and are deployment
/// independent.
pub const SCHEMA_DOCUMENTS: &[&str] = &[
    "workflow",
    "plugin-manifest",
    "node-descriptor",
    "extension",
    "execution-event",
    "agent-session",
    "agent-tool-call",
];

/// `GET /schema/{document}` — a published JSON Schema.
///
/// The path accepts the bare name or the published file name, so both
/// `/schema/workflow` and `/schema/workflow.schema.json` resolve.
async fn get_schema(
    State(state): State<Arc<RuntimeState>>,
    Path(document): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let name = document
        .trim_end_matches(".json")
        .trim_end_matches(".schema");
    let schema = match name {
        "workflow" => nodara_schema::workflow_schema_for(&state.registry.descriptors()),
        "plugin-manifest" => nodara_schema::manifest_schema(),
        "node-descriptor" => nodara_schema::descriptor_schema(),
        "extension" => nodara_schema::extension_schema(),
        "execution-event" => nodara_schema::event_schema(),
        "agent-session" => nodara_schema::session_schema(),
        "agent-tool-call" => nodara_schema::tool_call_schema(),
        other => {
            return Err(ApiError::not_found(
                "E_SCHEMA_NOT_FOUND",
                format!(
                    "no schema named `{other}`; available: {}",
                    SCHEMA_DOCUMENTS.join(", ")
                ),
            ))
        }
    };
    Ok(Json(schema))
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    node_types: usize,
    extensions: usize,
    plugins: usize,
    runs: usize,
}

async fn health(State(state): State<Arc<RuntimeState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        node_types: state.registry.node_types().len(),
        extensions: state.extensions.len(),
        plugins: state.host.summaries().len(),
        runs: state.runs.list().len(),
    })
}

#[derive(Debug, Serialize)]
struct PluginListResponse {
    plugins: Vec<PluginSummary>,
    failures: Vec<PluginFailure>,
}

async fn list_plugins(State(state): State<Arc<RuntimeState>>) -> Json<PluginListResponse> {
    Json(PluginListResponse {
        plugins: state.host.summaries(),
        failures: state.plugin_failures.clone(),
    })
}

#[derive(Debug, Serialize)]
struct ExtensionListResponse {
    extensions: Vec<nodara_core::ExtensionDescriptor>,
}

async fn list_extensions(State(state): State<Arc<RuntimeState>>) -> Json<ExtensionListResponse> {
    Json(ExtensionListResponse {
        extensions: state.extensions.descriptors(),
    })
}

#[derive(Debug, Serialize)]
struct ArtifactListResponse {
    artifacts: Vec<nodara_core::ArtifactMeta>,
}

async fn list_run_artifacts(
    State(state): State<Arc<RuntimeState>>,
    Path(run_id): Path<String>,
) -> Result<Json<ArtifactListResponse>, ApiError> {
    let run = state.runs.get(&run_id).ok_or_else(|| {
        ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{run_id}`"))
    })?;
    Ok(Json(ArtifactListResponse {
        artifacts: run.artifacts().list(),
    }))
}

async fn get_run_artifact(
    State(state): State<Arc<RuntimeState>>,
    Path((run_id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let run = state.runs.get(&run_id).ok_or_else(|| {
        ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{run_id}`"))
    })?;
    let Some((meta, bytes)) = run.artifacts().get_with_meta(&artifact_id) else {
        return Err(ApiError::not_found(
            "E_ARTIFACT_NOT_FOUND",
            format!("run `{run_id}` has no artifact `{artifact_id}`"),
        ));
    };
    let mut response = Response::new(Body::from(bytes));
    let content_type = HeaderValue::from_str(&meta.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=300"),
    );
    Ok(response)
}

#[derive(Debug, Serialize)]
struct NodeTypeListResponse {
    node_types: Vec<NodeDescriptor>,
}

async fn list_node_types(State(state): State<Arc<RuntimeState>>) -> Json<NodeTypeListResponse> {
    Json(NodeTypeListResponse {
        node_types: state.registry.descriptors(),
    })
}

/// `POST /workflows/validate` body.
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    /// Workflow document to validate.
    pub workflow: Workflow,
    /// Optional validation strictness overrides.
    #[serde(default)]
    pub options: Option<ValidationOptionsDto>,
}

/// Strictness overrides accepted by the validate endpoint.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ValidationOptionsDto {
    /// Reject cycles.
    #[serde(default)]
    pub reject_cycles: Option<bool>,
    /// Require a `core.Start` node.
    #[serde(default)]
    pub require_start: Option<bool>,
    /// Require a `core.End` node.
    #[serde(default)]
    pub require_end: Option<bool>,
    /// Warn about unreachable nodes.
    #[serde(default)]
    pub warn_unreachable: Option<bool>,
    /// Warn about dead ends.
    #[serde(default)]
    pub warn_dead_end: Option<bool>,
    /// Check `{{variable}}` references.
    #[serde(default)]
    pub check_variable_references: Option<bool>,
}

impl ValidationOptionsDto {
    fn apply(self, base: &ValidationOptions) -> ValidationOptions {
        ValidationOptions {
            reject_cycles: self.reject_cycles.unwrap_or(base.reject_cycles),
            require_start: self.require_start.unwrap_or(base.require_start),
            require_end: self.require_end.unwrap_or(base.require_end),
            warn_unreachable: self.warn_unreachable.unwrap_or(base.warn_unreachable),
            warn_dead_end: self.warn_dead_end.unwrap_or(base.warn_dead_end),
            check_variable_references: self
                .check_variable_references
                .unwrap_or(base.check_variable_references),
        }
    }
}

async fn validate_workflow(
    State(state): State<Arc<RuntimeState>>,
    Json(request): Json<ValidateRequest>,
) -> Json<ValidationReport> {
    let options = request
        .options
        .map(|overrides| overrides.apply(&state.engine.options().validation))
        .unwrap_or_else(|| state.engine.options().validation.clone());
    let report = validate_with(&request.workflow, state.registry.as_ref(), &options);
    Json(report)
}

/// `POST /runs` body.
#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
    /// Workflow to execute.
    pub workflow: Workflow,
    /// Variables that override the workflow's own defaults.
    #[serde(default)]
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Agent session this run belongs to, when one started it.
    ///
    /// Binding the run to its session *before* it starts is what makes gated
    /// capabilities work: a node that needs approval can only ask a session that
    /// already knows about the run.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Validate before running. Defaults to the engine setting.
    #[serde(default)]
    pub validate: Option<bool>,
    /// Start paused before the first node, useful for manual stepping.
    #[serde(default)]
    pub start_paused: bool,
    /// Approval handling for this run. Omitted keeps the runtime default.
    #[serde(default)]
    pub approval: Option<ApprovalMode>,
}

/// Per-run approval strategy requested by a client.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// Approve gated capabilities automatically while still auditing them.
    Auto,
    /// Route gated capabilities through the owning agent session.
    Session,
}

async fn create_run(
    State(state): State<Arc<RuntimeState>>,
    Json(request): Json<CreateRunRequest>,
) -> Result<(axum::http::StatusCode, Json<RunSnapshot>), ApiError> {
    let options = ValidationOptionsDto::default().apply(&state.engine.options().validation);
    let report = validate_with(&request.workflow, state.registry.as_ref(), &options);
    if request.validate.unwrap_or(true) && !report.is_valid() {
        return Err(ApiError::unprocessable(
            "E_WORKFLOW_INVALID",
            format!(
                "workflow `{}` failed validation with {} error(s)",
                request.workflow.id,
                report.error_count()
            ),
        )
        .with_detail(serde_json::to_value(&report).unwrap_or_default()));
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    if let Some(session_id) = &request.session_id {
        if state.sessions.get(session_id).is_none() {
            return Err(ApiError::not_found(
                "E_SESSION_NOT_FOUND",
                format!("no session with id `{session_id}`"),
            ));
        }
        state.sessions.attach_run(session_id, &run_id);
    }
    let approval: Option<Arc<dyn ApprovalHandler>> = match request.approval {
        Some(ApprovalMode::Auto) => Some(Arc::new(AutoApprove)),
        Some(ApprovalMode::Session) => {
            Some(Arc::new(crate::approval::SessionApprovalHandler::new(
                state.sessions.clone(),
                state.config.approval_timeout,
            )))
        }
        None => None,
    };
    let handle = state.runs.start_with_options(
        run_id,
        request.workflow,
        request.variables,
        request.start_paused,
        approval,
    );
    Ok((axum::http::StatusCode::ACCEPTED, Json(handle.snapshot())))
}

async fn list_runs(State(state): State<Arc<RuntimeState>>) -> Json<Vec<RunSnapshot>> {
    Json(state.runs.list())
}

async fn get_run(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<RunSnapshot>, ApiError> {
    state
        .runs
        .get(&id)
        .map(|handle| Json(handle.snapshot()))
        .ok_or_else(|| ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{id}`")))
}

async fn pause_run(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<RunSnapshot>, ApiError> {
    Ok(Json(state.runs.pause(&id)?.snapshot()))
}

async fn resume_run(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<RunSnapshot>, ApiError> {
    Ok(Json(state.runs.resume(&id)?.snapshot()))
}

async fn step_run(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<RunSnapshot>, ApiError> {
    Ok(Json(state.runs.step(&id)?.snapshot()))
}

async fn cancel_run(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<RunSnapshot>, ApiError> {
    let handle = state.runs.cancel(&id)?;
    state.host.cancel_run(&id);
    Ok(Json(handle.snapshot()))
}

async fn stream_run_events(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Result<impl IntoResponse, ApiError> {
    let handle = state
        .runs
        .get(&id)
        .ok_or_else(|| ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{id}`")))?;
    Ok(upgrade.on_upgrade(move |socket| pump(socket, handle)))
}

/// `GET /runs/{id}/event-log` — the buffered event history.
///
/// The Studio streams over the WebSocket; the agent polls this instead, because
/// a batch process that already speaks REST does not need a WebSocket stack to
/// observe a run. Both read the same sequence, so neither can miss an event the
/// other saw.
async fn run_events(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<nodara_schema::EventEnvelope>>, ApiError> {
    let handle = state
        .runs
        .get(&id)
        .ok_or_else(|| ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{id}`")))?;
    Ok(Json(handle.history()))
}

/// `GET /runs/{id}/workflow` — the exact workflow snapshot captured at start.
async fn run_workflow(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<nodara_schema::Workflow>, ApiError> {
    let handle = state
        .runs
        .get(&id)
        .ok_or_else(|| ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{id}`")))?;
    Ok(Json(handle.workflow()))
}

#[derive(Debug, Serialize)]
struct AgentSessionList {
    sessions: Vec<AgentSession>,
    pending_approvals: Vec<PendingApproval>,
}

#[derive(Debug, Serialize)]
struct PendingApproval {
    session_id: String,
    approval: nodara_schema::ApprovalRequest,
}

async fn list_agent_sessions(State(state): State<Arc<RuntimeState>>) -> Json<AgentSessionList> {
    Json(AgentSessionList {
        sessions: state.sessions.list(),
        pending_approvals: state
            .sessions
            .pending_approvals()
            .into_iter()
            .map(|(session_id, approval)| PendingApproval {
                session_id,
                approval,
            })
            .collect(),
    })
}

async fn create_agent_session(
    State(state): State<Arc<RuntimeState>>,
    Json(request): Json<SessionRequest>,
) -> (axum::http::StatusCode, Json<AgentSession>) {
    let session = state.sessions.create(request.goal, request.provider);
    (axum::http::StatusCode::CREATED, Json(session))
}

async fn get_agent_session(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentSession>, ApiError> {
    state.sessions.get(&id).map(Json).ok_or_else(|| {
        ApiError::not_found("E_SESSION_NOT_FOUND", format!("no session with id `{id}`"))
    })
}

async fn append_agent_message(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
    Json(request): Json<SessionMessageRequest>,
) -> Result<Json<AgentSession>, ApiError> {
    state
        .sessions
        .append_message(&id, request.role, request.text);
    state.sessions.get(&id).map(Json).ok_or_else(|| {
        ApiError::not_found("E_SESSION_NOT_FOUND", format!("no session with id `{id}`"))
    })
}

async fn publish_agent_plan(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
    Json(preview): Json<PlanPreview>,
) -> Result<Json<AgentSession>, ApiError> {
    state
        .sessions
        .set_plan(&id, preview)
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found("E_SESSION_NOT_FOUND", format!("no session with id `{id}`"))
        })
}

async fn decide_agent_approval(
    State(state): State<Arc<RuntimeState>>,
    Path((id, approval_id)): Path<(String, String)>,
    Json(request): Json<ApprovalDecisionRequest>,
) -> Result<Json<AgentSession>, ApiError> {
    state
        .sessions
        .decide(&id, &approval_id, request.decision, &request.decided_by)
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found(
                "E_APPROVAL_NOT_FOUND",
                format!("no approval `{approval_id}` in session `{id}`"),
            )
        })
}

#[derive(Debug, Deserialize)]
struct SessionStatusRequest {
    status: nodara_schema::SessionStatus,
}

/// Let the agent mark a session finished, failed or cancelled.
async fn set_agent_status(
    State(state): State<Arc<RuntimeState>>,
    Path(id): Path<String>,
    Json(request): Json<SessionStatusRequest>,
) -> Result<Json<AgentSession>, ApiError> {
    state
        .sessions
        .set_status(&id, request.status)
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found("E_SESSION_NOT_FOUND", format!("no session with id `{id}`"))
        })
}

async fn list_pending_approvals(
    State(state): State<Arc<RuntimeState>>,
) -> Json<Vec<PendingApproval>> {
    Json(
        state
            .sessions
            .pending_approvals()
            .into_iter()
            .map(|(session_id, approval)| PendingApproval {
                session_id,
                approval,
            })
            .collect(),
    )
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    /// Restrict the answer to one run.
    #[serde(default)]
    run_id: Option<String>,
    /// Return at most this many records, newest last.
    #[serde(default)]
    limit: Option<usize>,
}

/// `GET /audit` — what the runtime allowed, refused and recorded.
///
/// The audit log is the durable counterpart to the event stream: events are for
/// live observation and a slow subscriber may fall behind, whereas these records
/// are what an operator reviews afterwards.
async fn list_audit_records(
    State(state): State<Arc<RuntimeState>>,
    axum::extract::Query(query): axum::extract::Query<AuditQuery>,
) -> Json<Vec<nodara_core::AuditRecord>> {
    let mut records = state.audit.records();
    if let Some(run_id) = &query.run_id {
        records.retain(|record| record.run_id == *run_id);
    }
    if let Some(limit) = query.limit {
        if records.len() > limit {
            records.drain(..records.len() - limit);
        }
    }
    Json(records)
}

/// Replay the buffered events, then forward live ones until the client leaves.
async fn pump(mut socket: WebSocket, handle: Arc<crate::runs::RunHandle>) {
    let (replay, last_replayed_seq, mut receiver) = handle.subscribe();
    for envelope in replay {
        if send_event(&mut socket, &envelope).await.is_err() {
            return;
        }
    }
    loop {
        tokio::select! {
            event = receiver.recv() => match event {
                Ok(envelope) => {
                    // An event published between subscribing and snapshotting
                    // the history is in both the replay and the stream; skip
                    // the part of the overlap that was already sent.
                    if last_replayed_seq.is_some_and(|last| envelope.seq <= last) {
                        continue;
                    }
                    if send_event(&mut socket, &envelope).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            },
            incoming = socket.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => return,
                Some(Err(_)) => return,
                Some(Ok(_)) => continue,
            },
        }
    }
}

async fn send_event(
    socket: &mut WebSocket,
    envelope: &nodara_schema::EventEnvelope,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(envelope).unwrap_or_else(|_| "{}".to_string());
    socket.send(Message::Text(text.into())).await
}
