/// Provides a point of time since start of the application
#[derive(Debug, PartialEq, Eq, Clone, Copy)]

/// Instants describes a point in time
/// The inner type is a strictly monotonic timestamp in milliseconds
pub struct Instant(pub u64);

pub(crate) struct Duration(u64);

impl Duration {
    /// Create duration from milliseconds
    pub const fn new(ms: u64) -> Self {
        Self(ms)
    }
    /// Check if the given timeout is over
    pub const fn timeout(&self, start: Instant, now: Instant) -> bool {
        (now.0 - start.0) > self.0
    }
}

// ----------------------------

/// Provides the internal time base for the J1939 Stack
pub trait TimerDriver {
    /// Get current timestamp as Instant
    fn now(&self) -> Instant;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_time::TestTimer;
    #[test]
    fn timer_init() {
        let timer = TestTimer::new();
        assert_eq!(timer.now(), Instant(0));
    }
    #[test]
    fn timer_delay() {
        let mut timer = TestTimer::new();
        timer.set_time(100);
        assert_eq!(timer.now(), Instant(100));
    }
    #[test]
    fn duration_timeout() {
        let mut timer = TestTimer::new();
        let start = timer.now();
        timer.set_time(100);
        assert_eq!(Duration::new(100).timeout(start, timer.now()), false);
        timer.set_time(101);
        assert_eq!(Duration::new(100).timeout(start, timer.now()), true);
    }
}
