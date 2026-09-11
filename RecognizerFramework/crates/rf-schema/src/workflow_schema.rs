//! Composing the published workflow schema with the installed node catalog.
//!
//! [`crate::workflow_schema`] alone can only describe the *shape* of a
//! workflow: `schemars` does not know which node types a deployment has
//! installed, so `node.type` comes out as a bare string and `node.config` as an
//! unconstrained object. An editor looking at such a schema has nothing to
//! suggest.
//!
//! The runtime does know. The descriptors registered in a [`CapabilityRegistry`]
//! carry, for every node type, a human title, a description and a JSON Schema
//! for its configuration. This module folds that catalog into the schema, which
//! restores the editing experience the original single-process implementation
//! had (one generated schema that knew every node model) while keeping the
//! plugin architecture: a deployment publishes the schema for *its* node set.
//!
//! The composed shape is deliberately the one editors complete best:
//!
//! ```text
//! Node
//! ├── properties.type   -> $ref #/definitions/NodeType   (enum, so values complete)
//! └── allOf[ per node type ]
//!     ├── if   { type: { const: "core.Log" } }
//!     └── then { type: { const, description }, config: { $ref: "...NodeConfig.core.Log" } }
//! ```
//!
//! `allOf` plus `if`/`then` is what makes a config editor offer the right keys:
//! the branch whose `type` `const` matches the document is the branch whose
//! `config` schema completes, and value completion still sees the whole enum.
//! An undiscriminated `config.oneOf` completes keys from the wrong node type,
//! and a bare `oneOf` of whole node variants hides the current type from value
//! completion.
//!
//! [`CapabilityRegistry`]: https://docs.rs/rf-core

use serde_json::{json, Map, Value};

use crate::descriptor::NodeDescriptor;
use crate::workflow::Workflow;

/// Definition holding the enum of installed node types.
pub const NODE_TYPE_DEFINITION: &str = "NodeType";

/// Prefix of the definition holding one node type's configuration schema.
pub const NODE_CONFIG_PREFIX: &str = "NodeConfig.";

/// JSON Schema for a workflow document, with no knowledge of installed nodes.
///
/// `node.type` is any string and `node.config` any object. Use
/// [`workflow_schema_for`] when the installed node catalog is known.
pub fn workflow_schema() -> Value {
    crate::json_schema::<Workflow>()
}

/// JSON Schema for a workflow document that knows the installed node types.
///
/// Every descriptor contributes its type to the `type` enum and its
/// configuration schema to the matching `config` property, so an editor can
/// complete, document and validate node configuration. An empty `descriptors`
/// slice returns the plain [`workflow_schema`].
///
/// The result depends only on the *set* of descriptors, not their order, so two
/// runtimes with the same plugins publish byte-identical schemas.
pub fn workflow_schema_for(descriptors: &[NodeDescriptor]) -> Value {
    let catalog = Catalog::new(descriptors);
    let mut schema = workflow_schema();
    if catalog.is_empty() {
        return schema;
    }

    let Some(root) = schema.as_object_mut() else {
        return schema;
    };
    let definitions = definitions_mut(root);

    definitions.insert(NODE_TYPE_DEFINITION.to_string(), catalog.type_definition());
    for descriptor in catalog.entries() {
        definitions.insert(
            config_definition_name(&descriptor.node_type),
            config_definition(descriptor),
        );
    }
    if let Some(node) = definitions.get_mut("Node").and_then(Value::as_object_mut) {
        apply_catalog(node, &catalog);
    }
    schema
}

/// The `definitions` object, created when `schemars` did not emit one.
fn definitions_mut(root: &mut Map<String, Value>) -> &mut Map<String, Value> {
    if !root.contains_key("definitions") {
        root.insert("definitions".to_string(), Value::Object(Map::new()));
    }
    root.get_mut("definitions")
        .and_then(Value::as_object_mut)
        .expect("definitions is an object")
}

