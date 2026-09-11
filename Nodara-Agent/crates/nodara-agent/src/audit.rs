//! Decision trace and replay.
//!
//! Every step the agent takes is recorded: the goal, each model call with its
//! token usage, each validation report, each guardrail decision and each run
//! outcome. The trace is JSON Lines, so it appends cheaply and replays without
//! loading the whole history.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AgentResult;

/// What kind of step a trace entry records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceStep {
    /// The operator's goal.
    Goal,
    /// A model call.
    ModelCall,
    /// The runtime's verdict on a draft.
    Validation,
    /// A guardrail decision.
    Guardrail,
    /// A run was started.
    RunStarted,
    /// A run finished.
    RunFinished,
    /// Anything else worth keeping.
    Note,
}

/// One recorded step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    /// Monotonic sequence number.
    pub seq: u64,
    /// Unix epoch milliseconds.
    pub timestamp_ms: u64,
    /// Step kind.
    pub step: TraceStep,
    /// Short human-readable summary.
    pub summary: String,
    /// Structured payload.
    #[serde(default)]
    pub payload: Value,
}

/// An append-only trace.
#[derive(Debug)]
pub struct AuditTrace {
    path: Option<PathBuf>,
    entries: Vec<TraceEntry>,
}

impl AuditTrace {
    /// Keep the trace in memory only.
    pub fn in_memory() -> Self {
        Self {
            path: None,
            entries: Vec::new(),
        }
    }

    /// Append to `path` as well as keeping the trace in memory.
    pub fn open(path: impl Into<PathBuf>) -> AgentResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            path: Some(path),
            entries: Vec::new(),
        })
    }

    /// Where the trace is being written, when it is persisted.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Record a step.
    pub fn record(&mut self, step: TraceStep, summary: impl Into<String>, payload: Value) {
        let entry = TraceEntry {
            seq: self.entries.len() as u64,
            timestamp_ms: nodara_schema::event::now_ms(),
            step,
            summary: summary.into(),
            payload,
        };
        if let Some(path) = &self.path {
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path) {
                if let Ok(line) = serde_json::to_string(&entry) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
        self.entries.push(entry);
    }

    /// Every entry recorded so far.
    pub fn entries(&self) -> &[TraceEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Read a trace back from disk.
    pub fn load(path: impl AsRef<Path>) -> AgentResult<Vec<TraceEntry>> {
        let content = std::fs::read_to_string(path)?;
        Ok(content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<TraceEntry>(line).ok())
            .collect())
    }
}

impl Default for AuditTrace {
    fn default() -> Self {
        Self::in_memory()
    }
}

/// Render a trace as a readable timeline.
pub fn render(entries: &[TraceEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{:>2}  {:?}  {}", entry.seq, entry.step, entry.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_monotonic_sequence_numbers() {
        let mut trace = AuditTrace::in_memory();
        trace.record(TraceStep::Goal, "do a thing", Value::Null);
        trace.record(TraceStep::Note, "and another", Value::Null);
        assert_eq!(trace.len(), 2);
        assert_eq!(trace.entries()[0].seq, 0);
        assert_eq!(trace.entries()[1].seq, 1);
    }

    #[test]
    fn round_trips_through_jsonl() {
        let directory = std::env::temp_dir().join(format!(
            "nodara-agent-trace-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        ));
        let path = directory.join("trace.jsonl");
        {
            let mut trace = AuditTrace::open(&path).expect("open");
            trace.record(
                TraceStep::ModelCall,
                "draft",
                serde_json::json!({ "tokens": 42 }),
            );
            trace.record(TraceStep::Validation, "accepted", Value::Null);
        }
        let entries = AuditTrace::load(&path).expect("load");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].step, TraceStep::ModelCall);
        assert_eq!(entries[0].payload["tokens"], 42);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
