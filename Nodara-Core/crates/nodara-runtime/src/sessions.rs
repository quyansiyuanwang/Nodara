//! Agent sessions.
//!
//! The runtime stores sessions as *data*. It never calls a model, never builds a
//! prompt and never plans: the agent process produces all of that and publishes
//! it here, and the Studio reads it back. That is what lets the core stay free
//! of agent concerns while the editor still shows a conversation, a plan preview
//! and approval prompts.
//!
//! The same store is what makes approvals work. When policy asks for approval
//! and the operator has not pre-authorised the session, the run blocks here
//! until a decision arrives or the request times out.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use nodara_core::policy::CapabilityRequest;
use nodara_schema::{
    AgentSession, ApprovalDecision, ApprovalRequest, MessageRole, PlanPreview, SessionMessage,
    SessionStatus,
};

/// How long a run waits for an operator decision by default.
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug)]
struct Waiter {
    sender: SyncSender<ApprovalDecision>,
    receiver: Mutex<Option<Receiver<ApprovalDecision>>>,
}

#[derive(Debug)]
struct Inner {
    sessions: HashMap<String, AgentSession>,
    waiters: HashMap<String, Arc<Waiter>>,
}

/// An in-memory store of agent sessions and the approvals they are waiting on.
#[derive(Debug)]
pub struct AgentSessionStore {
    inner: Mutex<Inner>,
    approval_timeout: Duration,
}

