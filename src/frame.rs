use smallvec::SmallVec;

/// PGN contains a unique id, describing the content of a J1939 frame
#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct PGN(pub u32);

impl PGN {
    /// Creates a new PGN from a u32 number
    /// Attention: Currently no checks are in place if this PGN is valid
    pub const fn new(pgn: u32) -> Self {
        Self(pgn)
    }
    /// Get a PGN as u32
    pub const fn raw(&self) -> u32 {
        self.0
    }
    /// Checks if the pgn is a broadcast pgn as defined by the j1939 standard
    pub const fn is_broadcast(&self) -> bool {
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
    pub(crate) pgn: PGN,
    pub(crate) priority: u8,
    pub(crate) source_address: u8,
    pub(crate) destination_address: Option<u8>,
}

impl Header {
    /// Creates a new header
    /// destination address must only be None if the PGN is a broadcast pgn
    pub const fn new(
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
    /// Returns the PGN
    pub const fn pgn(&self) -> PGN {
        self.pgn
    }
    /// Returns the priority
    pub const fn priority(&self) -> u8 {
        self.priority
    }
    /// Returns the source address
    pub const fn source_address(&self) -> u8 {
        self.source_address
    }
    /// Returns the destination address
    pub const fn destination_address(&self) -> Option<u8> {
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
    data: SmallVec<[u8; 8]>,
}

impl Frame {
    /// Creates a new Frame with given Header and data
    /// The data is copied
    pub fn new(header: Header, data: &[u8]) -> Self {
        Self {
            header,
            data: SmallVec::from_slice(data),
        }
    }
    /// Returns frame header
    pub const fn header(&self) -> &Header {
        &self.header
    }
    /// Returns a view of the frame data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub(crate) fn update_source_address(&mut self, address: u8) {
        self.header.source_address = address;
    }

    pub(crate) fn can<CanFrame: embedded_can::Frame>(self) -> CanFrame {
        let id: u32 = (*self.header()).into();
        CanFrame::new(embedded_can::ExtendedId::new(id).unwrap(), self.data()).unwrap()
    }
}

/// A J1939 Frame specialized to request a specific PGN to be send on the bus
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Request {
    header: Header,
    pgn: PGN,
}

impl Request {
    /// Creates a new Request Frame
    /// pgn is the requested PGN
    /// destination address can either a control function which provides the pgn or 0xFF address all ECUs
    /// source address should be the valid local address of a control function
    pub const fn new(pgn: PGN, source_address: u8, destination_address: u8) -> Self {
        let header = Header {
            pgn: PGN_REQUEST,
            priority: 3,
            source_address,
            destination_address: Some(destination_address),
        };
        Self { header, pgn }
    }
    /// Returns the [Header]
    pub const fn header(&self) -> &Header {
        &self.header
    }
    /// Returns the requested PGN by this Request Frame
    pub const fn pgn(&self) -> &PGN {
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
    fn from(req: Request) -> Self {
        let bytes: [u8; 4] = req.pgn().raw().to_le_bytes();
        Self::new(req.header, &bytes[0..3])
    }
}

/// Acknowledgement as response for a request
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Ack {
    header: Header,
    ack_type: AckType,
    group_function_value: Option<u8>,
    address: u8,
    requested_pgn: PGN,
}

/// Ack message type
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AckType {
    /// Request was successful
    PositiveAck,
    /// Request was not successful
    NegativeAck,
    /// requester has no access
    AccessDenied,
    /// unable to respond to the request
    CannotRespond,
    /// Other ack types
    Other(u8),
}
impl From<u8> for AckType {
    fn from(raw: u8) -> Self {
        match raw {
            0 => AckType::PositiveAck,
            1 => AckType::NegativeAck,
            2 => AckType::AccessDenied,
            3 => AckType::CannotRespond,
            _ => AckType::Other(raw),
        }
    }
}
impl From<AckType> for u8 {
    fn from(ack_type: AckType) -> Self {
        match ack_type {
            AckType::PositiveAck => 0,
            AckType::NegativeAck => 1,
            AckType::AccessDenied => 2,
            AckType::CannotRespond => 3,
            AckType::Other(raw) => raw,
        }
    }
}
impl Ack {
    /// Creates a new ACK Frame
    /// pgn is the requested PGN
    /// destination address can either a control function which provides the pgn or 0xFF address all ECUs
    /// the destination address will be used in the j1939 header and the address field of the ack message
    /// source address should be the valid local address of a control function
    pub const fn new(
        ack_type: AckType,
        group_function_value: Option<u8>,
        pgn: PGN,
        source_address: u8,
        destination_address: u8,
    ) -> Self {
        let header = Header {
            pgn: PGN_ACK,
            priority: 3,
            source_address,
            destination_address: Some(destination_address),
        };
        Self {
            header,
            ack_type,
            group_function_value,
            address: destination_address,
            requested_pgn: pgn,
        }
    }
    /// Returns the [Header]
    pub const fn header(&self) -> &Header {
        &self.header
    }
    /// Returns the requested PGN by this Request Frame
    pub const fn pgn(&self) -> &PGN {
        &self.requested_pgn
    }
    /// Returns the type of this Ack message
    pub const fn ack_type(&self) -> &AckType {
        &self.ack_type
    }
    /// Returns the option group function value
    pub const fn group_function_value(&self) -> Option<u8> {
        self.group_function_value
    }
    /// Returns the address whose request is answered by this ack
    pub const fn address(&self) -> u8 {
        self.address
    }
}
impl TryFrom<Frame> for Ack {
    type Error = ();
    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        if frame.header.pgn() == PGN_ACK {
            let mut bytes: [u8; 4] = [0; 4];
            bytes[0..3].copy_from_slice(&frame.data()[5..8]);

            let group_function_value = if frame.data()[1] == 0xFF {
                None
            } else {
                Some(frame.data()[1])
            };
            Ok(Self {
                header: frame.header,
                requested_pgn: PGN::new(u32::from_le_bytes(bytes)),
                ack_type: frame.data()[0].into(),
                group_function_value,
                address: frame.data()[4],
            })
        } else {
            Err(())
        }
    }
}

impl From<Ack> for Frame {
    fn from(ack: Ack) -> Self {
        let mut bytes = [0xFF; 8];
        bytes[0] = ack.ack_type.into();
        bytes[1] = ack.group_function_value().unwrap_or(0xFF);
        bytes[4] = ack.address;
        bytes[5..8].copy_from_slice(&ack.pgn().raw().to_le_bytes()[0..3]);
        Self::new(ack.header, &bytes)
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
