//! Reliable packet delivery with Jacobson/Karels RTT estimation, adaptive RTO,
//! fast retransmit, and bounded in-flight tracking.
use crate::stats::ReliabilityStats;
use crate::util::{sequence_diff, sequence_greater_than};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const INITIAL_RTO_MILLIS: u64 = 100;
pub const ACK_BITS_WINDOW: u16 = 32;
pub const FAST_RETRANSMIT_THRESHOLD: u32 = 3;
pub const RTT_ALPHA: f64 = 0.125;
pub const RTT_BETA: f64 = 0.25;
pub const MIN_RTO_MS: f64 = 50.0;
pub const MAX_RTO_MS: f64 = 2000.0;
use crate::config::MAX_BACKOFF_EXPONENT;
const LOSS_WINDOW_SIZE: usize = 256;

/// Tracks sent packets for reliability and acknowledgment.
#[derive(Debug)]
pub struct ReliableEndpoint {
    local_sequence: u16,
    remote_sequence: u16,
    ack_bits: u32,

    sent_packets: HashMap<u16, SentPacketData>,
    received_packets: SequenceBuffer<bool>,

    max_sequence_distance: u16,
    max_retries: u32,
    max_in_flight: usize,

    srtt: f64,
    rttvar: f64,
    rto: Duration,
    has_rtt_sample: bool,

    loss_window: [bool; 256],
    loss_window_index: usize,
    loss_window_count: usize,

    dup_ack_counts: HashMap<u16, u32>,

    total_packets_sent: u64,
    total_packets_acked: u64,
    total_packets_lost: u64,
    packets_evicted: u64,
    bytes_sent: u64,
    bytes_acked: u64,
}

#[derive(Debug, Clone)]
struct SentPacketData {
    send_time: Instant,
    retry_count: u32,
    data: Vec<u8>,
    size: usize,
}

