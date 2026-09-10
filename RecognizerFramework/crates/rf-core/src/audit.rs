//! Append-only audit records.
//!
//! The audit log is the durable counterpart to the event stream: events are for
//! live observation and may be dropped by a slow subscriber, whereas audit
//! records are retained and are expected to survive the process.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Category of an audit record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditCategory {
    /// A run started.
    RunStarted,
    /// A run reached a terminal state.
    RunFinished,
    /// A node started.
    NodeStarted,
    /// A node succeeded.
    NodeFinished,
    /// A node failed.
    NodeFailed,
    /// A capability was evaluated by policy.
    CapabilityEvaluated,
    /// An approval decision was taken.
    Approval,
    /// A structured log line.
    Log,
}

/// One immutable audit entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// Monotonic sequence number within the log.
    pub seq: u64,
    /// Unix epoch milliseconds.
    pub timestamp_ms: u64,
    /// Run the record belongs to.
    pub run_id: String,
    /// Record category.
    pub category: AuditCategory,
    /// Related node, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Related node type, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    /// Capability under evaluation, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// Decision or status string, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    /// Human-readable summary.
    pub message: String,
    /// Structured payload.
    #[serde(default)]
    pub detail: serde_json::Value,
}

impl AuditRecord {
    /// Start building a record for `run_id`.
    pub fn new(
        run_id: impl Into<String>,
        category: AuditCategory,
        message: impl Into<String>,
    ) -> Self {
        Self {
            seq: 0,
            timestamp_ms: rf_schema::event::now_ms(),
            run_id: run_id.into(),
            category,
            node_id: None,
            node_type: None,
            capability: None,
            decision: None,
            message: message.into(),
            detail: serde_json::Value::Null,
        }
    }

    /// Builder-style node attribution.
    #[must_use]
    pub fn node(mut self, node_id: impl Into<String>, node_type: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self.node_type = Some(node_type.into());
        self
    }

    /// Builder-style capability attribution.
    #[must_use]
    pub fn capability(
        mut self,
        capability: impl Into<String>,
        decision: impl Into<String>,
    ) -> Self {
        self.capability = Some(capability.into());
        self.decision = Some(decision.into());
        self
    }

    /// Builder-style detail payload.
    #[must_use]
    pub fn detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = detail;
        self
    }
}

/// Sink for audit records.
pub trait AuditLog: Send + Sync {
    /// Append a record, assigning it a sequence number.
    fn record(&self, record: AuditRecord);

    /// Every record retained so far, oldest first.
    fn records(&self) -> Vec<AuditRecord>;
}

/// Discards records.
#[derive(Debug, Default)]
pub struct NullAuditLog;

impl AuditLog for NullAuditLog {
    fn record(&self, _record: AuditRecord) {}

    fn records(&self) -> Vec<AuditRecord> {
        Vec::new()
    }
}

/// Retains records in memory.
#[derive(Debug, Default)]
pub struct InMemoryAuditLog {
    records: Mutex<Vec<AuditRecord>>,
    seq: AtomicU64,
}

impl InMemoryAuditLog {
    /// Create an empty log.
    pub fn new() -> Self {
        Self::default()
    }
}

impl AuditLog for InMemoryAuditLog {
    fn record(&self, mut record: AuditRecord) {
        record.seq = self.seq.fetch_add(1, Ordering::SeqCst);
        self.records.lock().push(record);
    }

    fn records(&self) -> Vec<AuditRecord> {
        self.records.lock().clone()
    }
}

/// Appends newline-delimited JSON to a file.
///
/// JSON Lines is chosen because it is append-only, streamable and trivially
/// replayable without loading the whole history.
#[derive(Debug)]
pub struct JsonlAuditLog {
    path: PathBuf,
    file: Mutex<std::fs::File>,
    seq: AtomicU64,
}

impl JsonlAuditLog {
    /// Open (creating if necessary) an audit file for appending.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
            seq: AtomicU64::new(0),
        })
    }

    /// Path of the underlying file.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl AuditLog for JsonlAuditLog {
    fn record(&self, mut record: AuditRecord) {
        record.seq = self.seq.fetch_add(1, Ordering::SeqCst);
        if let Ok(line) = serde_json::to_string(&record) {
            let mut file = self.file.lock();
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    }

    fn records(&self) -> Vec<AuditRecord> {
        std::fs::read_to_string(&self.path)
            .map(|content| {
                content
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .filter_map(|line| serde_json::from_str::<AuditRecord>(line).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_log_assigns_sequence_numbers() {
        let log = InMemoryAuditLog::new();
        log.record(AuditRecord::new(
            "run",
            AuditCategory::RunStarted,
            "started",
        ));
        log.record(AuditRecord::new("run", AuditCategory::RunFinished, "done"));
        let records = log.records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].seq, 0);
        assert_eq!(records[1].seq, 1);
    }

    #[test]
    fn jsonl_log_round_trips() {
        let dir = std::env::temp_dir().join(format!("rf-audit-{}", uuid::Uuid::new_v4()));
        let path = dir.join("audit.jsonl");
        let log = JsonlAuditLog::open(&path).expect("open");
        log.record(
            AuditRecord::new("run", AuditCategory::CapabilityEvaluated, "checked")
                .capability("windows.Input.Keyboard", "require_approval"),
        );
        let records = log.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].decision.as_deref(), Some("require_approval"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
