//! Server-side networking API.
//!
//! [`NetServer`] manages multiple client connections, handles the connection
//! handshake, and dispatches incoming messages as [`ServerEvent`]s.
use rand::random;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use crate::{
    congestion,
    connection::{Connection, ConnectionState, DisconnectReason},
    packet::{deny_reason, disconnect_reason, Packet, PacketType},
    security::{self, ConnectionRateLimiter},
    socket::{SocketError, UdpSocket},
    wire, NetworkConfig, NetworkStats,
};

/// Events emitted by [`NetServer::update`].
#[derive(Debug)]
pub enum ServerEvent {
    ClientConnected(SocketAddr),
    ClientDisconnected(SocketAddr, DisconnectReason),
    Message {
        addr: SocketAddr,
        channel: u8,
        data: Vec<u8>,
    },
}

struct PendingConnection {
    server_salt: u64,
    created_at: Instant,
}

/// A game server that listens for client connections over UDP.
///
/// Call [`NetServer::update`] once per game tick to process packets,
/// send keepalives, and collect events.
pub struct NetServer {
    socket: UdpSocket,
    connections: HashMap<SocketAddr, Connection>,
    pending: HashMap<SocketAddr, PendingConnection>,
    disconnecting: HashMap<SocketAddr, Connection>,
    config: NetworkConfig,
    rate_limiter: ConnectionRateLimiter,
}

impl NetServer {
    /// Bind a server to the given address with the specified configuration.
    pub fn bind(addr: SocketAddr, config: NetworkConfig) -> Result<Self, SocketError> {
        if let Err(e) = config.validate() {
            return Err(SocketError::Other(e.to_string()));
        }
        let socket = UdpSocket::bind(addr)?;
        let rate_limit = config.rate_limit_per_second;
        Ok(Self {
            socket,
            connections: HashMap::new(),
            pending: HashMap::new(),
            disconnecting: HashMap::new(),
            config: config.clone(),
            rate_limiter: ConnectionRateLimiter::new(rate_limit),
        })
    }

    /// Process incoming packets, send keepalives, and return events.
    /// Call this once per game tick.
    pub fn update(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();

        // Collect incoming packets into a buffer first
        let mut incoming: Vec<(SocketAddr, Packet)> = Vec::new();
        loop {
            match self.socket.recv_from() {
                Ok((data, addr)) => {
                    let validated = match security::validate_and_strip_crc32(data) {
                        Some(valid) => valid.to_vec(),
                        None => continue,
                    };
                    let packet = match Packet::deserialize(&validated) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    if packet.header.protocol_id != self.config.protocol_id {
                        continue;
                    }
                    // Track received bytes for connected clients
                    if let Some(conn) = self.connections.get_mut(&addr) {
                        conn.record_bytes_received(validated.len());
                    }
                    incoming.push((addr, packet));
                }
                Err(SocketError::WouldBlock) => break,
                Err(_) => break,
            }
        }

        // Process incoming packets
        for (addr, packet) in incoming {
            self.handle_server_packet(addr, packet, &mut events);
        }

        // Update all connections via update_tick (reliability, retransmission, congestion, keepalive)
        let mut disconnected = Vec::new();
        let addrs: Vec<SocketAddr> = self.connections.keys().copied().collect();
        for addr in addrs {
            let conn = self.connections.get_mut(&addr).unwrap();

            // Run the connection tick (handles retransmission, keepalive, congestion, etc.)
            if let Err(_e) = conn.update_tick() {
                disconnected.push((addr, DisconnectReason::Timeout));
                continue;
            }

            // Drain the send queue and send packets over the wire
            let packets = conn.drain_send_queue();
            for packet in packets {
                if let Ok(data) = packet.serialize() {
                    let mut data_with_crc = data;
                    security::append_crc32(&mut data_with_crc);
                    let byte_len = data_with_crc.len();
                    if let Err(e) = self.socket.send_to(&data_with_crc, addr) {
                        log::warn!("Failed to send to {}: {:?}", addr, e);
                        let conn = self.connections.get_mut(&addr).unwrap();
                        conn.stats.send_errors += 1;
                    } else {
                        let conn = self.connections.get_mut(&addr).unwrap();
                        conn.record_bytes_sent(byte_len);
                    }
                }
            }

            // Drain received messages
            let conn = self.connections.get_mut(&addr).unwrap();
            let max_channels = conn.channel_count();
            for ch in 0..max_channels as u8 {
                while let Some(data) = conn.receive(ch) {
                    events.push(ServerEvent::Message {
                        addr,
                        channel: ch,
                        data,
                    });
                }
            }
        }

        for (addr, reason) in disconnected {
            self.connections.remove(&addr);
            events.push(ServerEvent::ClientDisconnected(addr, reason));
        }

        // Update disconnecting connections (flush remaining disconnect packets)
        let mut finished_disconnecting = Vec::new();
        for (addr, conn) in &mut self.disconnecting {
            let _ = conn.update(&mut self.socket);
            if conn.state() == ConnectionState::Disconnected {
                finished_disconnecting.push(*addr);
            }
        }
        for addr in finished_disconnecting {
            self.disconnecting.remove(&addr);
        }

        // Cleanup
        let timeout = self.config.connection_request_timeout;
        self.pending.retain(|_, p| p.created_at.elapsed() < timeout);
        self.rate_limiter.cleanup();

        events
    }

