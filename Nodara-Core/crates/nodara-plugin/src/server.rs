//! The plugin side of the protocol.
//!
//! An official plugin is nothing more than a [`CapabilityRegistry`] served over
//! JSON-RPC. `nodara-platform` and `nodara-vision` both ship binaries built on
//! [`serve_stdio`], which proves the plugin boundary is real rather than a
//! re-labelling of in-process calls.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Arc;

use nodara_core::{
    AllowAllPolicy, ArtifactStore, AutoApprove, CapabilityRegistry, EventBus, EventSink, NodeError,
    NullAuditLog, RunControl,
};
use nodara_schema::{ExecutionEvent, NodeDescriptor};
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::error::{PluginError, PluginResult};
use crate::jsonrpc::{
    self, codes, ErrorObject, Incoming, Notification, Request, Response, JSONRPC_VERSION,
};
use crate::protocol::{
    methods, CancelParams, DescribeParams, DescribeResult, ExecuteParams, ExecuteResult,
    InitializeParams, InitializeResult, PluginInfo, PROTOCOL_VERSION,
};
use crate::transport::NotificationSink;

/// Identity a plugin advertises during the handshake.
#[derive(Debug, Clone)]
pub struct PluginServerInfo {
    /// Manifest id.
    pub id: String,
    /// Human-facing name.
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// Capability identifiers the plugin advertises. When empty the server
    /// reports its node types instead, which is the correct default for a plugin
    /// that exposes capabilities one-to-one.
    pub capabilities: Vec<String>,
}

impl PluginServerInfo {
    /// Identity with no explicit capability list.
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            capabilities: Vec::new(),
        }
    }

    /// Builder-style capability list.
    #[must_use]
    pub fn with_capabilities<I, S>(mut self, capabilities: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.capabilities = capabilities.into_iter().map(Into::into).collect();
        self
    }
}

impl From<PluginServerInfo> for PluginInfo {
    fn from(info: PluginServerInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            version: info.version,
        }
    }
}

/// A registry served over the plugin protocol.
pub struct PluginServer {
    registry: Arc<CapabilityRegistry>,
    info: PluginInfo,
    capabilities: Vec<String>,
    sink: Arc<dyn NotificationSink>,
    runs: Mutex<HashMap<String, RunControl>>,
    shutting_down: std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for PluginServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginServer")
            .field("id", &self.info.id)
            .field("node_types", &self.registry.node_types())
            .finish()
    }
}

