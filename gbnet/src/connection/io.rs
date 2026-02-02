use std::time::Instant;

use crate::{
    packet::{disconnect_reason, Packet, PacketType},
    security,
    socket::{SocketError, UdpSocket},
};

use super::{Connection, ConnectionError, ConnectionState};

impl Connection {
    /// Full update cycle including socket I/O. Used by Connection-driven flows
    /// (e.g. disconnecting connections that own their socket interaction).
    pub fn update(&mut self, socket: &mut UdpSocket) -> Result<(), ConnectionError> {
        self.update_tick()?;
        self.process_send_queue(socket)?;
        self.receive_packets(socket)?;
        Ok(())
    }

    /// Update connection state without socket I/O. Server/Client call this per tick
    /// after feeding received packets in, then drain `send_queue` themselves.
    pub fn update_tick(&mut self) -> Result<(), ConnectionError> {
        let now = Instant::now();

        // Check for timeout
        if self.state != ConnectionState::Disconnected
            && self.state != ConnectionState::Disconnecting
        {
            let time_since_recv = now.duration_since(self.last_packet_recv_time);
            if time_since_recv > self.config.connection_timeout {
                self.state = ConnectionState::Disconnected;
                self.reset_connection();
                return Err(ConnectionError::Timeout);
            }
        }

        // Cleanup expired fragment buffers (runs in all states)
        self.fragment_assembler.cleanup();

        match self.state {
            ConnectionState::Connecting => {
                if let Some(request_time) = self.connection_request_time {
                    if now.duration_since(request_time) > self.config.connection_request_timeout {
                        self.connection_retry_count += 1;
                        if self.connection_retry_count > self.config.connection_request_max_retries
                        {
                            self.state = ConnectionState::Disconnected;
                            return Err(ConnectionError::Timeout);
                        }
                        self.send_connection_request()?;
                        self.connection_request_time = Some(now);
                    }
                }
            }
            ConnectionState::Connected => {
                // Update congestion control
                self.congestion
                    .update(self.stats.packet_loss, self.stats.rtt);

                // Send keepalive if needed
                let time_since_send = now.duration_since(self.last_packet_send_time);
                if time_since_send > self.config.keepalive_interval {
                    self.send_keepalive()?;
                }

                // MTU discovery probing
                if let Some(probe_size) = self.mtu_discovery.next_probe() {
                    let header = self.create_header();
                    let padding = vec![0u8; probe_size.saturating_sub(16)];
                    let packet = Packet::new(
                        header,
                        PacketType::MtuProbe {
                            probe_size: probe_size as u16,
                        },
                    )
                    .with_payload(padding);
                    self.send_queue.push_back(packet);
                }

                // Track packets sent this cycle for congestion limiting
                let mut packets_sent_this_cycle: u32 = 0;

                // Drain channel outgoing messages into packets
                for ch_idx in 0..self.channels.len() {
                    loop {
                        if !self.congestion.can_send(packets_sent_this_cycle) {
                            break;
                        }
                        let Some((msg_seq, wire_data)) =
                            self.channels[ch_idx].get_outgoing_message()
                        else {
                            break;
                        };
                        packets_sent_this_cycle += 1;
                        let header = self.create_header();
                        let packet = Packet::new(
                            header,
                            PacketType::Payload {
                                channel: ch_idx as u8,
                                is_fragment: false,
                            },
                        )
                        .with_payload(wire_data.clone());
                        self.send_queue.push_back(packet);

                        if self.channels[ch_idx].is_reliable() {
                            self.reliability.on_packet_sent(msg_seq, now, wire_data);
                        }
                    }

                    // Handle retransmissions
                    let rto = self.reliability.rto();
                    let retransmits = self.channels[ch_idx].get_retransmit_messages(now, rto);
                    for (_seq, wire_data) in retransmits {
                        let header = self.create_header();
                        let packet = Packet::new(
                            header,
                            PacketType::Payload {
                                channel: ch_idx as u8,
                                is_fragment: false,
                            },
                        )
                        .with_payload(wire_data);
                        self.send_queue.push_back(packet);
                    }
                }

                // Update channel state (ordered buffer timeouts, etc.)
                for channel in &mut self.channels {
                    channel.update();
                }

                // Update reliability system (retransmit timed-out packets)
                let packets_to_retry = self.reliability.update(now);
                for (sequence, data) in packets_to_retry {
                    let mut header = self.create_header();
                    header.sequence = sequence;
                    let packet = Packet::new(
                        header,
                        PacketType::Payload {
                            channel: 0,
                            is_fragment: false,
                        },
                    )
                    .with_payload(data);
                    self.send_queue.push_back(packet);
                }
            }
            ConnectionState::Disconnecting => {
                if let Some(disc_time) = self.disconnect_time {
                    if now.duration_since(disc_time) > self.config.disconnect_retry_timeout {
                        if self.disconnect_retry_count >= self.config.disconnect_retries {
                            self.state = ConnectionState::Disconnected;
                            self.reset_connection();
                        } else {
                            self.disconnect_retry_count += 1;
                            self.disconnect_time = Some(now);
                            let header = self.create_header();
                            let packet = Packet::new(
                                header,
                                PacketType::Disconnect {
                                    reason: disconnect_reason::REQUESTED,
                                },
                            );
                            self.send_queue.push_back(packet);
                        }
                    }
                }
            }
            _ => {}
        }

        // Update stats
        self.stats.rtt = self.reliability.srtt_ms() as f32;
        self.stats.packet_loss = self.reliability.packet_loss_percent();
        self.stats.bandwidth_up = self.bandwidth_up.bytes_per_second() as f32;
        self.stats.bandwidth_down = self.bandwidth_down.bytes_per_second() as f32;

        Ok(())
    }

