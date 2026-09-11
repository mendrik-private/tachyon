//! One owned reflow per editor, including across document switches. Failure
//! suppresses the same request rather than starting a new worker every frame.
use crate::DocumentSessionId;
use document_core::NodeId;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

// Recovery policy, not a frame-time budget. Ordinary cold preparation has
// substantially more room than scrolling's 16.67ms presentation budget.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

/// One ready recovery value per worker. The UI only takes an already prepared
/// value; it never waits on rendering or starts another worker on timeout.
pub(super) struct Recovery<T>(std::sync::Mutex<Option<T>>);

impl<T> Default for Recovery<T> {
    fn default() -> Self {
        Self(std::sync::Mutex::new(None))
    }
}

impl<T> Recovery<T> {
    pub fn store(&self, value: T) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(value);
        }
    }

    pub fn take(&self) -> Option<T> {
        self.0.try_lock().ok()?.take()
    }
}

#[derive(Clone)]
pub(super) struct Deadline {
    at: Option<Instant>,
    cancelled: Arc<AtomicBool>,
    #[cfg(test)]
    checks_left: Option<Arc<std::sync::atomic::AtomicUsize>>,
    #[cfg(any(test, feature = "layout-validation"))]
    pub fail_planner: bool,
}

impl Deadline {
    pub fn new() -> Self {
        Self {
            at: Some(Instant::now() + TIMEOUT),
            cancelled: Arc::default(),
            #[cfg(test)]
            checks_left: None,
            #[cfg(any(test, feature = "layout-validation"))]
            fail_planner: false,
        }
    }

    #[cfg(test)]
    pub fn unlimited() -> Self {
        Self {
            at: None,
            cancelled: Arc::default(),
            checks_left: None,
            fail_planner: false,
        }
    }

    #[cfg(test)]
    pub fn after_checks(count: usize) -> Self {
        Self {
            checks_left: Some(Arc::new(std::sync::atomic::AtomicUsize::new(count))),
            ..Self::unlimited()
        }
    }

