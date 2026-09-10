//! Transports that carry JSON-RPC frames to and from a plugin.
//!
//! The runtime only ever sees [`JsonRpcTransport`]. Stdio is the first
//! implementation; swapping in an HTTP or remote transport later requires no
//! change anywhere above this trait.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::error::{PluginError, PluginResult};
use crate::jsonrpc::{self, ErrorObject, Incoming, Notification, Request, Response};

/// Receives plugin-originated notifications (`progress`, `log`, ...).
pub trait NotificationSink: Send + Sync {
    /// Handle one notification.
    fn on_notification(&self, notification: Notification);
}

/// Drops every notification.
#[derive(Debug, Default)]
pub struct NullNotificationSink;

impl NotificationSink for NullNotificationSink {
    fn on_notification(&self, _notification: Notification) {}
}

/// A bidirectional JSON-RPC channel to a plugin.
pub trait JsonRpcTransport: Send + Sync {
    /// Send a request and wait for its response.
    fn request(&self, method: &str, params: Value, timeout: Duration) -> PluginResult<Value>;

    /// Send a notification. Never waits for a reply.
    fn notify(&self, method: &str, params: Value) -> PluginResult<()>;

    /// True while the underlying channel is usable.
    fn is_alive(&self) -> bool;

    /// Release the channel.
    fn close(&self);
}

/// A JSON-RPC transport over a child process' standard streams.
pub struct StdioTransport {
    child: Mutex<Option<Child>>,
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, crossbeam_channel::Sender<Response>>>>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
}

impl std::fmt::Debug for StdioTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioTransport")
            .field("alive", &self.is_alive())
            .finish()
    }
}

impl StdioTransport {
    /// Launch `program` and start pumping its stdout.
    pub fn launch(
        id: &str,
        program: &Path,
        args: &[String],
        sink: Arc<dyn NotificationSink>,
    ) -> PluginResult<Self> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|source| PluginError::Launch {
            id: id.to_string(),
            source,
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| PluginError::Protocol("plugin process has no stdin pipe".to_string()))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            PluginError::Protocol("plugin process has no stdout pipe".to_string())
        })?;

        let plugin_id = id.to_string();
        let pending: Arc<Mutex<HashMap<u64, crossbeam_channel::Sender<Response>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));

        {
            let pending = pending.clone();
            let alive = alive.clone();
            std::thread::Builder::new()
                .name(format!("rf-plugin-{plugin_id}"))
                .spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        let Ok(line) = line else { break };
                        if line.trim().is_empty() {
                            continue;
                        }
                        let value: Value = match serde_json::from_str(&line) {
                            Ok(value) => value,
                            Err(error) => {
                                tracing::warn!(
                                    plugin = %plugin_id,
                                    %error,
                                    "undecodable plugin frame"
                                );
                                continue;
                            }
                        };
                        match jsonrpc::decode(value) {
                            Ok(Incoming::Response(response)) => {
                                let id_value = response.id.as_ref().and_then(Value::as_u64);
                                if let Some(id_value) = id_value {
                                    if let Some(sender) = pending.lock().remove(&id_value) {
                                        let _ = sender.send(response);
                                    }
                                }
                            }
                            Ok(Incoming::Notification(notification)) => {
                                sink.on_notification(notification);
                            }
                            Ok(Incoming::Request(request)) => {
                                tracing::warn!(
                                    plugin = %plugin_id,
                                    method = %request.method,
                                    "plugin sent a request; plugins are servers, not clients"
                                );
                            }
                            Err(error) => {
                                tracing::warn!(
                                    plugin = %plugin_id,
                                    message = %error.message,
                                    "invalid plugin frame"
                                );
                            }
                        }
                    }
                    alive.store(false, Ordering::SeqCst);
                    // Wake any waiter still holding a channel.
                    pending.lock().clear();
                })
                .map_err(|source| PluginError::Launch {
                    id: id.to_string(),
                    source,
                })?;
        }

        Ok(Self {
            child: Mutex::new(Some(child)),
            stdin: Mutex::new(stdin),
            pending,
            next_id: AtomicU64::new(0),
            alive,
        })
    }

    fn write_frame(&self, value: &Value) -> PluginResult<()> {
        let line = serde_json::to_string(value)?;
        let mut stdin = self.stdin.lock();
        stdin.write_all(line.as_bytes())?;
        stdin.write_all(b"\n")?;
        stdin.flush()?;
        Ok(())
    }
}

impl JsonRpcTransport for StdioTransport {
    fn request(&self, method: &str, params: Value, timeout: Duration) -> PluginResult<Value> {
        if !self.is_alive() {
            return Err(PluginError::Disconnected);
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (sender, receiver) = crossbeam_channel::bounded(1);
        self.pending.lock().insert(id, sender);
        let request = Request::new(json!(id), method, params);
        if let Err(error) = self.write_frame(&serde_json::to_value(request)?) {
            self.pending.lock().remove(&id);
            return Err(error);
        }
        match receiver.recv_timeout(timeout) {
            Ok(response) => {
                if let Some(error) = response.error {
                    Err(PluginError::Remote {
                        code: error.code,
                        message: error.message,
                        data: error.data,
                    })
                } else {
                    Ok(response.result.unwrap_or(Value::Null))
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                self.pending.lock().remove(&id);
                Err(PluginError::Timeout {
                    method: method.to_string(),
                    timeout_ms: timeout.as_millis() as u64,
                })
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                Err(PluginError::Disconnected)
            }
        }
    }

    fn notify(&self, method: &str, params: Value) -> PluginResult<()> {
        if !self.is_alive() {
            return Err(PluginError::Disconnected);
        }
        let notification = Notification::new(method, params);
        self.write_frame(&serde_json::to_value(notification)?)
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn close(&self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Some(mut child) = self.child.lock().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Some(child) = self.child.get_mut().as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Build an error object from a transport failure, for server-side replies.
pub fn transport_error(method: &str, error: &PluginError) -> ErrorObject {
    match error {
        PluginError::Timeout { timeout_ms, .. } => ErrorObject::new(
            jsonrpc::codes::APP_TIMEOUT,
            format!("`{method}` timed out after {timeout_ms}ms"),
        ),
        PluginError::Disconnected => ErrorObject::new(
            jsonrpc::codes::INTERNAL_ERROR,
            "channel closed before a reply arrived",
        ),
        PluginError::Remote {
            code,
            message,
            data,
        } => ErrorObject {
            code: *code,
            message: message.clone(),
            data: data.clone(),
        },
        other => ErrorObject::new(jsonrpc::codes::INTERNAL_ERROR, other.to_string()),
    }
}
