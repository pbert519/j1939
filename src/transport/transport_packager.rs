use crate::frame::{Frame, Header, PGN};
use crate::transport::tp_frames::*;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

struct BroadcastReceier {
    pub data: Vec<u8>,
    pub pgn: PGN,
    pub priority: u8,
    pub last_packet_index: u8,
}
struct BroadcastSender {
    pub pdu: Frame,
    pub last_packet_index: u8,
    pub packet_count: u8,
}

struct P2PReceier {
    pub data: Vec<u8>,
    pub pgn: PGN,
    pub priority: u8,
    pub max_packets_per_cts: u8,
    pub last_packet_index: u8,
    pub last_requested_index: u8,
}

struct P2PSender {
    pub pdu: Frame,
    pub last_packet_index: u8,
    pub send_till_index: u8,
}

pub struct TransportPackager {
    in_broadcast: BTreeMap<u8, BroadcastReceier>,
    out_broadcast: Option<BroadcastSender>,
    in_p2p: BTreeMap<(u8, u8), P2PReceier>,
    out_p2p: BTreeMap<(u8, u8), P2PSender>,
}

impl TransportPackager {
    pub fn new() -> Self {
        TransportPackager {
            in_broadcast: BTreeMap::new(),
            out_broadcast: None,
            in_p2p: BTreeMap::new(),
            out_p2p: BTreeMap::new(),
        }
    }

    pub fn process_tpcm<CanDriver: embedded_can::blocking::Can>(
        &mut self,
        tpcm: TPCM,
        can_driver: &mut CanDriver,
    ) {
        match tpcm {
            TPCM::Rts {
                message_size,
                packet_count: _,
                max_packets_per_cts,
                pgn,
                remote_address,
                local_address,
            } => {
                if let alloc::collections::btree_map::Entry::Vacant(e) =
                    self.in_p2p.entry((remote_address, local_address))
                {
                    e.insert(P2PReceier {
                        data: Vec::with_capacity(message_size as usize),
                        pgn,
                        priority: 0,
                        max_packets_per_cts,
                        last_packet_index: 0,
                        last_requested_index: 0,
                    });

                    let cts = TPCM::Cts {
                        expected_packets: self.in_p2p[&(remote_address, local_address)]
                            .max_packets_per_cts,
                        next_packet_number: 1,
                        pgn: self.in_p2p[&(remote_address, local_address)].pgn,
                        remote_address,
                        local_address,
                    };
                    can_driver
                        .transmit(&Frame::from(cts).can())
                        .expect("Can Transmit Error!");
                } else {
                    let abort = TPCM::Abort {
                        abort_reason: AbortReason::AlreadyConnected,
                        pgn,
                        remote_address,
                        local_address,
                    };
                    can_driver
                        .transmit(&Frame::from(abort).can())
                        .expect("Can Transmit Error!");
                }
            }
            TPCM::Cts {
                expected_packets,
                next_packet_number,
                pgn: _,
                remote_address,
                local_address,
            } => {
                // check pgn
                if let Some(sender) = self.out_p2p.get_mut(&(local_address, remote_address)) {
                    sender.last_packet_index = next_packet_number - 1;
                    sender.send_till_index = sender.last_packet_index + expected_packets;
                }
            }
            TPCM::EndOfMsg {
                message_size: _,
                packet_count: _,
                pgn: _,
                remote_address,
                local_address,
            } => {
                // check pgn
                self.out_p2p.remove(&(remote_address, local_address));
            }
            TPCM::Abort {
                abort_reason: _,
                pgn,
                remote_address,
                local_address,
            } => {
                // ToDo reciver or sender abort -> check pgn?
                if let Some(transfer) = self.in_p2p.get(&(local_address, remote_address)) {
                    if transfer.pgn == pgn {
                        self.in_p2p.remove(&(local_address, remote_address));
                    }
                }
                if let Some(transfer) = self.out_p2p.get(&(remote_address, local_address)) {
                    if transfer.pdu.header().pgn() == pgn {
                        self.out_p2p.remove(&(remote_address, local_address));
                    }
                }
            }
            TPCM::Bam {
                message_size,
                packet_count: _,
                pgn,
                remote_address,
                local_address: _,
            } => {
                self.in_broadcast
                    .entry(remote_address)
                    .or_insert(BroadcastReceier {
                        data: Vec::with_capacity(message_size as usize),
                        pgn,
                        priority: 0,
                        last_packet_index: 0,
                    });
            }
        }
    }