impl ReliableEndpoint {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            local_sequence: 0,
            remote_sequence: 0,
            ack_bits: 0,
            sent_packets: HashMap::new(),
            received_packets: SequenceBuffer::new(buffer_size),
            max_sequence_distance: crate::config::DEFAULT_MAX_SEQUENCE_DISTANCE,
            max_retries: crate::config::DEFAULT_MAX_RELIABLE_RETRIES,
            max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
            srtt: 0.0,
            rttvar: 0.0,
            rto: Duration::from_millis(INITIAL_RTO_MILLIS),
            has_rtt_sample: false,
            loss_window: [false; LOSS_WINDOW_SIZE],
            loss_window_index: 0,
            loss_window_count: 0,
            dup_ack_counts: HashMap::new(),
            total_packets_sent: 0,
            total_packets_acked: 0,
            total_packets_lost: 0,
            packets_evicted: 0,
            bytes_sent: 0,
            bytes_acked: 0,
        }
    }

    pub fn with_max_in_flight(mut self, max: usize) -> Self {
        self.max_in_flight = max;
        self
    }

    /// Gets the next sequence number for outgoing packets.
    pub fn next_sequence(&mut self) -> u16 {
        let seq = self.local_sequence;
        self.local_sequence = self.local_sequence.wrapping_add(1);
        seq
    }

    /// Records a packet as sent for reliability tracking.
    pub fn on_packet_sent(&mut self, sequence: u16, send_time: Instant, data: Vec<u8>) {
        let size = data.len();

        if self.sent_packets.len() >= self.max_in_flight {
            self.evict_worst_in_flight();
        }

        self.sent_packets.insert(
            sequence,
            SentPacketData {
                send_time,
                retry_count: 0,
                data,
                size,
            },
        );
        self.total_packets_sent += 1;
        self.bytes_sent += size as u64;
    }

    /// Evict the in-flight packet with the highest retry count (tiebreak: oldest send_time).
    fn evict_worst_in_flight(&mut self) {
        let worst_seq = self
            .sent_packets
            .iter()
            .max_by(|(_, a), (_, b)| {
                a.retry_count
                    .cmp(&b.retry_count)
                    .then_with(|| b.send_time.cmp(&a.send_time))
            })
            .map(|(&seq, _)| seq);

        if let Some(seq) = worst_seq {
            self.sent_packets.remove(&seq);
            self.record_loss_sample(true);
            self.total_packets_lost += 1;
            self.packets_evicted += 1;
        }
    }

    /// Processes an incoming packet and updates ack information.
    pub fn on_packet_received(&mut self, sequence: u16, _receive_time: Instant) {
        let distance = sequence_diff(sequence, self.remote_sequence).unsigned_abs();
        if distance > self.max_sequence_distance as u32 {
            return;
        }

        if !self.received_packets.exists(sequence) {
            self.received_packets.insert(sequence, true);

            if sequence_greater_than(sequence, self.remote_sequence) {
                let diff = sequence_diff(sequence, self.remote_sequence) as u32;
                if diff <= ACK_BITS_WINDOW as u32 {
                    self.ack_bits = (self.ack_bits << diff) | 1;
                } else {
                    self.ack_bits = 1;
                }
                self.remote_sequence = sequence;
            } else {
                let diff = sequence_diff(self.remote_sequence, sequence) as u32;
                if diff > 0 && diff <= ACK_BITS_WINDOW as u32 {
                    self.ack_bits |= 1 << (diff - 1);
                }
            }
        }
    }

    /// Processes acknowledgments from the remote endpoint.
    pub fn process_acks(&mut self, ack: u16, ack_bits: u32) {
        self.ack_single(ack);

        for i in 0..ACK_BITS_WINDOW {
            if (ack_bits & (1 << i)) != 0 {
                let acked_seq = ack.wrapping_sub(i + 1);
                self.ack_single(acked_seq);
            }
        }
    }

    fn ack_single(&mut self, sequence: u16) {
        if let Some(packet_data) = self.sent_packets.remove(&sequence) {
            let rtt_sample = packet_data.send_time.elapsed().as_secs_f64() * 1000.0;

            // Karn's algorithm: skip RTT samples from retransmitted packets
            if packet_data.retry_count == 0 {
                self.update_rtt(rtt_sample);
            }

            self.total_packets_acked += 1;
            self.bytes_acked += packet_data.size as u64;

            self.record_loss_sample(false);
        }

        self.dup_ack_counts.remove(&sequence);
    }

    /// Update RTT using Jacobson/Karels algorithm.
    pub fn update_rtt(&mut self, sample_ms: f64) {
        if !self.has_rtt_sample {
            self.srtt = sample_ms;
            self.rttvar = sample_ms / 2.0;
            self.has_rtt_sample = true;
        } else {
            self.rttvar = (1.0 - RTT_BETA) * self.rttvar + RTT_BETA * (sample_ms - self.srtt).abs();
            self.srtt = (1.0 - RTT_ALPHA) * self.srtt + RTT_ALPHA * sample_ms;
        }
        let rto_ms = (self.srtt + 4.0 * self.rttvar).clamp(MIN_RTO_MS, MAX_RTO_MS);
        self.rto = Duration::from_millis(rto_ms as u64);
    }

    pub fn record_loss_sample(&mut self, lost: bool) {
        self.loss_window[self.loss_window_index % LOSS_WINDOW_SIZE] = lost;
        self.loss_window_index += 1;
        if self.loss_window_count < LOSS_WINDOW_SIZE {
            self.loss_window_count += 1;
        }
    }

    /// Updates the reliability system. Returns packets needing retransmission.
    pub fn update(&mut self, current_time: Instant) -> SmallVec<[(u16, Vec<u8>); 8]> {
        let mut packets_to_resend: SmallVec<[(u16, Vec<u8>); 8]> = SmallVec::new();
        let mut packets_to_remove = Vec::new();

        for (&sequence, packet_data) in &mut self.sent_packets {
            let elapsed = current_time.duration_since(packet_data.send_time);
            let backoff_rto =
                self.rto * (1u32 << packet_data.retry_count.min(MAX_BACKOFF_EXPONENT));
            if elapsed >= backoff_rto {
                if packet_data.retry_count >= self.max_retries {
                    packets_to_remove.push(sequence);
                } else {
                    packet_data.retry_count += 1;
                    packet_data.send_time = current_time;
                    packets_to_resend.push((sequence, packet_data.data.clone()));
                }
            }
        }

        for sequence in packets_to_remove {
            self.sent_packets.remove(&sequence);
            self.total_packets_lost += 1;
            self.record_loss_sample(true);
        }

        packets_to_resend
    }

    /// Trigger fast retransmit for a sequence (on 3 duplicate ACKs).
    pub fn on_duplicate_ack(&mut self, sequence: u16) -> Option<(u16, Vec<u8>)> {
        let count = self.dup_ack_counts.entry(sequence).or_insert(0);
        *count += 1;
        if *count == FAST_RETRANSMIT_THRESHOLD {
            if let Some(packet_data) = self.sent_packets.get_mut(&sequence) {
                packet_data.send_time = Instant::now();
                packet_data.retry_count += 1;
                return Some((sequence, packet_data.data.clone()));
            }
        }
        None
    }

    /// Gets current ack information to include in outgoing packets.
    pub fn get_ack_info(&self) -> (u16, u32) {
        (self.remote_sequence, self.ack_bits)
    }

    /// Returns the current adaptive RTO.
    pub fn rto(&self) -> Duration {
        self.rto
    }

    /// Returns smoothed RTT in milliseconds.
    pub fn srtt_ms(&self) -> f64 {
        self.srtt
    }

    /// Calculates packet loss percentage from rolling window.
    pub fn packet_loss_percent(&self) -> f32 {
        if self.loss_window_count == 0 {
            return 0.0;
        }
        let lost = self.loss_window[..self.loss_window_count.min(LOSS_WINDOW_SIZE)]
            .iter()
            .filter(|&&l| l)
            .count();
        lost as f32 / self.loss_window_count as f32
    }

    pub fn packets_in_flight(&self) -> usize {
        self.sent_packets.len()
    }

    pub fn packets_evicted(&self) -> u64 {
        self.packets_evicted
    }

    pub fn stats(&self) -> ReliabilityStats {
        ReliabilityStats {
            packets_in_flight: self.sent_packets.len(),
            local_sequence: self.local_sequence,
            remote_sequence: self.remote_sequence,
            srtt_ms: self.srtt,
            rttvar_ms: self.rttvar,
            rto_ms: self.rto.as_millis() as f64,
            packet_loss: self.packet_loss_percent(),
            total_sent: self.total_packets_sent,
            total_acked: self.total_packets_acked,
            total_lost: self.total_packets_lost,
            packets_evicted: self.packets_evicted,
        }
    }
}

