//! # nodara-schema
//!
//! Stable, host-agnostic data contracts for Nodara-Core.
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
//! The workflow document is the one schema that is *composed* rather than merely
//! derived: [`workflow_schema_for`] folds the installed node descriptors into it,
//! so `node.type` and `node.config` carry the same titles, descriptions, defaults
//! and enums the original single-process implementation published. An editor
//! picks them up from the document's `$schema` field; see
//! [`workflow_schema`] for the composition itself.
//!
//! ## Versioning axes
//!
//! The project versions three things independently (see [`version`]):
//!
//! | axis                | field              | owner        |
//! |---------------------|--------------------|--------------|
//! | workflow format     | `schema_version`   | this crate   |
//! | plugin/runtime wire | `protocol_version` | this crate   |
//! | public HTTP API     | `api_version`      | `nodara-runtime` |

pub mod descriptor;
pub mod error;
pub mod event;
pub mod extension;
pub mod graph;
pub mod manifest;
pub mod migration;
pub mod session;
pub mod tool;
pub mod validation;
pub mod version;
pub mod workflow;
pub mod workflow_schema;

pub use descriptor::{NodeDescriptor, PortDescriptor, PortKind, ValueType};
pub use error::{SchemaError, SchemaResult};
pub use event::{EventEnvelope, ExecutionEvent, LogLevel, NodeInputSnapshot, RunStatus};
pub use extension::{ExtensionDescriptor, ExtensionKind};
pub use graph::{GraphError, WorkflowGraph};
pub use manifest::{PluginFeature, PluginManifest, MANIFEST_FILE};
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
pub use workflow::{Edge, EdgeBranch, Metadata, Node, Position, RetryBackoff, Variable, Workflow};
pub use workflow_schema::{
    config_definition_name, workflow_schema, workflow_schema_for, NODE_CONFIG_PREFIX,
    NODE_TYPE_DEFINITION,
};

/// Serialize the JSON Schema for `T`.
pub fn json_schema<T: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(serde_json::Value::Null)
}

/// JSON Schema for a plugin manifest.
pub fn manifest_schema() -> serde_json::Value {
    json_schema::<PluginManifest>()
}

/// JSON Schema for a node descriptor.
pub fn descriptor_schema() -> serde_json::Value {
    json_schema::<NodeDescriptor>()
}

/// JSON Schema for a unified runtime extension registration.
pub fn extension_schema() -> serde_json::Value {
    json_schema::<ExtensionDescriptor>()
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
