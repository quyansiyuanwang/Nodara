//! Unified registration metadata for runtime capabilities.
//!
//! Node execution stays in [`crate::CapabilityRegistry`]. This registry answers
//! the management question above it: which built-in, in-process or plugin
//! extension provided a capability, is it loaded, and what does it expose?

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Origin category for one runtime extension.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    /// Ships with the runtime itself.
    Builtin,
    /// Linked into the runtime by an embedding host.
    InProcess,
    /// Loaded through a plugin manifest.
    Plugin,
    /// A future UI-only contribution.
    Ui,
    /// A future policy contribution.
    Policy,
    /// Any extension that does not fit a more specific category.
    Other,
}

/// One discoverable runtime extension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionDescriptor {
    /// Stable reverse-DNS identifier.
    pub id: String,
    /// Human-facing name.
    pub name: String,
    /// Extension version.
    pub version: String,
    /// Registration category.
    pub kind: ExtensionKind,
    /// Where the extension came from, e.g. `runtime` or `plugin`.
    pub source: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Capability identifiers contributed by the extension.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Permissions declared by the extension.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Node types contributed by the extension.
    #[serde(default)]
    pub node_types: Vec<String>,
    /// Whether the extension's executable side is currently available.
    pub loaded: bool,
}

/// Registry shared by built-in, in-process and plugin extensions.
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    entries: BTreeMap<String, ExtensionDescriptor>,
}

impl ExtensionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace one extension.
    pub fn register(&mut self, descriptor: ExtensionDescriptor) -> &mut Self {
        self.entries.insert(descriptor.id.clone(), descriptor);
        self
    }

    /// Look up one extension by id.
    pub fn get(&self, id: &str) -> Option<&ExtensionDescriptor> {
        self.entries.get(id)
    }

    /// Every extension sorted by id.
    pub fn descriptors(&self) -> Vec<ExtensionDescriptor> {
        self.entries.values().cloned().collect()
    }

    /// Number of registered extensions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no extension is registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(id: &str, loaded: bool) -> ExtensionDescriptor {
        ExtensionDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            version: "1.0.0".to_string(),
            kind: ExtensionKind::Plugin,
            source: "test".to_string(),
            description: None,
            capabilities: Vec::new(),
            permissions: Vec::new(),
            node_types: Vec::new(),
            loaded,
        }
    }

    #[test]
    fn registrations_are_sorted_and_replaceable() {
        let mut registry = ExtensionRegistry::new();
        registry.register(descriptor("z.plugin", true));
        registry.register(descriptor("a.plugin", false));
        assert_eq!(registry.descriptors()[0].id, "a.plugin");

        let mut updated = descriptor("a.plugin", true);
        updated.name = "Updated".to_string();
        registry.register(updated);
        assert_eq!(registry.get("a.plugin").unwrap().name, "Updated");
        assert_eq!(registry.len(), 2);
    }
}
