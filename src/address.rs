use crate::frame::{Frame, Request, PGN_ADDRESSCLAIM, PGN_REQUEST};
use crate::name::Name;
use alloc::collections::BTreeMap;

pub struct AddressMonitor {
    cf: BTreeMap<u8, Name>,
}

impl AddressMonitor {
    pub fn new() -> Self {
        Self {
            cf: BTreeMap::new(),
        }
    }
    pub fn handle_frame(&mut self, frame: &Frame) {
        if frame.header().pgn() == PGN_ADDRESSCLAIM {
            let sa = frame.header().source_address();
            let name = Name::from(u64::from_le_bytes(frame.data().try_into().unwrap()));
            let mut old_sa = None;
            for (isa, iname) in &self.cf {
                if *iname == name {
                    old_sa = Some(*isa);
                }
            }
            if let Some(old_sa) = old_sa {
                self.cf.remove(&old_sa);
            }

            self.cf.insert(sa, name);
        } else if frame.header().pgn() == PGN_REQUEST {
            // clear list of active ecus, because all active ecus must response the to addressclaim request
            let req: Request = frame
                .clone()
                .try_into()
                .expect("Could not serialize Request");
            if *req.pgn() == PGN_ADDRESSCLAIM {
                self.cf.clear();
            }
        }
    }
    pub fn control_function_list(&self) -> &BTreeMap<u8, Name> {
        &self.cf
    }
}
