//! # nodara-core
//!
//! The execution engine and the Rust SDK for Nodara-Core.
//!
//! The crate is deliberately split into small, independently testable pieces:
//!
//! | module       | responsibility                                          |
//! |--------------|---------------------------------------------------------|
//! | [`executor`] | the [`NodeExecutor`] trait every capability implements  |
//! | [`registry`] | node type -> executor, doubling as a schema [`NodeTypeIndex`] |
//! | [`engine`]   | deterministic, pausable graph execution                 |
//! | [`control`]  | pause / resume / step / cancel signalling               |
//! | [`policy`]   | capability authorisation decisions                      |
//! | [`audit`]    | append-only audit records                               |
//! | [`events`]   | sequenced execution event bus                           |
//! | [`builtin`]  | the `core.*` and `system.*` node types                  |
//!
//! Everything that performs a side effect is expressed as a [`NodeExecutor`],
//! whether it lives in-process (see [`builtin`]) or in another process (see the
//! `nodara-plugin` crate). The engine cannot tell the difference, which is what makes
//! the plugin boundary real rather than cosmetic.

pub mod audit;
pub mod builtin;
pub mod context;
pub mod control;
pub mod engine;
pub mod error;
pub mod events;
pub mod executor;
pub mod expr;
pub mod extension;
pub mod policy;
pub mod registry;

pub use audit::{
    AuditCategory, AuditLog, AuditRecord, InMemoryAuditLog, JsonlAuditLog, NullAuditLog,
};
pub use builtin::register_builtins;
pub use context::{ArtifactMeta, ArtifactStore, ExecutionContext};
pub use control::RunControl;
pub use engine::{EngineOptions, RunFailure, RunOutcome, RunRequest, RunningRun, WorkflowEngine};
pub use error::{ExecutionError, ExecutionResult, NodeError, NodeResult};
pub use events::{ChannelEventSink, CollectingEventSink, EventBus, EventSink, NullEventSink};
pub use executor::{NodeExecutor, NodeInput, NodeOutput};
pub use expr::evaluate_expression;
pub use extension::{ExtensionDescriptor, ExtensionKind, ExtensionRegistry};
pub use policy::{
    AllowAllPolicy, AllowlistPolicy, ApprovalHandler, AutoApprove, AutoDeny, CapabilityPolicy,
    CapabilityRequest, Decision, DefaultPolicy, DenyAllPolicy, PolicyChain,
};
pub use registry::CapabilityRegistry;
