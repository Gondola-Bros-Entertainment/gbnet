// wire.rs - Shared packet sending utilities
use std::net::SocketAddr;

use crate::packet::{Packet, PacketHeader, PacketType};
use crate::security;
use crate::socket::UdpSocket;

/// Serialize and send a control packet with CRC32 appended.
/// Used by both NetServer and NetClient for handshake/control packets.
pub fn send_raw_packet(
    socket: &mut UdpSocket,
    addr: SocketAddr,
    protocol_id: u32,
    sequence: u16,
    packet_type: PacketType,
) {
    let header = PacketHeader {
        protocol_id,
        sequence,
        ack: 0,
        ack_bits: 0,
    };
    let packet = Packet::new(header, packet_type);
    if let Ok(data) = packet.serialize() {
        let mut data_with_crc = data;
        security::append_crc32(&mut data_with_crc);
        if let Err(e) = socket.send_to(&data_with_crc, addr) {
            log::warn!("Failed to send raw packet to {}: {:?}", addr, e);
        }
    }
}