/// Point the `Node` definition at the catalog.
fn apply_catalog(node: &mut Map<String, Value>, catalog: &Catalog) {
    if let Some(properties) = node.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "type".to_string(),
            json!({
                "$ref": format!("#/definitions/{NODE_TYPE_DEFINITION}"),
                "description": "Installed node type. The editor completes this from the \
                                descriptors the runtime published.",
            }),
        );
        if let Some(config) = properties.get_mut("config").and_then(Value::as_object_mut) {
            config.insert(
                "description".to_string(),
                Value::String(
                    "Node-type specific configuration. The shape depends on `type`; the \
                     published schema carries one branch per installed node type."
                        .to_string(),
                ),
            );
        }
    }
    node.insert(
        "allOf".to_string(),
        Value::Array(catalog.branches().collect()),
    );
}

/// Definition name for one node type's configuration schema.
pub fn config_definition_name(node_type: &str) -> String {
    format!("{NODE_CONFIG_PREFIX}{node_type}")
}

/// Normalise a descriptor's config schema for publication.
///
/// Descriptors are written by plugin authors, so the composer fills in the
/// fields an editor relies on rather than trusting every schema to carry them.
fn config_definition(descriptor: &NodeDescriptor) -> Value {
    let mut schema = descriptor.config_schema.clone();
    let Value::Object(object) = &mut schema else {
        return json!({
            "type": "object",
            "title": format!("{} configuration", descriptor.display_name),
            "description": descriptor.description,
            "properties": {},
            "additionalProperties": descriptor.allows_additional_config,
        });
    };

    object
        .entry("type")
        .or_insert_with(|| Value::String("object".to_string()));
    object
        .entry("title")
        .or_insert_with(|| Value::String(format!("{} configuration", descriptor.display_name)));
    if !descriptor.description.is_empty() {
        object
            .entry("description")
            .or_insert_with(|| Value::String(descriptor.description.clone()));
    }
    object
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()));
    object
        .entry("additionalProperties")
        .or_insert_with(|| Value::Bool(descriptor.allows_additional_config));
    object.insert(
        "x-rf-node-type".to_string(),
        Value::String(descriptor.node_type.clone()),
    );
    schema
}

/// The installed node types, sorted and de-duplicated.
struct Catalog<'a> {
    entries: Vec<&'a NodeDescriptor>,
}

impl<'a> Catalog<'a> {
    fn new(descriptors: &'a [NodeDescriptor]) -> Self {
        let mut entries: Vec<&NodeDescriptor> = descriptors
            .iter()
            .filter(|descriptor| !descriptor.node_type.trim().is_empty())
            .collect();
        entries.sort_by(|left, right| left.node_type.cmp(&right.node_type));
        entries.dedup_by(|left, right| left.node_type == right.node_type);
        Self { entries }
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entries(&self) -> impl Iterator<Item = &'a NodeDescriptor> + '_ {
        self.entries.iter().copied()
    }

    /// The enum of installed node types.
    fn type_definition(&self) -> Value {
        json!({
            "type": "string",
            "description": "Namespaced node type, e.g. `windows.Input.Keyboard`. One of the \
                            node types the runtime has installed.",
            "enum": self
                .entries()
                .map(|descriptor| Value::String(descriptor.node_type.clone()))
                .collect::<Vec<_>>(),
        })
    }

    /// One `if`/`then` branch per node type.
    fn branches(&self) -> impl Iterator<Item = Value> + '_ {
        self.entries().map(branch)
    }
}

