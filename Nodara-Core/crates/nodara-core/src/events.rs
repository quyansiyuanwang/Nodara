//! Sequenced execution event bus.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use nodara_schema::{EventEnvelope, ExecutionEvent};
use parking_lot::Mutex;

/// Destination for execution events.
pub trait EventSink: Send + Sync {
    /// Publish one envelope.
    fn emit(&self, envelope: EventEnvelope);
}

/// Discards every event.
#[derive(Debug, Default)]
pub struct NullEventSink;

impl EventSink for NullEventSink {
    fn emit(&self, _envelope: EventEnvelope) {}
}

/// Forwards events into a bounded channel.
#[derive(Debug)]
pub struct ChannelEventSink {
    sender: crossbeam_channel::Sender<EventEnvelope>,
}

impl ChannelEventSink {
    /// Create a sink and the matching receiver.
    pub fn new(capacity: usize) -> (Self, crossbeam_channel::Receiver<EventEnvelope>) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        (Self { sender }, receiver)
    }
}

impl EventSink for ChannelEventSink {
    fn emit(&self, envelope: EventEnvelope) {
        // A disconnected subscriber must never abort a run.
        let _ = self.sender.try_send(envelope);
    }
}

/// Retains every event in memory, for tests and replay.
#[derive(Debug, Default)]
pub struct CollectingEventSink {
    events: Mutex<Vec<EventEnvelope>>,
}

impl CollectingEventSink {
    /// Create an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the events collected so far.
    pub fn snapshot(&self) -> Vec<EventEnvelope> {
        self.events.lock().clone()
    }

    /// Number of events collected.
    pub fn len(&self) -> usize {
        self.events.lock().len()
    }

    /// True when no events have been collected.
    pub fn is_empty(&self) -> bool {
        self.events.lock().is_empty()
    }
}

impl EventSink for CollectingEventSink {
    fn emit(&self, envelope: EventEnvelope) {
        self.events.lock().push(envelope);
    }
}

/// Assigns monotonic sequence numbers to events for one run.
pub struct EventBus {
    run_id: String,
    seq: AtomicU64,
    sink: Arc<dyn EventSink>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("run_id", &self.run_id)
            .field("emitted", &self.emitted())
            .finish()
    }
}

impl EventBus {
    /// Create a bus for `run_id` feeding `sink`.
    pub fn new(run_id: impl Into<String>, sink: Arc<dyn EventSink>) -> Arc<Self> {
        Arc::new(Self {
            run_id: run_id.into(),
            seq: AtomicU64::new(0),
            sink,
        })
    }

    /// The run this bus belongs to.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Wrap, sequence and publish an event.
    pub fn emit(&self, event: ExecutionEvent) -> EventEnvelope {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let envelope = EventEnvelope::new(self.run_id.clone(), seq, event);
        self.sink.emit(envelope.clone());
        envelope
    }

    /// Number of events emitted so far.
    pub fn emitted(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodara_schema::ExecutionEvent;

    #[test]
    fn sequence_numbers_are_monotonic() {
        let sink = Arc::new(CollectingEventSink::new());
        let bus = EventBus::new("run-1", sink.clone());
        bus.emit(ExecutionEvent::RunStarted {
            workflow_id: "wf".into(),
        });
        bus.emit(ExecutionEvent::RunPaused);
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[0].run_id, "run-1");
    }
}