    pub fn process_tpdt<CanDriver: embedded_can::blocking::Can>(
        &mut self,
        tpdt: TPDT,
        can_driver: &mut CanDriver,
    ) -> Option<Frame> {
        let mut result = None;
        if tpdt.local_address == 0xFF {
            if let Some(rec) = &mut self.in_broadcast.get_mut(&tpdt.remote_address) {
                if rec.last_packet_index + 1 == tpdt.sequence_number {
                    rec.last_packet_index += 1;
                    let missing_bytes = rec.data.capacity() - rec.data.len();
                    if missing_bytes <= 8 {
                        // last packet
                        rec.data.extend_from_slice(&tpdt.data[0..missing_bytes]);
                        // finialize packet
                        let entry = self.in_broadcast.remove(&tpdt.remote_address).unwrap();
                        result = Some(Frame::new(
                            Header::new(
                                entry.pgn,
                                entry.priority,
                                tpdt.remote_address,
                                Some(tpdt.local_address),
                            ),
                            &entry.data,
                        ));
                    } else {
                        rec.data.extend_from_slice(&tpdt.data)
                    }
                } else {
                    self.in_broadcast.remove(&tpdt.remote_address);
                }
            }
        } else if let Some(rec) = &mut self
            .in_p2p
            .get_mut(&(tpdt.remote_address, tpdt.local_address))
        {
            if rec.last_packet_index + 1 == tpdt.sequence_number {
                rec.last_packet_index += 1;
                let missing_bytes = rec.data.capacity() - rec.data.len();
                if missing_bytes <= 8 {
                    // last packet
                    rec.data.extend_from_slice(&tpdt.data[0..missing_bytes]);
                    // finialize packet
                    let entry = self
                        .in_p2p
                        .remove(&(tpdt.remote_address, tpdt.local_address))
                        .unwrap();
                    let received_bytes = entry.data.len();
                    result = Some(Frame::new(
                        Header::new(
                            entry.pgn,
                            entry.priority,
                            tpdt.remote_address,
                            Some(tpdt.local_address),
                        ),
                        &entry.data,
                    ));
                    let ack = TPCM::EndOfMsg {
                        message_size: received_bytes as u16,
                        packet_count: entry.last_packet_index,
                        pgn: entry.pgn,
                        remote_address: tpdt.remote_address,
                        local_address: tpdt.local_address,
                    };
                    can_driver
                        .transmit(&Frame::from(ack).can())
                        .expect("Can Transmit Error!");
                } else {
                    rec.data.extend_from_slice(&tpdt.data);
                    if rec.last_requested_index + rec.max_packets_per_cts == rec.last_packet_index {
                        let cts = TPCM::Cts {
                            expected_packets: rec.max_packets_per_cts,
                            next_packet_number: rec.last_packet_index + 1,
                            pgn: rec.pgn,
                            remote_address: tpdt.remote_address,
                            local_address: tpdt.local_address,
                        };
                        rec.last_requested_index += rec.max_packets_per_cts;
                        can_driver
                            .transmit(&Frame::from(cts).can())
                            .expect("Can Transmit Error!");
                    }
                }
            } else {
                // Abort wrong Sequence Number
                let abort = TPCM::Abort {
                    abort_reason: AbortReason::UnexpectedTransfer,
                    pgn: rec.pgn, // PGN is not known
                    remote_address: tpdt.remote_address,
                    local_address: tpdt.local_address,
                };
                can_driver
                    .transmit(&Frame::from(abort).can())
                    .expect("Can Transmit Error!");
                self.in_broadcast.remove(&tpdt.remote_address);
            }
        } else {
            // Abort unexpected transfer
            let abort = TPCM::Abort {
                abort_reason: AbortReason::UnexpectedTransfer,
                pgn: PGN::new(0xFFFFFFFF), // PGN is not known
                remote_address: tpdt.remote_address,
                local_address: tpdt.local_address,
            };
            can_driver
                .transmit(&Frame::from(abort).can())
                .expect("Can Transmit Error!");
        }
        result
    }

