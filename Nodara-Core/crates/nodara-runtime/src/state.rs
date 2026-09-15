//! Runtime composition root.

use std::sync::Arc;

use nodara_core::{
    builtin_extension, in_process_extension, register_builtins, AllowAllPolicy, AllowlistPolicy,
    ApprovalHandler, AutoApprove, CapabilityPolicy, CapabilityRegistry, DefaultPolicy,
    ExtensionDescriptor, ExtensionRegistry, InMemoryAuditLog, JsonlAuditLog, NodeExecutor,
    WorkflowEngine,
};
use nodara_plugin::PluginHost;

use crate::approval::SessionApprovalHandler;
use crate::config::{PolicyMode, RuntimeConfig};
use crate::error::RuntimeResult;
use crate::runs::RunManager;
use crate::sessions::AgentSessionStore;

/// Everything a request handler needs.
pub struct RuntimeState {
    /// Configuration the runtime was started with.
    pub config: RuntimeConfig,
    /// All runnable node types.
    pub registry: Arc<CapabilityRegistry>,
    /// Unified built-in, in-process and plugin registration metadata.
    pub extensions: Arc<ExtensionRegistry>,
    /// Installed plugins.
    pub host: Arc<PluginHost>,
    /// Active and completed runs.
    pub runs: Arc<RunManager>,
    /// Agent sessions and the approvals they are waiting on.
    pub sessions: Arc<AgentSessionStore>,
    /// The audit log, exposed so operators can read what the runtime allowed.
    pub audit: Arc<dyn nodara_core::AuditLog>,
    /// The engine, shared with the run manager.
    pub engine: WorkflowEngine,
    /// Plugin loading failures, surfaced through `GET /api/v1/plugins`.
    pub plugin_failures: Vec<PluginFailure>,
}

impl std::fmt::Debug for RuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeState")
            .field("node_types", &self.registry.node_types().len())
            .field("extensions", &self.extensions.len())
            .field("plugins", &self.host.summaries().len())
            .finish()
    }
}

/// A plugin that was discovered but could not be started.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginFailure {
    /// Manifest id.
    pub id: String,
    /// Why it failed to load.
    pub message: String,
}

/// Builds a [`RuntimeState`], keeping the runtime free of any dependency on the
/// official capability crates.
pub struct RuntimeBuilder {
    config: RuntimeConfig,
    registry: CapabilityRegistry,
    extensions: ExtensionRegistry,
    policy: Option<Arc<dyn CapabilityPolicy>>,
    approval: Option<Arc<dyn ApprovalHandler>>,
}

impl std::fmt::Debug for RuntimeBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeBuilder")
            .field("config", &self.config)
            .field("registry", &self.registry)
            .field("extensions", &self.extensions.descriptors())
            .finish()
    }
}

impl RuntimeBuilder {
    /// Start a builder with the built-in `core.*` and `system.*` node types.
    pub fn new(config: RuntimeConfig) -> Self {
        let mut registry = CapabilityRegistry::new();
        register_builtins(&mut registry);
        let mut extensions = ExtensionRegistry::new();
        extensions.register(builtin_extension(registry.node_types()));
        Self {
            config,
            registry,
            extensions,
            policy: None,
            approval: None,
        }
    }