    fn send_keepalive(&mut self) -> Result<(), ConnectionError> {
        let header = self.create_header();
        let packet = Packet::new(header, PacketType::KeepAlive);
        self.send_queue.push_back(packet);
        Ok(())
    }

    fn process_send_queue(&mut self, socket: &mut UdpSocket) -> Result<(), ConnectionError> {
        while let Some(packet) = self.send_queue.pop_front() {
            let data = packet
                .serialize()
                .map_err(|_| ConnectionError::InvalidPacket)?;

            #[cfg(feature = "encryption")]
            let data = if let Some(ref enc) = self.encryption_state {
                enc.encrypt(&data, self.local_sequence as u64)
                    .unwrap_or(data)
            } else {
                data
            };

            let mut data_with_crc = data;
            security::append_crc32(&mut data_with_crc);

            socket.send_to(&data_with_crc, self.remote_addr)?;

            self.bandwidth_up.record(data_with_crc.len());
            self.last_packet_send_time = Instant::now();
            self.local_sequence = self.local_sequence.wrapping_add(1);
            self.stats.packets_sent += 1;
            self.stats.bytes_sent += data_with_crc.len() as u64;
        }
        Ok(())
    }

    fn receive_packets(&mut self, socket: &mut UdpSocket) -> Result<(), ConnectionError> {
        loop {
            match socket.recv_from() {
                Ok((data, addr)) => {
                    if addr != self.remote_addr {
                        continue;
                    }

                    let validated = match security::validate_and_strip_crc32(data) {
                        Some(valid) => valid,
                        None => continue,
                    };

                    #[cfg(feature = "encryption")]
                    let decrypted;
                    #[cfg(feature = "encryption")]
                    let validated = if let Some(ref enc) = self.encryption_state {
                        match enc.decrypt(validated, self.remote_sequence as u64) {
                            Ok(d) => {
                                decrypted = d;
                                &decrypted
                            }
                            Err(_) => continue,
                        }
                    } else {
                        validated
                    };

                    let packet = match Packet::deserialize(validated) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    if packet.header.protocol_id != self.config.protocol_id {
                        continue;
                    }

                    self.bandwidth_down.record(data.len());
                    self.last_packet_recv_time = Instant::now();
                    self.stats.packets_received += 1;
                    self.stats.bytes_received += data.len() as u64;

                    self.handle_packet(packet)?;
                }
                Err(SocketError::WouldBlock) => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
