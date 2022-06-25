use alloc::vec::Vec;
/// PGN wraps the pgn of a j1939 frame
/// pgn contains a unique id, describing the content of a j1939 frame
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PGN(pub u32);

impl PGN {
    pub fn new(pgn: u32) -> Self {
        Self(pgn)
    }

    pub fn raw(&self) -> u32 {
        self.0
    }

    pub fn is_broadcast(&self) -> bool {
        ((self.0 >> 8) & 0xFF) > 240
    }
}

pub const PGN_TP_CM: PGN = PGN(0xEC00);
pub const PGN_TP_DT: PGN = PGN(0xEB00);
pub const PGN_ETP_CM: PGN = PGN(0xC800);
pub const PGN_ETP_DT: PGN = PGN(0xC700);
pub const PGN_ADDRESSCLAIM: PGN = PGN(0xEE00);
pub const PGN_ADDRESSCOMMAND: PGN = PGN(0xFED8);
pub const PGN_REQUEST: PGN = PGN(0xEA00);
pub const PGN_ACK: PGN = PGN(0xE800);

/// Header of a decoded J1939 Frame
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Header {
    pgn: PGN,
    priority: u8,
    source_address: u8,
    destination_address: Option<u8>,
}

impl Header {
    pub fn new(
        pgn: PGN,
        priority: u8,
        source_address: u8,
        destination_address: Option<u8>,
    ) -> Self {
        Self {
            pgn,
            priority,
            source_address,
            destination_address,
        }
    }

    pub fn pgn(&self) -> PGN {
        self.pgn
    }
    pub fn priority(&self) -> u8 {
        self.priority
    }
    pub fn source_address(&self) -> u8 {
        self.source_address
    }
    pub fn destination_address(&self) -> Option<u8> {
        self.destination_address
    }
}

impl From<u32> for Header {
    fn from(id: u32) -> Self {
        let (pgn, da) = if ((id >> 16) & 0xFF) > 240 {
            // broadcast
            (PGN(id >> 8 & 0x3FFFF), None)
        } else {
            // peer to peer
            let pgn = PGN(id >> 8 & 0x3FF00);
            let da: u8 = (id >> 8) as u8;
            (pgn, Some(da))
        };
        let sa = id as u8;
        let priority = (id >> 26) as u8 & 0x7;
        Self {
            destination_address: da,
            pgn,
            priority,
            source_address: sa,
        }
    }
}
impl From<Header> for u32 {
    fn from(header: Header) -> u32 {
        let mut id = header.source_address() as u32;
        id |= (header.pgn().raw()) << 8;
        if let Some(da) = header.destination_address() {
            id |= (da as u32) << 8;
        }
        id |= (header.priority() as u32) << 26;
        id
    }
}

/// Decoded J1929 Frame.
/// If the data length is higher than 8, this frame is disassembled for transport over can
#[derive(Debug, PartialEq, Clone)]
pub struct Frame {
    header: Header,
    data: Vec<u8>,
}

impl Frame {
    pub fn new(header: Header, data: &[u8]) -> Self {
        Frame {
            header,
            data: data.to_vec(),
        }
    }
    pub fn header(&self) -> &Header {
        &self.header
    }
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn can<CanFrame: embedded_hal::can::Frame>(self) -> CanFrame {
        let id: u32 = (*self.header()).into();
        CanFrame::new(
            embedded_hal::can::ExtendedId::new(id).unwrap(),
            self.data(),
        ).unwrap()
    }
}

pub struct Request {
    header: Header,
    pgn: PGN,
}

impl Request {
    pub fn header(&self) -> &Header {
        &self.header
    }
    pub fn pgn(&self) -> &PGN {
        &self.pgn
    }
}
impl TryFrom<Frame> for Request {
    type Error = ();
    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        if frame.header.pgn() == PGN_REQUEST {
            let mut bytes: [u8; 4] = [0; 4];
            bytes[0..3].copy_from_slice(frame.data());
            Ok(Self {
                header: frame.header,
                pgn: PGN::new(u32::from_le_bytes(bytes)),
            })
        } else {
            Err(())
        }
    }
}

impl From<Request> for Frame {
    fn from(req: Request) -> Frame {
        let bytes: [u8; 4] = req.pgn().raw().to_le_bytes();
        Frame {
            header: req.header,
            data: Vec::from(bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::frame::*;
    #[test]
    fn pgn() {
        let pgn = PGN::new(20);
        assert_eq!(pgn.raw(), 20);
    }
    #[test]
    fn header() {
        let header = Header::new(PGN::new(20), 2, 18, Some(50));
        assert_eq!(header.pgn().raw(), 20);
        assert_eq!(header.priority(), 2);
        assert_eq!(header.source_address(), 18);
        assert_eq!(header.destination_address(), Some(50));
    }

    #[test]
    fn broadcast_header() {
        let id: u32 = 0x3FF2032;
        let header = Header {
            pgn: PGN(0x3FF20),
            priority: 0,
            source_address: 0x32,
            destination_address: None,
        };
        assert_eq!(header, Header::from(id));
        assert_eq!(id, header.into());
    }

    #[test]
    fn p2p_header() {
        let id: u32 = 0x142F1810;
        let header = Header {
            pgn: PGN(0x02F00),
            priority: 5,
            source_address: 0x10,
            destination_address: Some(0x18),
        };
        assert_eq!(header, Header::from(id));
        assert_eq!(id, header.into());
    }
}
