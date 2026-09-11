//! # nodara-testkit
//!
//! Test doubles and builders shared by every crate in the workspace.
//!
//! Two problems recur when testing a plugin architecture: building graphs by
//! hand, and standing up a plugin process. This crate solves both — a fluent
//! workflow builder and an in-process plugin harness that drives the *real*
//! protocol code.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use nodara_core::{
    CapabilityRegistry, ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput,
    NodeResult,
};
use nodara_plugin::{
    InProcessTransport, NullNotificationSink, PluginClient, PluginServer, PluginServerInfo,
};
use nodara_schema::{
    Edge, Node, NodeDescriptor, NodeTypeIndex, PluginManifest, PortDescriptor, PortKind, ValueType,
    Workflow, SCHEMA_VERSION,
};

/// Fluent builder for workflow documents.
#[derive(Debug, Clone)]
pub struct WorkflowBuilder {
    workflow: Workflow,
}

impl WorkflowBuilder {
    /// Start a builder with a `core.Start` node called `start`.
    pub fn new(id: impl Into<String>) -> Self {
        let mut workflow = Workflow::new(id);
        workflow.add_node(Node::new("start", "core.Start"));
        Self { workflow }
    }

    /// Add a node with a JSON configuration.
    #[must_use]
    pub fn node(mut self, id: &str, node_type: &str, config: serde_json::Value) -> Self {
        self.workflow
            .add_node(Node::new(id, node_type).with_config(config));
        self
    }

    /// Add a node with no configuration.
    #[must_use]
    pub fn bare_node(mut self, id: &str, node_type: &str) -> Self {
        self.workflow.add_node(Node::new(id, node_type));
        self
    }

    /// Connect two nodes with a generated edge id.
    #[must_use]
    pub fn edge(mut self, source: &str, target: &str) -> Self {
        let id = format!("e{}-{}", source, target);
        self.workflow.add_edge(Edge::new(id, source, target));
        self
    }

    /// Connect two nodes with a guard expression.
    #[must_use]
    pub fn guarded_edge(mut self, source: &str, target: &str, condition: &str) -> Self {
        let id = format!("e{}-{}-guard", source, target);
        let mut edge = Edge::new(id, source, target);
        edge.condition = Some(condition.to_string());
        self.workflow.add_edge(edge);
        self
    }

    /// Terminate the graph with a `core.End` node.
    #[must_use]
    pub fn end(mut self) -> Self {
        self.workflow.add_node(Node::new("end", "core.End"));
        self
    }

    /// Declare a workflow-scoped variable.
    #[must_use]
    pub fn variable(mut self, name: &str, value: serde_json::Value) -> Self {
        self.workflow.variables.insert(
            name.to_string(),
            nodara_schema::Variable {
                value,
                ..nodara_schema::Variable::default()
            },
        );
        self
    }

    /// Finish.
    pub fn build(self) -> Workflow {
        self.workflow
    }
}

/// A `core.Log`-shaped executor that records every input it receives.
#[derive(Debug, Default)]
pub struct RecordingExecutor {
    node_type: String,
    calls: Mutex<Vec<NodeInput>>,
    descriptor_override: Option<NodeDescriptor>,
}

impl RecordingExecutor {
    /// A recording executor for `node_type`, described with an open config schema.
    pub fn new(node_type: impl Into<String>) -> Self {
        Self {
            node_type: node_type.into(),
            calls: Mutex::new(Vec::new()),
            descriptor_override: None,
        }
    }

    /// Replace the emitted descriptor.
    #[must_use]
    pub fn with_descriptor(mut self, descriptor: NodeDescriptor) -> Self {
        self.descriptor_override = Some(descriptor);
        self
    }

    /// Every input this executor has seen.
    pub fn calls(&self) -> Vec<NodeInput> {
        self.calls.lock().clone()
    }

    /// How many times the executor ran.
    pub fn call_count(&self) -> usize {
        self.calls.lock().len()
    }
}

impl NodeExecutor for RecordingExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        self.descriptor_override
            .clone()
            .unwrap_or_else(|| NodeDescriptor {
                node_type: self.node_type.clone(),
                display_name: self.node_type.clone(),
                category: "Test".to_string(),
                description: String::new(),
                version: "0.0.0".to_string(),
                inputs: vec![PortDescriptor::new(
                    "in",
                    "In",
                    PortKind::Input,
                    ValueType::Any,
                )],
                outputs: vec![PortDescriptor::new(
                    "out",
                    "Out",
                    PortKind::Output,
                    ValueType::Any,
                )],
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
            })
    }

    fn execute(&self, input: NodeInput, _context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        self.calls.lock().push(input.clone());
        Ok(NodeOutput::new()
            .with_output("out", serde_json::json!({ "node": input.node_id }))
            .with_variable(format!("ran_{}", input.node_id), serde_json::json!(true)))
    }
}

