// congestion.rs - Binary congestion control (Gaffer-style) and message batching
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const CONGESTION_RATE_REDUCTION: f32 = 0.5;
pub const MIN_SEND_RATE: f32 = 1.0;
pub const BATCH_HEADER_SIZE: usize = 1;
pub const BATCH_LENGTH_SIZE: usize = 2;
pub const MAX_BATCH_MESSAGES: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CongestionMode {
    Good,
    Bad,
}

/// Binary congestion controller inspired by Gaffer on Games.
#[derive(Debug)]
pub struct CongestionController {
    mode: CongestionMode,
    good_conditions_start: Option<Instant>,
    recovery_time: Duration,
    loss_threshold: f32,
    rtt_threshold_ms: f32,
    base_send_rate: f32,
    current_send_rate: f32,
}

impl CongestionController {
    pub fn new(
        base_send_rate: f32,
        loss_threshold: f32,
        rtt_threshold_ms: f32,
        recovery_time: Duration,
    ) -> Self {
        Self {
            mode: CongestionMode::Good,
            good_conditions_start: None,
            recovery_time,
            loss_threshold,
            rtt_threshold_ms,
            base_send_rate,
            current_send_rate: base_send_rate,
        }
    }

    /// Update congestion state based on current network conditions.
    pub fn update(&mut self, packet_loss: f32, rtt_ms: f32) {
        let is_bad = packet_loss > self.loss_threshold || rtt_ms > self.rtt_threshold_ms;

        match self.mode {
            CongestionMode::Good => {
                if is_bad {
                    self.mode = CongestionMode::Bad;
                    self.current_send_rate =
                        (self.base_send_rate * CONGESTION_RATE_REDUCTION).max(MIN_SEND_RATE);
                    self.good_conditions_start = None;
                }
            }
            CongestionMode::Bad => {
                if !is_bad {
                    match self.good_conditions_start {
                        None => {
                            self.good_conditions_start = Some(Instant::now());
                        }
                        Some(start) => {
                            if start.elapsed() >= self.recovery_time {
                                self.mode = CongestionMode::Good;
                                self.current_send_rate = self.base_send_rate;
                                self.good_conditions_start = None;
                            }
                        }
                    }
                } else {
                    self.good_conditions_start = None;
                }
            }
        }
    }

    pub fn mode(&self) -> CongestionMode {
        self.mode
    }

    pub fn send_rate(&self) -> f32 {
        self.current_send_rate
    }

    /// Returns true if a packet can be sent given the number of packets
    /// already sent this update cycle. The send rate is in packets per second,
    /// so this acts as a per-cycle budget when called once per tick.
    pub fn can_send(&self, packets_sent_this_cycle: u32) -> bool {
        (packets_sent_this_cycle as f32) < self.current_send_rate
    }
}

/// Packs multiple small messages into a single UDP packet up to MTU.
/// Wire format: [u8 message_count][u16 len][data]...
pub fn batch_messages(messages: &[Vec<u8>], max_size: usize) -> Vec<Vec<u8>> {
    let mut batches = Vec::new();
    let mut current_batch = Vec::new();
    let mut current_size = BATCH_HEADER_SIZE;
    let mut msg_count: u8 = 0;

    for msg in messages {
        let msg_wire_size = BATCH_LENGTH_SIZE + msg.len();
        if current_size + msg_wire_size > max_size && msg_count > 0 {
            // Finalize current batch
            let mut batch = Vec::with_capacity(current_size);
            batch.push(msg_count);
            batch.extend_from_slice(&current_batch);
            batches.push(batch);

            current_batch.clear();
            current_size = BATCH_HEADER_SIZE;
            msg_count = 0;
        }

        let len = msg.len() as u16;
        current_batch.extend_from_slice(&len.to_be_bytes());
        current_batch.extend_from_slice(msg);
        current_size += msg_wire_size;
        msg_count += 1;

        if msg_count == MAX_BATCH_MESSAGES {
            let mut batch = Vec::with_capacity(current_size);
            batch.push(msg_count);
            batch.extend_from_slice(&current_batch);
            batches.push(batch);

            current_batch.clear();
            current_size = BATCH_HEADER_SIZE;
            msg_count = 0;
        }
    }

    if msg_count > 0 {
        let mut batch = Vec::with_capacity(current_size);
        batch.push(msg_count);
        batch.extend_from_slice(&current_batch);
        batches.push(batch);
    }

    batches
}

