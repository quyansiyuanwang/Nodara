//! The runtime's plugin host.
//!
//! The host owns the set of installed plugins, routes their notifications into
//! the right run, and installs their executors into a [`CapabilityRegistry`] so
//! the engine cannot tell local from remote nodes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use nodara_core::{CapabilityRegistry, EventSink};
use nodara_schema::{EventEnvelope, ExecutionEvent, LogLevel, NodeDescriptor, PluginManifest};
use parking_lot::Mutex;

use crate::client::PluginClient;
use crate::discovery::DiscoveryOutcome;
use crate::error::{PluginError, PluginResult};
use crate::executor::PluginExecutor;
use crate::jsonrpc::Notification;
use crate::protocol::{methods, LogNotification, ProgressNotification};
use crate::transport::NotificationSink;

/// One installed plugin and its connection state.
struct InstalledPlugin {
    manifest: PluginManifest,
    directory: PathBuf,
    client: Option<Arc<PluginClient>>,
    descriptors: Vec<NodeDescriptor>,
}

/// A cheap, serialisable view of an installed plugin.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginSummary {
    /// Manifest id.
    pub id: String,
    /// Human-facing name.
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// Wire protocol version.
    pub protocol_version: String,
    /// Declared capabilities.
    pub capabilities: Vec<String>,
    /// Declared permissions.
    pub permissions: Vec<String>,
    /// Node types the plugin provides.
    pub node_types: Vec<String>,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the process is currently running and handshaken.
    pub loaded: bool,
}

/// Owns installed plugins and routes their notifications.
pub struct PluginHost {
    plugins: Mutex<Vec<InstalledPlugin>>,
    active_runs: Mutex<HashMap<String, RunRoute>>,
}

/// A destination for events belonging to one run.
struct RunRoute {
    run_id: String,
    sink: Arc<dyn EventSink>,
    seq: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for PluginHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginHost")
            .field("plugins", &self.summaries().len())
            .field("active_runs", &self.active_runs.lock().len())
            .finish()
    }
}

