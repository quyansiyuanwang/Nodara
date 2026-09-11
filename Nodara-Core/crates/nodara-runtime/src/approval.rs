//! Approvals, routed through agent sessions.
//!
//! `nodara-core` decides *whether* approval is needed; this module decides *how* it
//! is obtained. The run thread blocks here until an operator answers through the
//! session API, so policy is enforced in the execution path rather than in a UI.

use std::sync::Arc;
use std::time::Duration;

use nodara_core::{ApprovalHandler, CapabilityRequest};
use nodara_schema::ApprovalDecision;

use crate::sessions::AgentSessionStore;

/// Consults the session store for an operator decision.
///
/// When no session owns the run there is nobody to ask. `default_allow` decides
/// what happens then, and it defaults to refusal: an unattended approval request
/// must never silently authorise a side effect.
#[derive(Debug)]
pub struct SessionApprovalHandler {
    store: Arc<AgentSessionStore>,
    timeout: Duration,
    default_allow: bool,
}

impl SessionApprovalHandler {
    /// Ask the operator, waiting at most `timeout` for an answer.
    pub fn new(store: Arc<AgentSessionStore>, timeout: Duration) -> Self {
        Self {
            store,
            timeout,
            default_allow: false,
        }
    }

    /// Choose what happens when the run belongs to no session.
    #[must_use]
    pub fn with_default(mut self, default_allow: bool) -> Self {
        self.default_allow = default_allow;
        self
    }
}

impl ApprovalHandler for SessionApprovalHandler {
    fn approve(&self, request: &CapabilityRequest) -> bool {
        match self.store.raise_and_wait(request, self.timeout) {
            Some(ApprovalDecision::Approved) => true,
            Some(ApprovalDecision::Denied) => false,
            None => self.default_allow,
        }
    }
}