impl AgentSessionStore {
    /// Create an empty store.
    pub fn new(approval_timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                sessions: HashMap::new(),
                waiters: HashMap::new(),
            }),
            approval_timeout,
        })
    }

    /// How long an approval request waits before being refused.
    pub fn approval_timeout(&self) -> Duration {
        self.approval_timeout
    }

    /// Create a session.
    pub fn create(&self, goal: impl Into<String>, provider: impl Into<String>) -> AgentSession {
        let id = uuid::Uuid::new_v4().to_string();
        let mut session = AgentSession::new(id.clone(), goal);
        session.provider = provider.into();
        session.messages.push(SessionMessage {
            seq: 0,
            at_ms: nodara_schema::event::now_ms(),
            role: MessageRole::Operator,
            text: session.goal.clone(),
        });
        self.inner.lock().sessions.insert(id, session.clone());
        session
    }

    /// Look up a session.
    pub fn get(&self, session_id: &str) -> Option<AgentSession> {
        self.inner.lock().sessions.get(session_id).cloned()
    }

    /// Store a session verbatim. Used by the agent when it owns the state.
    pub fn upsert(&self, session: AgentSession) -> AgentSession {
        let mut session = session;
        session.updated_at_ms = nodara_schema::event::now_ms();
        self.inner
            .lock()
            .sessions
            .insert(session.id.clone(), session.clone());
        session
    }

    /// Every session, newest first.
    pub fn list(&self) -> Vec<AgentSession> {
        let inner = self.inner.lock();
        let mut sessions: Vec<AgentSession> = inner.sessions.values().cloned().collect();
        sessions.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        sessions
    }

    /// Every approval still waiting for a decision, across all sessions.
    pub fn pending_approvals(&self) -> Vec<(String, ApprovalRequest)> {
        let inner = self.inner.lock();
        inner
            .sessions
            .values()
            .flat_map(|session| {
                session
                    .pending_approvals()
                    .map(|approval| (session.id.clone(), approval.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Append a conversation turn.
    pub fn append_message(
        &self,
        session_id: &str,
        role: MessageRole,
        text: impl Into<String>,
    ) -> Option<SessionMessage> {
        let mut inner = self.inner.lock();
        let session = inner.sessions.get_mut(session_id)?;
        let message = SessionMessage {
            seq: session.messages.len() as u64,
            at_ms: nodara_schema::event::now_ms(),
            role,
            text: text.into(),
        };
        session.messages.push(message.clone());
        session.updated_at_ms = nodara_schema::event::now_ms();
        Some(message)
    }

    /// Publish a plan preview and move the session to a state that reflects it.
    pub fn set_plan(&self, session_id: &str, preview: PlanPreview) -> Option<AgentSession> {
        let mut inner = self.inner.lock();
        let session = inner.sessions.get_mut(session_id)?;
        let needs_attention = session.needs_attention();
        session.plan = Some(preview.clone());
        session.status = if needs_attention {
            SessionStatus::AwaitingApproval
        } else if preview.valid {
            SessionStatus::Ready
        } else {
            SessionStatus::Failed
        };
        session.updated_at_ms = nodara_schema::event::now_ms();
        Some(session.clone())
    }

    /// Set the session status.
    pub fn set_status(&self, session_id: &str, status: SessionStatus) -> Option<AgentSession> {
        let mut inner = self.inner.lock();
        let session = inner.sessions.get_mut(session_id)?;
        session.status = status;
        session.updated_at_ms = nodara_schema::event::now_ms();
        Some(session.clone())
    }

    /// Attach the run started from this session.
    pub fn attach_run(&self, session_id: &str, run_id: impl Into<String>) -> Option<AgentSession> {
        let mut inner = self.inner.lock();
        let session = inner.sessions.get_mut(session_id)?;
        session.run_id = Some(run_id.into());
        session.status = SessionStatus::Running;
        session.updated_at_ms = nodara_schema::event::now_ms();
        Some(session.clone())
    }

    /// Record token consumption.
    pub fn record_tokens(&self, session_id: &str, tokens: u64) -> Option<AgentSession> {
        let mut inner = self.inner.lock();
        let session = inner.sessions.get_mut(session_id)?;
        session.tokens_used = session.tokens_used.saturating_add(tokens);
        session.updated_at_ms = nodara_schema::event::now_ms();
        Some(session.clone())
    }

    /// Find the session that owns `run_id`.
    pub fn session_for_run(&self, run_id: &str) -> Option<AgentSession> {
        self.inner
            .lock()
            .sessions
            .values()
            .find(|session| session.run_id.as_deref() == Some(run_id))
            .cloned()
    }

    /// Raise an approval request for a blocked capability and wait for an answer.
    ///
    /// Returns `None` when no session owns the run, which lets the caller apply
    /// its own default. Returns the decision otherwise, and `Denied` on timeout:
    /// an unanswered request must never silently authorise a side effect.
    pub fn raise_and_wait(
        &self,
        capability: &CapabilityRequest,
        timeout: Duration,
    ) -> Option<ApprovalDecision> {
        let (approval_id, waiter) = {
            let mut inner = self.inner.lock();
            let session = inner
                .sessions
                .values_mut()
                .find(|session| session.run_id.as_deref() == Some(capability.run_id.as_str()))?;

            let approval = ApprovalRequest {
                id: uuid::Uuid::new_v4().to_string(),
                requested_at_ms: nodara_schema::event::now_ms(),
                run_id: capability.run_id.clone(),
                node_id: capability.node_id.clone(),
                node_type: capability.node_type.clone(),
                capability: capability.capability.clone(),
                permissions: capability.permissions.clone(),
                reason: format!(
                    "`{}` requested `{}` with {:?}",
                    capability.node_id, capability.node_type, capability.permissions
                ),
                input: capability.input.clone(),
                decision: None,
                decided_at_ms: None,
                decided_by: None,
            };
            let approval_id = approval.id.clone();
            session.approvals.push(approval);
            session.status = SessionStatus::AwaitingApproval;
            session.updated_at_ms = nodara_schema::event::now_ms();

            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            let waiter = Arc::new(Waiter {
                sender,
                receiver: Mutex::new(Some(receiver)),
            });
            inner.waiters.insert(approval_id.clone(), waiter.clone());
            (approval_id, waiter)
        };

        let receiver = waiter
            .receiver
            .lock()
            .take()
            .expect("waiter receiver is taken exactly once");
        let decision = receiver
            .recv_timeout(timeout)
            .unwrap_or(ApprovalDecision::Denied);
        self.inner.lock().waiters.remove(&approval_id);
        Some(decision)
    }

    /// Record an operator decision and release the waiting run.
    pub fn decide(
        &self,
        session_id: &str,
        approval_id: &str,
        decision: ApprovalDecision,
        decided_by: &str,
    ) -> Option<AgentSession> {
        let (session, waiter) = {
            let mut guard = self.inner.lock();
            // Split the guard so the session and the waiter map can be borrowed
            // independently; `guard.sessions` and `guard.waiters` are disjoint.
            let Inner { sessions, waiters } = &mut *guard;
            let session = sessions.get_mut(session_id)?;
            let approval = session
                .approvals
                .iter_mut()
                .find(|approval| approval.id == approval_id)?;
            approval.decision = Some(decision);
            approval.decided_at_ms = Some(nodara_schema::event::now_ms());
            approval.decided_by = Some(decided_by.to_string());
            session.updated_at_ms = nodara_schema::event::now_ms();
            if !session.needs_attention() && session.status == SessionStatus::AwaitingApproval {
                session.status = SessionStatus::Running;
            }
            let waiter = waiters.remove(approval_id);
            (session.clone(), waiter)
        };
        if let Some(waiter) = waiter {
            // A disconnected receiver means the run already timed out; that is
            // not an error, the decision is still recorded above.
            let _ = waiter.sender.try_send(decision);
        }
        Some(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodara_core::policy::CapabilityRequest;

    fn capability(run_id: &str) -> CapabilityRequest {
        CapabilityRequest {
            run_id: run_id.to_string(),
            node_id: "keyboard".to_string(),
            node_type: "windows.Input.Keyboard".to_string(),
            capability: "windows.Input.Keyboard".to_string(),
            permissions: vec!["input.control".to_string()],
            dangerous: true,
            input: serde_json::json!({ "keys": "a" }),
        }
    }

    #[test]
    fn create_append_and_attach() {
        let store = AgentSessionStore::new(Duration::from_millis(50));
        let session = store.create("do a thing", "mock");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, MessageRole::Operator);

        store.append_message(&session.id, MessageRole::Agent, "working on it");
        store.attach_run(&session.id, "run-1");

        let session = store.get(&session.id).unwrap();
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.run_id.as_deref(), Some("run-1"));
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(store.session_for_run("run-1").unwrap().id, session.id);
    }

    #[test]
    fn approvals_block_then_release() {
        let store = AgentSessionStore::new(Duration::from_secs(5));
        let session = store.create("keyboard thing", "mock");
        store.attach_run(&session.id, "run-1");

        let store_for_thread = store.clone();
        let decision = std::thread::spawn(move || {
            store_for_thread.raise_and_wait(&capability("run-1"), Duration::from_secs(5))
        });

        // Wait for the request to appear, then approve it.
        let approval_id = loop {
            if let Some((_, approval)) = store.pending_approvals().into_iter().next() {
                break approval.id;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            store.get(&session.id).unwrap().status,
            SessionStatus::AwaitingApproval
        );

        store.decide(
            &session.id,
            &approval_id,
            ApprovalDecision::Approved,
            "operator",
        );
        assert_eq!(decision.join().unwrap(), Some(ApprovalDecision::Approved));
        assert!(store.pending_approvals().is_empty());
    }

    #[test]
    fn an_unanswered_request_is_denied() {
        let store = AgentSessionStore::new(Duration::from_millis(30));
        let session = store.create("thing", "mock");
        store.attach_run(&session.id, "run-2");
        let decision = store.raise_and_wait(&capability("run-2"), Duration::from_millis(30));
        assert_eq!(decision, Some(ApprovalDecision::Denied));
    }

    #[test]
    fn a_run_without_a_session_is_not_raised() {
        let store = AgentSessionStore::new(Duration::from_millis(30));
        assert_eq!(
            store.raise_and_wait(&capability("unknown-run"), Duration::from_millis(30)),
            None
        );
    }

    #[test]
    fn plan_preview_drives_the_status() {
        let store = AgentSessionStore::new(Duration::from_millis(30));
        let session = store.create("thing", "mock");
        let mut workflow = nodara_schema::Workflow::new("wf");
        workflow.add_node(nodara_schema::Node::new("start", "core.Start"));
        let preview = PlanPreview {
            at_ms: 0,
            workflow,
            valid: true,
            errors: 0,
            warnings: 0,
            diagnostics: Vec::new(),
        };
        store.set_plan(&session.id, preview);
        assert_eq!(store.get(&session.id).unwrap().status, SessionStatus::Ready);
    }
}
