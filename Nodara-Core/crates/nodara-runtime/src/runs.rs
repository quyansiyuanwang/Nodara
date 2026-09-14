//! Run lifecycle management and event fan-out.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use nodara_core::{EventSink, RunControl, RunFailure, RunOutcome, RunRequest, WorkflowEngine};
use nodara_schema::{EventEnvelope, ExecutionEvent, RunStatus, Workflow};
use parking_lot::Mutex;
use tokio::sync::broadcast;

use crate::error::ApiError;

/// How many events a slow WebSocket subscriber may fall behind before it is
/// skipped ahead. Bounded so one stalled client cannot grow memory without limit.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Point-in-time view of a run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunSnapshot {
    /// Run identifier.
    pub id: String,
    /// Workflow that is executing.
    pub workflow_id: String,
    /// Current lifecycle state.
    pub status: RunStatus,
    /// Unix epoch milliseconds when the run started.
    pub started_at_ms: u64,
    /// Unix epoch milliseconds when the run reached a terminal state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    /// Nodes executed so far.
    pub nodes_executed: usize,
    /// Final or partial variable scope.
    pub variables: BTreeMap<String, serde_json::Value>,
    /// Failure detail, when the run did not succeed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<RunFailure>,
    /// Number of events recorded for the run.
    pub event_count: usize,
}

#[derive(Debug)]
struct RunState {
    status: RunStatus,
    started_at_ms: u64,
    finished_at_ms: Option<u64>,
    nodes_executed: usize,
    variables: BTreeMap<String, serde_json::Value>,
    failure: Option<RunFailure>,
}

/// A live run: its state, its event history and its subscribers.
pub struct RunHandle {
    id: String,
    workflow_id: Mutex<String>,
    control: RunControl,
    state: Mutex<RunState>,
    history: Mutex<Vec<EventEnvelope>>,
    next_seq: AtomicU64,
    broadcaster: broadcast::Sender<EventEnvelope>,
}

impl std::fmt::Debug for RunHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunHandle").field("id", &self.id).finish()
    }
}

