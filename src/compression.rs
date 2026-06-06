//! TernaryCompression: compress room tick history.
//!
//! Delta encoding (only store trit changes) and run-length encoding for
//! constant ternary values.

use crate::room_state::TernaryRoomState;

/// A delta entry: which trit positions changed and to what value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaEntry {
    /// (position, new_trit_value)
    pub changes: Vec<(usize, i8)>,
}

impl DeltaEntry {
    fn new(changes: Vec<(usize, i8)>) -> Self {
        Self { changes }
    }

    /// Whether this delta has no changes.
    pub fn is_noop(&self) -> bool {
        self.changes.is_empty()
    }
}

/// An RLE run of identical room states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RleRun {
    /// The repeated state.
    pub state: TernaryRoomState,
    /// How many consecutive ticks this state repeats.
    pub count: usize,
}

/// Compressed room history using delta encoding.
#[derive(Debug, Clone)]
pub struct DeltaCompressed {
    /// The initial state at tick 0.
    pub initial: TernaryRoomState,
    /// Delta entries for each subsequent tick.
    pub deltas: Vec<DeltaEntry>,
}

/// Compressed room history using RLE.
#[derive(Debug, Clone)]
pub struct RleCompressed {
    /// Sequence of runs.
    pub runs: Vec<RleRun>,
}

/// Compression engine for ternary room state histories.
pub struct TernaryCompression;

impl TernaryCompression {
    /// Delta-encode a history of room states.
    /// Stores only the changes between consecutive ticks.
    pub fn delta_encode(history: &[TernaryRoomState]) -> Option<DeltaCompressed> {
        if history.is_empty() {
            return None;
        }

        let initial = history[0].clone();
        let trit_count = initial.len();
        let mut deltas = Vec::with_capacity(history.len().saturating_sub(1));

        for i in 1..history.len() {
            let mut changes = Vec::new();
            for pos in 0..trit_count {
                if history[i].trits()[pos] != history[i - 1].trits()[pos] {
                    changes.push((pos, history[i].trits()[pos]));
                }
            }
            deltas.push(DeltaEntry::new(changes));
        }

        Some(DeltaCompressed { initial, deltas })
    }

    /// Decompress delta-encoded history back to original ticks.
    pub fn delta_decode(compressed: &DeltaCompressed) -> Vec<TernaryRoomState> {
        let mut result = vec![compressed.initial.clone()];
        let trit_count = compressed.initial.len();
        let mut current = compressed.initial.clone();

        for delta in &compressed.deltas {
            let mut next_trits = current.trits().to_vec();
            for &(pos, val) in &delta.changes {
                next_trits[pos] = val;
            }
            let next = TernaryRoomState::new(next_trits);
            result.push(next.clone());
            current = next;
        }

        result
    }

    /// RLE-encode a history of room states.
    /// Consecutive identical states are compressed into (state, count) pairs.
    pub fn rle_encode(history: &[TernaryRoomState]) -> RleCompressed {
        let mut runs: Vec<RleRun> = Vec::new();

        for state in history {
            if let Some(last) = runs.last_mut() {
                if last.state == *state {
                    last.count += 1;
                    continue;
                }
            }
            runs.push(RleRun {
                state: state.clone(),
                count: 1,
            });
        }

        RleCompressed { runs }
    }

    /// Decompress RLE-encoded history back to original ticks.
    pub fn rle_decode(compressed: &RleCompressed) -> Vec<TernaryRoomState> {
        let mut result = Vec::new();
        for run in &compressed.runs {
            for _ in 0..run.count {
                result.push(run.state.clone());
            }
        }
        result
    }

    /// Compress and decompress to verify lossless round-trip.
    pub fn roundtrip_check(history: &[TernaryRoomState]) -> bool {
        if let Some(compressed) = Self::delta_encode(history) {
            let decompressed = Self::delta_decode(&compressed);
            decompressed == history
        } else {
            history.is_empty()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(trits: Vec<i8>) -> TernaryRoomState {
        TernaryRoomState::new(trits)
    }

    #[test]
    fn delta_encode_empty() {
        let result = TernaryCompression::delta_encode(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn delta_encode_single() {
        let history = vec![make_state(vec![0, 1, -1])];
        let compressed = TernaryCompression::delta_encode(&history).unwrap();
        assert_eq!(compressed.deltas.len(), 0);
    }

    #[test]
    fn delta_round_trip() {
        let history = vec![
            make_state(vec![0, 1, -1]),
            make_state(vec![1, 1, -1]),
            make_state(vec![1, 0, -1]),
            make_state(vec![1, 0, 0]),
        ];
        let compressed = TernaryCompression::delta_encode(&history).unwrap();
        let decompressed = TernaryCompression::delta_decode(&compressed);
        assert_eq!(decompressed, history);
    }

    #[test]
    fn delta_noop_when_unchanged() {
        let state = make_state(vec![0, 1, -1]);
        let history = vec![state.clone(), state.clone(), state.clone()];
        let compressed = TernaryCompression::delta_encode(&history).unwrap();
        assert!(compressed.deltas[0].is_noop());
        assert!(compressed.deltas[1].is_noop());
    }

    #[test]
    fn rle_constant_values() {
        let state = make_state(vec![1, 1, 1]);
        let history = vec![state.clone(); 10];
        let compressed = TernaryCompression::rle_encode(&history);
        assert_eq!(compressed.runs.len(), 1);
        assert_eq!(compressed.runs[0].count, 10);
    }

    #[test]
    fn rle_round_trip() {
        let history = vec![
            make_state(vec![0, 1]),
            make_state(vec![0, 1]),
            make_state(vec![1, 0]),
            make_state(vec![1, 0]),
            make_state(vec![1, 0]),
        ];
        let compressed = TernaryCompression::rle_encode(&history);
        let decompressed = TernaryCompression::rle_decode(&compressed);
        assert_eq!(decompressed, history);
    }

    #[test]
    fn rle_empty() {
        let compressed = TernaryCompression::rle_encode(&[]);
        assert!(compressed.runs.is_empty());
        let decompressed = TernaryCompression::rle_decode(&compressed);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn roundtrip_check_passes() {
        let history = vec![
            make_state(vec![0, 0]),
            make_state(vec![1, 0]),
            make_state(vec![1, -1]),
        ];
        assert!(TernaryCompression::roundtrip_check(&history));
    }

    #[test]
    fn compress_1000_ticks() {
        // Simulate 1000 ticks with occasional changes
        let mut history = Vec::with_capacity(1000);
        let base = make_state(vec![0, 0, 0, 0]);
        for i in 0..1000 {
            if i % 100 == 0 && i > 0 {
                // Change every 100 ticks
                history.push(make_state(vec![1, 0, 0, 0]));
            } else {
                history.push(base.clone());
            }
        }
        let compressed = TernaryCompression::delta_encode(&history).unwrap();
        let decompressed = TernaryCompression::delta_decode(&compressed);
        assert_eq!(decompressed.len(), 1000);
        assert_eq!(decompressed, history);
    }
}