impl PluginHost {
    /// Create an empty host.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            plugins: Mutex::new(Vec::new()),
            active_runs: Mutex::new(HashMap::new()),
        })
    }

    /// Register manifests found on disk without launching anything.
    ///
    /// This is what lets a UI show an authoritative node palette even when a
    /// plugin is currently stopped.
    pub fn register_discovered(&self, outcome: &DiscoveryOutcome) {
        let mut plugins = self.plugins.lock();
        for discovered in &outcome.plugins {
            let already = plugins
                .iter()
                .any(|entry| entry.manifest.id == discovered.manifest.id);
            if already {
                continue;
            }
            plugins.push(InstalledPlugin {
                manifest: discovered.manifest.clone(),
                directory: discovered.directory.clone(),
                client: None,
                descriptors: Vec::new(),
            });
        }
    }

    /// Launch a plugin and complete its handshake.
    pub fn load(
        self: &Arc<Self>,
        manifest: PluginManifest,
        directory: PathBuf,
    ) -> PluginResult<()> {
        manifest
            .validate()
            .map_err(|errors| PluginError::Manifest(errors.join("; ")))?;
        let sink: Arc<dyn NotificationSink> = self.clone();
        let client = Arc::new(PluginClient::connect_with_sink(
            &manifest, &directory, sink,
        )?);
        self.install_client(manifest, directory, client);
        Ok(())
    }

    /// Attach an already-connected client without launching a process.
    ///
    /// Production code goes through [`Self::load`]. Tests (and any future
    /// in-process host) use this with [`PluginClient::from_transport`].
    pub fn install_client(
        &self,
        manifest: PluginManifest,
        directory: PathBuf,
        client: Arc<PluginClient>,
    ) {
        let descriptors = client.descriptors().to_vec();
        let mut plugins = self.plugins.lock();
        if let Some(existing) = plugins
            .iter_mut()
            .find(|entry| entry.manifest.id == manifest.id)
        {
            existing.client = Some(client);
            existing.descriptors = descriptors;
            existing.directory = directory;
            existing.manifest = manifest;
        } else {
            plugins.push(InstalledPlugin {
                manifest,
                directory,
                client: Some(client),
                descriptors,
            });
        }
    }

    /// Launch every plugin found during discovery.
    ///
    /// Failures are returned rather than raised, so one broken plugin cannot stop
    /// the runtime from starting.
    pub fn load_all(self: &Arc<Self>, outcome: &DiscoveryOutcome) -> Vec<(String, PluginError)> {
        let mut failures = Vec::new();
        for discovered in &outcome.plugins {
            if let Err(error) = self.load(discovered.manifest.clone(), discovered.directory.clone())
            {
                failures.push((discovered.manifest.id.clone(), error));
            }
        }
        failures
    }

    /// Install every known node type into a capability registry.
    ///
    /// Loaded plugins contribute runnable executors; unloaded plugins contribute
    /// descriptors only, which keeps `GET /node-types` and validation honest about
    /// what exists while `execute` still fails loudly for a stopped plugin.
    ///
    /// A node type that already has an executor — typically an in-process
    /// official capability registered before plugins are installed — is never
    /// replaced. `serve --in-process --plugin-dir plugins` would otherwise
    /// launch the same official plugins and swap the embedded executors for
    /// stdio RPC copies of themselves.
    pub fn install_into(&self, registry: &mut CapabilityRegistry) {
        let plugins = self.plugins.lock();
        for entry in plugins.iter() {
            match &entry.client {
                Some(client) => {
                    for descriptor in &entry.descriptors {
                        if registry.can_execute(&descriptor.node_type) {
                            continue;
                        }
                        registry.register_arc(Arc::new(PluginExecutor::new(
                            client.clone(),
                            descriptor.clone(),
                        )));
                    }
                }
                None => {
                    // A plugin that is discovered but not launched still
                    // contributes its palette entries — but it must never
                    // overwrite a node type an in-process host already
                    // registered, or the embedded capability would be replaced
                    // by a non-executable placeholder.
                    for node_type in &entry.manifest.node_types {
                        if registry.descriptor(node_type).is_some() {
                            continue;
                        }
                        registry.register_descriptor(NodeDescriptor::new(
                            node_type.clone(),
                            node_type.clone(),
                            "Plugin",
                        ));
                    }
                }
            }
        }
    }

    /// Summaries suitable for `GET /api/v1/plugins`.
    pub fn summaries(&self) -> Vec<PluginSummary> {
        self.plugins
            .lock()
            .iter()
            .map(|entry| PluginSummary {
                id: entry.manifest.id.clone(),
                name: entry.manifest.name.clone(),
                version: entry.manifest.version.clone(),
                protocol_version: entry.manifest.protocol_version.clone(),
                capabilities: entry.manifest.capabilities.clone(),
                permissions: entry.manifest.permissions.clone(),
                node_types: entry.manifest.node_types.clone(),
                description: entry.manifest.description.clone(),
                loaded: entry.client.is_some(),
            })
            .collect()
    }

    /// Every descriptor known across plugins.
    pub fn descriptors(&self) -> Vec<NodeDescriptor> {
        let plugins = self.plugins.lock();
        let mut descriptors: Vec<NodeDescriptor> = plugins
            .iter()
            .flat_map(|entry| entry.descriptors.clone())
            .collect();
        descriptors.sort_by_key(|a| a.node_type.clone());
        descriptors
    }

    /// Bind a run id to its event sink so notifications can be routed.
    pub fn register_run(&self, run_id: impl Into<String>, sink: Arc<dyn EventSink>) {
        let run_id = run_id.into();
        self.active_runs.lock().insert(
            run_id.clone(),
            RunRoute {
                run_id,
                sink,
                seq: std::sync::atomic::AtomicU64::new(0),
            },
        );
    }

    /// Stop routing events for a finished run.
    pub fn unregister_run(&self, run_id: &str) {
        self.active_runs.lock().remove(run_id);
    }

    /// Ask every plugin to abandon a run.
    ///
    /// The RPCs run outside the plugins lock: each call can block for up to a
    /// control timeout, and holding the mutex that long would stall every
    /// other host operation. The client handles are owned `Arc`s, so they stay
    /// valid after the lock is released.
    pub fn cancel_run(&self, run_id: &str) {
        let clients: Vec<_> = self
            .plugins
            .lock()
            .iter()
            .filter_map(|entry| entry.client.clone())
            .collect();
        for client in clients {
            let _ = client.cancel(crate::protocol::CancelParams {
                run_id: run_id.to_string(),
                node_id: None,
            });
        }
    }

    /// Gracefully shut every plugin down.
    pub fn shutdown(&self) {
        let clients: Vec<_> = {
            let mut plugins = self.plugins.lock();
            plugins
                .iter_mut()
                .filter_map(|entry| entry.client.take())
                .collect()
        };
        for client in clients {
            let _ = client.shutdown();
        }
    }

    /// Whether any plugin is currently launched.
    pub fn has_loaded_plugins(&self) -> bool {
        self.plugins
            .lock()
            .iter()
            .any(|entry| entry.client.is_some())
    }
}

impl NotificationSink for PluginHost {
    fn on_notification(&self, notification: Notification) {
        match notification.method.as_str() {
            methods::PROGRESS => {
                let Ok(progress) =
                    serde_json::from_value::<ProgressNotification>(notification.params)
                else {
                    return;
                };
                if let Some(route) = self.active_runs.lock().get(&progress.run_id) {
                    route.emit(ExecutionEvent::NodeProgress {
                        node_id: progress.node_id,
                        progress: progress.progress,
                        message: progress.message,
                    });
                }
            }
            methods::LOG => {
                let Ok(log) = serde_json::from_value::<LogNotification>(notification.params) else {
                    return;
                };
                if let Some(route) = self.active_runs.lock().get(&log.run_id) {
                    route.emit(ExecutionEvent::Log {
                        level: parse_level(&log.level),
                        message: log.message,
                        node_id: log.node_id,
                    });
                }
            }
            other => {
                tracing::debug!(method = other, "unrouted plugin notification");
            }
        }
    }
}

impl RunRoute {
    fn emit(&self, event: ExecutionEvent) {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.sink
            .emit(EventEnvelope::new(self.run_id.clone(), seq, event));
    }
}

fn parse_level(level: &str) -> LogLevel {
    match level.to_ascii_lowercase().as_str() {
        "debug" => LogLevel::Debug,
        "warn" | "warning" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    }
}