/// A circular buffer for tracking sequence numbers.
#[derive(Debug)]
pub struct SequenceBuffer<T> {
    entries: Vec<Option<T>>,
    sequence: u16,
    size: usize,
}

impl<T> SequenceBuffer<T> {
    pub fn new(size: usize) -> Self {
        let mut entries = Vec::with_capacity(size);
        for _ in 0..size {
            entries.push(None);
        }
        Self {
            entries,
            sequence: 0,
            size,
        }
    }

    pub fn insert(&mut self, sequence: u16, data: T) {
        if sequence_greater_than(sequence, self.sequence) {
            let diff = sequence_diff(sequence, self.sequence) as usize;
            if diff < self.size {
                for _ in 0..diff {
                    self.sequence = self.sequence.wrapping_add(1);
                    let index = self.sequence as usize % self.size;
                    self.entries[index] = None;
                }
            } else {
                for entry in &mut self.entries {
                    *entry = None;
                }
                self.sequence = sequence;
            }
        }

        let index = sequence as usize % self.size;
        self.entries[index] = Some(data);
    }

    pub fn exists(&self, sequence: u16) -> bool {
        let index = sequence as usize % self.size;
        self.entries[index].is_some()
    }

    pub fn get(&self, sequence: u16) -> Option<&T> {
        let index = sequence as usize % self.size;
        self.entries[index].as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtt_convergence() {
        let mut endpoint = ReliableEndpoint::new(256);

        // Directly feed RTT samples to test the algorithm
        for _ in 0..20 {
            endpoint.update_rtt(50.0);
        }

        assert!(
            endpoint.srtt_ms() > 40.0 && endpoint.srtt_ms() < 60.0,
            "SRTT {} should be near 50ms",
            endpoint.srtt_ms()
        );
    }

    #[test]
    fn test_adaptive_rto() {
        let mut endpoint = ReliableEndpoint::new(256);

        // First sample
        endpoint.update_rtt(50.0);
        assert!(endpoint.rto.as_millis() >= 50);

        // High jitter sample
        endpoint.update_rtt(200.0);
        assert!(
            endpoint.rto.as_millis() > 50,
            "RTO should increase with jitter"
        );

        // RTO bounded
        endpoint.update_rtt(5000.0);
        assert!(
            endpoint.rto.as_millis() <= 2000,
            "RTO should be capped at 2s"
        );
    }

    #[test]
    fn test_packet_loss_tracking() {
        let mut endpoint = ReliableEndpoint::new(256);

        // Simulate mixed success/failure by directly recording samples
        for _ in 0..8 {
            endpoint.record_loss_sample(false); // success
        }
        for _ in 0..2 {
            endpoint.record_loss_sample(true); // loss
        }

        let loss = endpoint.packet_loss_percent();
        assert!(
            (loss - 0.2).abs() < 0.01,
            "Should be ~20% loss, got {}",
            loss
        );
    }

    #[test]
    fn test_progressive_backoff() {
        let mut endpoint = ReliableEndpoint::new(256);
        let now = Instant::now();

        endpoint.on_packet_sent(0, now, vec![1, 2, 3]);

        // First timeout at RTO
        let t1 = now + endpoint.rto + Duration::from_millis(1);
        let resent = endpoint.update(t1);
        assert_eq!(resent.len(), 1);

        // Second timeout should take ~2x RTO from t1
        let t2 = t1 + endpoint.rto;
        let resent2 = endpoint.update(t2);
        assert_eq!(resent2.len(), 0, "Should not resend yet (backoff)");

        let t3 = t1 + endpoint.rto * 2 + Duration::from_millis(1);
        let resent3 = endpoint.update(t3);
        assert_eq!(resent3.len(), 1, "Should resend after 2x RTO");
    }

    #[test]
    fn test_fast_retransmit() {
        let mut endpoint = ReliableEndpoint::new(256);
        let now = Instant::now();

        endpoint.on_packet_sent(5, now, vec![5, 5, 5]);

        // 1st dup ack - no retransmit
        assert!(endpoint.on_duplicate_ack(5).is_none());
        // 2nd dup ack - no retransmit
        assert!(endpoint.on_duplicate_ack(5).is_none());
        // 3rd dup ack - trigger fast retransmit
        let result = endpoint.on_duplicate_ack(5);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, 5);
    }

