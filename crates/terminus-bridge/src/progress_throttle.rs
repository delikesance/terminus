use std::time::{Duration, Instant};

const MIN_INTERVAL: Duration = Duration::from_millis(100);

/// Lets a progress sample through at most once per [`MIN_INTERVAL`], always
/// passing the first and the completing one so the bar starts and ends exact.
#[derive(Debug, Default)]
pub struct ProgressThrottle {
    last_emit: Option<Instant>,
}

impl ProgressThrottle {
    pub fn should_emit(&mut self, now: Instant, done: u64, total: u64) -> bool {
        let finished = total > 0 && done >= total;
        let due = self
            .last_emit
            .is_none_or(|last| now.duration_since(last) >= MIN_INTERVAL);
        if !(finished || due) {
            return false;
        }
        self.last_emit = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_passes() {
        let mut throttle = ProgressThrottle::default();
        assert!(throttle.should_emit(Instant::now(), 0, 100));
    }

    #[test]
    fn samples_inside_the_interval_are_dropped() {
        let mut throttle = ProgressThrottle::default();
        let start = Instant::now();
        throttle.should_emit(start, 0, 100);
        assert!(!throttle.should_emit(start + Duration::from_millis(10), 10, 100));
    }

    #[test]
    fn sample_after_the_interval_passes() {
        let mut throttle = ProgressThrottle::default();
        let start = Instant::now();
        throttle.should_emit(start, 0, 100);
        assert!(throttle.should_emit(start + MIN_INTERVAL, 50, 100));
    }

    #[test]
    fn completing_sample_always_passes() {
        let mut throttle = ProgressThrottle::default();
        let start = Instant::now();
        throttle.should_emit(start, 0, 100);
        assert!(throttle.should_emit(start + Duration::from_millis(1), 100, 100));
    }

    #[test]
    fn unknown_total_is_never_treated_as_finished() {
        let mut throttle = ProgressThrottle::default();
        let start = Instant::now();
        throttle.should_emit(start, 0, 0);
        assert!(!throttle.should_emit(start + Duration::from_millis(1), 5, 0));
    }
}