/// A single `if`/`then` branch keyed on the node type.
fn branch(descriptor: &NodeDescriptor) -> Value {
    let mut then_type = json!({ "const": descriptor.node_type });
    if !descriptor.description.is_empty() {
        then_type["description"] = Value::String(descriptor.description.clone());
    }
    let mut then_properties = Map::new();
    then_properties.insert("type".to_string(), then_type);
    then_properties.insert(
        "config".to_string(),
        json!({ "$ref": format!("#/definitions/{}", config_definition_name(&descriptor.node_type)) }),
    );
    json!({
        "if": {
            "properties": { "type": { "const": descriptor.node_type } },
            "required": ["type"],
        },
        "then": { "properties": Value::Object(then_properties) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::NodeDescriptor;

    fn descriptor(node_type: &str, description: &str) -> NodeDescriptor {
        NodeDescriptor {
            config_schema: json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Message template" },
                    "level": { "type": "string", "enum": ["info", "warn"], "default": "info" }
                },
                "required": ["message"],
                "additionalProperties": false,
            }),
            ..NodeDescriptor::new(node_type, node_type, "Test").with_description(description)
        }
    }

    #[test]
    fn bare_schema_leaves_config_open() {
        let schema = workflow_schema();
        let node_type = &schema["definitions"]["Node"]["properties"]["type"];
        assert_eq!(node_type["type"], "string");
        assert!(node_type["enum"].is_null());
    }

    #[test]
    fn the_document_documents_its_own_schema_reference() {
        let schema = workflow_schema();
        assert_eq!(
            schema["properties"]["$schema"]["type"],
            json!(["string", "null"])
        );
        assert!(schema["properties"]["$schema"]["description"].is_string());
    }

    #[test]
    fn empty_catalog_keeps_the_bare_schema() {
        assert_eq!(workflow_schema_for(&[]), workflow_schema());
    }

    #[test]
    fn publishes_every_node_type_sorted() {
        let schema = workflow_schema_for(&[
            descriptor("windows.Input.Keyboard", "Keyboard"),
            descriptor("core.Log", "Log"),
        ]);
        assert_eq!(
            schema["definitions"][NODE_TYPE_DEFINITION]["enum"],
            json!(["core.Log", "windows.Input.Keyboard"])
        );
        assert_eq!(
            schema["definitions"]["Node"]["properties"]["type"]["$ref"],
            format!("#/definitions/{NODE_TYPE_DEFINITION}")
        );
    }

    #[test]
    fn publishes_one_if_then_branch_per_node_type() {
        let schema = workflow_schema_for(&[descriptor("core.Log", "Log")]);
        let branches = schema["definitions"]["Node"]["allOf"]
            .as_array()
            .expect("allOf is an array");
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0]["if"]["properties"]["type"]["const"], "core.Log");
        assert_eq!(
            branches[0]["then"]["properties"]["config"]["$ref"],
            "#/definitions/NodeConfig.core.Log"
        );
        assert_eq!(
            branches[0]["then"]["properties"]["type"]["description"],
            "Log"
        );
    }

    #[test]
    fn config_definition_keeps_the_descriptor_schema() {
        let schema = workflow_schema_for(&[descriptor("core.Log", "Log")]);
        let config = &schema["definitions"]["NodeConfig.core.Log"];
        assert_eq!(
            config["properties"]["message"]["description"],
            "Message template"
        );
        assert_eq!(
            config["properties"]["level"]["enum"],
            json!(["info", "warn"])
        );
        assert_eq!(config["properties"]["level"]["default"], "info");
        assert_eq!(config["required"], json!(["message"]));
        assert_eq!(config["additionalProperties"], false);
        assert_eq!(config["x-rf-node-type"], "core.Log");
    }

    #[test]
    fn config_definition_fills_in_missing_metadata() {
        let bare = NodeDescriptor {
            config_schema: json!({}),
            ..NodeDescriptor::new("test.Bare", "Bare Node", "Test").with_description("Bare")
        };
        let schema = workflow_schema_for(&[bare]);
        let config = &schema["definitions"]["NodeConfig.test.Bare"];
        assert_eq!(config["type"], "object");
        assert_eq!(config["title"], "Bare Node configuration");
        assert_eq!(config["description"], "Bare");
        assert_eq!(config["additionalProperties"], true);
    }

    #[test]
    fn output_is_independent_of_descriptor_order() {
        let forward =
            workflow_schema_for(&[descriptor("core.Log", "Log"), descriptor("core.End", "End")]);
        let reverse =
            workflow_schema_for(&[descriptor("core.End", "End"), descriptor("core.Log", "Log")]);
        assert_eq!(forward, reverse);
    }

    #[test]
    fn duplicate_node_types_are_ignored() {
        let schema = workflow_schema_for(&[
            descriptor("core.Log", "first"),
            descriptor("core.Log", "second"),
        ]);
        assert_eq!(
            schema["definitions"][NODE_TYPE_DEFINITION]["enum"],
            json!(["core.Log"])
        );
        assert_eq!(
            schema["definitions"]["Node"]["allOf"]
                .as_array()
                .expect("allOf")
                .len(),
            1
        );
    }

    #[test]
    fn blank_node_types_are_ignored() {
        let schema = workflow_schema_for(&[descriptor("  ", "Blank")]);
        assert_eq!(schema, workflow_schema());
    }
}
