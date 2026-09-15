//! Unified registration metadata for runtime capabilities.
//!
//! Node execution stays in [`crate::CapabilityRegistry`]. This registry answers
//! the management question above it: which built-in, in-process or plugin
//! extension provided a capability, is it loaded, and what does it expose?

use std::collections::BTreeMap;

use nodara_schema::{ExtensionDescriptor, ExtensionKind};

/// Descriptor for the extension that ships with the runtime.
pub fn builtin_extension(node_types: Vec<String>) -> ExtensionDescriptor {
    ExtensionDescriptor {
        id: "nodara.builtins".to_string(),
        name: "Nodara built-ins".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: ExtensionKind::Builtin,
        source: "runtime".to_string(),
        description: Some("Core workflow control, logging, calculation and variables.".to_string()),
        capabilities: Vec::new(),
        permissions: Vec::new(),
        node_types,
        loaded: true,
    }
}

/// Descriptor for capabilities registered directly by an embedding host.
pub fn in_process_extension(node_types: Vec<String>) -> ExtensionDescriptor {
    ExtensionDescriptor {
        id: "nodara.in-process".to_string(),
        name: "In-process extensions".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: ExtensionKind::InProcess,
        source: "host".to_string(),
        description: Some("Capabilities registered directly by an embedding host.".to_string()),
        capabilities: Vec::new(),
        permissions: Vec::new(),
        node_types,
        loaded: true,
    }
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
