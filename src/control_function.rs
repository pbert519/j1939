use crate::frame::{Frame, Header, Request, PGN_ADDRESSCLAIM, PGN_REQUEST};

use crate::name::Name;
use crate::time;
use crate::time::{Duration, Instant};
use crossbeam_queue::ArrayQueue;

#[derive(Debug, PartialEq)]
pub enum AddressState {
    AddressClaimed,
    Preferred,
    WaitForVeto(time::Instant),
    CannotClaim,
}

pub struct ControlFunction {
    name: Name,
    pub(crate) send_queue: ArrayQueue<Frame>,
    receive_queue: ArrayQueue<Frame>,
    address_state: AddressState,
    address: u8,
    address_configurable: bool,
}

impl ControlFunction {
    pub fn new(name: Name, prefered_address: u8) -> Self {
        Self {
            name,
            send_queue: ArrayQueue::new(20),
            receive_queue: ArrayQueue::new(20),
            address_state: AddressState::Preferred,
            address: prefered_address,
            address_configurable: name.address_capable,
        }
    }

    pub fn address_state(&self) -> &AddressState {
        &self.address_state
    }

    pub fn is_online(&self) -> Option<u8> {
        if self.address_state == AddressState::AddressClaimed {
            Some(self.address)
        } else {
            None
        }
    }

    pub fn send_frame(&mut self, frame: Frame) -> bool {
        if self.address_state == AddressState::AddressClaimed {
            self.send_queue.force_push(frame);
            true
        } else {
            false
        }
    }

    pub fn get_frame(&mut self) -> Option<Frame> {
        self.receive_queue.pop()
    }

    // ------------------------------ private ------------------------------------------------------

    pub(crate) fn handle_new_frame(&mut self, frame: &Frame) {
        // Skip own frames
        if frame.header().source_address() == self.address {
            return;
        }
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
                    self.send_addressclaim();
                } else {
                    self.receive_queue.force_push(frame.clone());
                }
            }
        } else {
            self.receive_queue.force_push(frame.clone());
        }
    }

    pub(crate) fn process(&mut self) {
        // do address management
        match self.address_state {
            AddressState::Preferred => {
                self.send_addressclaim();
                self.address_state = AddressState::WaitForVeto(Instant::now());
            }
            AddressState::WaitForVeto(requested) => {
                if Duration::new(250).timeout(requested) {
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
        ) {
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
                        self.address_state = AddressState::WaitForVeto(Instant::now());
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
        let name_raw: u64 = self.name.into();
        let frame = Frame::new(
            Header::new(PGN_ADDRESSCLAIM, 6, 0xFE, Some(255)),
            &name_raw.to_le_bytes(),
        );
        self.send_queue.force_push(frame);
    }
}
