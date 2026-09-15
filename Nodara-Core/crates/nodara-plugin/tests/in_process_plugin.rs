//! Proves the plugin boundary end to end without spawning a process.
//!
//! The same `PluginServer` used by the official stdio binaries is driven through
//! an [`InProcessTransport`], wrapped by a [`PluginClient`], exposed as a
//! [`nodara_core::NodeExecutor`], and finally executed by the real engine.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nodara_core::{
    register_builtins, ArtifactStore, CapabilityRegistry, EngineOptions, ExecutionContext,
    NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult, RunControl, RunRequest,
    WorkflowEngine,
};
use nodara_plugin::{
    InProcessTransport, PluginClient, PluginError, PluginExecutor, PluginHost, PluginServer,
    PluginServerInfo,
};
use nodara_schema::{
    Edge, LogLevel, Node, NodeDescriptor, PluginManifest, PortDescriptor, PortKind, RunStatus,
    ValueType, Workflow,
};
use serde_json::{json, Value};

/// A node that echoes its configuration and publishes a variable.
#[derive(Debug, Default)]
struct EchoExecutor;

/// Same type as [`EchoExecutor`], with a descriptor that is easy to tell apart
/// from the plugin copy. Used to prove `install_into` does not clobber
/// in-process executors.
#[derive(Debug, Default)]
struct InProcessEcho;

impl NodeExecutor for EchoExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
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
                ValueType::String,
            )],
            config_schema: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"],
                "additionalProperties": false
            }),
            allows_additional_config: false,
            ..NodeDescriptor::new("test.Echo", "Echo", "Test")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let text = input.require_str("text")?;
        context.log(LogLevel::Info, format!("echoing {text}"));
        context.progress(Some(1.0), Some("done".to_string()));
        Ok(NodeOutput::new()
            .with_output("out", json!(text))
            .with_variable("echoed", json!(text)))
    }
}

/// A node that produces a binary artifact through the plugin protocol.
#[derive(Debug, Default)]
struct ArtifactExecutor;

impl NodeExecutor for ArtifactExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor::new("test.Artifact", "Artifact", "Test")
    }

    fn execute(&self, _input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let meta = context
            .artifacts()
            .put("preview", "image/png", vec![1, 2, 3, 4]);
        let value = json!(meta);
        context.set_variable("artifact", value.clone());
        Ok(NodeOutput::new().with_output("artifact", value))
    }
}

impl NodeExecutor for InProcessEcho {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor::new("test.Echo", "In-process Echo", "Test").with_description("in-process")
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        EchoExecutor.execute(input, context)
    }
}

fn manifest() -> PluginManifest {
    serde_json::from_value(json!({
        "id": "nodara.test.echo",
        "name": "Test Echo",
        "version": "1.0.0",
        "protocol_version": "1",
        "executable": "unused.exe",
        "capabilities": ["Test.Echo"],
        "permissions": [],
        "node_types": ["test.Echo"]
    }))
    .expect("manifest")
}

/// Build a client wired to an in-process server exposing `EchoExecutor`.
fn client() -> PluginClient {
    // A plugin process exposes only the node types it provides.
    let mut server_registry = CapabilityRegistry::new();
    server_registry.register(EchoExecutor);

    let server = PluginServer::new(
        Arc::new(server_registry),
        PluginServerInfo::new("nodara.test.echo", "Test Echo", "1.0.0")
            .with_capabilities(["Test.Echo"]),
        Arc::new(nodara_plugin::NullNotificationSink),
    );
    let transport = Arc::new(InProcessTransport::new(server));
    PluginClient::from_transport(manifest(), transport).expect("handshake")
}

fn registry_with_plugin(client: Arc<PluginClient>) -> Arc<CapabilityRegistry> {
    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    for descriptor in client.descriptors() {
        registry.register_arc(Arc::new(PluginExecutor::new(
            client.clone(),
            descriptor.clone(),
        )));
    }
    Arc::new(registry)
}

