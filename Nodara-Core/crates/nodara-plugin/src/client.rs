//! The runtime side of a plugin session.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use nodara_schema::PluginManifest;
use serde_json::Value;

use crate::error::{PluginError, PluginResult};
use crate::protocol::{
    methods, CancelParams, DescribeParams, DescribeResult, ExecuteParams, ExecuteResult,
    InitializeParams, InitializeResult, PluginInfo, PROTOCOL_VERSION,
};
use crate::transport::{JsonRpcTransport, NullNotificationSink, StdioTransport};

/// Deadline for control-plane calls (`initialize`, `describe`, `cancel`, ...).
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default deadline for a single `execute` call.
pub const EXECUTE_TIMEOUT: Duration = Duration::from_secs(300);

/// A connected, handshaken plugin.
pub struct PluginClient {
    manifest: PluginManifest,
    transport: Arc<dyn JsonRpcTransport>,
    info: PluginInfo,
    capabilities: Vec<String>,
    nodes: Vec<nodara_schema::NodeDescriptor>,
}

impl std::fmt::Debug for PluginClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginClient")
            .field("id", &self.info.id)
            .field("node_types", &self.nodes.len())
            .finish()
    }
}

impl PluginClient {
    /// Launch the plugin described by `manifest` and complete the handshake.
    pub fn connect(manifest: &PluginManifest, dir: &Path) -> PluginResult<Self> {
        Self::connect_with_sink(manifest, dir, Arc::new(NullNotificationSink))
    }

    /// Launch the plugin and route its notifications to `sink`.
    pub fn connect_with_sink(
        manifest: &PluginManifest,
        dir: &Path,
        sink: Arc<dyn crate::transport::NotificationSink>,
    ) -> PluginResult<Self> {
        let program = resolve_executable(manifest, dir);
        let transport = Arc::new(StdioTransport::launch(
            &manifest.id,
            &program,
            &manifest.args,
            sink,
        )?);
        Self::from_transport(manifest.clone(), transport)
    }

    /// Complete the handshake over an already-established transport.
    ///
    /// This is the seam that lets tests (and future remote plugins) provide a
    /// different transport without touching any other code.
    pub fn from_transport(
        manifest: PluginManifest,
        transport: Arc<dyn JsonRpcTransport>,
    ) -> PluginResult<Self> {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            granted_permissions: manifest.permissions.clone(),
        };
        let raw = transport.request(
            methods::INITIALIZE,
            serde_json::to_value(params)?,
            CONTROL_TIMEOUT,
        )?;
        let initialized: InitializeResult = serde_json::from_value(raw)?;

        if nodara_schema::version::major_of(&initialized.protocol_version)
            != nodara_schema::version::major_of(PROTOCOL_VERSION)
        {
            transport.close();
            return Err(PluginError::Handshake(format!(
                "plugin `{}` speaks protocol {} but the runtime requires {}",
                manifest.id, initialized.protocol_version, PROTOCOL_VERSION
            )));
        }

        let raw = transport.request(
            methods::DESCRIBE,
            serde_json::to_value(DescribeParams::default())?,
            CONTROL_TIMEOUT,
        )?;
        let described: DescribeResult = serde_json::from_value(raw)?;

        let client = Self {
            manifest,
            transport,
            info: initialized.plugin,
            capabilities: initialized.capabilities,
            nodes: described.nodes,
        };
        client.warn_about_undeclared_nodes();
        Ok(client)
    }

    /// Compare declared node types against described node types and log drift.
    fn warn_about_undeclared_nodes(&self) {
        use std::collections::HashSet;
        let described: HashSet<&str> = self
            .nodes
            .iter()
            .map(|descriptor| descriptor.node_type.as_str())
            .collect();
        for declared in &self.manifest.node_types {
            if !described.contains(declared.as_str()) {
                tracing::warn!(
                    plugin = %self.manifest.id,
                    node_type = %declared,
                    "manifest declares a node type the plugin did not describe"
                );
            }
        }
    }

    /// Plugin identity reported during the handshake.
    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    /// The manifest the plugin was launched from.
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Capabilities reported by the plugin.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// Descriptors captured during the handshake.
    pub fn descriptors(&self) -> &[nodara_schema::NodeDescriptor] {
        &self.nodes
    }

    /// Underlying transport, for liveness checks.
    pub fn transport(&self) -> &Arc<dyn JsonRpcTransport> {
        &self.transport
    }

    /// Re-query descriptors, optionally restricted to specific node types.
    pub fn describe(
        &self,
        node_types: Vec<String>,
    ) -> PluginResult<Vec<nodara_schema::NodeDescriptor>> {
        let raw = self.transport.request(
            methods::DESCRIBE,
            serde_json::to_value(DescribeParams { node_types })?,
            CONTROL_TIMEOUT,
        )?;
        let described: DescribeResult = serde_json::from_value(raw)?;
        Ok(described.nodes)
    }

    /// Execute one node.
    pub fn execute(&self, params: ExecuteParams) -> PluginResult<ExecuteResult> {
        let timeout = params
            .timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(EXECUTE_TIMEOUT);
        let raw =
            self.transport
                .request(methods::EXECUTE, serde_json::to_value(params)?, timeout)?;
        Ok(serde_json::from_value(raw)?)
    }

    /// Ask the plugin to abandon a run.
    pub fn cancel(&self, params: CancelParams) -> PluginResult<()> {
        self.transport.request(
            methods::CANCEL,
            serde_json::to_value(params)?,
            CONTROL_TIMEOUT,
        )?;
        Ok(())
    }

    /// Probe liveness.
    pub fn health(&self) -> PluginResult<bool> {
        match self
            .transport
            .request(methods::HEALTH, Value::Null, CONTROL_TIMEOUT)
        {
            Ok(value) => Ok(value
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| status == "ok")),
            Err(PluginError::Disconnected) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Ask the plugin to exit and release the transport.
    pub fn shutdown(&self) -> PluginResult<()> {
        let _ = self
            .transport
            .request(methods::SHUTDOWN, Value::Null, CONTROL_TIMEOUT);
        self.transport.close();
        Ok(())
    }
}

/// Locate a plugin executable.
///
/// The search order is deliberate and standard for side-by-side deployments:
///
/// 1. `<plugin dir>/<executable>` — an installed, self-contained plugin;
/// 2. `<dir of the running binary>/<executable>` — a `cargo build` output tree,
///    where every plugin binary lands next to the runtime;
/// 3. `<dir of the running binary>/plugins/<plugin id>/<executable>` — a staged
///    release layout.
///
/// When nothing matches, the first candidate is returned so the resulting error
/// message names the path the operator most likely expected.
pub fn resolve_executable(manifest: &PluginManifest, dir: &Path) -> std::path::PathBuf {
    let installed = manifest.executable_path(dir);
    if installed.exists() {
        return installed;
    }

    let name = std::path::Path::new(&manifest.executable);
    if let Ok(current) = std::env::current_exe() {
        if let Some(binary_dir) = current.parent() {
            let sibling = if name.is_absolute() {
                name.to_path_buf()
            } else {
                binary_dir.join(name)
            };
            if sibling.exists() {
                return sibling;
            }
            let staged = binary_dir.join("plugins").join(&manifest.id).join(name);
            if staged.exists() {
                return staged;
            }
        }
    }

    installed
}
