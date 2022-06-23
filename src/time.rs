use alloc::boxed::Box;

/// Provides a point of time since start of the application
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Instant(pub u64);

impl Instant {
    pub fn now() -> Self {
        Instant(Timer::now())
    }
}

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
    pub fn timeout(&self, start: Instant) -> bool {
        (Instant::now().0 - start.0) > self.0
    }
}

// ----------------------------

static mut TIMER: Option<Timer> = None;

pub struct Timer(Box<dyn TimerDriver>);

impl Timer {
    pub fn init(driver: Box<dyn TimerDriver>) {
        let timer = Self(driver);
        unsafe {
            TIMER = Some(timer);
        }
    }

    fn get_now(&self) -> u64 {
        self.0.now()
    }

    pub fn now() -> u64 {
        unsafe {
            if let Some(timer) = &TIMER {
                timer.get_now()
            } else {
                panic!("Timer not inialized")
            }
        }
    }
}

pub trait TimerDriver {
    /// Get current time
    fn now(&self) -> u64;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::testtime::TestTimer;
    #[test]
    fn timer_init() {
        Timer::init(Box::new(TestTimer::new()));
        assert_eq!(Instant::now(), Instant(0));
    }
    #[test]
    fn timer_delay() {
        Timer::init(Box::new(TestTimer::new()));
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(Instant::now(), Instant(100));
    }
    #[test]
    fn duration_timeout() {
        Timer::init(Box::new(TestTimer::new()));
        let start = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(Duration::new(100).timeout(start), false);
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert_eq!(Duration::new(100).timeout(start), true);
    }
}