fn workflow_with(config: Value) -> Workflow {
    let mut workflow = Workflow::new("wf.plugin");
    workflow.variables.insert(
        "name".to_string(),
        nodara_schema::Variable {
            value: json!("world"),
            ..nodara_schema::Variable::default()
        },
    );
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("echo", "test.Echo").with_config(config));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "echo"));
    workflow.add_edge(Edge::new("e2", "echo", "end"));
    workflow
}

#[test]
fn handshake_reports_descriptors_and_capabilities() {
    let client = client();
    assert_eq!(client.info().id, "nodara.test.echo");
    assert!(client.capabilities().contains(&"Test.Echo".to_string()));
    let descriptors = client.descriptors();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].node_type, "test.Echo");
    assert!(client.health().unwrap());
}

#[test]
fn engine_executes_a_plugin_node() {
    let client = Arc::new(client());
    let engine = WorkflowEngine::new(registry_with_plugin(client));
    let outcome = engine.run(
        RunRequest::new(workflow_with(json!({ "text": "hello {{name}}" }))),
        &RunControl::new(),
    );

    assert_eq!(
        outcome.status,
        RunStatus::Completed,
        "{:?}",
        outcome.failure
    );
    assert_eq!(outcome.nodes_executed, 3);
    assert_eq!(outcome.variables.get("echoed"), Some(&json!("hello world")));
}

#[test]
fn artifacts_cross_the_plugin_boundary_and_reach_the_shared_store() {
    let mut server_registry = CapabilityRegistry::new();
    server_registry.register(ArtifactExecutor);
    let manifest: PluginManifest = serde_json::from_value(json!({
        "id": "nodara.test.artifact",
        "name": "Test Artifact",
        "version": "1.0.0",
        "protocol_version": "1",
        "executable": "unused.exe",
        "node_types": ["test.Artifact"]
    }))
    .unwrap();
    let server = PluginServer::new(
        Arc::new(server_registry),
        PluginServerInfo::new("nodara.test.artifact", "Test Artifact", "1.0.0"),
        Arc::new(nodara_plugin::NullNotificationSink),
    );
    let client = Arc::new(
        PluginClient::from_transport(manifest, Arc::new(InProcessTransport::new(server))).unwrap(),
    );
    let registry = {
        let mut registry = CapabilityRegistry::new();
        register_builtins(&mut registry);
        registry.register_arc(Arc::new(PluginExecutor::new(
            client,
            NodeDescriptor::new("test.Artifact", "Artifact", "Test"),
        )));
        Arc::new(registry)
    };
    let artifacts = Arc::new(ArtifactStore::new());
    let mut workflow = Workflow::new("wf.artifact-transfer");
    workflow.add_node(Node::new("start", "core.Start"));
    workflow.add_node(Node::new("artifact", "test.Artifact"));
    workflow.add_node(Node::new("end", "core.End"));
    workflow.add_edge(Edge::new("e1", "start", "artifact"));
    workflow.add_edge(Edge::new("e2", "artifact", "end"));

    let outcome = WorkflowEngine::new(registry).run(
        RunRequest::new(workflow).with_artifacts(artifacts.clone()),
        &RunControl::new(),
    );

    assert!(outcome.is_success(), "{:?}", outcome.failure);
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts.get("preview"), None);
    let meta = artifacts.list().pop().expect("artifact metadata");
    assert_eq!(meta.content_type, "image/png");
    assert_eq!(artifacts.get(&meta.id), Some(vec![1, 2, 3, 4]));
    assert_eq!(outcome.variables.get("artifact"), Some(&json!(meta)));
}

#[test]
fn plugin_errors_are_mapped_back_to_node_errors() {
    let client = Arc::new(client());
    let engine = WorkflowEngine::new(registry_with_plugin(client)).with_options(EngineOptions {
        validate: false,
        ..EngineOptions::default()
    });

    let outcome = engine.run(
        RunRequest::new(workflow_with(json!({}))),
        &RunControl::new(),
    );

    assert_eq!(outcome.status, RunStatus::Failed);
    assert_eq!(outcome.failure.unwrap().code, "E_INVALID_CONFIG");
}

