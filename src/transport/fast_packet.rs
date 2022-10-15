use crate::frame::*;
use alloc::{collections::BTreeMap, vec::Vec};

/// A de- and encoder for NEMA2000 PGNs using the fast packet transport protocol
pub struct FastPacketCoder {
    pgns: Vec<PGN>,
    receiver: BTreeMap<PGN, Receiver>,
    transmitter: BTreeMap<PGN, Transmitter>,
    last_used_sequence: BTreeMap<PGN, u8>,
}

struct Receiver {
    expected_bytes: u8,
    sequence: u8,
    data: Vec<u8>,
}
struct Transmitter {
    frame: Frame,
    item: u8,
    sequence: u8,
}

impl FastPacketCoder {
    pub fn new(pgns: &[PGN]) -> Self {
        FastPacketCoder {
            pgns: pgns.to_vec(),
            receiver: BTreeMap::new(),
            transmitter: BTreeMap::new(),
            last_used_sequence: BTreeMap::new(),
        }
    }

    pub fn is_fastpacket(&self, pgn: &PGN) -> bool {
        self.pgns.contains(pgn)
    }

    pub fn handle_frame(&mut self, header: Header, data: &[u8]) -> Option<Frame> {
        let identifier = data[0];
        // extract sequence identifier
        let sequence = (identifier & 0xE0) >> 5;
        // extract item identifier
        let item = identifier & 0x1F;

        // is this the first message of a transfer?
        if item == 0 {
            let expected_bytes = data[1];
            // create new receiver, if pgn is not already received
            if let alloc::collections::btree_map::Entry::Vacant(e) =
                self.receiver.entry(header.pgn())
            {
                let mut rx_bytes = Vec::with_capacity(expected_bytes as usize);
                rx_bytes.extend_from_slice(&data[2..]);
                e.insert(Receiver {
                    expected_bytes,
                    sequence,
                    data: rx_bytes,
                });
            }
        } else {
            // get receiver for given PGN, if no receiver was created ignore message
            if let Some(rec) = self.receiver.get_mut(&header.pgn()) {
                // check for correct sequence else ignore message
                if rec.sequence == sequence {
                    let copy_till = 8.min((rec.expected_bytes as usize - rec.data.len()) + 1);
                    rec.data.extend_from_slice(&data[1..copy_till]);

                    // check if receive is done
                    if rec.data.len() >= rec.expected_bytes as usize {
                        let entry = self.receiver.remove(&header.pgn()).unwrap();
                        return Some(Frame::new(header, &entry.data));
                    }
                }
            }
        }
        None
    }

    pub fn process_out_transfers<CanDriver: embedded_can::blocking::Can>(
        &mut self,
        can_driver: &mut CanDriver,
    ) {
        // ToDo: Refactor this
        let mut finished = Vec::new();

        for transmitter in self.transmitter.values_mut() {
            let bytes_send_min = 6 + transmitter.item as usize * 7;
            let bytes_send_max = transmitter.frame.data().len().min(bytes_send_min + 7);
            let bytes_to_copy = bytes_send_max - bytes_send_min;

            transmitter.item += 1;
            let mut data = [0xFF; 8];
            data[0] = transmitter.sequence << 5 | (transmitter.item & 0x1F);

            data[1..1 + bytes_to_copy]
                .copy_from_slice(&transmitter.frame.data()[bytes_send_min..bytes_send_max]);
            let id = embedded_can::ExtendedId::new((*transmitter.frame.header()).into()).unwrap();
            can_driver
                .transmit(&embedded_can::Frame::new(id, &data).unwrap())
                .unwrap();

            if bytes_send_max >= transmitter.frame.data().len() {
                let pgn = transmitter.frame.header().pgn();
                finished.push(pgn);
            }
        }

        for f in finished {
            self.transmitter.remove(&f);
        }
    }

    pub fn send_frame<CanDriver: embedded_can::blocking::Can>(
        &mut self,
        pdu: Frame,
        can_driver: &mut CanDriver,
    ) {
        let sequence = self
            .last_used_sequence
            .entry(pdu.header().pgn())
            .or_default();
        let item = 0;
        let bytes = pdu.data().len();

        // ignore packet if pgn is alredy send
        if let alloc::collections::btree_map::Entry::Vacant(e) =
            self.transmitter.entry(pdu.header().pgn())
        {
            let mut data = [0xFF; 8];
            data[0] = *sequence << 5 | (item & 0x1F);
            data[1] = bytes as u8;
            data[2..8].copy_from_slice(&pdu.data()[0..6]);
            let id = embedded_can::ExtendedId::new((*pdu.header()).into()).unwrap();
            can_driver
                .transmit(&embedded_can::Frame::new(id, &data).unwrap())
                .unwrap();

            e.insert(Transmitter {
                frame: pdu,
                sequence: *sequence,
                item,
            });
            *sequence += 1;
        }
    }
}
