//! # Plato Ternary Bridge
//!
//! Bridges Plato room sensor values to ternary {-1, 0, +1} states, enabling
//! ternary alarm evaluation, consensus voting across rooms, and compression
//! of room state into compact ternary vectors.
//!
//! ## The Ternary Insight
//!
//! Every sensor reading can be reduced to a ternary signal:
//! - **-1** (under threshold / abnormal low)
//! - **0** (normal / in range)
//! - **+1** (over threshold / abnormal high)
//!
//! An entire room's state (8 sensors) becomes a single ternary vector (8 trits = 2 bytes packed).
//!
//! ## Modules
//!
//! - [`threshold`] — Convert sensor values to trits using various threshold strategies
//! - [`room_state`] — Represent an entire room as a ternary vector
//! - [`fleet_vote`] — Ternary consensus voting across rooms
//! - [`alarm_ternary`] — Alarm evaluation using ternary state
//! - [`compression`] — Compress room tick history with delta/RLE encoding

pub mod alarm_ternary;
pub mod compression;
pub mod fleet_vote;
pub mod room_state;
pub mod threshold;

#[cfg(test)]
mod integration_tests {
    use crate::alarm_ternary::{AlarmPriority, TernaryAlarm};
    use crate::compression::TernaryCompression;
    use crate::fleet_vote::FleetVote;
    use crate::room_state::TernaryRoomState;
    use crate::threshold::{to_trit, TernaryThreshold};

    /// Full pipeline: sensor → threshold → ternary state → alarm → consensus
    #[test]
    fn full_pipeline() {
        // Room A: 4 sensors
        let room_a_values = [22.0, 30.0, 15.0, 25.0];
        let room_a_thresholds = vec![
            TernaryThreshold::range(18.0, 26.0), // 22 → 0
            TernaryThreshold::range(18.0, 26.0), // 30 → +1
            TernaryThreshold::range(18.0, 26.0), // 15 → -1
            TernaryThreshold::single(24.0),      // 25 → +1
        ];
        let state_a = TernaryRoomState::from_sensors(&room_a_values, &room_a_thresholds);
        assert_eq!(state_a.trits(), &[0, 1, -1, 1]);

        // Alarm evaluation
        let alarm = TernaryAlarm::evaluate(state_a.clone());
        assert!(alarm.is_alarm());
        assert_eq!(alarm.alarm_count(), 3);
        assert_eq!(alarm.priority(), AlarmPriority::Medium);

        // Pack and round-trip
        let packed = state_a.pack();
        let unpacked = TernaryRoomState::unpack(packed, 4);
        assert_eq!(state_a, unpacked);
    }

    /// Two rooms vote on a shared condition.
    #[test]
    fn two_rooms_vote() {
        // Room A: mostly alarming high → votes +1
        let state_a = TernaryRoomState::new(vec![1, 1, 1, 0]);
        // Room B: mixed → votes 0 (no consensus)
        let state_b = TernaryRoomState::new(vec![1, -1, 0, 0]);

        // Extract dominant vote per room (sum of trits)
        let vote_a: i8 = if state_a.trits().iter().sum::<i8>() > 0 { 1 } else if state_a.trits().iter().sum::<i8>() < 0 { -1 } else { 0 };
        let vote_b: i8 = if state_b.trits().iter().sum::<i8>() > 0 { 1 } else if state_b.trits().iter().sum::<i8>() < 0 { -1 } else { 0 };

        assert_eq!(vote_a, 1);
        assert_eq!(vote_b, 0);

        let result = FleetVote::majority(&[vote_a, vote_b], 1);
        // 1 vs 0: +1 wins (pos=1 > neg=0, pos > zero? pos=1, zero=1 → tie → 0)
        // Actually both are equal (pos=1, zero=1) → tie → 0
        assert_eq!(result.consensus, 0);

        // Now with 3 rooms all voting +1
        let votes = vec![1, 1, 1];
        let result = FleetVote::majority(&votes, 1);
        assert_eq!(result.consensus, 1);
    }

    /// Compress 1000 ticks of room history.
    #[test]
    fn compress_1000_ticks() {
        let mut history = Vec::with_capacity(1000);
        let normal = TernaryRoomState::new(vec![0, 0, 0, 0, 0, 0, 0, 0]);

        for i in 0..1000 {
            if i % 200 == 0 && i > 0 {
                // Anomalous tick
                history.push(TernaryRoomState::new(vec![1, 0, -1, 0, 1, 0, 0, 0]));
            } else {
                history.push(normal.clone());
            }
        }

        // Delta compression
        let compressed = TernaryCompression::delta_encode(&history).unwrap();
        assert!(compressed.deltas.len() < 1000); // Should be much smaller
        let decompressed = TernaryCompression::delta_decode(&compressed);
        assert_eq!(decompressed, history);

        // RLE compression
        let rle = TernaryCompression::rle_encode(&history);
        // Most ticks are identical → few runs
        assert!(rle.runs.len() < 20);
        let rle_decompressed = TernaryCompression::rle_decode(&rle);
        assert_eq!(rle_decompressed, history);
    }

    /// Sensor → DeadZone → trit, verify hysteresis chain.
    #[test]
    fn deadzone_hysteresis_chain() {
        let mut prev = None;
        let values = [28.0, 27.0, 26.5, 26.0, 25.0];
        // Safe zone: [18, 26], dead band: [16, 28]
        let mut results = Vec::new();
        for v in values {
            let t = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, prev);
            let trit = to_trit(v, &t);
            results.push(trit);
            prev = Some(trit);
        }
        // 28 → +1, 27 (dead band, prev=+1) → +1, 26.5 (dead band, prev=+1) → +1,
        // 26 (safe) → 0, 25 (safe) → 0
        assert_eq!(results, vec![1, 1, 1, 0, 0]);
    }

    /// Hamming distance and dot product between two rooms.
    #[test]
    fn room_similarity_measures() {
        let room_a = TernaryRoomState::new(vec![1, 0, -1, 1, 0, -1, 1, 0]);
        let room_b = TernaryRoomState::new(vec![1, 0, 1, -1, 0, -1, 1, 0]);

        // 2 positions differ (index 2 and 3)
        assert_eq!(room_a.hamming_distance(&room_b), 2);

        // Dot: 1+0+(-1)+(-1)+0+1+1+0 = 1
        assert_eq!(room_a.dot_product(&room_b), 1);
    }
}