impl PluginServer {
    /// Wrap a registry for serving.
    pub fn new(
        registry: Arc<CapabilityRegistry>,
        info: PluginServerInfo,
        sink: Arc<dyn NotificationSink>,
    ) -> Arc<Self> {
        Arc::new(Self {
            registry,
            capabilities: info.capabilities.clone(),
            info: info.into(),
            sink,
            runs: Mutex::new(HashMap::new()),
            shutting_down: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// True once `shutdown` has been requested.
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Handle one request and produce its response.
    pub fn handle(&self, request: Request) -> Response {
        let id = request.id.clone();
        match self.dispatch(&request.method, request.params) {
            Ok(result) => Response::success(id, result),
            Err(error) => Response::failure(Some(id), error),
        }
    }

    /// Handle a notification. Notifications never produce a response.
    pub fn handle_notification(&self, notification: Notification) {
        match notification.method.as_str() {
            methods::CANCEL => {
                if let Ok(params) = serde_json::from_value::<CancelParams>(notification.params) {
                    if let Some(control) = self.runs.lock().get(&params.run_id) {
                        control.cancel();
                    }
                }
            }
            other => {
                tracing::debug!(method = other, "ignoring plugin notification");
            }
        }
    }

    fn dispatch(&self, method: &str, params: Value) -> Result<Value, ErrorObject> {
        match method {
            methods::INITIALIZE => self.initialize(params),
            methods::DESCRIBE => self.describe(params),
            methods::EXECUTE => self.execute(params),
            methods::CANCEL => self.cancel(params),
            methods::SHUTDOWN => {
                self.shutting_down
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(json!({}))
            }
            methods::HEALTH => Ok(json!({ "status": "ok", "plugin": self.info.id })),
            other => Err(ErrorObject::new(
                codes::METHOD_NOT_FOUND,
                format!("unknown method `{other}`"),
            )),
        }
    }

    fn initialize(&self, params: Value) -> Result<Value, ErrorObject> {
        let params: InitializeParams = serde_json::from_value(params)
            .map_err(|error| ErrorObject::new(codes::INVALID_PARAMS, error.to_string()))?;
        if nodara_schema::version::major_of(&params.protocol_version)
            != nodara_schema::version::major_of(PROTOCOL_VERSION)
        {
            return Err(ErrorObject::new(
                codes::INVALID_PARAMS,
                format!(
                    "runtime speaks protocol {} but this plugin requires {}",
                    params.protocol_version, PROTOCOL_VERSION
                ),
            ));
        }
        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            plugin: self.info.clone(),
            capabilities: if self.capabilities.is_empty() {
                self.registry.node_types()
            } else {
                self.capabilities.clone()
            },
        };
        serde_json::to_value(result).map_err(internal_error)
    }

    fn describe(&self, params: Value) -> Result<Value, ErrorObject> {
        let params: DescribeParams = if params.is_null() {
            DescribeParams::default()
        } else {
            serde_json::from_value(params)
                .map_err(|error| ErrorObject::new(codes::INVALID_PARAMS, error.to_string()))?
        };
        let nodes: Vec<NodeDescriptor> = if params.node_types.is_empty() {
            self.registry.descriptors()
        } else {
            params
                .node_types
                .iter()
                .filter_map(|node_type| self.registry.descriptor(node_type))
                .collect()
        };
        serde_json::to_value(DescribeResult { nodes }).map_err(internal_error)
    }

    fn execute(&self, params: Value) -> Result<Value, ErrorObject> {
        let params: ExecuteParams = serde_json::from_value(params)
            .map_err(|error| ErrorObject::new(codes::INVALID_PARAMS, error.to_string()))?;

        let Some(executor) = self.registry.get(&params.node_type) else {
            return Err(ErrorObject::new(
                codes::APP_UNSUPPORTED,
                format!("plugin does not provide `{}`", params.node_type),
            ));
        };

        let control = RunControl::new();
        self.runs
            .lock()
            .insert(params.run_id.clone(), control.clone());

        // Progress and log records produced by the executor are converted into
        // protocol notifications so the runtime sees them live.
        let bus = EventBus::new(
            params.run_id.clone(),
            Arc::new(NotificationEventSink {
                run_id: params.run_id.clone(),
                sink: self.sink.clone(),
            }),
        );
        let mut context = nodara_core::ExecutionContext::new(
            params.run_id.clone(),
            params.run_id.clone(),
            params.variables.clone(),
            std::collections::HashSet::new(),
            control.clone(),
            bus,
            Arc::new(AllowAllPolicy),
            Arc::new(AutoApprove),
            Arc::new(NullAuditLog),
            Arc::new(ArtifactStore::new()),
        );
        context.set_node(Some(params.node_id.clone()));

        let input = nodara_core::NodeInput {
            node_id: params.node_id.clone(),
            node_type: params.node_type.clone(),
            config: params.config.clone(),
            resolved_config: params.config.clone(),
            inputs: params.inputs.clone(),
            timeout_ms: params.timeout_ms,
        };

        let outcome = executor.execute(input, &mut context);
        self.runs.lock().remove(&params.run_id);

        match outcome {
            Ok(output) => {
                let result = ExecuteResult {
                    outputs: output.outputs,
                    variables: output.variables,
                };
                serde_json::to_value(result).map_err(internal_error)
            }
            Err(error) => Err(node_error_to_object(&error, &params.node_type)),
        }
    }

    fn cancel(&self, params: Value) -> Result<Value, ErrorObject> {
        let params: CancelParams = serde_json::from_value(params)
            .map_err(|error| ErrorObject::new(codes::INVALID_PARAMS, error.to_string()))?;
        if let Some(control) = self.runs.lock().get(&params.run_id) {
            control.cancel();
        }
        Ok(json!({}))
    }
}

fn internal_error(error: serde_json::Error) -> ErrorObject {
    ErrorObject::new(codes::INTERNAL_ERROR, error.to_string())
}

/// Translate an executor failure into a protocol error.
pub fn node_error_to_object(error: &NodeError, node_type: &str) -> ErrorObject {
    let code = match error {
        NodeError::InvalidConfig(_) => codes::APP_INVALID_CONFIG,
        NodeError::PermissionDenied { .. } => codes::APP_PERMISSION_DENIED,
        NodeError::Cancelled => codes::APP_CANCELLED,
        NodeError::Timeout => codes::APP_TIMEOUT,
        NodeError::Unsupported(_) => codes::APP_UNSUPPORTED,
        NodeError::Io(_) => codes::APP_IO,
        NodeError::Execution(_) => codes::APP_EXECUTION,
        _ => codes::APP_EXECUTION,
    };
    ErrorObject::new(code, error.to_string()).with_data(json!({ "node_type": node_type }))
}

/// Adapts runtime events into plugin protocol notifications.
struct NotificationEventSink {
    run_id: String,
    sink: Arc<dyn NotificationSink>,
}

impl EventSink for NotificationEventSink {
    fn emit(&self, envelope: nodara_schema::EventEnvelope) {
        let notification = match envelope.event {
            ExecutionEvent::NodeProgress {
                node_id,
                progress,
                message,
            } => Notification::new(
                methods::PROGRESS,
                json!({
                    "run_id": self.run_id,
                    "node_id": node_id,
                    "progress": progress,
                    "message": message,
                }),
            ),
            ExecutionEvent::Log {
                level,
                message,
                node_id,
            } => Notification::new(
                methods::LOG,
                json!({
                    "run_id": self.run_id,
                    "node_id": node_id,
                    "level": format!("{level:?}").to_lowercase(),
                    "message": message,
                }),
            ),
            _ => return,
        };
        self.sink.on_notification(notification);
    }
}

/// Serve a plugin over stdin/stdout until `shutdown` or EOF.
///
/// Requests are dispatched on their own threads so a long-running `execute` does
/// not block an incoming `cancel`.
pub fn serve_stdio(registry: Arc<CapabilityRegistry>, info: PluginServerInfo) -> PluginResult<()> {
    let stdout = Arc::new(Mutex::new(std::io::stdout()));
    let sink: Arc<dyn NotificationSink> = Arc::new(StdioNotificationSink {
        writer: stdout.clone(),
    });
    let server = PluginServer::new(registry, info, sink);
    let stdin = std::io::stdin();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                write_response(
                    &stdout,
                    &Response::failure(
                        None,
                        ErrorObject::new(codes::PARSE_ERROR, error.to_string()),
                    ),
                );
                continue;
            }
        };
        match jsonrpc::decode(value) {
            Ok(Incoming::Request(request)) => {
                let server = server.clone();
                let writer = stdout.clone();
                std::thread::spawn(move || {
                    let response = server.handle(request);
                    write_response(&writer, &response);
                });
            }
            Ok(Incoming::Notification(notification)) => server.handle_notification(notification),
            Ok(Incoming::Response(_)) => {
                tracing::debug!("ignoring unexpected response on the plugin channel");
            }
            Err(error) => write_response(&stdout, &Response::failure(None, error)),
        }
        if server.is_shutting_down() {
            break;
        }
    }

    Ok(())
}

