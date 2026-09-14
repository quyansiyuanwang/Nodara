//! Run-scoped execution context.
//!
//! The context is the only surface a node uses to touch the outside world:
//! variables, artefacts, logs, progress, cancellation and capability
//! authorisation. Because everything funnels through it, the runtime can audit,
//! constrain and replay a run without cooperation from individual nodes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use nodara_schema::{ExecutionEvent, LogLevel};
use parking_lot::Mutex;

use crate::audit::{AuditCategory, AuditLog, AuditRecord};
use crate::control::RunControl;
use crate::error::NodeError;
use crate::events::EventBus;
use crate::policy::{ApprovalHandler, CapabilityPolicy, CapabilityRequest, Decision};

/// Metadata for a stored artefact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactMeta {
    /// Opaque identifier.
    pub id: String,
    /// Logical name chosen by the producing node.
    pub name: String,
    /// MIME type.
    pub content_type: String,
    /// Size in bytes.
    pub size: usize,
}

/// In-memory artefact store for images, OCR text and other intermediate data.
#[derive(Debug, Default)]
pub struct ArtifactStore {
    blobs: Mutex<HashMap<String, (ArtifactMeta, Vec<u8>)>>,
}

impl ArtifactStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a blob and return its metadata.
    pub fn put(
        &self,
        name: impl Into<String>,
        content_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> ArtifactMeta {
        let meta = ArtifactMeta {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            content_type: content_type.into(),
            size: bytes.len(),
        };
        self.blobs
            .lock()
            .insert(meta.id.clone(), (meta.clone(), bytes));
        meta
    }

    /// Retrieve a blob by id.
    pub fn get(&self, id: &str) -> Option<Vec<u8>> {
        self.blobs.lock().get(id).map(|(_, bytes)| bytes.clone())
    }

    /// Retrieve metadata by id.
    pub fn meta(&self, id: &str) -> Option<ArtifactMeta> {
        self.blobs.lock().get(id).map(|(meta, _)| meta.clone())
    }

    /// List stored artefacts.
    pub fn list(&self) -> Vec<ArtifactMeta> {
        self.blobs
            .lock()
            .values()
            .map(|(meta, _)| meta.clone())
            .collect()
    }

    /// Number of stored artefacts.
    pub fn len(&self) -> usize {
        self.blobs.lock().len()
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.blobs.lock().is_empty()
    }
}

/// Everything a node executor may observe or mutate during a run.
pub struct ExecutionContext {
    run_id: String,
    workflow_id: String,
    node_id: Option<String>,
    variables: BTreeMap<String, serde_json::Value>,
    secrets: HashSet<String>,
    control: RunControl,
    bus: Arc<EventBus>,
    policy: Arc<dyn CapabilityPolicy>,
    approval: Arc<dyn ApprovalHandler>,
    audit: Arc<dyn AuditLog>,
    artifacts: Arc<ArtifactStore>,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("run_id", &self.run_id)
            .field("workflow_id", &self.workflow_id)
            .field("node_id", &self.node_id)
            .field("variables", &self.variables.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ExecutionContext {
    /// Create a context for one run.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: impl Into<String>,
        workflow_id: impl Into<String>,
        variables: BTreeMap<String, serde_json::Value>,
        secrets: HashSet<String>,
        control: RunControl,
        bus: Arc<EventBus>,
        policy: Arc<dyn CapabilityPolicy>,
        approval: Arc<dyn ApprovalHandler>,
        audit: Arc<dyn AuditLog>,
        artifacts: Arc<ArtifactStore>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            workflow_id: workflow_id.into(),
            node_id: None,
            variables,
            secrets,
            control,
            bus,
            policy,
            approval,
            audit,
            artifacts,
        }
    }

    /// Run identifier.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Workflow identifier.
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Currently executing node, when the engine is between nodes.
    pub fn node_id(&self) -> Option<&str> {
        self.node_id.as_deref()
    }

    /// Called by the engine before each node.
    pub fn set_node(&mut self, node_id: Option<String>) {
        self.node_id = node_id;
    }

    /// The run's control handle.
    pub fn control(&self) -> &RunControl {
        &self.control
    }

    /// The artefact store.
    pub fn artifacts(&self) -> &ArtifactStore {
        &self.artifacts
    }

    /// All current variables.
    pub fn variables(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.variables
    }

    /// All current variables with secret values masked.
    ///
    /// Anything crossing a trust boundary — a plugin process, a run snapshot,
    /// a CLI report — must use this view; the raw scope is for node execution
    /// only.
    pub fn redacted_variables(&self) -> BTreeMap<String, serde_json::Value> {
        self.variables
            .iter()
            .map(|(name, value)| {
                if self.secrets.contains(name) {
                    (name.clone(), serde_json::Value::String("***".into()))
                } else {
                    (name.clone(), value.clone())
                }
            })
            .collect()
    }