/// Unbatch a batched packet into individual messages.
pub fn unbatch_messages(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    if data.is_empty() {
        return None;
    }

    let msg_count = data[0] as usize;
    let mut messages = Vec::with_capacity(msg_count);
    let mut offset = 1;

    for _ in 0..msg_count {
        if offset + 2 > data.len() {
            return None;
        }
        let len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        if offset + len > data.len() {
            return None;
        }
        messages.push(data[offset..offset + len].to_vec());
        offset += len;
    }

    Some(messages)
}

/// Bandwidth tracker using a sliding window backed by a `VecDeque`
/// to avoid unbounded growth.
#[derive(Debug)]
pub struct BandwidthTracker {
    window: VecDeque<(Instant, usize)>,
    window_duration: Duration,
}

impl BandwidthTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            window: VecDeque::new(),
            window_duration,
        }
    }

    pub fn record(&mut self, bytes: usize) {
        self.window.push_back((Instant::now(), bytes));
        self.cleanup();
    }

    pub fn bytes_per_second(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        let total_bytes: usize = self.window.iter().map(|(_, b)| b).sum();
        let elapsed = self.window_duration.as_secs_f64();
        if elapsed > 0.0 {
            total_bytes as f64 / elapsed
        } else {
            0.0
        }
    }

    fn cleanup(&mut self) {
        let cutoff = self.window_duration;
        while let Some(&(t, _)) = self.window.front() {
            if t.elapsed() >= cutoff {
                self.window.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_congestion_mode_transition() {
        let mut cc = CongestionController::new(60.0, 0.1, 250.0, Duration::from_millis(100));

        assert_eq!(cc.mode(), CongestionMode::Good);
        assert_eq!(cc.send_rate(), 60.0);

        // Bad conditions
        cc.update(0.2, 100.0);
        assert_eq!(cc.mode(), CongestionMode::Bad);
        assert_eq!(cc.send_rate(), 30.0);

        // Good conditions but not long enough
        cc.update(0.01, 50.0);
        assert_eq!(cc.mode(), CongestionMode::Bad);

        // Wait for recovery
        std::thread::sleep(Duration::from_millis(150));
        cc.update(0.01, 50.0);
        assert_eq!(cc.mode(), CongestionMode::Good);
        assert_eq!(cc.send_rate(), 60.0);
    }

    #[test]
    fn test_batch_unbatch_roundtrip() {
        let messages = vec![b"hello".to_vec(), b"world".to_vec(), b"test".to_vec()];

        let batches = batch_messages(&messages, 1200);
        assert_eq!(batches.len(), 1);

        let unbatched = unbatch_messages(&batches[0]).unwrap();
        assert_eq!(unbatched, messages);
    }

    #[test]
    fn test_batch_splits_at_mtu() {
        let messages: Vec<Vec<u8>> = (0..10).map(|_| vec![0u8; 200]).collect();
        let batches = batch_messages(&messages, 500);
        assert!(batches.len() > 1);

        // Verify all messages are preserved
        let mut total = 0;
        for batch in &batches {
            total += unbatch_messages(batch).unwrap().len();
        }
        assert_eq!(total, 10);
    }

    #[test]
    fn test_can_send_respects_rate() {
        let cc = CongestionController::new(60.0, 0.1, 250.0, Duration::from_secs(10));
        assert!(cc.can_send(0));
        assert!(cc.can_send(59));
        assert!(!cc.can_send(60));
    }

    #[test]
    fn test_bandwidth_tracker() {
        let mut tracker = BandwidthTracker::new(Duration::from_secs(1));
        tracker.record(1000);
        tracker.record(2000);
        assert!(tracker.bytes_per_second() > 0.0);
    }
}
