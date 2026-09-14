//! Workflow document model (`schema_version` 2.x).
//!
//! The model is intentionally a *service description*, never an execution plan:
//! it declares what should happen, not how a runtime schedules it. That keeps the
//! same document usable by the Studio editor, the autonomous agent and the
//! headless runtime.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::version::SCHEMA_VERSION;

/// Conventional file name for a workflow document on disk.
pub const WORKFLOW_FILE: &str = "workflow.json";

fn default_schema_version() -> String {
    SCHEMA_VERSION.to_string()
}

fn default_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

/// Human-facing metadata attached to a workflow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(default)]
pub struct Metadata {
    /// Display name.
    pub name: String,
    /// Longer description.
    pub description: Option<String>,
    /// Free-form tags used for search.
    pub tags: Vec<String>,
    /// Author or owning team.
    pub author: Option<String>,
    /// Semantic version of the workflow itself.
    pub version: Option<String>,
    /// ISO-8601 creation timestamp.
    pub created_at: Option<String>,
    /// ISO-8601 last-modified timestamp.
    pub updated_at: Option<String>,
    /// Extension bag. Unknown keys are preserved on round-trip.
    pub extensions: BTreeMap<String, serde_json::Value>,
}

/// Canvas position for editor round-tripping. Ignored by the runtime.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Position {
    /// Horizontal offset in canvas units.
    pub x: f64,
    /// Vertical offset in canvas units.
    pub y: f64,
}

/// A variable declared at workflow scope.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(default)]
pub struct Variable {
    /// Default value.
    pub value: serde_json::Value,
    /// Optional documentation string.
    pub description: Option<String>,
    /// Marks a value that must never be written to logs or audit records.
    pub secret: bool,
}

/// A node in the workflow graph.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Node {
    /// Unique, stable identifier within the workflow.
    pub id: String,
    /// Namespaced node type, e.g. `windows.Input.Keyboard`.
    ///
    /// `workflow_schema_for` publishes the enum of types the runtime installed,
    /// so an editor can complete this field.
    #[serde(rename = "type")]
    pub node_type: String,
    /// Optional display label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Node-type specific configuration, validated against the node descriptor's
    /// config schema (published per node type in the workflow JSON Schema).
    #[serde(default = "default_object")]
    pub config: serde_json::Value,
    /// Editor position (ignored by the runtime).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Whether this node participates in execution. Disabled nodes are treated
    /// as transparent pass-throughs, so their outgoing branches remain usable.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// Delay before the executor is invoked, in milliseconds.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub delay_before_ms: u64,
    /// Delay after successful execution, before outgoing branches activate.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub delay_after_ms: u64,
    /// Whether execution continues through this node's outgoing branches after
    /// all retry attempts fail. Policy denials and validation errors never use
    /// this escape hatch.
    #[serde(default, skip_serializing_if = "is_false")]
    pub continue_on_error: bool,
    /// Number of additional attempts after the first failed execution.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub retry: u32,
    /// Delay between failed attempts, in milliseconds.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub retry_delay_ms: u64,
    /// Extension bag.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl Node {
    /// Construct a node with an empty configuration object.
    pub fn new(id: impl Into<String>, node_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            node_type: node_type.into(),
            label: None,
            config: default_object(),
            position: None,
            enabled: true,
            delay_before_ms: 0,
            delay_after_ms: 0,
            continue_on_error: false,
            retry: 0,
            retry_delay_ms: 0,
            metadata: BTreeMap::new(),
        }
    }

    /// Construct a node with a display label.
    pub fn labelled(
        id: impl Into<String>,
        node_type: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        let mut node = Self::new(id, node_type);
        node.label = Some(label.into());
        node
    }

    /// Builder-style configuration setter.
    #[must_use]
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }
}

/// A directed edge between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Edge {
    /// Unique identifier within the workflow.
    pub id: String,
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Optional named output port on the source node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_port: Option<String>,
    /// Optional named input port on the target node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_port: Option<String>,
    /// Optional guard expression. The edge is only taken when it evaluates truthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Optional display label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Edge {
    /// Construct an unconditional edge.
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            source_port: None,
            target_port: None,
            condition: None,
            label: None,
        }
    }
}

