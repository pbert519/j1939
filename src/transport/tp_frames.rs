use crate::frame::{Frame, Header, PGN, PGN_TP_CM, PGN_TP_DT};
const ADDRESS_GLOBAL: u8 = 0xFF;

// ------------------------------------------------- TP DT ---------------------------------------
#[derive(Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub struct TPDT {
    pub remote_address: u8,
    pub local_address: u8,
    pub sequence_number: u8,
    pub data: [u8; 7],
}

impl From<TPDT> for Frame {
    fn from(tpdt: TPDT) -> Self {
        let mut data = [0xFF; 8];
        data[0] = tpdt.sequence_number;
        data[1..8].copy_from_slice(&tpdt.data);
        Frame::new(
            Header::new(PGN_TP_DT, 7, tpdt.local_address, Some(tpdt.remote_address)),
            &data,
        )
    }
}

impl TPDT {
    pub fn from_frame(header: Header, data: &[u8]) -> Self {
        Self {
            remote_address: header.source_address(),
            local_address: header.destination_address().unwrap(),
            sequence_number: data[0],
            data: data[1..8].try_into().unwrap(),
        }
    }
}

// ------------------------------------------------- TP CM ---------------------------------------

#[derive(Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum TPCM {
    Rts {
        message_size: u16,
        packet_count: u8,
        max_packets_per_cts: u8,
        pgn: PGN,
        remote_address: u8,
        local_address: u8,
    },
    Cts {
        expected_packets: u8,
        next_packet_number: u8,
        pgn: PGN,
        remote_address: u8,
        local_address: u8,
    },
    EndOfMsg {
        message_size: u16,
        packet_count: u8,
        pgn: PGN,
        remote_address: u8,
        local_address: u8,
    },
    Abort {
        abort_reason: AbortReason,
        pgn: PGN,
        remote_address: u8,
        local_address: u8,
    },
    Bam {
        message_size: u16,
        packet_count: u8,
        pgn: PGN,
        remote_address: u8,
        local_address: u8,
    },
}

const CTRL_BAM: u8 = 32;
const CTRL_CONN_ABORT: u8 = 255;
const CTRL_END_OF_MSG_ACK: u8 = 19;
const CTRL_CTS: u8 = 17;
const CTRL_RTS: u8 = 16;

impl From<TPCM> for Frame {
    fn from(tpcm: TPCM) -> Self {
        let mut data = [0xFF; 8];
        let sa;
        let da;
        match tpcm {
            TPCM::Rts {
                message_size,
                packet_count,
                max_packets_per_cts,
                pgn,
                remote_address,
                local_address,
            } => {
                data[0] = CTRL_RTS;
                data[1] = message_size as u8;
                data[2] = (message_size >> 8) as u8;
                data[3] = packet_count;
                data[4] = max_packets_per_cts;
                data[5] = pgn.raw() as u8;
                data[6] = (pgn.raw() >> 8) as u8;
                data[7] = (pgn.raw() >> 16) as u8;
                sa = local_address;
                da = remote_address;
            }
            TPCM::Cts {
                expected_packets,
                next_packet_number,
                pgn,
                remote_address,
                local_address,
            } => {
                data[0] = CTRL_CTS;
                data[1] = expected_packets;
                data[2] = next_packet_number;
                data[5] = pgn.raw() as u8;
                data[6] = (pgn.raw() >> 8) as u8;
                data[7] = (pgn.raw() >> 16) as u8;
                sa = local_address;
                da = remote_address;
            }
            TPCM::EndOfMsg {
                message_size,
                packet_count,
                pgn,
                remote_address,
                local_address,
            } => {
                data[0] = CTRL_END_OF_MSG_ACK;
                data[1] = message_size as u8;
                data[2] = (message_size >> 8) as u8;
                data[3] = packet_count;
                data[5] = pgn.raw() as u8;
                data[6] = (pgn.raw() >> 8) as u8;
                data[7] = (pgn.raw() >> 16) as u8;
                sa = local_address;
                da = remote_address;
            }
            TPCM::Abort {
                abort_reason,
                pgn,
                remote_address,
                local_address,
            } => {
                data[0] = CTRL_CONN_ABORT;
                data[1] = abort_reason as u8;
                data[5] = pgn.raw() as u8;
                data[6] = (pgn.raw() >> 8) as u8;
                data[7] = (pgn.raw() >> 16) as u8;
                sa = local_address;
                da = remote_address;
            }
            TPCM::Bam {
                message_size,
                packet_count,
                pgn,
                remote_address,
                local_address,
            } => {
                data[0] = CTRL_BAM;
                data[1] = message_size as u8;
                data[2] = (message_size >> 8) as u8;
                data[3] = packet_count;
                data[5] = pgn.raw() as u8;
                data[6] = (pgn.raw() >> 8) as u8;
                data[7] = (pgn.raw() >> 16) as u8;
                sa = local_address;
                da = remote_address;
            }
        };
        Frame::new(Header::new(PGN_TP_CM, 7, sa, Some(da)), &data)
    }
}

