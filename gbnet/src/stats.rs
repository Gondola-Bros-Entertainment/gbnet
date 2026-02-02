// stats.rs - Consolidated statistics types
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packet_loss: f32,
    pub rtt: f32,
    pub bandwidth_up: f32,
    pub bandwidth_down: f32,
    pub send_errors: u64,
}

impl Default for NetworkStats {
    fn default() -> Self {
        Self {
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            packet_loss: 0.0,
            rtt: 0.0,
            bandwidth_up: 0.0,
            bandwidth_down: 0.0,
            send_errors: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelStats {
    pub id: u8,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub send_buffer_size: usize,
    pub pending_ack_count: usize,
    pub receive_buffer_size: usize,
}

#[derive(Debug, Clone)]
pub struct ReliabilityStats {
    pub packets_in_flight: usize,
    pub local_sequence: u16,
    pub remote_sequence: u16,
    pub srtt_ms: f64,
    pub rttvar_ms: f64,
    pub rto_ms: f64,
    pub packet_loss: f32,
    pub total_sent: u64,
    pub total_acked: u64,
    pub total_lost: u64,
}

#[derive(Debug, Default)]
pub struct SocketStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub last_receive_time: Option<Instant>,
    pub last_send_time: Option<Instant>,
}