    pub fn cancel(&self) {
        // This flag publishes no data; it only requests a monotonic stop.
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn remaining(&self) -> Duration {
        self.at
            .map_or(TIMEOUT, |at| at.saturating_duration_since(Instant::now()))
    }

    pub fn check(&self) -> Result<(), Failed> {
        #[cfg(test)]
        if self.checks_left.as_ref().is_some_and(|left| {
            left.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                left.checked_sub(1)
            })
            .is_err()
        }) {
            return Err(Failed::TimedOut);
        }
        if self.cancelled.load(Ordering::Relaxed) || self.at.is_some_and(|at| Instant::now() >= at)
        {
            Err(Failed::TimedOut)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key {
    pub session: DocumentSessionId,
    pub document: u64,
    pub geometry: u64,
    pub width: u32,
    pub height: u32,
    pub zoom: u32,
    pub resources: u64,
    pub focus: Option<NodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket {
    serial: u64,
    pub key: Key,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Active {
    Running(Ticket),
    TimedOut(Ticket),
}

#[derive(Default)]
pub(super) struct State {
    serial: u64,
    active: Option<Active>,
    failed: Option<(Key, Failed)>,
    #[cfg(feature = "layout-validation")]
    pub native_fault: Option<super::validation::Fault>,
    #[cfg(test)]
    pub fail_next: bool,
    #[cfg(test)]
    pub fail_next_planner: bool,
    #[cfg(test)]
    pub hold_next: Option<futures::channel::oneshot::Receiver<()>>,
}

impl State {
    pub fn error(
        &self,
        session: DocumentSessionId,
        document: u64,
        geometry: u64,
    ) -> Option<&'static str> {
        self.failed.filter(|(key, _)| {
            key.session == session && key.document == document && key.geometry == geometry
        })
            .map(|(_, failure)| match failure {
                Failed::Panicked => "Layout update failed. The previous view is still available; edit the document or resize to retry.",
                Failed::TimedOut => "Layout update timed out. The previous view is still available.",
                Failed::PlannerPanicked => "Automatic layout failed. A single-column view is available; edit the document or resize to retry.",
                Failed::StackTimedOut => "Layout update timed out. A single-column view is available; edit the document or resize to retry.",
            })
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn begin(&mut self, key: Key) -> Option<Ticket> {
        if self.active.is_some() || self.failed.is_some_and(|(failed, _)| failed == key) {
            return None;
        }
        self.serial = self.serial.wrapping_add(1);
        let ticket = Ticket {
            serial: self.serial,
            key,
        };
        self.active = Some(Active::Running(ticket));
        Some(ticket)
    }

    pub fn timeout(&mut self, ticket: Ticket) -> bool {
        if self.active != Some(Active::Running(ticket)) {
            return false;
        }
        self.active = Some(Active::TimedOut(ticket));
        self.failed = Some((ticket.key, Failed::TimedOut));
        true
    }

    /// Release only real completion. False also rejects a timed-out result,
    /// even when the worker eventually produced successful geometry.
    pub fn finish(&mut self, ticket: Ticket, failure: Option<Failed>) -> bool {
        if self.active == Some(Active::TimedOut(ticket)) {
            self.active = None;
            // Keep a committed stack's new geometry key. Otherwise suppress
            // the original failed request; newer documents have another key.
            self.failed.get_or_insert((ticket.key, Failed::TimedOut));
            return false;
        }
        if self.active != Some(Active::Running(ticket)) {
            return false;
        }
        self.active = None;
        self.failed = failure.map(|failure| (ticket.key, failure));
        true
    }

    pub fn reset_document(&mut self) {
        // Resetting a view does not cancel synchronous work already running on
        // another thread. Its matching completion still owns this active slot.
        self.failed = None;
        #[cfg(feature = "layout-validation")]
        {
            self.native_fault = None;
        }
    }

    /// The fallback is now installed, with a new published geometry identity.
    /// Suppress this exact environment, not the pre-commit request generation.
    pub fn stack_committed(&mut self, key: Key, failure: Failed) {
        self.failed = Some((key, failure));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failed {
    Panicked,
    TimedOut,
    PlannerPanicked,
    StackTimedOut,
}

/// Only for preparation from owned/immutable snapshots, never editor mutations.
/// Partial geometry is discarded on unwind. Our measurement caches are optional
/// and skip poisoned locks; no canonical document is borrowed mutably here.
/// This does not catch aborts, OOM or deadlocks, or cancel synchronous work.
pub(super) fn prepare<T>(work: impl FnOnce() -> Result<T, Failed>) -> Result<T, Failed> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).map_err(|_| Failed::Panicked)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SharedDocumentSession;

    thread_local! {
        static TEST_SESSION: DocumentSessionId = SharedDocumentSession::new(
            document_core::Document::from_markdown("test").expect("document"),
        ).id();
    }

    fn key() -> Key {
        Key {
            session: TEST_SESSION.with(|session| *session),
            document: 0,
            geometry: 1,
            width: 900_f32.to_bits(),
            height: 800_f32.to_bits(),
            zoom: 1_f32.to_bits(),
            resources: 0,
            focus: None,
        }
    }

    #[test]
    fn timeout_keeps_live_ownership_and_rejects_late_success() {
        let mut state = State::default();
        let ticket = state.begin(key()).unwrap();
        assert!(state.timeout(ticket));
        assert!(!state.timeout(ticket));
        assert!(state.is_active());
        let newer = Key {
            geometry: 2,
            ..key()
        };
        assert!(state.begin(newer).is_none());
        state.reset_document();
        assert!(state.is_active());
        assert!(!state.finish(ticket, None));
        assert!(!state.is_active());
        assert!(state.begin(key()).is_none());
        let next = state.begin(newer).unwrap();
        assert!(!state.timeout(ticket));
        assert!(state.finish(next, None));
    }

    #[test]
    fn cancellation_and_elapsed_deadline_stop_at_checkpoints() {
        let deadline = Deadline::unlimited();
        let worker = deadline.clone();
        assert_eq!(worker.check(), Ok(()));
        deadline.cancel();
        assert_eq!(worker.check(), Err(Failed::TimedOut));
        let expired = Deadline {
            fail_planner: false,
            at: Some(Instant::now() - Duration::from_secs(1)),
            cancelled: Arc::default(),
            checks_left: None,
        };
        assert_eq!(expired.check(), Err(Failed::TimedOut));
        assert_eq!(expired.remaining(), Duration::ZERO);
    }

    #[test]
    fn switching_documents_does_not_release_live_work() {
        let mut state = State::default();
        let old = state.begin(key()).unwrap();
        state.reset_document();
        let new = Key {
            geometry: 2,
            ..key()
        };
        assert!(state.begin(new).is_none());
        assert!(state.finish(old, Some(Failed::Panicked)));
        let next = state.begin(new).unwrap();
        assert!(!state.finish(old, None));
        assert!(state.is_active());
        assert!(state.finish(next, None));
    }

    #[test]
    fn equal_generations_from_different_sessions_have_distinct_keys() {
        let old = key();
        let replacement = SharedDocumentSession::new(
            document_core::Document::from_markdown("test").expect("document"),
        );
        let new = Key {
            session: replacement.id(),
            ..old
        };

        assert_ne!(old, new);
    }

    #[test]
    fn failure_is_suppressed_until_request_changes() {
        let mut state = State::default();
        let ticket = state.begin(key()).unwrap();
        assert!(state.finish(ticket, Some(Failed::Panicked)));
        for _ in 0..100 {
            assert!(state.begin(key()).is_none());
        }
        let changed = Key {
            width: 800_f32.to_bits(),
            ..key()
        };
        let ticket = state.begin(changed).unwrap();
        assert!(state.finish(ticket, None));
        let next = state.begin(changed).unwrap();
        assert!(
            !state.finish(ticket, Some(Failed::Panicked)),
            "an old completion cannot clear a new identical request"
        );
        assert!(state.finish(next, None));
    }

    #[test]
    fn preparation_unwinds_to_an_observable_failure() {
        assert_eq!(
            prepare(|| panic!("synthetic preparation failure")),
            Err::<(), _>(Failed::Panicked)
        );
        assert_eq!(prepare(|| Ok(42)), Ok(42));
    }
}