    /// Send a reliable message to a connected client on the given channel.
    pub fn send(
        &mut self,
        addr: SocketAddr,
        channel: u8,
        data: &[u8],
    ) -> Result<(), crate::connection::ConnectionError> {
        self.send_with_reliability(addr, channel, data, true)
    }

    pub fn send_with_reliability(
        &mut self,
        addr: SocketAddr,
        channel: u8,
        data: &[u8],
        reliable: bool,
    ) -> Result<(), crate::connection::ConnectionError> {
        if let Some(conn) = self.connections.get_mut(&addr) {
            conn.send(channel, data, reliable)
        } else {
            Err(crate::connection::ConnectionError::NotConnected)
        }
    }

    /// Broadcast a message to all connected clients, optionally excluding one.
    pub fn broadcast(&mut self, channel: u8, data: &[u8], except: Option<SocketAddr>) {
        let addrs: Vec<SocketAddr> = self.connections.keys().copied().collect();
        for addr in addrs {
            if except == Some(addr) {
                continue;
            }
            let _ = self.send(addr, channel, data);
        }
    }

    /// Disconnect a client with the given reason code.
    pub fn disconnect(&mut self, addr: SocketAddr, reason: u8) {
        if let Some(mut conn) = self.connections.remove(&addr) {
            let _ = conn.disconnect(reason);
            let _ = conn.update(&mut self.socket);
            self.disconnecting.insert(addr, conn);
        }
    }

    /// Shut down the server, disconnecting all clients gracefully.
    pub fn shutdown(&mut self) {
        let addrs: Vec<SocketAddr> = self.connections.keys().copied().collect();
        for addr in addrs {
            self.disconnect(addr, disconnect_reason::REQUESTED);
        }
    }

    pub fn connections(&self) -> impl Iterator<Item = (&SocketAddr, &Connection)> {
        self.connections.iter()
    }

    pub fn stats(&self, addr: SocketAddr) -> Option<&NetworkStats> {
        self.connections.get(&addr).map(|c| c.stats())
    }

    pub fn client_count(&self) -> usize {
        self.connections.len()
    }

    pub fn local_addr(&self) -> Result<SocketAddr, SocketError> {
        self.socket.local_addr()
    }

