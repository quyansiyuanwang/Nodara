//! Stable extension registration descriptors.
//!
//! These types cross the runtime API and plugin manifest boundary. The concrete
//! registry lives in `nodara-core`; the wire contract lives here so every host
//! and client reads the same shape.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Origin category for one runtime extension.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    /// Ships with the runtime itself.
    Builtin,
    /// Linked into the runtime by an embedding host.
    InProcess,
    /// Loaded through a plugin manifest.
    Plugin,
    /// A declarative or host-provided UI contribution.
    Ui,
    /// A policy contribution.
    Policy,
    /// A host or external-system integration.
    #[default]
    Integration,
    /// Any extension that does not fit a more specific category.
    Other,
}

/// One discoverable runtime extension.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ExtensionDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: ExtensionKind,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub node_types: Vec<String>,
    pub loaded: bool,
}