    /// Register one extra in-process node.
    pub fn register_executor<E: NodeExecutor + 'static>(&mut self, executor: E) -> &mut Self {
        let node_type = executor.descriptor().node_type;
        self.registry.register(executor);
        self.add_in_process_node(node_type);
        self
    }

    /// Register a whole capability set with one call.
    pub fn register_set<F>(&mut self, register: F) -> &mut Self
    where
        F: FnOnce(&mut CapabilityRegistry),
    {
        let before = self.registry.node_types();
        register(&mut self.registry);
        for node_type in self.registry.node_types() {
            if !before.contains(&node_type) {
                self.add_in_process_node(node_type);
            }
        }
        self
    }

    /// Register arbitrary extension metadata for an embedding host.
    pub fn register_extension(&mut self, descriptor: ExtensionDescriptor) -> &mut Self {
        self.extensions.register(descriptor);
        self
    }

    fn add_in_process_node(&mut self, node_type: String) {
        let mut descriptor = self
            .extensions
            .get("nodara.in-process")
            .cloned()
            .unwrap_or_else(|| in_process_extension(Vec::new()));
        if !descriptor.node_types.contains(&node_type) {
            descriptor.node_types.push(node_type);
            descriptor.node_types.sort();
        }
        descriptor.loaded = true;
        self.extensions.register(descriptor);
    }

    /// Override the capability policy.
    #[must_use]
    pub fn with_policy(mut self, policy: Arc<dyn CapabilityPolicy>) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Override the approval handler.
    #[must_use]
    pub fn with_approval(mut self, approval: Arc<dyn ApprovalHandler>) -> Self {
        self.approval = Some(approval);
        self
    }

    /// Finish construction: discover plugins, assemble the engine and start the
    /// run manager.
    pub fn build(mut self) -> RuntimeResult<Arc<RuntimeState>> {
        let host = PluginHost::new();
        let mut plugin_failures = Vec::new();

        let discovered = nodara_plugin::discover_plugins(&self.config.plugin_dirs);
        for error in &discovered.errors {
            plugin_failures.push(PluginFailure {
                id: error.directory.display().to_string(),
                message: error.message.clone(),
            });
        }
        host.register_discovered(&discovered);
        if self.config.autoload_plugins {
            for (id, error) in host.load_all(&discovered) {
                plugin_failures.push(PluginFailure {
                    id,
                    message: error.to_string(),
                });
            }
        }
        host.install_into(&mut self.registry);
        for descriptor in host.extension_descriptors() {
            self.extensions.register(descriptor);
        }

        let sessions = AgentSessionStore::new(self.config.approval_timeout);
        let policy = self
            .policy
            .unwrap_or_else(|| default_policy(&self.config.policy));
        let approval = self
            .approval
            .unwrap_or_else(|| default_approval(&self.config, sessions.clone()));
        let audit: Arc<dyn nodara_core::AuditLog> = match &self.config.audit_path {
            Some(path) => Arc::new(JsonlAuditLog::open(path)?),
            None => Arc::new(InMemoryAuditLog::new()),
        };

        let registry = Arc::new(self.registry);
        let extensions = Arc::new(self.extensions);
        let engine = WorkflowEngine::new(registry.clone())
            .with_policy(policy)
            .with_approval(approval)
            .with_audit(audit.clone());
        let runs = RunManager::new(engine.clone());

        Ok(Arc::new(RuntimeState {
            config: self.config,
            registry,
            extensions,
            host,
            runs,
            sessions,
            audit,
            engine,
            plugin_failures,
        }))
    }
}

fn default_policy(mode: &PolicyMode) -> Arc<dyn CapabilityPolicy> {
    match mode {
        PolicyMode::AllowAll => Arc::new(AllowAllPolicy),
        PolicyMode::Allowlist(capabilities) => Arc::new(AllowlistPolicy::new(capabilities.clone())),
        PolicyMode::Default => Arc::new(DefaultPolicy),
    }
}

/// Choose the approval strategy.
///
/// With `auto_approve` the runtime consents on the operator's behalf, which is
/// the documented "phase one" behaviour for unattended runs. Otherwise every
/// request is raised against the owning agent session and the run blocks until
/// an operator answers. A run that belongs to no session is refused, because an
/// unanswered approval request must never silently authorise a side effect.
fn default_approval(
    config: &RuntimeConfig,
    sessions: Arc<AgentSessionStore>,
) -> Arc<dyn ApprovalHandler> {
    if config.auto_approve {
        Arc::new(AutoApprove)
    } else {
        Arc::new(SessionApprovalHandler::new(
            sessions,
            config.approval_timeout,
        ))
    }
}
