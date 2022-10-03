use alloc::vec::Vec;

/// PGN contains a unique id, describing the content of a J1939 frame
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PGN(pub u32);

impl PGN {
    /// Creates a new PGN from a u32 number
    /// Attention: Currently no checks are in place if this PGN is valid
    pub fn new(pgn: u32) -> Self {
        Self(pgn)
    }
    /// Get a PGN as u32
    pub fn raw(&self) -> u32 {
        self.0
    }
    /// Checks if the pgn is a broadcast pgn as definied by the j1939 standard
    pub fn is_broadcast(&self) -> bool {
        ((self.0 >> 8) & 0xFF) > 240
    }
}
/// Transport Protocol Control Flow PGN
pub const PGN_TP_CM: PGN = PGN(0xEC00);
/// Transport Protocol Data Transport PGN
pub const PGN_TP_DT: PGN = PGN(0xEB00);
/// Extended Transport Protocol Control Flow PGN
/// Specific to ISO11783
pub const PGN_ETP_CM: PGN = PGN(0xC800);
/// Extended Transport Protocol Data Transport PGN
/// Specific to ISO11783
pub const PGN_ETP_DT: PGN = PGN(0xC700);
/// Address claim PGN
pub const PGN_ADDRESSCLAIM: PGN = PGN(0xEE00);
/// Address Command PGN
pub const PGN_ADDRESSCOMMAND: PGN = PGN(0xFED8);
/// PGN Request PGN
pub const PGN_REQUEST: PGN = PGN(0xEA00);
/// ACK PGN
pub const PGN_ACK: PGN = PGN(0xE800);

/// Header of a decoded J1939 Frame
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Header {
    pgn: PGN,
    priority: u8,
    source_address: u8,
    destination_address: Option<u8>,
}

impl Header {
    /// Creates a new header
    /// destination address must only be None if the PGN is a broadcast pgn
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
    /// Returs the PGN
    pub fn pgn(&self) -> PGN {
        self.pgn
    }
    /// Returns the priority
    pub fn priority(&self) -> u8 {
        self.priority
    }
    /// Returns the source address
    pub fn source_address(&self) -> u8 {
        self.source_address
    }
    /// Returns the destinaation address
    pub fn destination_address(&self) -> Option<u8> {
        self.destination_address
    }
}

impl From<u32> for Header {
    fn from(id: u32) -> Self {
        let (pgn, da) = if ((id >> 16) & 0xFF) >= 240 {
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
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Frame {
    header: Header,
    data: Vec<u8>,
}

impl Frame {
    /// Creates a new Frame with given Header and data
    /// The data is copied
    pub fn new(header: Header, data: &[u8]) -> Self {
        Frame {
            header,
            data: data.to_vec(),
        }
    }
    /// Returns frame header
    pub fn header(&self) -> &Header {
        &self.header
    }
    /// Returns a view of the frame data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub(crate) fn can<CanFrame: embedded_can::Frame>(self) -> CanFrame {
        let id: u32 = (*self.header()).into();
        CanFrame::new(embedded_can::ExtendedId::new(id).unwrap(), self.data()).unwrap()
    }
}

/// A J1939 Frame specalized to request a specific PGN to be send on the bus
pub struct Request {
    header: Header,
    pgn: PGN,
}

impl Request {
    /// Creates a new Request Frame
    /// pgn is the requested PGN
    /// destination address can either a control function which provides the pgn or 0xFF address all ECUs
    /// source address should be the valid local address of a control function
    pub fn new(pgn: PGN, source_address: u8, destination_address: u8) -> Self {
        let header = Header {
            pgn: PGN_REQUEST,
            priority: 3,
            source_address,
            destination_address: Some(destination_address),
        };
        Self { header, pgn }
    }
    /// Returns the [Header]
    pub fn header(&self) -> &Header {
        &self.header
    }
    /// Returns the requested PGN by this Request Frame
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
            data: Vec::from(&bytes[0..3]),
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