    /// Read a variable.
    pub fn variable(&self, name: &str) -> Option<&serde_json::Value> {
        self.variables.get(name)
    }

    /// Publish a variable.
    pub fn set_variable(&mut self, name: impl Into<String>, value: serde_json::Value) {
        self.variables.insert(name.into(), value);
    }

    /// True when `name` is marked as a secret and must not be logged.
    pub fn is_secret(&self, name: &str) -> bool {
        self.secrets.contains(name)
    }

    /// True when cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }

    /// Fail fast when cancellation has been requested.
    pub fn check_cancelled(&self) -> Result<(), NodeError> {
        if self.control.is_cancelled() {
            Err(NodeError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Emit a structured log record.
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        let message = message.into();
        self.bus.emit(ExecutionEvent::Log {
            level,
            message: message.clone(),
            node_id: self.node_id.clone(),
        });
        let mut record = AuditRecord::new(&self.run_id, AuditCategory::Log, message)
            .detail(serde_json::json!({ "level": format!("{level:?}").to_lowercase() }));
        if let Some(node_id) = &self.node_id {
            record = record.node(node_id.clone(), String::new());
        }
        self.audit.record(record);
    }

    /// Report incremental progress for the current node.
    pub fn progress(&self, progress: Option<f64>, message: Option<String>) {
        self.bus.emit(ExecutionEvent::NodeProgress {
            node_id: self.node_id.clone().unwrap_or_default(),
            progress,
            message,
        });
    }

    /// Ask policy (and, if required, an approver) whether a capability may run.
    pub fn authorize(
        &self,
        node_type: &str,
        permissions: &[String],
        dangerous: bool,
        input: &serde_json::Value,
    ) -> Result<Decision, NodeError> {
        let request = CapabilityRequest {
            run_id: self.run_id.clone(),
            node_id: self.node_id.clone().unwrap_or_default(),
            node_type: node_type.to_string(),
            capability: node_type.to_string(),
            permissions: permissions.to_vec(),
            dangerous,
            input: input.clone(),
        };

        let decision = self.policy.decide(&request);
        self.bus.emit(ExecutionEvent::CapabilityDecision {
            capability: request.capability.clone(),
            decision: decision.name().to_string(),
            node_id: self.node_id.clone(),
        });
        self.audit.record(
            AuditRecord::new(
                &self.run_id,
                AuditCategory::CapabilityEvaluated,
                format!("capability `{node_type}` -> {}", decision.name()),
            )
            .node(request.node_id.clone(), node_type)
            .capability(node_type, decision.name())
            .detail(serde_json::json!({
                "permissions": permissions,
                "dangerous": dangerous,
            })),
        );

        match decision {
            Decision::Allow => Ok(Decision::Allow),
            Decision::Deny { reason } => Err(NodeError::denied(node_type, reason)),
            Decision::RequireApproval { reason } => {
                let approved = self.approval.approve(&request);
                self.audit.record(
                    AuditRecord::new(
                        &self.run_id,
                        AuditCategory::Approval,
                        format!(
                            "approval for `{node_type}` {}",
                            if approved { "granted" } else { "refused" }
                        ),
                    )
                    .node(request.node_id.clone(), node_type)
                    .capability(node_type, if approved { "approved" } else { "rejected" }),
                );
                if approved {
                    Ok(Decision::Allow)
                } else {
                    Err(NodeError::denied(node_type, reason))
                }
            }
        }
    }

    /// Resolve `{{variable}}` placeholders in a string.
    pub fn interpolate(&self, text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                if let Some(end) = text[i + 2..].find("}}") {
                    let raw = &text[i + 2..i + 2 + end];
                    let name = raw.trim();
                    if let Some(resolved) = self.lookup(name) {
                        result.push_str(&resolved);
                        i += 2 + end + 2;
                        continue;
                    }
                }
            }
            let ch = text[i..].chars().next().unwrap_or('\u{fffd}');
            result.push(ch);
            i += ch.len_utf8();
        }
        result
    }

    /// Recursively resolve placeholders throughout a JSON value.
    pub fn resolve_value(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(text) => serde_json::Value::String(self.interpolate(text)),
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(|v| self.resolve_value(v)).collect())
            }
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), self.resolve_value(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Resolve a variable name, supporting dotted paths and secret masking.
    fn lookup(&self, name: &str) -> Option<String> {
        let mut segments = name.split('.');
        let head = segments.next()?;
        let mut current = self.variables.get(head)?;
        for segment in segments {
            current = current.get(segment)?;
        }
        if self.secrets.contains(head) {
            return Some("***".to_string());
        }
        Some(match current {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        })
    }
}
