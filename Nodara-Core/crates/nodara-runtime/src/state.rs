//! Runtime composition root.

use std::sync::Arc;

use nodara_core::{
    register_builtins, AllowAllPolicy, AllowlistPolicy, ApprovalHandler, AutoApprove,
    CapabilityPolicy, CapabilityRegistry, DefaultPolicy, InMemoryAuditLog, JsonlAuditLog,
    NodeExecutor, WorkflowEngine,
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
    policy: Option<Arc<dyn CapabilityPolicy>>,
    approval: Option<Arc<dyn ApprovalHandler>>,
}

impl std::fmt::Debug for RuntimeBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeBuilder")
            .field("config", &self.config)
            .field("registry", &self.registry)
            .finish()
    }
}

impl RuntimeBuilder {
    /// Start a builder with the built-in `core.*` and `system.*` node types.
    pub fn new(config: RuntimeConfig) -> Self {
        let mut registry = CapabilityRegistry::new();
        register_builtins(&mut registry);
        Self {
            config,
            registry,
            policy: None,
            approval: None,
        }
    }

    /// Register one extra in-process node.
    pub fn register_executor<E: NodeExecutor + 'static>(&mut self, executor: E) -> &mut Self {
        self.registry.register(executor);
        self
    }

    /// Register a whole capability set with one call.
    pub fn register_set<F>(&mut self, register: F) -> &mut Self
    where
        F: FnOnce(&mut CapabilityRegistry),
    {
        register(&mut self.registry);
        self
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
        let engine = WorkflowEngine::new(registry.clone())
            .with_policy(policy)
            .with_approval(approval)
            .with_audit(audit.clone());
        let runs = RunManager::new(engine.clone());

        Ok(Arc::new(RuntimeState {
            config: self.config,
            registry,
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