    fn handle_server_packet(
        &mut self,
        addr: SocketAddr,
        packet: Packet,
        events: &mut Vec<ServerEvent>,
    ) {
        match packet.packet_type {
            PacketType::ConnectionRequest => {
                if !self.rate_limiter.allow(addr) {
                    return;
                }

                // Dedup: if already fully connected, resend accept
                if self.connections.contains_key(&addr) {
                    self.send_raw(addr, PacketType::ConnectionAccept);
                    return;
                }

                // Dedup: if already pending, resend the same challenge
                if let Some(pending) = self.pending.get(&addr) {
                    self.send_raw(
                        addr,
                        PacketType::ConnectionChallenge {
                            server_salt: pending.server_salt,
                        },
                    );
                    return;
                }

                if self.pending.len() >= self.config.max_pending {
                    return;
                }
                if self.connections.len() >= self.config.max_clients {
                    self.send_raw(
                        addr,
                        PacketType::ConnectionDeny {
                            reason: deny_reason::SERVER_FULL,
                        },
                    );
                    return;
                }

                let server_salt: u64 = random();
                self.send_raw(addr, PacketType::ConnectionChallenge { server_salt });
                self.pending.insert(
                    addr,
                    PendingConnection {
                        server_salt,
                        created_at: Instant::now(),
                    },
                );
            }
            PacketType::ConnectionResponse { client_salt } => {
                // Dedup: if already connected, resend accept
                if self.connections.contains_key(&addr) {
                    self.send_raw(addr, PacketType::ConnectionAccept);
                    return;
                }

                if let Some(pending) = self.pending.remove(&addr) {
                    // Validate: client must not echo server_salt or send zero
                    if client_salt == 0 || client_salt == pending.server_salt {
                        self.send_raw(
                            addr,
                            PacketType::ConnectionDeny {
                                reason: crate::packet::deny_reason::INVALID_CHALLENGE,
                            },
                        );
                        return;
                    }
                    self.send_raw(addr, PacketType::ConnectionAccept);

                    let local_addr = self.socket.local_addr().unwrap_or(addr);
                    let mut conn = Connection::new(self.config.clone(), local_addr, addr);
                    conn.set_state(ConnectionState::Connected);
                    conn.touch_recv_time();
                    self.connections.insert(addr, conn);
                    events.push(ServerEvent::ClientConnected(addr));
                }
            }
            PacketType::Disconnect { reason } => {
                if self.connections.remove(&addr).is_some() {
                    self.send_raw(
                        addr,
                        PacketType::Disconnect {
                            reason: disconnect_reason::REQUESTED,
                        },
                    );
                    events.push(ServerEvent::ClientDisconnected(
                        addr,
                        DisconnectReason::from(reason),
                    ));
                }
            }
            PacketType::Payload {
                channel,
                is_fragment,
            } => {
                if let Some(conn) = self.connections.get_mut(&addr) {
                    if packet.payload.len() > conn.config().default_channel_config.max_message_size
                    {
                        return;
                    }
                    conn.touch_recv_time();
                    // Process reliability/ACK info from header
                    conn.process_incoming_header(&packet.header);
                    if is_fragment {
                        if let Some(assembled) =
                            conn.fragment_assembler.process_fragment(&packet.payload)
                        {
                            conn.receive_payload_direct(channel, assembled);
                        }
                    } else {
                        conn.receive_payload_direct(channel, packet.payload);
                    }
                }
            }
            PacketType::BatchedPayload { channel } => {
                if let Some(conn) = self.connections.get_mut(&addr) {
                    conn.touch_recv_time();
                    // Process reliability/ACK info from header
                    conn.process_incoming_header(&packet.header);
                    if let Some(messages) = congestion::unbatch_messages(&packet.payload) {
                        for msg in messages {
                            conn.receive_payload_direct(channel, msg);
                        }
                    }
                }
            }
            PacketType::MtuProbe { probe_size } => {
                if let Some(conn) = self.connections.get_mut(&addr) {
                    conn.touch_recv_time();
                    conn.process_incoming_header(&packet.header);
                    // Send ACK back
                    self.send_raw(addr, PacketType::MtuProbeAck { probe_size });
                }
            }
            PacketType::MtuProbeAck { probe_size } => {
                if let Some(conn) = self.connections.get_mut(&addr) {
                    conn.touch_recv_time();
                    conn.process_incoming_header(&packet.header);
                    conn.mtu_discovery.on_probe_success(probe_size as usize);
                }
            }
            PacketType::KeepAlive => {
                if let Some(conn) = self.connections.get_mut(&addr) {
                    conn.touch_recv_time();
                    conn.process_incoming_header(&packet.header);
                }
            }
            _ => {}
        }
    }

    fn send_raw(&mut self, addr: SocketAddr, packet_type: PacketType) {
        wire::send_raw_packet(
            &mut self.socket,
            addr,
            self.config.protocol_id,
            0,
            packet_type,
        );
    }
}

impl Drop for NetServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