    pub fn new_out_transfer<CanDriver: embedded_can::blocking::Can>(
        &mut self,
        pdu: Frame,
        can_driver: &mut CanDriver,
    ) {
        let bytes_to_send = pdu.data().len() as u16;
        let packets_to_send = ((pdu.data().len() + 7/* ceil division */) / 8) as u8;

        if pdu.header().pgn().is_broadcast() || pdu.header().destination_address() == Some(0xFF) {
            // create bam transfer
            let bam = TPCM::Bam {
                message_size: bytes_to_send,
                packet_count: packets_to_send,
                pgn: pdu.header().pgn(),
                remote_address: 0xFF,
                local_address: pdu.header().source_address(),
            };
            can_driver
                .transmit(&Frame::from(bam).can())
                .expect("Can Transmit Error!");
            self.out_broadcast = Some(BroadcastSender {
                pdu,
                last_packet_index: 0,
                packet_count: packets_to_send,
            });
        } else {
            let rts = TPCM::Rts {
                message_size: bytes_to_send,
                packet_count: packets_to_send,
                max_packets_per_cts: 1,
                pgn: pdu.header().pgn(),
                remote_address: pdu.header().destination_address().unwrap(),
                local_address: pdu.header().source_address(),
            };
            can_driver
                .transmit(&Frame::from(rts).can())
                .expect("Can Transmit Error!");
            self.out_p2p.insert(
                (
                    pdu.header().source_address(),
                    pdu.header().destination_address().unwrap(),
                ),
                P2PSender {
                    pdu,
                    last_packet_index: 0,
                    send_till_index: 0,
                },
            );
        }
    }

    pub fn process_out_transfers<CanDriver: embedded_can::blocking::Can>(
        &mut self,
        can_driver: &mut CanDriver,
    ) {
        // process broadcasts
        if let Some(sender) = &mut self.out_broadcast {
            let mut data = [0xFF; 7];
            let start = sender.last_packet_index as usize * 7;
            let stop = (start + 7).min(sender.pdu.data().len());
            data[0..(stop - start)].copy_from_slice(&sender.pdu.data()[start..stop]);
            let tpdt = TPDT {
                remote_address: 0xFF,
                local_address: sender.pdu.header().source_address(),
                sequence_number: sender.last_packet_index + 1,
                data,
            };
            sender.last_packet_index += 1;
            can_driver
                .transmit(&Frame::from(tpdt).can())
                .expect("Can Transmit Error!");
            if sender.last_packet_index >= sender.packet_count {
                self.out_broadcast = None;
            }
        }
        // process peer to peer transfers
        for sender in self.out_p2p.values_mut() {
            // we can send something
            if sender.last_packet_index < sender.send_till_index {
                let mut data = [0xFF; 7];
                let start = sender.last_packet_index as usize * 7;
                let stop = (start + 7).min(sender.pdu.data().len());
                data[0..(stop - start)].copy_from_slice(&sender.pdu.data()[start..stop]);

                let tpdt = TPDT {
                    remote_address: sender.pdu.header().destination_address().unwrap(),
                    local_address: sender.pdu.header().source_address(),
                    sequence_number: sender.last_packet_index + 1,
                    data,
                };
                sender.last_packet_index += 1;
                can_driver
                    .transmit(&Frame::from(tpdt).can())
                    .expect("Can Transmit Error!");
            }
        }
    }
}