    #[test]
    fn test_sequence_buffer_operations() {
        let mut buffer: SequenceBuffer<u32> = SequenceBuffer::new(16);

        buffer.insert(0, 100);
        buffer.insert(1, 200);
        buffer.insert(2, 300);

        assert!(buffer.exists(0));
        assert!(buffer.exists(1));
        assert!(buffer.exists(2));
        assert!(!buffer.exists(3));

        assert_eq!(*buffer.get(0).unwrap(), 100);
        assert_eq!(*buffer.get(1).unwrap(), 200);
        assert_eq!(*buffer.get(2).unwrap(), 300);
    }

    #[test]
    fn test_sequence_buffer_wraparound() {
        let mut buffer: SequenceBuffer<u32> = SequenceBuffer::new(16);

        // Insert near u16::MAX
        buffer.insert(65534, 100);
        buffer.insert(65535, 200);
        buffer.insert(0, 300);
        buffer.insert(1, 400);

        assert!(buffer.exists(65534));
        assert!(buffer.exists(65535));
        assert!(buffer.exists(0));
        assert!(buffer.exists(1));
    }

    #[test]
    fn test_in_flight_cap_eviction() {
        let mut endpoint = ReliableEndpoint::new(256).with_max_in_flight(4);
        let now = Instant::now();

        // Send 4 packets (at capacity)
        for i in 0..4 {
            endpoint.on_packet_sent(i, now, vec![i as u8]);
        }
        assert_eq!(endpoint.packets_in_flight(), 4);

        // Send a 5th — should evict one
        endpoint.on_packet_sent(4, now, vec![4]);
        assert_eq!(endpoint.packets_in_flight(), 4);
        assert_eq!(endpoint.packets_evicted(), 1);
    }

    #[test]
    fn test_in_flight_evicts_highest_retry() {
        let mut endpoint = ReliableEndpoint::new(256).with_max_in_flight(3);
        let now = Instant::now();

        endpoint.on_packet_sent(0, now, vec![0]);
        endpoint.on_packet_sent(1, now, vec![1]);
        endpoint.on_packet_sent(2, now, vec![2]);

        // Simulate retries on seq 1 by updating past RTO multiple times
        let t1 = now + endpoint.rto() + Duration::from_millis(1);
        let _ = endpoint.update(t1);
        // seq 0,1,2 all got retried once. Now retry again past 2x RTO
        let t2 = t1 + endpoint.rto() * 2 + Duration::from_millis(1);
        let _ = endpoint.update(t2);

        // Now send a 4th packet — should evict the one with highest retry
        endpoint.on_packet_sent(3, now + Duration::from_secs(1), vec![3]);
        assert_eq!(endpoint.packets_in_flight(), 3);
        assert_eq!(endpoint.packets_evicted(), 1);
    }
}
