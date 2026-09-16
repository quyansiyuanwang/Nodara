//! # nodara-agent
//!
//! The autonomous layer: it turns a natural-language goal into a workflow,
//! repairs it until the runtime accepts it, and then operates it under budgets
//! and policy.
//!
//! ## Dependency rule
//!
//! This crate depends on `nodara-schema` — the *published contract* — and on nothing
//! else from the core repository. It has no access to the engine, the registry or
//! the plugin host. Everything it does to a runtime goes through
//! [`runtime_client::RuntimeClient`], exactly like the Studio. That is what makes
//! "the agent is just another client" true rather than aspirational, and it is
//! why the permission layer cannot be bypassed by the agent: it has no other
//! route in.

pub mod agent;
pub mod audit;
pub mod error;
pub mod model;
pub mod planner;
pub mod policy;
pub mod prompt;
pub mod provider;
pub mod report;
pub mod runtime_client;
pub mod selector;

pub use agent::{Agent, AgentConfig, AgentOutcome, ExplainTarget, RunApprovalMode};
pub use audit::{AuditTrace, TraceEntry, TraceStep};
pub use error::{AgentError, AgentResult};
pub use model::{ChatImage, ChatMessage, ChatRequest, ChatResponse, Role, TokenUsage};
pub use planner::{PlanEvent, PlanObserver, PlanRequest, Planner};
pub use policy::{Budget, GuardrailPolicy, ToolPolicy};
pub use prompt::system_prompt;
pub use provider::{LlmProvider, MockProvider, OpenAiProvider};
pub use report::ExecutionReport;
pub use runtime_client::RuntimeClient;
pub use selector::ToolSelector;