#[test]
fn handshake_rejects_an_incompatible_protocol() {
    /// A transport that answers `initialize` with a future protocol major.
    struct BadVersionTransport;

    impl nodara_plugin::JsonRpcTransport for BadVersionTransport {
        fn request(
            &self,
            method: &str,
            _params: Value,
            _timeout: Duration,
        ) -> Result<Value, PluginError> {
            if method == nodara_plugin::methods::INITIALIZE {
                Ok(json!({
                    "protocol_version": "9.0",
                    "plugin": { "id": "x", "name": "x", "version": "1" },
                    "capabilities": []
                }))
            } else {
                Ok(json!({}))
            }
        }

        fn notify(&self, _method: &str, _params: Value) -> Result<(), PluginError> {
            Ok(())
        }

        fn is_alive(&self) -> bool {
            true
        }

        fn close(&self) {}
    }

    let error = PluginClient::from_transport(manifest(), Arc::new(BadVersionTransport))
        .expect_err("protocol mismatch must be rejected");
    assert!(matches!(error, PluginError::Handshake(_)));
}

#[test]
fn host_registers_descriptors_for_unloaded_plugins() {
    use nodara_plugin::{DiscoveredPlugin, DiscoveryOutcome};

    let host = PluginHost::new();
    host.register_discovered(&DiscoveryOutcome {
        plugins: vec![DiscoveredPlugin {
            manifest: manifest(),
            directory: std::path::PathBuf::from("."),
        }],
        errors: Vec::new(),
    });

    assert!(!host.has_loaded_plugins());
    let summaries = host.summaries();
    assert_eq!(summaries.len(), 1);
    assert!(!summaries[0].loaded);

    let mut registry = CapabilityRegistry::new();
    host.install_into(&mut registry);
    assert!(registry.node_types().contains(&"test.Echo".to_string()));
    // Descriptor-only registration must not pretend to be executable.
    assert!(!registry.can_execute("test.Echo"));
}

#[test]
fn loaded_plugin_does_not_replace_an_in_process_executor() {
    let host = PluginHost::new();
    host.install_client(
        manifest(),
        std::path::PathBuf::from("."),
        Arc::new(client()),
    );
    assert!(host.has_loaded_plugins());

    let mut registry = CapabilityRegistry::new();
    registry.register(InProcessEcho);
    host.install_into(&mut registry);

    let descriptor = registry.descriptor("test.Echo").unwrap();
    assert_eq!(descriptor.display_name, "In-process Echo");
    assert_eq!(descriptor.description, "in-process");
    assert!(registry.can_execute("test.Echo"));
}

#[test]
fn loaded_plugin_still_registers_types_the_registry_cannot_yet_execute() {
    let host = PluginHost::new();
    host.install_client(
        manifest(),
        std::path::PathBuf::from("."),
        Arc::new(client()),
    );

    let mut registry = CapabilityRegistry::new();
    register_builtins(&mut registry);
    host.install_into(&mut registry);
    assert!(registry.can_execute("test.Echo"));
}

#[test]
fn executor_is_reachable_through_the_registry() {
    let client = Arc::new(client());
    let registry = registry_with_plugin(client);
    assert!(registry.can_execute("test.Echo"));
    let descriptor = registry.descriptor("test.Echo").unwrap();
    assert!(descriptor
        .config_schema
        .get("required")
        .and_then(|v| v.as_array())
        .is_some_and(|required| required.contains(&json!("text"))));

    // The registry satisfies the schema crate's validation seam.
    use nodara_schema::NodeTypeIndex;
    let index: &dyn NodeTypeIndex = registry.as_ref();
    assert_eq!(index.node_types().len(), registry.node_types().len());

    let error: NodeError = NodeError::InvalidConfig("x".to_string());
    assert_eq!(error.code(), "E_INVALID_CONFIG");

    let _: BTreeMap<String, Value> = BTreeMap::new();
}
