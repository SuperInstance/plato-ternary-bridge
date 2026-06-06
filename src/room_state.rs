//! TernaryRoomState: entire room as a ternary vector.
//!
//! Packs sensor trit values into a compact u32 representation (16 trits per u32).

use std::fmt;

/// Maximum number of trits storable in a u32 (2 bits per trit).
pub const MAX_TRITS: usize = 16;

/// Represents an entire room's sensor state as a ternary vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TernaryRoomState {
    /// Trit values, each in {-1, 0, +1}. Length <= MAX_TRITS.
    trits: Vec<i8>,
}

impl TernaryRoomState {
    /// Create from a vector of trit values.
    ///
    /// # Panics
    /// Panics if any value is not in {-1, 0, +1} or if length exceeds MAX_TRITS.
    pub fn new(trits: Vec<i8>) -> Self {
        assert!(trits.len() <= MAX_TRITS, "too many trits for u32 packing");
        for &t in &trits {
            assert!(t == -1 || t == 0 || t == 1, "trit must be -1, 0, or +1");
        }
        Self { trits }
    }

    /// Create from sensor values and threshold configs.
    pub fn from_sensors(values: &[f64], thresholds: &[crate::threshold::TernaryThreshold]) -> Self {
        assert_eq!(values.len(), thresholds.len());
        let trits: Vec<i8> = values
            .iter()
            .zip(thresholds.iter())
            .map(|(v, t)| crate::threshold::to_trit(*v, t))
            .collect();
        Self::new(trits)
    }

    /// Access the trit values.
    pub fn trits(&self) -> &[i8] {
        &self.trits
    }

    /// Number of trits.
    pub fn len(&self) -> usize {
        self.trits.len()
    }

    /// Whether the state is empty.
    pub fn is_empty(&self) -> bool {
        self.trits.is_empty()
    }

    /// Pack into a u32. Each trit uses 2 bits: -1 → 0b11, 0 → 0b00, +1 → 0b01.
    /// Bits 0-1 = trit[0], bits 2-3 = trit[1], etc.
    pub fn pack(&self) -> u32 {
        let mut packed: u32 = 0;
        for (i, &t) in self.trits.iter().enumerate() {
            let bits = match t {
                -1 => 0b11u32,
                0 => 0b00,
                1 => 0b01,
                _ => unreachable!(),
            };
            packed |= bits << (i * 2);
        }
        packed
    }

    /// Unpack from a u32 and a known number of trits.
    pub fn unpack(packed: u32, count: usize) -> Self {
        assert!(count <= MAX_TRITS);
        let mut trits = Vec::with_capacity(count);
        for i in 0..count {
            let bits = (packed >> (i * 2)) & 0b11;
            let t = match bits {
                0b00 => 0,
                0b01 => 1,
                0b11 => -1,
                _ => 0, // 0b10 unused, default to 0
            };
            trits.push(t);
        }
        Self { trits }
    }

    /// Compute Hamming distance (count of differing trits) between two room states.
    pub fn hamming_distance(&self, other: &TernaryRoomState) -> usize {
        assert_eq!(self.len(), other.len());
        self.trits
            .iter()
            .zip(other.trits.iter())
            .filter(|(a, b)| a != b)
            .count()
    }

    /// Compute ternary dot product (sum of a_i * b_i).
    /// Returns a value in [-N, +N] where N = number of trits.
    /// High positive = strong agreement, high negative = strong disagreement.
    pub fn dot_product(&self, other: &TernaryRoomState) -> i32 {
        assert_eq!(self.len(), other.len());
        self.trits
            .iter()
            .zip(other.trits.iter())
            .map(|(a, b)| (*a as i32) * (*b as i32))
            .sum()
    }
}

impl fmt::Display for TernaryRoomState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for (i, &t) in self.trits.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            match t {
                -1 => write!(f, "-1")?,
                0 => write!(f, "0")?,
                1 => write!(f, "+1")?,
                _ => write!(f, "?")?,
            }
        }
        write!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threshold::TernaryThreshold;

    #[test]
    fn create_from_trits() {
        let state = TernaryRoomState::new(vec![-1, 0, 1, 0, -1, 1, 0, 0]);
        assert_eq!(state.len(), 8);
        assert_eq!(state.trits(), &[-1, 0, 1, 0, -1, 1, 0, 0]);
    }

    #[test]
    fn pack_round_trip() {
        let trits = vec![-1, 0, 1, 0, -1, 1, 0, 0];
        let state = TernaryRoomState::new(trits.clone());
        let packed = state.pack();
        let unpacked = TernaryRoomState::unpack(packed, trits.len());
        assert_eq!(state, unpacked);
    }

    #[test]
    fn pack_all_zeros() {
        let state = TernaryRoomState::new(vec![0, 0, 0, 0]);
        assert_eq!(state.pack(), 0u32);
    }

    #[test]
    fn pack_all_negative() {
        let state = TernaryRoomState::new(vec![-1, -1, -1, -1]);
        let packed = state.pack();
        assert_eq!(packed, 0b11111111u32);
    }

    #[test]
    fn hamming_distance_same() {
        let a = TernaryRoomState::new(vec![-1, 0, 1, 0]);
        assert_eq!(a.hamming_distance(&a), 0);
    }

    #[test]
    fn hamming_distance_different() {
        let a = TernaryRoomState::new(vec![-1, 0, 1, 0]);
        let b = TernaryRoomState::new(vec![1, 0, -1, 1]);
        assert_eq!(a.hamming_distance(&b), 3);
    }

    #[test]
    fn dot_product_agreement() {
        let a = TernaryRoomState::new(vec![1, 1, 1, 1]);
        assert_eq!(a.dot_product(&a), 4);
    }

    #[test]
    fn dot_product_disagreement() {
        let a = TernaryRoomState::new(vec![1, 1, 1, 1]);
        let b = TernaryRoomState::new(vec![-1, -1, -1, -1]);
        assert_eq!(a.dot_product(&b), -4);
    }

    #[test]
    fn dot_product_mixed() {
        let a = TernaryRoomState::new(vec![1, 0, -1, 1]);
        let b = TernaryRoomState::new(vec![1, 0, -1, -1]);
        // 1*1 + 0*0 + (-1)*(-1) + 1*(-1) = 1 + 0 + 1 - 1 = 1
        assert_eq!(a.dot_product(&b), 1);
    }

    #[test]
    fn display_format() {
        let state = TernaryRoomState::new(vec![-1, 0, 1, 0]);
        assert_eq!(format!("{}", state), "{-1,0,+1,0}");
    }

    #[test]
    fn from_sensors() {
        let values = [22.0, 30.0, 18.0];
        let thresholds = vec![
            TernaryThreshold::range(18.0, 26.0), // 22 in range → 0
            TernaryThreshold::range(18.0, 26.0), // 30 above → +1
            TernaryThreshold::range(18.0, 26.0), // 18 at boundary → 0
        ];
        let state = TernaryRoomState::from_sensors(&values, &thresholds);
        assert_eq!(state.trits(), &[0, 1, 0]);
    }
}
