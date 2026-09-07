//! Coalesce resource-only invalidations without indefinitely postponing layout.
//! This owns no resources or document state; the editor owns the single wake task.
use std::time::{Duration, Instant};

const QUIET_PERIOD: Duration = Duration::from_millis(80);
const MAX_WAIT: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(super) struct ResourceBatch {
    observed_generation: u64,
    pending: Option<Pending>,
}

struct Pending {
    first: Instant,
    latest: Instant,
}

impl ResourceBatch {
    pub fn observe(&mut self, generation: u64, now: Instant) {
        if generation == self.observed_generation {
            return;
        }
        self.observed_generation = generation;
        let pending = self.pending.get_or_insert(Pending {
            first: now,
            latest: now,
        });
        pending.latest = now;
    }

    pub fn delay(&self, now: Instant) -> Option<Duration> {
        let pending = self.pending.as_ref()?;
        let deadline = (pending.latest + QUIET_PERIOD).min(pending.first + MAX_WAIT);
        let remaining = deadline.saturating_duration_since(now);
        (!remaining.is_zero()).then_some(remaining)
    }

    /// Dispatch consumes the batch, not the observed generation. Repainting
    /// during that job must not start a new batch for the same resource set.
    pub fn dispatched(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_waits_for_quiet_but_not_forever() {
        let start = Instant::now();
        let mut batch = ResourceBatch::default();
        assert_eq!(batch.delay(start), None);
        for (generation, ms) in [(1, 0), (2, 60), (3, 120), (4, 180), (5, 240)] {
            let now = start + Duration::from_millis(ms);
            batch.observe(generation, now);
            assert_eq!(
                batch.delay(now),
                Some(QUIET_PERIOD.min(MAX_WAIT - Duration::from_millis(ms)))
            );
        }
        assert_eq!(batch.delay(start + MAX_WAIT), None);
        batch.dispatched();
        batch.observe(5, start + MAX_WAIT);
        assert_eq!(batch.delay(start + MAX_WAIT), None);
        batch.observe(6, start + MAX_WAIT);
        assert_eq!(batch.delay(start + MAX_WAIT), Some(QUIET_PERIOD));
    }

    #[test]
    fn unchanged_frames_do_not_extend_quiet_deadline() {
        let start = Instant::now();
        let mut batch = ResourceBatch::default();
        batch.observe(1, start);
        batch.observe(1, start + Duration::from_millis(70));
        assert_eq!(batch.delay(start + QUIET_PERIOD), None);
        batch.observe(2, start + Duration::from_millis(70));
        assert_eq!(
            batch.delay(start + QUIET_PERIOD),
            Some(Duration::from_millis(70))
        );
        batch.dispatched();
        assert_eq!(batch.delay(start + QUIET_PERIOD), None);
    }
}
