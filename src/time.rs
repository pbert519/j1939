/// Provides a point of time since start of the application

/// Instants describes a point in time
/// The inner type is a strictly monotonic timestamp in milliseconds
pub type Instant = fugit::TimerInstantU64<1000>;
/// Duration between two Instants
pub type Duration = fugit::MillisDurationU64;

// ----------------------------

/// Provides the internal time base for the J1939 Stack
pub trait TimerDriver {
    /// Get current timestamp as Instant
    fn now(&self) -> Instant;
}

/// TimeDriver implementation for std builds, gated by the std feature
#[cfg(feature = "std")]
pub mod std {
    use super::*;

    /// TimeDriver implementation for std builds, gated by the std feature
    #[derive(Clone)]
    pub struct StdTimerDriver(::std::time::Instant);

    impl StdTimerDriver {
        /// Creates a new StdTimerDriver
        pub fn new() -> Self {
            Self(::std::time::Instant::now())
        }
    }

    impl TimerDriver for StdTimerDriver {
        fn now(&self) -> Instant {
            let duration = self.0.elapsed().as_millis();
            Instant::from_ticks(duration as u64)
        }
    }
}
