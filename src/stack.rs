use crate::address::AddressMonitor;
use crate::control_function::ControlFunction;
use crate::frame::*;
use crate::name::Name;
use crate::transport::TransportManager;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crossbeam_queue::ArrayQueue;

/// Represents a single J1939 stack
/// The stack itself manages transport protocols
/// get and setter for frames and source address based filtering is implemented.
/// It is possible to register a ControlFunction, which handles AddressManagement and provides a pgn based filter utility
pub struct Stack<CanDriver: embedded_hal::can::nb::Can> {
    received_frames: ArrayQueue<Frame>,
    listen_sa: Vec<u8>,
    transport: TransportManager,
    cf: Vec<ControlFunction>,
    address_monitor: AddressMonitor,
    can_driver: CanDriver,
}

impl<CanDriver: embedded_hal::can::nb::Can> Stack<CanDriver> {
    pub fn new(can: CanDriver) -> Self {
        Self {
            received_frames: ArrayQueue::new(20),
            listen_sa: Vec::new(),
            transport: TransportManager::new(),
            cf: Vec::new(),
            address_monitor: AddressMonitor::new(),
            can_driver: can,
        }
    }

    fn push_can_frame<CanFrame: embedded_hal::can::Frame>(&mut self, frame: CanFrame) {
        if let embedded_hal::can::Id::Extended(eid) = frame.id() {
            let header: Header = eid.as_raw().into();
            // 1. check if the frame is addressed to me
            // broadcast or da == 0xFF or address of a registered control function
            if !self.check_destination(header.destination_address()) {
                return;
            }
            // 2. is it a transport protocol message?
            // yes -> handle transport protocol
            // no -> forward to control function (this includes address management) and move it into our buffer
            if header.pgn() == PGN_TP_CM
                || header.pgn() == PGN_TP_DT
                || header.pgn() == PGN_ETP_CM
                || header.pgn() == PGN_ETP_DT
            {
                let transport_frame =
                    self.transport
                        .handle_frame(header, frame.data(), &mut self.can_driver);
                if let Some(frame) = transport_frame {
                    self.handle_new_frame(frame);
                }
            } else {
                let frame = Frame::new(header, frame.data());
                self.handle_new_frame(frame);
            }
        }
    }

    /// run long running tasks like sending with transport protocol and address management
    pub fn process(&mut self) {
        while let Ok(frame) = self.can_driver.receive() {
            self.push_can_frame(frame);
        }

        // check cf for ongoing work
        for cf_index in 0..self.cf.len() {
            // check cf address management for ongoing transactions
            self.cf[cf_index].process();
            // check cf send queues and move them into stack queue
            // ToDo loopback all frames for interested parties, but not the sender?
            while let Some(frame) = self.cf[cf_index].send_queue.pop() {
                self.send_frame(frame.clone());
                self.handle_new_frame(frame);
            }
        }
        // handle ongoing transport protocol transactions
        self.transport.process(&mut self.can_driver);
    }

    /// Provides a map with all ecus on the bus
    /// key is the source address, value the name of that ecu
    pub fn control_function_list(&self) -> &BTreeMap<u8, Name> {
        self.address_monitor.control_function_list()
    }

    // ---------------------- control functions ----------------------------------------------------
    pub fn register_control_function(
        &mut self,
        preferred_address: u8,
        name: Name,
    ) -> ControlFunctionHandle {
        self.cf.push(ControlFunction::new(name, preferred_address));
        ControlFunctionHandle(self.cf.len() - 1)
    }

    pub fn control_function(&mut self, handle: &ControlFunctionHandle) -> &mut ControlFunction {
        &mut self.cf[handle.0]
    }

    // --------------------------- direct stack usage ----------------------------------------------
    pub fn get_frame(&mut self) -> Option<Frame> {
        self.received_frames.pop()
    }

    pub fn send_frame(&mut self, frame: Frame) {
        if frame.data().len() > 8 {
            self.transport.send_frame(frame, &mut self.can_driver)
        } else {
            self.can_driver.transmit(&frame.can()).expect("Can Transmit Error!");
        }
    }

    pub fn set_accepted_sa(&mut self, sa_list: Vec<u8>) {
        self.listen_sa = sa_list;
    }

    // ------------------------private--------------------------------------------------------------
    fn handle_new_frame(&mut self, frame: Frame) {
        if frame.header().pgn() == PGN_ADDRESSCLAIM || frame.header().pgn() == PGN_REQUEST {
            self.address_monitor.handle_frame(&frame);
        }

        for cf in &mut self.cf {
            cf.handle_new_frame(&frame);
        }
        if let Some(da) = frame.header().destination_address() {
            if da == 0xFF || self.listen_sa.contains(&da) {
                self.received_frames.force_push(frame);
            }
        } else {
            self.received_frames.force_push(frame);
        }
    }

    fn check_destination(&self, destination_address: Option<u8>) -> bool {
        if let Some(da) = destination_address {
            let cf_address = self.cf.iter().find(|cf| cf.is_online() == Some(da));

            self.listen_sa.contains(&da) || cf_address.is_some() || da == 0xFF
        } else {
            true
        }
    }
}

