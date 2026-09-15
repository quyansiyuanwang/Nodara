//! Node descriptors: the machine-readable contract a plugin publishes for each
//! node type it provides.
//!
//! A descriptor is what makes the ecosystem work. The Studio renders node
//! palettes and configuration forms from it, the agent selects capabilities from
//! it, and the runtime validates configs against it. None of those consumers
//! need to know the plugin's implementation language.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// Direction of a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    /// Data flows into the node.
    Input,
    /// Data flows out of the node.
    Output,
}

/// Abstract data type used for editor-time compatibility checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    /// Any JSON value.
    Any,
    /// UTF-8 string.
    String,
    /// Integer or float.
    Number,
    /// Boolean.
    Boolean,
    /// JSON object.
    Object,
    /// JSON array.
    Array,
    /// Raw image bytes plus dimensions.
    Image,
    /// A native window handle.
    Window,
    /// A filesystem path.
    Path,
}

impl ValueType {
    /// Whether a value produced as `self` can feed a port expecting `target`.
    ///
    /// This is intentionally conservative: `Any` accepts everything, paths and
    /// strings are interchangeable text carriers, and every other pair must
    /// match exactly. The check is shared by the editor and runtime validation.
    pub fn is_compatible_with(self, target: Self) -> bool {
        self == target
            || self == Self::Any
            || target == Self::Any
            || matches!(
                (self, target),
                (Self::String, Self::Path) | (Self::Path, Self::String)
            )
    }
}

/// A single input or output port on a node.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct PortDescriptor {
    /// Stable port name used by edges.
    pub name: String,
    /// Human-facing label.
    pub display_name: String,
    /// Direction of the port.
    pub kind: PortKind,
    /// Abstract type carried by the port.
    pub value_type: ValueType,
    /// Whether an edge must be attached for execution to succeed.
    #[serde(default)]
    pub required: bool,
    /// Optional documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Default value shown in the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

impl PortDescriptor {
    /// Construct a port descriptor.
    pub fn new(
        name: impl Into<String>,
        display_name: impl Into<String>,
        kind: PortKind,
        value_type: ValueType,
    ) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            kind,
            value_type,
            required: false,
            description: None,
            default: None,
        }
    }

    /// Builder-style required flag.
    #[must_use]
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
}

/// Machine-readable description of a node type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct NodeDescriptor {
    /// Namespaced node type, e.g. `windows.Input.Keyboard`.
    pub node_type: String,
    /// Human-facing name shown in the palette.
    pub display_name: String,
    /// Palette grouping, e.g. `Input`.
    pub category: String,
    /// Short description shown as a tooltip.
    #[serde(default)]
    pub description: String,
    /// Semantic version of this node type's contract.
    #[serde(default)]
    pub version: String,
    /// Declared input ports.
    #[serde(default)]
    pub inputs: Vec<PortDescriptor>,
    /// Declared output ports.
    #[serde(default)]
    pub outputs: Vec<PortDescriptor>,
    /// JSON Schema describing the `config` object.
    #[serde(default)]
    pub config_schema: serde_json::Value,
    /// Capability identifiers this node uses, e.g. `Input.Keyboard`.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Permissions required, e.g. `input.control`.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Whether the node performs a side effect that policy may gate.
    #[serde(default)]
    pub dangerous: bool,
    /// Whether unknown config keys are permitted.
    #[serde(default = "default_true")]
    pub allows_additional_config: bool,
    /// Contributing plugin id, when the node comes from a plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

impl NodeDescriptor {
    /// Create a minimal descriptor for a node type.
    pub fn new(
        node_type: impl Into<String>,
        display_name: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            node_type: node_type.into(),
            display_name: display_name.into(),
            category: category.into(),
            description: String::new(),
            version: String::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": true
            }),
            capabilities: Vec::new(),
            permissions: Vec::new(),
            dangerous: false,
            allows_additional_config: true,
            plugin_id: None,
        }
    }

    /// Builder-style description setter.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Builder-style config-schema setter.
    #[must_use]
    pub fn with_config_schema(mut self, schema: serde_json::Value) -> Self {
        self.config_schema = schema;
        self
    }

    /// Builder-style permission setter.
    #[must_use]
    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }
}

#[cfg(test)]
mod value_type_tests {
    use super::ValueType;

    #[test]
    fn compatibility_is_conservative_and_any_is_a_wildcard() {
        assert!(ValueType::Any.is_compatible_with(ValueType::Image));
        assert!(ValueType::String.is_compatible_with(ValueType::Path));
        assert!(ValueType::Path.is_compatible_with(ValueType::String));
        assert!(ValueType::Number.is_compatible_with(ValueType::Number));
        assert!(!ValueType::Number.is_compatible_with(ValueType::String));
        assert!(!ValueType::Image.is_compatible_with(ValueType::Object));
    }
}
