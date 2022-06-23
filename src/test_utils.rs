pub mod frame {
    use alloc::vec::Vec;
    use embedded_hal::can::Frame;

    #[derive(Debug, PartialEq)]
    pub struct TestFrame {
        id: embedded_hal::can::Id,
        data: Vec<u8>,
    }

    impl TestFrame {
        pub fn new2(id: u32, data: &[u8]) -> Self {
            TestFrame::new(
                embedded_hal::can::Id::Extended(embedded_hal::can::ExtendedId::new(id).unwrap()),
                data,
            )
            .unwrap()
        }
    }

    impl embedded_hal::can::Frame for TestFrame {
        fn new(id: impl Into<embedded_hal::can::Id>, data: &[u8]) -> Option<Self> {
            Some(TestFrame {
                id: id.into(),
                data: Vec::from(data),
            })
        }

        fn new_remote(_id: impl Into<embedded_hal::can::Id>, _dlc: usize) -> Option<Self> {
            None
        }

        fn is_extended(&self) -> bool {
            match self.id {
                embedded_hal::can::Id::Standard(_) => false,
                embedded_hal::can::Id::Extended(_) => true,
            }
        }

        fn is_remote_frame(&self) -> bool {
            false
        }

        fn id(&self) -> embedded_hal::can::Id {
            self.id
        }

        fn dlc(&self) -> usize {
            self.data.len()
        }

        fn data(&self) -> &[u8] {
            &self.data
        }
    }
}

pub mod testtime {
    pub struct TestTimer(std::time::Instant);

    impl TestTimer {
        pub fn new() -> Self {
            Self(std::time::Instant::now())
        }
    }

    impl crate::time::TimerDriver for TestTimer {
        fn now(&self) -> u64 {
            let duration = self.0.elapsed();
            duration.as_millis() as u64
        }
    }
}
