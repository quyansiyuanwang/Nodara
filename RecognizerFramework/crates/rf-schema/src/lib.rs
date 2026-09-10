//! # rf-schema
//!
//! Stable, host-agnostic data contracts for RecognizerFramework.
//!
//! This crate is deliberately free of UI, model and platform concerns. It is the
//! single source of truth for every document that crosses a process boundary:
//!
//! * workflow documents ([`Workflow`])
//! * plugin manifests ([`PluginManifest`])
//! * node descriptors / capabilities ([`NodeDescriptor`])
//! * execution events ([`EventEnvelope`])
//!
//! Every type derives [`schemars::JsonSchema`], so the published JSON Schema
//! documents under `schema/` are generated from the same definitions the runtime
//! uses. That keeps protocol, SDK and documentation from drifting apart.
//!
//! ## Versioning axes
//!
//! The project versions three things independently (see [`version`]):
//!
//! | axis                | field              | owner        |
//! |---------------------|--------------------|--------------|
//! | workflow format     | `schema_version`   | this crate   |
//! | plugin/runtime wire | `protocol_version` | this crate   |
//! | public HTTP API     | `api_version`      | `rf-runtime` |

pub mod descriptor;
pub mod error;
pub mod event;
pub mod graph;
pub mod manifest;
pub mod migration;
pub mod session;
pub mod tool;
pub mod validation;
pub mod version;
pub mod workflow;

pub use descriptor::{NodeDescriptor, PortDescriptor, PortKind, ValueType};
pub use error::{SchemaError, SchemaResult};
pub use event::{EventEnvelope, ExecutionEvent, LogLevel, RunStatus};
pub use graph::{GraphError, WorkflowGraph};
pub use manifest::{PluginManifest, MANIFEST_FILE};
pub use migration::{migrate, MigrationReport};
pub use session::{
    AgentSession, ApprovalDecision, ApprovalDecisionRequest, ApprovalRequest, MessageRole,
    PlanPreview, SessionMessage, SessionMessageRequest, SessionRequest, SessionStatus,
};
pub use tool::{ToolCall, ToolCallOutcome};
pub use validation::{
    validate, validate_with, Diagnostic, NodeTypeIndex, Severity, ValidationOptions,
    ValidationReport,
};
pub use version::{API_VERSION, PROTOCOL_VERSION, SCHEMA_VERSION};
pub use workflow::{Edge, Metadata, Node, Position, Variable, Workflow};

/// Serialize the JSON Schema for `T`.
pub fn json_schema<T: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(serde_json::Value::Null)
}

/// JSON Schema for a workflow document.
pub fn workflow_schema() -> serde_json::Value {
    json_schema::<Workflow>()
}

/// JSON Schema for a plugin manifest.
pub fn manifest_schema() -> serde_json::Value {
    json_schema::<PluginManifest>()
}

/// JSON Schema for a node descriptor.
pub fn descriptor_schema() -> serde_json::Value {
    json_schema::<NodeDescriptor>()
}

/// JSON Schema for an execution event envelope.
pub fn event_schema() -> serde_json::Value {
    json_schema::<EventEnvelope>()
}

/// JSON Schema for an agent tool call.
pub fn tool_call_schema() -> serde_json::Value {
    json_schema::<ToolCall>()
}

/// JSON Schema for an agent session.
pub fn session_schema() -> serde_json::Value {
    json_schema::<AgentSession>()
}