impl TPCM {
    pub fn from_frame(header: Header, data: &[u8]) -> Self {
        let data_pgn: PGN = PGN::new(u32::from_le_bytes([data[5], data[6], data[7], 0x00]));
        if header.destination_address().unwrap() == ADDRESS_GLOBAL && data[0] == CTRL_BAM {
            let bytes = u16::from_le_bytes([data[1], data[2]]);
            TPCM::Bam {
                message_size: bytes,
                packet_count: data[3],
                pgn: data_pgn,
                remote_address: header.source_address(),
                local_address: header.destination_address().unwrap(),
            }
        } else if data[0] == CTRL_RTS {
            let bytes = u16::from_le_bytes([data[1], data[2]]);
            TPCM::Rts {
                message_size: bytes,
                packet_count: data[3],
                max_packets_per_cts: data[4],
                pgn: data_pgn,
                remote_address: header.source_address(),
                local_address: header.destination_address().unwrap(),
            }
        } else if data[0] == CTRL_CTS {
            TPCM::Cts {
                expected_packets: data[1],
                next_packet_number: data[2],
                pgn: data_pgn,
                remote_address: header.source_address(),
                local_address: header.destination_address().unwrap(),
            }
        } else if data[0] == CTRL_END_OF_MSG_ACK {
            let bytes = u16::from_le_bytes([data[1], data[2]]);
            TPCM::EndOfMsg {
                message_size: bytes,
                packet_count: data[3],
                pgn: data_pgn,
                remote_address: header.source_address(),
                local_address: header.destination_address().unwrap(),
            }
        } else if data[0] == CTRL_CONN_ABORT {
            TPCM::Abort {
                abort_reason: from_u8(data[1]),
                pgn: data_pgn,
                remote_address: header.source_address(),
                local_address: header.destination_address().unwrap(),
            }
        } else {
            panic!("TPCM Message with invalid control byte")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    Reserved = 0,
    AlreadyConnected = 1,
    NoResources = 2,
    Timeout = 3,
    CTSWhileTransfer = 4,
    RetransmitLimit = 5,
    UnexpectedTransfer = 6,
    BadSequenceNumber = 7,
    DuplicateSequenceNumber = 8,
    MessageSizeToHigh = 9,
    Other = 250,
}

const fn from_u8(n: u8) -> AbortReason {
    match n {
        0 => AbortReason::Reserved,
        1 => AbortReason::AlreadyConnected,
        2 => AbortReason::NoResources,
        3 => AbortReason::Timeout,
        4 => AbortReason::CTSWhileTransfer,
        5 => AbortReason::RetransmitLimit,
        6 => AbortReason::UnexpectedTransfer,
        7 => AbortReason::BadSequenceNumber,
        8 => AbortReason::DuplicateSequenceNumber,
        9 => AbortReason::MessageSizeToHigh,
        250 => AbortReason::Other,
        _ => AbortReason::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_pdu_tpdt() {
        let frame: Frame = TPDT {
            remote_address: 1,
            local_address: 50,
            sequence_number: 2,
            data: [1, 2, 3, 4, 5, 6, 7],
        }
        .into();
        assert_eq!(
            frame,
            Frame::new(Header::from(0x1CEB0132), &[2, 1, 2, 3, 4, 5, 6, 7])
        )
    }

    #[test]
    fn deserialize_pdu_tpdt() {
        let pdu = TPDT::from_frame(Header::from(0x00EBFF01), &[1, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            pdu,
            TPDT {
                remote_address: 1,
                local_address: 255,
                sequence_number: 1,
                data: [1, 2, 3, 4, 5, 6, 7]
            }
        );
    }

    #[test]
    fn serialize_pdu_tpcm_bam() {
        let frame: Frame = TPCM::Bam {
            message_size: 20,
            packet_count: 3,
            pgn: PGN::new(0xFEB0),
            remote_address: 255,
            local_address: 0x32,
        }
        .into();
        assert_eq!(
            frame,
            Frame::new(
                Header::from(0x1CECFF32),
                &[32, 20, 0, 3, 255, 0xB0, 0xFE, 0]
            )
        )
    }
    #[test]
    fn deserialize_pdu_tpcm_bam() {
        let pdu = TPCM::from_frame(
            Header::from(0x00ECFF01),
            &[32, 20, 0, 3, 255, 0xB0, 0xFE, 0],
        );
        assert_eq!(
            pdu,
            TPCM::Bam {
                message_size: 20,
                packet_count: 3,
                pgn: PGN::new(0xFEB0),
                remote_address: 1,
                local_address: 0xFF,
            }
        );
    }
    #[test]
    fn serialize_pdu_tpcm_rts() {
        let frame: Frame = TPCM::Rts {
            message_size: 20,
            packet_count: 3,
            pgn: PGN::new(0xFEB0),
            remote_address: 2,
            local_address: 0x32,
            max_packets_per_cts: 1,
        }
        .into();
        assert_eq!(
            frame,
            Frame::new(Header::from(0x1CEC0232), &[16, 20, 0, 3, 1, 176, 254, 0])
        )
    }
    #[test]
    fn deserialize_pdu_tpcm_rts() {
        let pdu = TPCM::from_frame(Header::from(0x18EC9B90), &[16, 20, 0, 3, 1, 0, 223, 0]);
        assert_eq!(
            pdu,
            TPCM::Rts {
                message_size: 20,
                packet_count: 3,
                pgn: PGN::new(0xDF00),
                remote_address: 0x90,
                max_packets_per_cts: 1,
                local_address: 0x9B,
            }
        );
    }
    #[test]
    fn serialize_pdu_tpcm_cts() {
        let frame: Frame = TPCM::Cts {
            pgn: PGN::new(0xDF00),
            remote_address: 0x9B,
            expected_packets: 1,
            next_packet_number: 3,
            local_address: 0x90,
        }
        .into();
        assert_eq!(
            frame,
            Frame::new(Header::from(0x1CEC9B90), &[17, 1, 3, 255, 255, 0, 223, 0])
        )
    }
    #[test]
    fn deserialize_pdu_tpcm_cts() {
        let pdu = TPCM::from_frame(Header::from(0x1CEC909B), &[17, 1, 1, 255, 255, 0, 223, 0]);
        assert_eq!(
            pdu,
            TPCM::Cts {
                pgn: PGN::new(0xDF00),
                remote_address: 0x9B,
                expected_packets: 1,
                next_packet_number: 1,
                local_address: 0x90,
            }
        );
    }
    #[test]
    fn serialize_pdu_tpcm_ack() {
        let frame: Frame = TPCM::EndOfMsg {
            message_size: 20,
            packet_count: 3,
            pgn: PGN::new(0xDF00),
            remote_address: 0x9B,
            local_address: 0x90,
        }
        .into();
        assert_eq!(
            frame,
            Frame::new(Header::from(0x1CEC9B90), &[19, 20, 0, 3, 255, 0, 223, 0])
        )
    }
    #[test]
    fn deserialize_pdu_tpcm_ack() {
        let pdu = TPCM::from_frame(Header::from(0x1CEC909B), &[19, 20, 0, 3, 255, 0, 223, 0]);
        assert_eq!(
            pdu,
            TPCM::EndOfMsg {
                message_size: 20,
                packet_count: 3,
                pgn: PGN::new(0xDF00),
                remote_address: 0x9B,
                local_address: 0x90,
            }
        );
    }
    #[test]
    fn serialize_pdu_tpcm_abort() {
        let frame: Frame = TPCM::Abort {
            pgn: PGN::new(0xFEB0),
            remote_address: 0x90,
            local_address: 0x9B,
            abort_reason: AbortReason::AlreadyConnected,
        }
        .into();
        assert_eq!(
            frame,
            Frame::new(
                Header::from(0x1CEC909B),
                &[255, 1, 255, 255, 255, 0xB0, 0xFE, 0]
            )
        )
    }
    #[test]
    fn deserialize_pdu_tpcm_abort() {
        let pdu = TPCM::from_frame(
            Header::from(0x1CEC909B),
            &[255, 1, 255, 255, 255, 0xB0, 0xFE, 0],
        );
        assert_eq!(
            pdu,
            TPCM::Abort {
                pgn: PGN::new(0xFEB0),
                remote_address: 0x9B,
                abort_reason: AbortReason::AlreadyConnected,
                local_address: 0x90,
            }
        );
    }
}