/// Handle to identify a control function
pub struct ControlFunctionHandle(usize);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Frame;
    use crate::test_utils::can_driver::TestDriver;
    use crate::test_utils::frame::TestFrame;
    use crate::test_utils::testtime::TestTimer;
    use crate::time::Timer;

    mod address {
        use super::*;
        use crate::control_function::AddressState;
        use crate::time::Instant;

        #[test]
        fn response_to_addressclaim_request() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            let handle = stack.register_control_function(
                0x85,
                Name {
                    address_capable: false,
                    ..Name::default()
                },
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 32]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            std::thread::sleep(std::time::Duration::from_millis(300));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
            // till here we just setup our stack with a valid address
            driver.push_can_frame(TestFrame::new2(0x00EAFF80, &[0, 0xEE, 0]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 32]))
            )
        }

        #[test]
        fn control_function_address_claim_fixed() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            let handle = stack.register_control_function(
                0x85,
                Name {
                    address_capable: false,
                    ..Name::default()
                },
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 32]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            std::thread::sleep(std::time::Duration::from_millis(300));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
        }
        #[test]
        fn control_function_address_claim_fixed_failed() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            let handle = stack.register_control_function(
                0x85,
                Name {
                    address_capable: false,
                    ..Name::default()
                },
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 32]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            driver.push_can_frame(TestFrame::new2(0x18EEFF84, &[0, 0, 0, 0, 0, 255, 2, 1]));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::CannotClaim
            );
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFFFE, &[0, 0, 0, 0, 0, 255, 2, 32]))
            );
            // ToDo monitor bus if the name claims a different SA?
        }
        #[test]
        fn control_function_address_claim() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            let handle = stack.register_control_function(0x85, Name::default());
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            std::thread::sleep(std::time::Duration::from_millis(250));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
        }
        #[test]
        fn control_function_address_claim_failed() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            let handle = stack.register_control_function(0x85, Name::default());
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
            driver.push_can_frame(TestFrame::new2(0x18EEFF84, &[0, 0, 0, 0, 0, 255, 2, 100]));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF86, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
        }
        #[test]
        fn control_function_address_claim_higher_priority() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            let handle = stack.register_control_function(0x85, Name::default());
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(0))
            );
            driver.push_can_frame(TestFrame::new2(0x18EEFF84, &[0, 0, 0, 0, 0, 255, 2, 180]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
            std::thread::sleep(std::time::Duration::from_millis(300));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
        }
    }

    mod transport {
        use super::*;
        #[test]
        fn broadcast_rx_short() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            driver.push_can_frame(TestFrame::new2(0x00FEB201, &[1, 2, 3, 4, 5, 6, 7, 8]));
            stack.process();
            assert_eq!(
                stack.get_frame(),
                Some(Frame::new(
                    Header::new(PGN::new(0xFEB2), 0, 0x01, None),
                    &[1, 2, 3, 4, 5, 6, 7, 8]
                ))
            );
        }

        #[test]
        fn broadcast_rx_long() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            driver.push_can_frame(TestFrame::new2(
                0x00ECFF01,
                &[32, 20, 0, 3, 255, 0xB0, 0xFE, 0],
            ));
            driver.push_can_frame(TestFrame::new2(0x00EBFF01, &[1, 1, 2, 3, 4, 5, 6, 7]));
            driver.push_can_frame(TestFrame::new2(0x00EBFF01, &[2, 1, 2, 3, 4, 5, 6, 7]));
            driver.push_can_frame(TestFrame::new2(0x00EBFF01, &[3, 1, 2, 3, 4, 5, 6, 255]));
            stack.process();
            assert_eq!(
                stack.get_frame(),
                Some(Frame::new(
                    Header::new(PGN::new(0xFEB0), 0, 0x01, Some(255)),
                    &[1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6]
                ))
            );
        }

        #[test]
        fn p2p_rx_short() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            stack.listen_sa.push(0x20);
            driver.push_can_frame(TestFrame::new2(0x00DC2080, &[1, 2, 3, 4, 5, 6, 7, 8]));
            stack.process();
            assert_eq!(
                stack.get_frame(),
                Some(Frame::new(
                    Header::new(PGN::new(0xDC00), 0, 0x80, Some(0x20)),
                    &[1, 2, 3, 4, 5, 6, 7, 8]
                ))
            );
        }
        #[test]
        fn p2p_rx_short_without_address() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            driver.push_can_frame(TestFrame::new2(0x00DC2001, &[1, 2, 3, 4, 5, 6, 7, 8]));
            stack.process();
            assert_eq!(stack.get_frame(), None);
        }
        #[test]
        fn p2p_rx_short_wrong_address() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            driver.push_can_frame(TestFrame::new2(0x00DC2001, &[1, 2, 3, 4, 5, 6, 7, 8]));
            stack.process();
            assert_eq!(stack.get_frame(), None);
        }
        #[test]
        fn p2p_rx_long() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            stack.listen_sa.push(0x02);
            driver.push_can_frame(TestFrame::new2(0x00EC0201, &[16, 20, 0, 3, 1, 176, 254, 0]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x1CEC0102,
                    &[17, 1, 1, 255, 255, 176, 254, 0]
                ))
            );
            driver.push_can_frame(TestFrame::new2(0x00EB0201, &[1, 1, 2, 3, 4, 5, 6, 7]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x1CEC0102,
                    &[17, 1, 2, 255, 255, 176, 254, 0]
                ))
            );
            driver.push_can_frame(TestFrame::new2(0x00EB0201, &[2, 1, 2, 3, 4, 5, 6, 7]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x1CEC0102,
                    &[17, 1, 3, 255, 255, 176, 254, 0]
                ))
            );
            driver.push_can_frame(TestFrame::new2(0x00EB0201, &[3, 1, 2, 3, 4, 5, 55, 255]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x1CEC0102,
                    &[19, 20, 0, 3, 255, 176, 254, 0]
                ))
            );
            assert_eq!(
                stack.get_frame(),
                Some(Frame::new(
                    Header::new(PGN::new(0xFEB0), 0, 0x01, Some(0x2)),
                    &[1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 55]
                ))
            );
            stack.process();
            assert_eq!(driver.get_can_frame(), None);
            assert_eq!(stack.get_frame(), None);
        }
        #[test]
        fn p2p_rx_long_abort_already_connected() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            stack.listen_sa.push(0x02);
            driver.push_can_frame(TestFrame::new2(0x00EC0201, &[16, 20, 0, 3, 1, 176, 254, 0]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x1CEC0102,
                    &[17, 1, 1, 255, 255, 176, 254, 0]
                ))
            );
            driver.push_can_frame(TestFrame::new2(0x00EC0201, &[16, 20, 0, 3, 1, 176, 254, 0]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x1CEC0102,
                    &[255, 1, 255, 255, 255, 176, 254, 0]
                ))
            );
            stack.process();
            assert_eq!(driver.get_can_frame(), None)
        }
        #[test]
        fn broadcast_tx_short() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            stack.send_frame(Frame::new(
                Header::new(PGN::new(0xFEB2), 0, 0x21, None),
                &[1, 2, 3, 4, 5, 6, 7, 8],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x00FEB221, &[1, 2, 3, 4, 5, 6, 7, 8]))
            )
        }
        #[test]
        fn broadcast_tx_long() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            stack.send_frame(Frame::new(
                Header::new(PGN::new(0xFEB0), 0, 0x21, None),
                &[1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x1CECFF21,
                    &[32, 20, 0, 3, 255, 0xB0, 0xFE, 0]
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEBFF21, &[1, 1, 2, 3, 4, 5, 6, 7]))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEBFF21, &[2, 1, 2, 3, 4, 5, 6, 7]))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEBFF21, &[3, 1, 2, 3, 4, 5, 6, 255]))
            );
            stack.process();
            assert_eq!(driver.get_can_frame(), None)
        }
        #[test]
        fn p2p_tx_short() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            stack.send_frame(Frame::new(
                Header::new(PGN::new(0xF000), 6, 0x21, Some(0x9B)),
                &[1, 2, 3, 4, 5, 6, 7, 8],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18F09B21, &[1, 2, 3, 4, 5, 6, 7, 8]))
            )
        }
        #[test]
        fn p2p_tx_long() {
            Timer::init(Box::new(TestTimer::new()));
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone());
            stack.listen_sa.push(0x90);
            stack.send_frame(Frame::new(
                Header::new(PGN::new(0xDF00), 0, 0x90, Some(0x9B)),
                &[1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6, 7, 1, 2, 3, 4, 5, 6],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEC9B90, &[16, 20, 0, 3, 1, 0, 223, 0]))
            );
            // without cts no further messages
            stack.process();
            assert_eq!(driver.get_can_frame(), None);
            driver.push_can_frame(TestFrame::new2(
                0x1CEC909B,
                &[17, 1, 1, 255, 255, 0, 223, 0],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEB9B90, &[1, 1, 2, 3, 4, 5, 6, 7]))
            );
            // without cts no further messages
            stack.process();
            assert_eq!(driver.get_can_frame(), None);
            driver.push_can_frame(TestFrame::new2(
                0x1CEC909B,
                &[17, 1, 2, 255, 255, 0, 223, 0],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEB9B90, &[2, 1, 2, 3, 4, 5, 6, 7]))
            );
            // repeat a packet
            driver.push_can_frame(TestFrame::new2(
                0x1CEC909B,
                &[17, 1, 2, 255, 255, 0, 223, 0],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEB9B90, &[2, 1, 2, 3, 4, 5, 6, 7]))
            );
            driver.push_can_frame(TestFrame::new2(
                0x1CEC909B,
                &[17, 1, 3, 255, 255, 0, 223, 0],
            ));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x1CEB9B90, &[3, 1, 2, 3, 4, 5, 6, 255]))
            );
            driver.push_can_frame(TestFrame::new2(0x1CEC909B, &[19, 20, 0, 3, 255, 0, 223, 0]));
            stack.process();
            assert_eq!(driver.get_can_frame(), None);
        }
    }
}
