//! # nodara-runtime
//!
//! The headless runtime: the single process the Studio, the agent and the CLI all
//! talk to.
//!
//! It owns the capability registry, the plugin host, the execution engine, the
//! policy layer and the audit log, and exposes them through the versioned HTTP and
//! WebSocket API described in the architecture document.
//!
//! ```text
//! GET  /api/v1/health
//! GET  /api/v1/plugins
//! GET  /api/v1/extensions
//! GET  /api/v1/node-types
//! GET  /api/v1/schema/{document}
//! POST /api/v1/workflows/validate
//! POST /api/v1/runs
//! GET  /api/v1/runs
//! GET  /api/v1/runs/{id}
//! POST /api/v1/runs/{id}/pause
//! POST /api/v1/runs/{id}/resume
//! POST /api/v1/runs/{id}/step
//! POST /api/v1/runs/{id}/cancel
//! WS   /api/v1/runs/{id}/events
//! ```
//!
//! Nothing in this crate knows about the Studio or the agent; they are ordinary
//! API clients, exactly as the document requires.

pub mod api;
pub mod approval;
pub mod config;
pub mod error;
pub mod runs;
pub mod server;
pub mod sessions;
pub mod state;

pub use approval::SessionApprovalHandler;
pub use config::{PolicyMode, RuntimeConfig};
pub use error::{ApiError, RuntimeError, RuntimeResult};
pub use runs::{RunHandle, RunManager, RunSnapshot};
pub use server::serve;
pub use sessions::{AgentSessionStore, DEFAULT_APPROVAL_TIMEOUT};
pub use state::{RuntimeBuilder, RuntimeState};