/// A complete workflow document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Workflow {
    /// Optional `$schema` reference to the JSON Schema this document follows.
    ///
    /// Editors resolve it to complete node types, configuration keys, defaults
    /// and enums — see [`crate::workflow_schema_for`]. The field is part of the
    /// document and survives every round-trip (migration, load, save), so a
    /// workflow keeps its content hints wherever it is opened.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema_url: Option<String>,
    /// Workflow format version; this build writes `2.0` and migrates `1.x`.
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Stable workflow identifier, e.g. `workflow.example`.
    pub id: String,
    /// Human-facing metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// All nodes in the graph.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// All edges in the graph.
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Workflow-scoped variables.
    #[serde(default)]
    pub variables: BTreeMap<String, Variable>,
}

impl Workflow {
    /// Create an empty workflow with the current schema version.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            schema_url: None,
            schema_version: default_schema_version(),
            metadata: Metadata {
                name: id.clone(),
                ..Metadata::default()
            },
            id,
            nodes: Vec::new(),
            edges: Vec::new(),
            variables: BTreeMap::new(),
        }
    }

    /// Add a node.
    pub fn add_node(&mut self, node: Node) -> &mut Self {
        self.nodes.push(node);
        self
    }

    /// Add an edge.
    pub fn add_edge(&mut self, edge: Edge) -> &mut Self {
        self.edges.push(edge);
        self
    }

    /// Look up a node by id.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Mutable lookup by id.
    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// True when a node with `id` exists.
    pub fn contains_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|n| n.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_current_schema_version() {
        let wf = Workflow::new("wf.test");
        assert_eq!(wf.schema_version, SCHEMA_VERSION);
        assert_eq!(wf.metadata.name, "wf.test");
    }

    #[test]
    fn node_defaults_config_to_object() {
        let node = Node::new("n1", "core.Log");
        assert!(node.config.is_object());
        assert!(node.config.as_object().unwrap().is_empty());
    }

    #[test]
    fn workflow_round_trips_through_json() {
        let mut wf = Workflow::new("wf.roundtrip");
        wf.add_node(Node::new("start", "core.Start"));
        wf.add_node(
            Node::labelled("log", "core.Log", "Greeting")
                .with_config(serde_json::json!({ "message": "hello" })),
        );
        wf.add_edge(Edge::new("e1", "start", "log"));

        let json = serde_json::to_string_pretty(&wf).unwrap();
        let back: Workflow = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wf);
    }

    #[test]
    fn execution_settings_round_trip_through_json() {
        let mut node = Node::new("log", "core.Log");
        node.enabled = false;
        node.delay_before_ms = 25;
        node.delay_after_ms = 50;
        node.continue_on_error = true;
        node.retry = 3;
        node.retry_delay_ms = 100;

        let json = serde_json::to_string(&node).unwrap();
        let back: Node = serde_json::from_str(&json).unwrap();
        assert!(!back.enabled);
        assert_eq!(back.delay_before_ms, 25);
        assert_eq!(back.delay_after_ms, 50);
        assert!(back.continue_on_error);
        assert_eq!(back.retry, 3);
        assert_eq!(back.retry_delay_ms, 100);
    }

    #[test]
    fn node_type_is_serialized_as_type_key() {
        let node = Node::new("n1", "windows.Input.Keyboard");
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["type"], "windows.Input.Keyboard");
        assert!(json.get("node_type").is_none());
    }

    #[test]
    fn schema_reference_round_trips() {
        let document = serde_json::json!({
            "$schema": "../Nodara-Core/schema/workflow.schema.json",
            "schema_version": "2.0",
            "id": "wf.hinted"
        });
        let workflow: Workflow = serde_json::from_value(document).unwrap();
        assert_eq!(
            workflow.schema_url.as_deref(),
            Some("../Nodara-Core/schema/workflow.schema.json")
        );
        let back = serde_json::to_value(&workflow).unwrap();
        assert_eq!(
            back["$schema"],
            "../Nodara-Core/schema/workflow.schema.json"
        );
    }

    #[test]
    fn documents_without_a_schema_reference_stay_clean() {
        let json = serde_json::to_value(Workflow::new("wf.plain")).unwrap();
        assert!(json.get("$schema").is_none());
    }
}
