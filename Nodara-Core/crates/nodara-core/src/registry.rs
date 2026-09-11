//! Capability registry: node type -> executor.
//!
//! The registry is the single point where in-process executors and
//! out-of-process plugin executors become indistinguishable. It also implements
//! [`nodara_schema::NodeTypeIndex`], so capability-aware validation works against
//! exactly the set of node types that can actually run.

use std::collections::HashMap;
use std::sync::Arc;

use nodara_schema::{NodeDescriptor, NodeTypeIndex};

use crate::executor::NodeExecutor;

/// A registry of available node executors.
#[derive(Default)]
pub struct CapabilityRegistry {
    executors: HashMap<String, Arc<dyn NodeExecutor>>,
    descriptors: HashMap<String, NodeDescriptor>,
}

impl std::fmt::Debug for CapabilityRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityRegistry")
            .field("node_types", &self.node_types())
            .finish()
    }
}

impl CapabilityRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an executor by value.
    pub fn register<E>(&mut self, executor: E) -> &mut Self
    where
        E: NodeExecutor + 'static,
    {
        self.register_arc(Arc::new(executor))
    }

    /// Register a shared executor. Later registrations of the same node type
    /// replace earlier ones, which makes plugin overrides trivial.
    pub fn register_arc(&mut self, executor: Arc<dyn NodeExecutor>) -> &mut Self {
        let descriptor = executor.descriptor();
        self.descriptors
            .insert(descriptor.node_type.clone(), descriptor.clone());
        self.executors.insert(descriptor.node_type, executor);
        self
    }

    /// Register a pre-computed descriptor without an executor.
    ///
    /// Used when a plugin is discovered but not yet launched: the palette can
    /// still show its nodes while execution is unavailable.
    pub fn register_descriptor(&mut self, descriptor: NodeDescriptor) -> &mut Self {
        self.descriptors
            .insert(descriptor.node_type.clone(), descriptor);
        self
    }

    /// Look up an executor.
    pub fn get(&self, node_type: &str) -> Option<Arc<dyn NodeExecutor>> {
        self.executors.get(node_type).cloned()
    }

    /// Look up a descriptor.
    pub fn descriptor(&self, node_type: &str) -> Option<NodeDescriptor> {
        self.descriptors.get(node_type).cloned()
    }

    /// Every node type known to the registry, sorted for stable output.
    pub fn node_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self.descriptors.keys().cloned().collect();
        types.sort();
        types
    }

    /// Every descriptor, sorted by node type.
    pub fn descriptors(&self) -> Vec<NodeDescriptor> {
        let mut descriptors: Vec<NodeDescriptor> = self.descriptors.values().cloned().collect();
        descriptors.sort_by(|a, b| a.node_type.cmp(&b.node_type));
        descriptors
    }

    /// True when an executor is registered for `node_type`.
    pub fn can_execute(&self, node_type: &str) -> bool {
        self.executors.contains_key(node_type)
    }

    /// Number of known node types.
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    /// True when nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

impl NodeTypeIndex for CapabilityRegistry {
    fn node_types(&self) -> Vec<String> {
        self.node_types()
    }

    fn descriptor(&self, node_type: &str) -> Option<NodeDescriptor> {
        self.descriptor(node_type)
    }
}
