use crate::frame::*;

mod tp_frames;
mod transport_packager;

use crate::transport::transport_packager::TransportPackager;

pub struct TransportManager {
    transport_packager: TransportPackager,
}

impl TransportManager {
    pub fn new() -> Self {
        Self {
            transport_packager: TransportPackager::new(),
        }
    }

    pub fn handle_frame<CanDriver: embedded_hal::can::nb::Can>(
        &mut self,
        header: Header,
        data: &[u8],
        can_driver: &mut CanDriver,
    ) -> Option<Frame> {
        let mut result = None;
        match header.pgn() {
            PGN_TP_CM => {
                let tpcm = tp_frames::TPCM::from_frame(header, data);
                self.transport_packager.process_tpcm(tpcm, can_driver);
            }
            PGN_TP_DT => {
                let tpdt = tp_frames::TPDT::from_frame(header, data);
                result = self.transport_packager.process_tpdt(tpdt, can_driver);
            }
            PGN_ETP_CM => todo!(),
            PGN_ETP_DT => todo!(),
            _ => panic!("Invalid PGN handled by Transportmanager"),
        }
        result
    }

    pub fn process<CanDriver: embedded_hal::can::nb::Can>(&mut self, can_driver: &mut CanDriver) {
        self.transport_packager.process_out_transfers(can_driver);
    }

    pub fn send_frame<CanDriver: embedded_hal::can::nb::Can>(&mut self, frame: Frame, can_driver: &mut CanDriver) {
        if frame.data().len() > 1785 {
            todo!("ETP not yet supported");
        } else {
            self.transport_packager
                .new_out_transfer(frame, can_driver)
        }
    }
}
