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
/// The stacks process() functions must be called on a regulary basis to perform internal long running tasks
pub struct Stack<CanDriver: embedded_can::blocking::Can, TimeDriver: crate::time::TimerDriver> {
    received_frames: ArrayQueue<Frame>,
    accept_all_da: bool,
    transport: TransportManager,
    cf: Vec<ControlFunction<TimeDriver>>,
    address_monitor: AddressMonitor,
    can_driver: CanDriver,
    time: TimeDriver,
}

impl<CanDriver: embedded_can::blocking::Can, TimeDriver: Clone + crate::time::TimerDriver>
    Stack<CanDriver, TimeDriver>
{
    /// Creates a new Stack object, capturing the can and timer driver
    /// The standard configuration receives all broadcast frames
    pub fn new(can: CanDriver, time: TimeDriver) -> Self {
        Self {
            received_frames: ArrayQueue::new(20),
            accept_all_da: false,
            transport: TransportManager::new(&[]),
            cf: Vec::new(),
            address_monitor: AddressMonitor::new(),
            can_driver: can,
            time,
        }
    }
    /// Creates a new Stack object, capturing the can and timer driver
    /// The standard configuration receives all broadcast frames
    pub fn new_with_nema2000(can: CanDriver, time: TimeDriver, pgns: &[PGN]) -> Self {
        Self {
            received_frames: ArrayQueue::new(20),
            accept_all_da: false,
            transport: TransportManager::new(pgns),
            cf: Vec::new(),
            address_monitor: AddressMonitor::new(),
            can_driver: can,
            time,
        }
    }

    /// run long running tasks like sending with transport protocol and address management
    /// should be called periodically
    pub fn process(&mut self) {
        while let Ok(frame) = self.can_driver.receive() {
            self.push_can_frame(frame);
        }

        // check cf for ongoing work
        self.process_control_functions();

        // handle ongoing transport protocol transactions
        self.transport.process(&mut self.can_driver);
    }

    /// Provides a map with all ecus on the bus
    /// key is the source address, value the name of that ecu
    pub fn control_function_list(&self) -> &BTreeMap<u8, Name> {
        self.address_monitor.control_function_list()
    }

    // ---------------------- control functions ----------------------------------------------------
    /// Creates a new [ControlFunction] with a preferred address and Name
    /// The functions returns a [ControlFunctionHandle] which can be used to access the created ControlFunction
    pub fn register_control_function(
        &mut self,
        preferred_address: u8,
        name: Name,
    ) -> ControlFunctionHandle {
        self.cf.push(ControlFunction::new(
            name,
            preferred_address,
            self.time.clone(),
        ));
        ControlFunctionHandle(self.cf.len() - 1)
    }
    /// Returns a mutable reference of a [ControlFunction].
    /// ControlsFunctions are identified by a [ControlFunctionHandle]
    pub fn control_function(
        &mut self,
        handle: &ControlFunctionHandle,
    ) -> &mut ControlFunction<TimeDriver> {
        &mut self.cf[handle.0]
    }

    fn process_control_functions(&mut self) {
        for cf_index in 0..self.cf.len() {
            // check cf address management for ongoing transactions
            self.cf[cf_index].process(&self.address_monitor);
            // check cf send queues and move them into stack queue
            // ToDo loopback all frames for interested parties, but not the sender?
            while let Some(frame) = self.cf[cf_index].send_queue.pop() {
                self.send_frame(frame.clone());
                for receiver_index in 0..self.cf.len() {
                    // Skip own message for control functions
                    if cf_index == receiver_index {
                        continue;
                    }
                    self.cf[cf_index].handle_new_frame(&frame);
                }
                self.handle_new_frame_stack(frame);
            }
        }
    }

    // --------------------------- direct stack usage ----------------------------------------------
    /// Returns a received J1939 Frame
    /// Frames longer than 8 Bytes are already assembled
    /// By default only broadcast messages are received, to receive additonal message the source address must be set using set_accepted_sa
    pub fn get_frame(&mut self) -> Option<Frame> {
        self.received_frames.pop()
    }
    /// Send a J1939 Frame
    /// Control functions are strongly prefered to send frames
    /// Frames longer than 8 bytes are send by a transport protocol
    /// Frames are not loop backed to control functions!
    pub fn send_frame(&mut self, frame: Frame) {
        if frame.data().len() > 8 {
            self.transport.send_frame(frame, &mut self.can_driver)
        } else {
            self.can_driver
                .transmit(&frame.can())
                .expect("Can Transmit Error!");
        }
    }
    /// Set if the stack accepts messages to all destination addresses
    /// If false broadcasts messages are accepted
    /// This has no effect for control functions
    pub fn set_accepted_all(&mut self, accept_all: bool) {
        self.accept_all_da = accept_all;
    }

    // ------------------------private--------------------------------------------------------------
    /// process a new incoming can frame
    fn push_can_frame<CanFrame: embedded_can::Frame>(&mut self, frame: CanFrame) {
        if let embedded_can::Id::Extended(eid) = frame.id() {
            let header: Header = eid.as_raw().into();
            // 1. check if the frame is addressed to me
            // broadcast or da == 0xFF or address of a registered control function
            if !self.check_destination(header.destination_address()) {
                return;
            }
            // 2. is it a transport protocol message?
            // yes -> handle transport protocol
            // no -> forward to control function (this includes address management) and move it into our buffer
            if self.transport.is_tp_frame(header.pgn()) {
                if let Some(decoded_frame) =
                    self.transport
                        .handle_frame(header, frame.data(), &mut self.can_driver)
                {
                    self.handle_new_frame(decoded_frame)
                }
            // just a normal message
            } else {
                let frame = Frame::new(header, frame.data());
                self.handle_new_frame(frame);
            }
        }
    }

    /// got a new j1939 frame decoded from can frames
    fn handle_new_frame(&mut self, frame: Frame) {
        // check if the new frame should be handley by the cf
        for cf in &mut self.cf {
            cf.handle_new_frame(&frame);
        }
        self.handle_new_frame_stack(frame);
    }

    fn handle_new_frame_stack(&mut self, frame: Frame) {
        // check if the new frame is address related
        if frame.header().pgn() == PGN_ADDRESSCLAIM || frame.header().pgn() == PGN_REQUEST {
            self.address_monitor.handle_frame(&frame);
        }
        // check if the new frame should be handled by the stack
        if let Some(da) = frame.header().destination_address() {
            if !(da == 0xFF || self.accept_all_da) {
                return;
            }
        }
        self.received_frames.force_push(frame);
    }

    fn check_destination(&self, destination_address: Option<u8>) -> bool {
        if let Some(da) = destination_address {
            let cf_address = self.cf.iter().any(|cf| cf.is_online() == Some(da));

            self.accept_all_da || cf_address || da == 0xFF
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

    mod address {
        use super::*;
        use crate::control_function::AddressState;
        use crate::time::Instant;

        #[test]
        fn response_to_addressclaim_request() {
            let mut timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            timer.set_time(300);
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
            let mut timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            timer.set_time(300);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
        }
        #[test]
        fn control_function_address_claim_fixed_failed() {
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            driver.push_can_frame(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 1]));
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
        /// Try to claim a address with a configurable address
        /// No response to the addressclaim request, addressclaim is successfull
        #[test]
        fn control_function_address_claim() {
            let mut timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            let handle = stack.register_control_function(0x85, Name::default());
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0CEAFFFE, &[0, 238, 0]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Requested(Instant(0))
            );
            assert_eq!(driver.get_can_frame(), None);
            timer.set_time(1600);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(1600))
            );
            timer.set_time(1900);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
        }
        /// Try to claim a address with a configurable address
        /// No response to the addressclaim request
        /// But after sending addressclaim an other addressclaim with higher priority is received
        #[test]
        fn control_function_address_claim_lower_priority() {
            let mut timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            let handle = stack.register_control_function(0x85, Name::default());
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0CEAFFFE, &[0, 238, 0]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Requested(Instant(0))
            );
            assert_eq!(driver.get_can_frame(), None);
            timer.set_time(1600);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(1600))
            );
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
            driver.push_can_frame(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 100]));
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(1600))
            );
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF86, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
        }
        /// Try to claim a address with a configurable address
        /// No response to the addressclaim request
        /// But after sending addressclaim an other addressclaim with lower priority is received, but overwritten
        #[test]
        fn control_function_address_claim_higher_priority() {
            let mut timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            let handle = stack.register_control_function(0x85, Name::default());
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0CEAFFFE, &[0, 238, 0]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Requested(Instant(0))
            );
            assert_eq!(driver.get_can_frame(), None);
            timer.set_time(1600);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(1600))
            );
            driver.push_can_frame(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 180]));
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
            timer.set_time(1900);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
        }
        /// Try to claim a address with a configurable address
        /// Get a response to the addressclaim request with the same address
        /// Send Addressclaim with an other free address
        #[test]
        fn control_function_address_conflict() {
            let mut timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            let handle = stack.register_control_function(0x85, Name::default());
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Preferred
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0CEAFFFE, &[0, 238, 0]))
            );
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::Requested(Instant(0))
            );
            assert_eq!(driver.get_can_frame(), None);
            driver.push_can_frame(TestFrame::new2(0x18EEFF85, &[0, 0, 0, 0, 0, 255, 2, 100]));
            timer.set_time(1600);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::WaitForVeto(Instant(1600))
            );
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x18EEFF7F, &[0, 0, 0, 0, 0, 255, 2, 160]))
            );
            assert_eq!(driver.get_can_frame(), None);
            timer.set_time(1900);
            stack.process();
            assert_eq!(
                *stack.control_function(&handle).address_state(),
                AddressState::AddressClaimed
            );
            assert_eq!(driver.get_can_frame(), None);
        }
    }

    mod transport {
        use super::*;
        #[test]
        fn broadcast_rx_short() {
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            stack.set_accepted_all(true);
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            driver.push_can_frame(TestFrame::new2(0x00DC2001, &[1, 2, 3, 4, 5, 6, 7, 8]));
            stack.process();
            assert_eq!(stack.get_frame(), None);
        }
        #[test]
        fn p2p_rx_short_wrong_address() {
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            driver.push_can_frame(TestFrame::new2(0x00DC2001, &[1, 2, 3, 4, 5, 6, 7, 8]));
            stack.process();
            assert_eq!(stack.get_frame(), None);
        }
        #[test]
        fn p2p_rx_long() {
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            stack.set_accepted_all(true);
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            stack.set_accepted_all(true);
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
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
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack = Stack::new(driver.clone(), timer.clone());
            stack.set_accepted_all(true);
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

    mod fastpacket {
        use super::*;

        #[test]
        fn receive_fastpacket() {
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack =
                Stack::new_with_nema2000(driver.clone(), timer.clone(), &[PGN(0x1F805)]);
            driver.push_can_frame(TestFrame::new2(
                0x0DF8051C,
                &[64, 43, 59, 80, 75, 166, 229, 223],
            ));
            stack.process();
            driver.push_can_frame(TestFrame::new2(
                0x0DF8051C,
                &[65, 32, 128, 198, 181, 39, 169, 179],
            ));
            stack.process();
            driver.push_can_frame(TestFrame::new2(
                0x0DF8051C,
                &[66, 198, 6, 128, 205, 146, 152, 121],
            ));
            stack.process();
            driver.push_can_frame(TestFrame::new2(
                0x0DF8051C,
                &[67, 247, 66, 1, 128, 84, 49, 19],
            ));
            stack.process();
            driver.push_can_frame(TestFrame::new2(0x0DF8051C, &[68, 0, 0, 0, 0, 0, 0, 0]));
            stack.process();
            driver.push_can_frame(TestFrame::new2(0x0DF8051C, &[69, 100, 0, 100, 0, 0, 0, 0]));
            stack.process();
            driver.push_can_frame(TestFrame::new2(
                0x0DF8051C,
                &[70, 0, 0, 255, 255, 255, 255, 255],
            ));
            stack.process();
            assert_eq!(
                stack.get_frame(),
                Some(Frame::new(
                    Header::new(PGN::new(0x1F805), 3, 0x1C, None),
                    &[
                        59, 80, 75, 166, 229, 223, 32, 128, 198, 181, 39, 169, 179, 198, 6, 128,
                        205, 146, 152, 121, 247, 66, 1, 128, 84, 49, 19, 0, 0, 0, 0, 0, 0, 0, 100,
                        0, 100, 0, 0, 0, 0, 0, 0
                    ]
                ))
            );
        }

        #[test]
        fn transmit_fast_packet() {
            let timer = TestTimer::new();
            let mut driver = TestDriver::new();
            let mut stack =
                Stack::new_with_nema2000(driver.clone(), timer.clone(), &[PGN(0x1F805)]);
            stack.send_frame(Frame::new(
                Header::new(PGN::new(0x1F805), 3, 0x1C, None),
                &[
                    59, 80, 75, 166, 229, 223, 32, 128, 198, 181, 39, 169, 179, 198, 6, 128, 205,
                    146, 152, 121, 247, 66, 1, 128, 84, 49, 19, 0, 0, 0, 0, 0, 0, 0, 100, 0, 100,
                    0, 0, 0, 0, 0, 0,
                ],
            ));
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[0, 43, 59, 80, 75, 166, 229, 223],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[1, 32, 128, 198, 181, 39, 169, 179],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[2, 198, 6, 128, 205, 146, 152, 121],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[3, 247, 66, 1, 128, 84, 49, 19],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0DF8051C, &[4, 0, 0, 0, 0, 0, 0, 0]))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0DF8051C, &[5, 100, 0, 100, 0, 0, 0, 0]))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[6, 0, 0, 255, 255, 255, 255, 255],
                ))
            );
            stack.process();
            assert_eq!(driver.get_can_frame(), None);

            // send a second time, now the sequence number must be different
            stack.send_frame(Frame::new(
                Header::new(PGN::new(0x1F805), 3, 0x1C, None),
                &[
                    59, 80, 75, 166, 229, 223, 32, 128, 198, 181, 39, 169, 179, 198, 6, 128, 205,
                    146, 152, 121, 247, 66, 1, 128, 84, 49, 19, 0, 0, 0, 0, 0, 0, 0, 100, 0, 100,
                    0, 0, 0, 0, 0, 0,
                ],
            ));
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[32, 43, 59, 80, 75, 166, 229, 223],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[33, 32, 128, 198, 181, 39, 169, 179],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[34, 198, 6, 128, 205, 146, 152, 121],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[35, 247, 66, 1, 128, 84, 49, 19],
                ))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0DF8051C, &[36, 0, 0, 0, 0, 0, 0, 0]))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(0x0DF8051C, &[37, 100, 0, 100, 0, 0, 0, 0]))
            );
            stack.process();
            assert_eq!(
                driver.get_can_frame(),
                Some(TestFrame::new2(
                    0x0DF8051C,
                    &[38, 0, 0, 255, 255, 255, 255, 255],
                ))
            );
            stack.process();
            assert_eq!(driver.get_can_frame(), None);
        }
    }
}
