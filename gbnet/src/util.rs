//! Shared sequence number utilities for u16 wraparound arithmetic.

/// Half the u16 sequence space, used for wraparound comparison.
const SEQUENCE_HALF_RANGE: u16 = 32768;

/// Compares two sequence numbers accounting for u16 wraparound.
/// Returns true if s1 is "greater than" s2 in circular sequence space.
pub fn sequence_greater_than(s1: u16, s2: u16) -> bool {
    ((s1 > s2) && (s1 - s2 <= SEQUENCE_HALF_RANGE))
        || ((s1 < s2) && (s2 - s1 > SEQUENCE_HALF_RANGE))
}

/// Computes the signed difference between two sequence numbers,
/// accounting for u16 wraparound.
pub fn sequence_diff(s1: u16, s2: u16) -> i32 {
    let half = SEQUENCE_HALF_RANGE as i32;
    let full = (SEQUENCE_HALF_RANGE as i32) * 2;
    let diff = s1 as i32 - s2 as i32;
    if diff > half {
        diff - full
    } else if diff < -half {
        diff + full
    } else {
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_greater_than_basic() {
        assert!(sequence_greater_than(1, 0));
        assert!(!sequence_greater_than(0, 1));
        assert!(sequence_greater_than(100, 50));
        assert!(!sequence_greater_than(50, 100));
    }

    #[test]
    fn test_sequence_greater_than_wraparound() {
        assert!(sequence_greater_than(0, 65535));
        assert!(!sequence_greater_than(65535, 0));
        assert!(sequence_greater_than(1, 65534));
        assert!(sequence_greater_than(100, 65500));
    }

    #[test]
    fn test_sequence_diff_basic() {
        assert_eq!(sequence_diff(5, 3), 2);
        assert_eq!(sequence_diff(3, 5), -2);
        assert_eq!(sequence_diff(100, 100), 0);
    }

    #[test]
    fn test_sequence_diff_wraparound() {
        assert_eq!(sequence_diff(0, 65535), 1);
        assert_eq!(sequence_diff(65535, 0), -1);
        assert_eq!(sequence_diff(5, 65530), 11);
    }

    #[test]
    fn test_sequence_at_u16_max() {
        assert!(sequence_greater_than(0, u16::MAX));
        assert!(!sequence_greater_than(u16::MAX, 0));
        assert_eq!(sequence_diff(0, u16::MAX), 1);
        assert_eq!(sequence_diff(u16::MAX, 0), -1);
    }
}
