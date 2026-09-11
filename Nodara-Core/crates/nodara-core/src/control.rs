//! Pause / resume / step / cancel signalling.
//!
//! The engine checks in with [`RunControl::await_permission`] at every node
//! boundary. That gives an operator a deterministic suspension point without
//! requiring nodes to be interruptible.

use std::sync::Arc;

use parking_lot::{Condvar, Mutex};

use crate::error::NodeError;

#[derive(Debug, Default)]
struct Flags {
    paused: bool,
    cancelled: bool,
    finished: bool,
    step_budget: u32,
}

#[derive(Debug)]
struct Inner {
    flags: Mutex<Flags>,
    signal: Condvar,
}

/// A cheap, cloneable handle used to steer a running workflow.
#[derive(Debug, Clone)]
pub struct RunControl {
    inner: Arc<Inner>,
}

impl Default for RunControl {
    fn default() -> Self {
        Self::new()
    }
}

impl RunControl {
    /// Create a control handle for a run that starts unpaused.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                flags: Mutex::new(Flags::default()),
                signal: Condvar::new(),
            }),
        }
    }

    /// Suspend at the next node boundary.
    pub fn pause(&self) {
        let mut flags = self.inner.flags.lock();
        flags.paused = true;
    }

    /// Resume free execution.
    pub fn resume(&self) {
        let mut flags = self.inner.flags.lock();
        flags.paused = false;
        flags.step_budget = 0;
        self.inner.signal.notify_all();
    }

    /// Allow exactly one more node to run while remaining paused.
    pub fn step(&self) {
        let mut flags = self.inner.flags.lock();
        flags.paused = true;
        flags.step_budget = flags.step_budget.saturating_add(1);
        self.inner.signal.notify_all();
    }

    /// Stop the run at the next opportunity.
    pub fn cancel(&self) {
        let mut flags = self.inner.flags.lock();
        flags.cancelled = true;
        flags.paused = false;
        self.inner.signal.notify_all();
    }

    /// Mark the run as finished so any waiter is released.
    pub fn finish(&self) {
        let mut flags = self.inner.flags.lock();
        flags.finished = true;
        flags.paused = false;
        self.inner.signal.notify_all();
    }

    /// True when the run is currently suspended.
    pub fn is_paused(&self) -> bool {
        self.inner.flags.lock().paused
    }

    /// True when cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.inner.flags.lock().cancelled
    }

    /// True when the engine has signalled completion.
    pub fn is_finished(&self) -> bool {
        self.inner.flags.lock().finished
    }

    /// Block until the engine may execute the next node.
    ///
    /// Returns [`NodeError::Cancelled`] once cancellation has been requested.
    pub fn await_permission(&self) -> Result<(), NodeError> {
        let mut flags = self.inner.flags.lock();
        loop {
            if flags.cancelled {
                return Err(NodeError::Cancelled);
            }
            if !flags.paused {
                return Ok(());
            }
            if flags.step_budget > 0 {
                flags.step_budget -= 1;
                return Ok(());
            }
            if flags.finished {
                return Ok(());
            }
            self.inner.signal.wait(&mut flags);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpaused_control_allows_immediately() {
        let control = RunControl::new();
        assert!(control.await_permission().is_ok());
    }

    #[test]
    fn cancelled_control_refuses() {
        let control = RunControl::new();
        control.cancel();
        assert_eq!(control.await_permission(), Err(NodeError::Cancelled));
    }

    #[test]
    fn step_grants_exactly_one_node() {
        let control = RunControl::new();
        control.pause();
        control.step();
        assert!(control.await_permission().is_ok());
        // Second call would block; verify by resuming from another thread.
        let cloned = control.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            cloned.cancel();
        });
        assert_eq!(control.await_permission(), Err(NodeError::Cancelled));
        handle.join().unwrap();
    }

    #[test]
    fn resume_releases_a_paused_waiter() {
        let control = RunControl::new();
        control.pause();
        let cloned = control.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            cloned.resume();
        });
        assert!(control.await_permission().is_ok());
        handle.join().unwrap();
    }
}