impl RunHandle {
    fn new(id: String, workflow_id: String, control: RunControl) -> Arc<Self> {
        let (broadcaster, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Arc::new(Self {
            id,
            workflow_id: Mutex::new(workflow_id),
            control,
            state: Mutex::new(RunState {
                status: RunStatus::Pending,
                started_at_ms: nodara_schema::event::now_ms(),
                finished_at_ms: None,
                nodes_executed: 0,
                variables: BTreeMap::new(),
                failure: None,
            }),
            history: Mutex::new(Vec::new()),
            next_seq: AtomicU64::new(0),
            broadcaster,
        })
    }

    /// Run identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Workflow being executed.
    pub fn workflow_id(&self) -> String {
        self.workflow_id.lock().clone()
    }

    /// The control handle used for pause / resume / step / cancel.
    pub fn control(&self) -> &RunControl {
        &self.control
    }

    /// Current snapshot.
    pub fn snapshot(&self) -> RunSnapshot {
        let state = self.state.lock();
        RunSnapshot {
            id: self.id.clone(),
            workflow_id: self.workflow_id(),
            status: state.status,
            started_at_ms: state.started_at_ms,
            finished_at_ms: state.finished_at_ms,
            nodes_executed: state.nodes_executed,
            variables: state.variables.clone(),
            failure: state.failure.clone(),
            event_count: self.history.lock().len(),
        }
    }

    /// Every event recorded so far.
    pub fn history(&self) -> Vec<EventEnvelope> {
        self.history.lock().clone()
    }

    /// Subscribe to future events. Returns the replay buffer, the highest
    /// sequence number it contains, and a live receiver.
    ///
    /// The receiver is created *before* the history is snapshotted, so no
    /// event can fall into the gap between the two. The overlap is on the
    /// caller instead: live events up to `last_replayed_seq` must be skipped,
    /// otherwise an event published between subscribe and snapshot would be
    /// delivered twice.
    pub fn subscribe(
        &self,
    ) -> (
        Vec<EventEnvelope>,
        Option<u64>,
        broadcast::Receiver<EventEnvelope>,
    ) {
        let receiver = self.broadcaster.subscribe();
        let history = self.history.lock().clone();
        let last_replayed_seq = history.last().map(|envelope| envelope.seq);
        (history, last_replayed_seq, receiver)
    }

    /// Re-sequence and publish one event.
    fn publish(&self, mut envelope: EventEnvelope) {
        envelope.seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        envelope.run_id = self.id.clone();
        if envelope.timestamp_ms == 0 {
            envelope.timestamp_ms = nodara_schema::event::now_ms();
        }
        self.apply(&envelope.event);
        self.history.lock().push(envelope.clone());
        let _ = self.broadcaster.send(envelope);
    }

    fn apply(&self, event: &ExecutionEvent) {
        let mut state = self.state.lock();
        match event {
            ExecutionEvent::RunStarted { workflow_id } => {
                state.status = RunStatus::Running;
                *self.workflow_id.lock() = workflow_id.clone();
            }
            ExecutionEvent::NodeFinished { .. } => {
                state.nodes_executed += 1;
            }
            ExecutionEvent::RunCompleted { .. } => {
                state.status = RunStatus::Completed;
                state.finished_at_ms = Some(nodara_schema::event::now_ms());
            }
            ExecutionEvent::RunFailed { code, message } => {
                state.status = RunStatus::Failed;
                state.finished_at_ms = Some(nodara_schema::event::now_ms());
                state.failure = Some(RunFailure {
                    code: code.clone(),
                    message: message.clone(),
                });
            }
            ExecutionEvent::RunCancelled { reason } => {
                state.status = RunStatus::Cancelled;
                state.finished_at_ms = Some(nodara_schema::event::now_ms());
                state.failure = Some(RunFailure {
                    code: "E_CANCELLED".to_string(),
                    message: reason
                        .clone()
                        .unwrap_or_else(|| "run cancelled".to_string()),
                });
            }
            _ => {}
        }
    }

    /// Record the final variable scope once the run thread reports back.
    fn complete(&self, outcome: &RunOutcome) {
        let mut state = self.state.lock();
        state.status = outcome.status;
        state.nodes_executed = outcome.nodes_executed;
        state.variables = outcome.variables.clone();
        state.failure = outcome.failure.clone();
        if outcome.status.is_terminal() && state.finished_at_ms.is_none() {
            state.finished_at_ms = Some(nodara_schema::event::now_ms());
        }
    }

    fn mark_paused(&self) {
        let mut state = self.state.lock();
        if !state.status.is_terminal() {
            state.status = RunStatus::Paused;
        }
    }

    fn mark_running(&self) {
        let mut state = self.state.lock();
        if state.status == RunStatus::Paused {
            state.status = RunStatus::Running;
        }
    }
}

/// Adapter that funnels engine events into a [`RunHandle`].
struct HandleSink {
    handle: Arc<RunHandle>,
}

impl EventSink for HandleSink {
    fn emit(&self, envelope: EventEnvelope) {
        self.handle.publish(envelope);
    }
}

/// Owns every run started through the runtime.
pub struct RunManager {
    engine: WorkflowEngine,
    runs: Mutex<HashMap<String, Arc<RunHandle>>>,
}

impl std::fmt::Debug for RunManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunManager")
            .field("runs", &self.runs.lock().len())
            .finish()
    }
}

impl RunManager {
    /// Create a manager around an engine.
    pub fn new(engine: WorkflowEngine) -> Arc<Self> {
        Arc::new(Self {
            engine,
            runs: Mutex::new(HashMap::new()),
        })
    }

    /// The engine in use.
    pub fn engine(&self) -> &WorkflowEngine {
        &self.engine
    }

