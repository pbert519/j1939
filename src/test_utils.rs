pub mod can_driver {
    use std::sync::Mutex;

    use super::frame::TestFrame;
    use alloc::{collections::VecDeque, sync::Arc};

    #[derive(Debug)]
    pub struct TestDriverError {}

    impl embedded_can::Error for TestDriverError {
        fn kind(&self) -> embedded_can::ErrorKind {
            embedded_can::ErrorKind::Other
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

    impl embedded_can::blocking::Can for TestDriver {
        type Frame = crate::test_utils::frame::TestFrame;

        type Error = TestDriverError;

        fn transmit(&mut self, frame: &Self::Frame) -> Result<(), Self::Error> {
            self.output.lock().unwrap().push_back(frame.clone());
            Ok(())
        }

        fn receive(&mut self) -> Result<Self::Frame, Self::Error> {
            if let Some(frame) = self.input.lock().unwrap().pop_front() {
                Ok(frame)
            } else {
                Err(TestDriverError {})
            }
        }
    }
}

pub mod frame {
    use alloc::vec::Vec;
    use embedded_can::Frame;

    #[derive(Debug, PartialEq, Clone)]
    pub struct TestFrame {
        id: embedded_can::Id,
        data: Vec<u8>,
    }

    impl TestFrame {
        pub fn new2(id: u32, data: &[u8]) -> Self {
            TestFrame::new(
                embedded_can::Id::Extended(embedded_can::ExtendedId::new(id).unwrap()),
                data,
            )
            .unwrap()
        }
    }

    impl embedded_can::Frame for TestFrame {
        fn new(id: impl Into<embedded_can::Id>, data: &[u8]) -> Option<Self> {
            Some(TestFrame {
                id: id.into(),
                data: Vec::from(data),
            })
        }

        fn new_remote(_id: impl Into<embedded_can::Id>, _dlc: usize) -> Option<Self> {
            None
        }

        fn is_extended(&self) -> bool {
            match self.id {
                embedded_can::Id::Standard(_) => false,
                embedded_can::Id::Extended(_) => true,
            }
        }

        fn is_remote_frame(&self) -> bool {
            false
        }

        fn id(&self) -> embedded_can::Id {
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

pub mod test_time {
    use alloc::sync::Arc;
    use std::sync::Mutex;

    use crate::time::Instant;

    #[derive(Clone)]
    pub struct TestTimer {
        time: Arc<Mutex<u64>>,
    }

    impl TestTimer {
        pub fn new() -> Self {
            Self {
                time: Arc::new(Mutex::new(0)),
            }
        }
        pub fn set_time(&mut self, time: u64) {
            if let Ok(mut t) = self.time.lock() {
                *t = time;
            }
        }
    }

    impl crate::time::TimerDriver for TestTimer {
        fn now(&self) -> Instant {
            Instant::from_ticks(*self.time.lock().unwrap())
        }
    }
}
