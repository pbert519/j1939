/// Provides a point of time since start of the application
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Instant(pub u64);

pub struct Duration(pub u64);

impl Duration {
    /// Create duration from milliseconds
    pub fn new(ms: u64) -> Self {
        Self(ms)
    }
    /// Get Duration as milliseconds
    pub fn ms(&self) -> u64 {
        self.0
    }
    /// Check if the given timeout is over
    pub fn timeout(&self, start: Instant, now: Instant) -> bool {
        (now.0 - start.0) > self.0
    }
}

// ----------------------------

pub trait TimerDriver {
    /// Get current time
    fn now(&self) -> Instant;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::testtime::TestTimer;
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
