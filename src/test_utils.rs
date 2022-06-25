pub mod can_driver {
    use std::sync::Mutex;

    use super::frame::TestFrame;
    use alloc::{collections::VecDeque, sync::Arc};

    #[derive(Debug)]
    pub struct TestDriverError {}

    impl embedded_hal::can::Error for TestDriverError {
        fn kind(&self) -> embedded_hal::can::ErrorKind {
            embedded_hal::can::ErrorKind::Other
        }
    }

    pub struct TestDriver {
        output: Arc<Mutex<VecDeque<TestFrame>>>,
        input: Arc<Mutex<VecDeque<TestFrame>>>,
    }
    impl Clone for TestDriver {
        fn clone(&self) -> Self {
            Self {
                output: self.output.clone(),
                input: self.input.clone(),
            }
        }
    }

    impl TestDriver {
        pub fn new() -> Self {
            Self {
                output: Arc::new(Mutex::new(VecDeque::new())),
                input: Arc::new(Mutex::new(VecDeque::new())),
            }
        }
        pub fn push_can_frame(&mut self, frame: TestFrame) {
            self.input.lock().unwrap().push_back(frame);
        }
        pub fn get_can_frame(&mut self) -> Option<TestFrame> {
            self.output.lock().unwrap().pop_front()
        }
    }

    impl embedded_hal::can::nb::Can for TestDriver {
        type Frame = crate::test_utils::frame::TestFrame;

        type Error = TestDriverError;

        fn transmit(
            &mut self,
            frame: &Self::Frame,
        ) -> embedded_hal::nb::Result<Option<Self::Frame>, Self::Error> {
            self.output.lock().unwrap().push_back(frame.clone());
            Ok(None)
        }

        fn receive(&mut self) -> embedded_hal::nb::Result<Self::Frame, Self::Error> {
            if let Some(frame) = self.input.lock().unwrap().pop_front() {
                Ok(frame)
            } else {
                Err(embedded_hal::nb::Error::WouldBlock)
            }
        }
    }
}

pub mod frame {
    use alloc::vec::Vec;
    use embedded_hal::can::Frame;

    #[derive(Debug, PartialEq, Clone)]
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
