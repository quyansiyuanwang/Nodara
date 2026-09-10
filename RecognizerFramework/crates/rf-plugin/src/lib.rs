//! # rf-plugin
//!
//! The plugin protocol, discovery, transport and host for RecognizerFramework.
//!
//! A plugin is an ordinary process described by a `manifest.json`. Once launched
//! it speaks JSON-RPC 2.0 over stdio. Because the wire format is the only
//! contract, a plugin may be written in any language; because the manifest is
//! read before launch, the runtime knows what a plugin provides before running
//! any of its code.
//!
//! ## Shape of a session
//!
//! ```text
//! Runtime                          Plugin process
//!   |---- initialize ------------->|   version + permission handshake
//!   |<--- InitializeResult --------|
//!   |---- describe --------------->|   which node types, with what schemas
//!   |<--- DescribeResult ----------|
//!   |---- execute ---------------->|   run one node
//!   |<--- progress / log ----------|   optional, in any order
//!   |<--- ExecuteResult -----------|
//!   |---- cancel ----------------->|   best-effort interruption
//!   |---- shutdown --------------->|
//! ```
//!
//! The same crate is used by official plugins to *serve* capabilities
//! ([`server`]) and by the runtime to *host* them ([`host`], [`client`]).

pub mod client;
pub mod discovery;
pub mod error;
pub mod executor;
pub mod host;
pub mod jsonrpc;
pub mod protocol;
pub mod server;
pub mod transport;

pub use client::PluginClient;
pub use discovery::{discover_plugins, DiscoveredPlugin, DiscoveryError, DiscoveryOutcome};
pub use error::{PluginError, PluginResult};
pub use executor::PluginExecutor;
pub use host::{PluginHost, PluginSummary};
pub use jsonrpc::{codes, ErrorObject, Incoming, Notification, Request, Response, JSONRPC_VERSION};
pub use protocol::{
    methods, CancelParams, DescribeParams, DescribeResult, ExecuteParams, ExecuteResult,
    InitializeParams, InitializeResult, LogNotification, PluginInfo, ProgressNotification,
    PROTOCOL_VERSION,
};
pub use rf_schema::PluginManifest;
pub use server::{serve_stdio, InProcessTransport, PluginServer, PluginServerInfo};
pub use transport::{
    transport_error, JsonRpcTransport, NotificationSink, NullNotificationSink, StdioTransport,
};

/// Install a stderr-only tracing subscriber.
///
/// Plugins **must not** write logs to stdout, because stdout carries the JSON-RPC
/// frames. This helper makes the correct behaviour the easy one.
pub fn tracing_init() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}