/// An executor that always fails, for exercising error paths.
#[derive(Debug, Default)]
pub struct FailingExecutor {
    node_type: String,
    error: Option<NodeError>,
}

impl FailingExecutor {
    /// Fail with `error` whenever it runs.
    pub fn new(node_type: impl Into<String>, error: NodeError) -> Self {
        Self {
            node_type: node_type.into(),
            error: Some(error),
        }
    }
}

impl NodeExecutor for FailingExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            node_type: self.node_type.clone(),
            display_name: self.node_type.clone(),
            category: "Test".to_string(),
            ..NodeDescriptor::new(self.node_type.clone(), self.node_type.clone(), "Test")
        }
    }

    fn execute(
        &self,
        _input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> NodeResult<NodeOutput> {
        Err(self
            .error
            .clone()
            .unwrap_or(NodeError::Execution("boom".into())))
    }
}

/// A registry pre-populated with the built-in node types.
pub fn builtin_registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    nodara_core::register_builtins(&mut registry);
    registry
}

/// A manifest suitable for driving the plugin harness.
pub fn test_manifest(id: &str, node_types: &[&str]) -> PluginManifest {
    PluginManifest {
        id: id.to_string(),
        name: id.to_string(),
        version: "0.0.0".to_string(),
        protocol_version: nodara_schema::PROTOCOL_VERSION.to_string(),
        executable: "unused".to_string(),
        args: Vec::new(),
        capabilities: node_types.iter().map(|t| (*t).to_string()).collect(),
        permissions: Vec::new(),
        node_types: node_types.iter().map(|t| (*t).to_string()).collect(),
        description: None,
        author: None,
        homepage: None,
        metadata: BTreeMap::new(),
    }
}

/// A plugin served in-process, whose client is ready to be installed.
#[derive(Debug)]
pub struct InProcessPlugin {
    /// Connected client.
    pub client: Arc<PluginClient>,
    /// Manifest describing the plugin.
    pub manifest: PluginManifest,
}

/// Serve `registry` as a plugin and hand back a connected client.
pub fn in_process_plugin(id: &str, registry: CapabilityRegistry) -> InProcessPlugin {
    let owned: Vec<String> = registry.node_types();
    let manifest = test_manifest(id, &owned.iter().map(String::as_str).collect::<Vec<_>>());
    let server = PluginServer::new(
        Arc::new(registry),
        PluginServerInfo::new(id, id, "0.0.0"),
        Arc::new(NullNotificationSink),
    );
    let transport = Arc::new(InProcessTransport::new(server));
    let client = Arc::new(
        PluginClient::from_transport(manifest.clone(), transport)
            .expect("in-process handshake succeeds"),
    );
    InProcessPlugin { client, manifest }
}

/// Install every node type from `plugin` into `registry`.
pub fn install_plugin(registry: &mut CapabilityRegistry, plugin: &InProcessPlugin) {
    for descriptor in plugin.client.descriptors() {
        registry.register_arc(Arc::new(nodara_plugin::PluginExecutor::new(
            plugin.client.clone(),
            descriptor.clone(),
        )));
    }
}

/// A capability index backed by an explicit descriptor list.
#[derive(Debug, Default, Clone)]
pub struct StaticNodeIndex {
    descriptors: Vec<NodeDescriptor>,
}

impl StaticNodeIndex {
    /// Build an index from descriptors.
    pub fn new(descriptors: Vec<NodeDescriptor>) -> Self {
        Self { descriptors }
    }
}

impl NodeTypeIndex for StaticNodeIndex {
    fn node_types(&self) -> Vec<String> {
        self.descriptors
            .iter()
            .map(|descriptor| descriptor.node_type.clone())
            .collect()
    }

    fn descriptor(&self, node_type: &str) -> Option<NodeDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.node_type == node_type)
            .cloned()
    }
}

/// The schema version new test workflows should use.
pub fn schema_version() -> &'static str {
    SCHEMA_VERSION
}
