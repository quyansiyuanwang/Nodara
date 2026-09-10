//! The versioned HTTP and WebSocket API.
//!
//! Handlers are intentionally thin: they translate between JSON and the
//! [`crate::RuntimeState`], and every real decision lives in `rf-core`,
//! `rf-plugin` or [`crate::runs`].

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use rf_plugin::PluginSummary;
use rf_schema::{
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
        .route("/node-types", get(list_node_types))
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
        .route("/agent/approvals", get(list_pending_approvals));

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
}

async fn root() -> Json<ApiRoot> {
    Json(ApiRoot {
        name: "RecognizerFramework Runtime",
        api_version: rf_schema::API_VERSION,
        schema_version: rf_schema::SCHEMA_VERSION,
        protocol_version: rf_schema::PROTOCOL_VERSION,
    })
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    node_types: usize,
    plugins: usize,
    runs: usize,
}

async fn health(State(state): State<Arc<RuntimeState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        node_types: state.registry.node_types().len(),
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
    let handle = state
        .runs
        .start_with_run_id(run_id, request.workflow, request.variables);
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
) -> Result<Json<Vec<rf_schema::EventEnvelope>>, ApiError> {
    let handle = state
        .runs
        .get(&id)
        .ok_or_else(|| ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{id}`")))?;
    Ok(Json(handle.history()))
}

#[derive(Debug, Serialize)]
struct AgentSessionList {
    sessions: Vec<AgentSession>,
    pending_approvals: Vec<PendingApproval>,
}

#[derive(Debug, Serialize)]
struct PendingApproval {
    session_id: String,
    approval: rf_schema::ApprovalRequest,
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
    status: rf_schema::SessionStatus,
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

/// Replay the buffered events, then forward live ones until the client leaves.
async fn pump(mut socket: WebSocket, handle: Arc<crate::runs::RunHandle>) {
    let (replay, mut receiver) = handle.subscribe();
    for envelope in replay {
        if send_event(&mut socket, &envelope).await.is_err() {
            return;
        }
    }
    loop {
        tokio::select! {
            event = receiver.recv() => match event {
                Ok(envelope) => {
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
    envelope: &rf_schema::EventEnvelope,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(envelope).unwrap_or_else(|_| "{}".to_string());
    socket.send(Message::Text(text.into())).await
}