fn write_response(writer: &Arc<Mutex<std::io::Stdout>>, response: &Response) {
    write_frame(writer, response);
}

fn write_frame(writer: &Arc<Mutex<std::io::Stdout>>, value: &impl serde::Serialize) {
    let Ok(line) = serde_json::to_string(value) else {
        return;
    };
    let mut handle = writer.lock();
    let _ = handle.write_all(line.as_bytes());
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// Writes plugin-originated notifications as JSON-RPC lines on stdout.
struct StdioNotificationSink {
    writer: Arc<Mutex<std::io::Stdout>>,
}

impl NotificationSink for StdioNotificationSink {
    fn on_notification(&self, notification: Notification) {
        write_frame(&self.writer, &notification);
    }
}

/// A transport that talks to a [`PluginServer`] in the same process.
///
/// Used by the CLI (`--in-process`) and by the test suite.
pub struct InProcessTransport {
    server: Arc<PluginServer>,
    next_id: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for InProcessTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcessTransport").finish()
    }
}

impl InProcessTransport {
    /// Wrap a server so it can be driven through the ordinary client.
    pub fn new(server: Arc<PluginServer>) -> Self {
        Self {
            server,
            next_id: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl crate::transport::JsonRpcTransport for InProcessTransport {
    fn request(
        &self,
        method: &str,
        params: Value,
        _timeout: std::time::Duration,
    ) -> PluginResult<Value> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let request = Request::new(json!(id), method, params);
        let response = self.server.handle(request);
        if let Some(error) = response.error {
            return Err(PluginError::Remote {
                code: error.code,
                message: error.message,
                data: error.data,
            });
        }
        Ok(response.result.unwrap_or(Value::Null))
    }

    fn notify(&self, method: &str, params: Value) -> PluginResult<()> {
        self.server
            .handle_notification(Notification::new(method, params));
        Ok(())
    }

    fn is_alive(&self) -> bool {
        true
    }

    fn close(&self) {}
}

/// Exposed for plugins that want to stringify a JSON-RPC version banner.
pub const fn jsonrpc_version() -> &'static str {
    JSONRPC_VERSION
}
