use crate::frame::*;

mod fast_packet;
mod tp_frames;
mod transport_packager;

use self::fast_packet::FastPacketCoder;
use crate::transport::transport_packager::TransportPackager;

pub struct TransportManager {
    transport_packager: TransportPackager,
    fast_packet: FastPacketCoder,
}

impl TransportManager {
    pub fn new(pgns: &[PGN]) -> Self {
        Self {
            transport_packager: TransportPackager::new(),
            fast_packet: FastPacketCoder::new(pgns),
        }
    }

    pub fn is_tp_frame(&self, pgn: PGN) -> bool {
        pgn == PGN_ETP_CM
            || pgn == PGN_ETP_DT
            || pgn == PGN_TP_CM
            || pgn == PGN_TP_DT
            || self.fast_packet.is_fastpacket(&pgn)
    }

    pub fn handle_frame<CanDriver: embedded_can::blocking::Can>(
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
            _ if self.fast_packet.is_fastpacket(&header.pgn()) => {
                result = self.fast_packet.handle_frame(header, data)
            }
            _ => panic!("Invalid PGN handled by Transportmanager"),
        }
        result
    }

    pub fn process<CanDriver: embedded_can::blocking::Can>(&mut self, can_driver: &mut CanDriver) {
        self.transport_packager.process_out_transfers(can_driver);
        self.fast_packet.process_out_transfers(can_driver);
    }

    pub fn send_frame<CanDriver: embedded_can::blocking::Can>(
        &mut self,
        frame: Frame,
        can_driver: &mut CanDriver,
    ) {
        if self.fast_packet.is_fastpacket(&frame.header().pgn()) {
            self.fast_packet.send_frame(frame, can_driver)
        } else if frame.data().len() > 1785 {
            todo!("ETP not yet supported");
        } else {
            self.transport_packager.new_out_transfer(frame, can_driver)
        }
    }
}