    /// Start a run and return its handle.
    pub fn start(
        self: &Arc<Self>,
        workflow: Workflow,
        variables: BTreeMap<String, serde_json::Value>,
    ) -> Arc<RunHandle> {
        let run_id = uuid::Uuid::new_v4().to_string();
        self.start_with_run_id(run_id, workflow, variables)
    }

    /// Start a run under a caller-chosen id.
    ///
    /// The runtime uses this so it can bind the run to an agent session *before*
    /// the run thread starts. Binding afterwards would race: a gated node could
    /// ask for approval before the session knew the run existed, and the request
    /// would be refused.
    pub fn start_with_run_id(
        self: &Arc<Self>,
        run_id: String,
        workflow: Workflow,
        variables: BTreeMap<String, serde_json::Value>,
    ) -> Arc<RunHandle> {
        let control = RunControl::new();
        let handle = RunHandle::new(run_id.clone(), workflow.id.clone(), control.clone());
        self.runs.lock().insert(run_id.clone(), handle.clone());

        let sink: Arc<dyn EventSink> = Arc::new(HandleSink {
            handle: handle.clone(),
        });
        let request = RunRequest::new(workflow)
            .with_run_id(run_id)
            .with_variables(variables)
            .with_event_sink(sink);

        let engine = self.engine.clone();
        let completion = handle.clone();
        if let Err(error) = std::thread::Builder::new()
            .name(format!("nodara-run-{}", handle.id()))
            .spawn(move || {
                let outcome = engine.run(request, &control);
                completion.complete(&outcome);
            })
        {
            // Thread creation failed (resource exhaustion): fail the run
            // through the normal path instead of aborting the API process.
            handle.complete(&RunOutcome {
                run_id: handle.id().to_string(),
                status: RunStatus::Failed,
                nodes_executed: 0,
                duration_ms: 0,
                variables: BTreeMap::new(),
                failure: Some(RunFailure {
                    code: "E_THREAD".to_string(),
                    message: format!("could not spawn the run thread: {error}"),
                }),
            });
        }

        handle
    }

    /// Look up a run.
    pub fn get(&self, run_id: &str) -> Option<Arc<RunHandle>> {
        self.runs.lock().get(run_id).cloned()
    }

    /// Snapshots of every known run, newest first.
    pub fn list(&self) -> Vec<RunSnapshot> {
        let runs = self.runs.lock();
        let mut snapshots: Vec<RunSnapshot> = runs.values().map(|run| run.snapshot()).collect();
        snapshots.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
        snapshots
    }

    /// Pause a run.
    pub fn pause(&self, run_id: &str) -> Result<Arc<RunHandle>, ApiError> {
        let handle = self.lookup(run_id)?;
        if handle.snapshot().status.is_terminal() {
            return Err(ApiError::conflict(
                "E_RUN_FINISHED",
                format!("run `{run_id}` has already finished"),
            ));
        }
        handle.control().pause();
        handle.mark_paused();
        Ok(handle)
    }

    /// Resume a paused run.
    pub fn resume(&self, run_id: &str) -> Result<Arc<RunHandle>, ApiError> {
        let handle = self.lookup(run_id)?;
        handle.control().resume();
        handle.mark_running();
        Ok(handle)
    }

    /// Allow exactly one more node to execute.
    pub fn step(&self, run_id: &str) -> Result<Arc<RunHandle>, ApiError> {
        let handle = self.lookup(run_id)?;
        handle.control().step();
        Ok(handle)
    }

    /// Cancel a run.
    pub fn cancel(&self, run_id: &str) -> Result<Arc<RunHandle>, ApiError> {
        let handle = self.lookup(run_id)?;
        handle.control().cancel();
        Ok(handle)
    }

    fn lookup(&self, run_id: &str) -> Result<Arc<RunHandle>, ApiError> {
        self.get(run_id).ok_or_else(|| {
            ApiError::not_found("E_RUN_NOT_FOUND", format!("no run with id `{run_id}`"))
        })
    }
}
