use crate::address::AddressMonitor;
use crate::frame::{Frame, Header, Request, PGN_ADDRESSCLAIM, PGN_REQUEST};
use crate::name::Name;
use crate::time::{Duration, Instant};
use crossbeam_queue::ArrayQueue;

#[derive(Debug, PartialEq)]
pub(crate) enum AddressState {
    Requested(Instant),
    AddressClaimed,
    Preferred,
    WaitForVeto(Instant),
    CannotClaim,
}

/// `ControlFunction` is a entity with an own address on a J1939 bus
/// Each `ControlFunction` participate in the J1939 address management
/// Send and receive frames with a address
pub struct ControlFunction<TimeDriver: crate::time::TimerDriver> {
    name: Name,
    pub(crate) send_queue: ArrayQueue<Frame>,
    receive_queue: ArrayQueue<Frame>,
    address_state: AddressState,
    address: u8,
    address_configurable: bool,
    time: TimeDriver,
}

impl<TimeDriver: crate::time::TimerDriver> ControlFunction<TimeDriver> {
    pub(crate) fn new(name: Name, preferred_address: u8, time: TimeDriver) -> Self {
        Self {
            name,
            send_queue: ArrayQueue::new(20),
            receive_queue: ArrayQueue::new(20),
            address_state: AddressState::Preferred,
            address: preferred_address,
            address_configurable: name.address_capable,
            time,
        }
    }
    /// If the control function is online and has a valid address Some(address) is returned
    pub fn is_online(&self) -> Option<u8> {
        if self.address_state == AddressState::AddressClaimed {
            Some(self.address)
        } else {
            None
        }
    }
    /// Send a frame using this control function
    /// The source address in the frame is overwritten by the address of the local bus
    /// Returns false if the `ControlFunction` has not a valid address
    /// The frame is send as soon as stack.process() is called.
    pub fn send_frame(&mut self, mut frame: Frame) -> bool {
        if self.address_state == AddressState::AddressClaimed {
            frame.update_source_address(self.address);
            self.send_queue.force_push(frame);
            true
        } else {
            false
        }
    }
    /// Returns the last received frame if any
    pub fn get_frame(&mut self) -> Option<Frame> {
        self.receive_queue.pop()
    }

    // ------------------------------ private ------------------------------------------------------
    #[cfg(test)]
    pub(crate) fn address_state(&self) -> &AddressState {
        &self.address_state
    }

    pub(crate) fn handle_new_frame(&mut self, frame: &Frame) {
        // check if the message targets this cf
        if let Some(da) = frame.header().destination_address() {
            if da == 0xFF
                || (self.address_state == AddressState::AddressClaimed && self.address == da)
            {
                if frame.header().pgn() == PGN_ADDRESSCLAIM {
                    self.handle_addressclaim(frame);
                } else if frame.header().pgn() == PGN_REQUEST
                    && TryInto::<Request>::try_into(frame.clone()).unwrap().pgn()
                        == &PGN_ADDRESSCLAIM
                {
                    match self.address_state {
                        AddressState::AddressClaimed | AddressState::WaitForVeto(_) => {
                            self.send_addressclaim()
                        }
                        AddressState::CannotClaim => self.send_cannotclaim(),
                        _ => (),
                    }
                } else {
                    self.receive_queue.force_push(frame.clone());
                }
            }
        } else {
            // broadcast
            self.receive_queue.force_push(frame.clone());
        }
    }

    pub(crate) fn process(&mut self, address_monitor: &AddressMonitor) {
        // do address management
        match self.address_state {
            AddressState::Preferred => {
                if self.address_configurable {
                    // we have a configurable address, send address request and wait for responses
                    self.send_queue
                        .force_push(Request::new(PGN_ADDRESSCLAIM, 0xFE, 0xFF).into());
                    self.address_state = AddressState::Requested(self.time.now());
                } else {
                    // we have a fixed address, therefore send addressclaim asap
                    self.send_addressclaim();
                    self.address_state = AddressState::WaitForVeto(self.time.now());
                }
            }
            AddressState::Requested(requested) => {
                // ToDo: change 1500 to RTxD?
                if Duration::millis(1500) < (self.time.now() - requested) {
                    // check if our preferred address is in the list
                    if address_monitor
                        .control_function_list()
                        .contains_key(&self.address)
                    {
                        // select an other address
                        let mut next_address = 127;
                        while address_monitor
                            .control_function_list()
                            .contains_key(&next_address)
                            || next_address >= 147
                        {
                            next_address += 1;
                        }
                        if next_address >= 147 {
                            // we did not found a free address
                            self.address_state = AddressState::CannotClaim;
                            self.send_cannotclaim();
                        } else {
                            // use a free address in range 127..247
                            self.address = next_address;
                            self.send_addressclaim();
                            self.address_state = AddressState::WaitForVeto(self.time.now());
                        }
                    } else {
                        // Use our preferred address
                        self.send_addressclaim();
                        self.address_state = AddressState::WaitForVeto(self.time.now());
                    }
                }
            }
            AddressState::WaitForVeto(requested) => {
                if Duration::millis(250) < (self.time.now() - requested) {
                    self.address_state = AddressState::AddressClaimed;
                }
            }
            _ => {} /* Nothing to do */
        }
    }

    fn handle_addressclaim(&mut self, frame: &Frame) {
        let name_raw = u64::from_le_bytes(frame.data().try_into().unwrap());
        if matches!(
            self.address_state,
            AddressState::AddressClaimed | AddressState::WaitForVeto(_)
        ) && frame.header().source_address() == self.address
        {
            match name_raw.cmp(&self.name.into()) {
                core::cmp::Ordering::Less => {
                    // we lose the address
                    if self.address_configurable {
                        // auto configured address should be in range 128 to 247
                        if self.address < 128 || self.address >= 247 {
                            self.address = 128;
                        } else {
                            self.address += 1;
                        };
                        self.address_state = AddressState::WaitForVeto(self.time.now());
                        self.send_addressclaim();
                    } else {
                        self.address_state = AddressState::CannotClaim;
                        self.send_cannotclaim();
                    }
                }
                core::cmp::Ordering::Greater => {
                    // we have higher priority
                    self.send_addressclaim();
                }
                core::cmp::Ordering::Equal => {
                    panic!("Same Name on Bus")
                }
            }
        }
    }

    fn send_addressclaim(&mut self) {
        let name_raw: u64 = self.name.into();
        let frame = Frame::new(
            Header::new(PGN_ADDRESSCLAIM, 6, self.address, Some(255)),
            &name_raw.to_le_bytes(),
        );
        self.send_queue.force_push(frame);
    }
    fn send_cannotclaim(&mut self) {
        // ToDo RTxD delay before sending
        let name_raw: u64 = self.name.into();
        let frame = Frame::new(
            Header::new(PGN_ADDRESSCLAIM, 6, 0xFE, Some(255)),
            &name_raw.to_le_bytes(),
        );
        self.send_queue.force_push(frame);
    }
}
