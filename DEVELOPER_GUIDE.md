# Developer Guide — plato-ternary-bridge

> Architecture deep-dive, module walkthrough, extension points, and contributing guide for the sensor-to-ternary conversion layer.

---

## Architecture Overview

`plato-ternary-bridge` is the translation layer between raw sensor values and the ternary representation used throughout the Plato Matrix. It converts continuous `f64` readings into discrete `{−1, 0, +1}` trits, packs them into compact binary representations, and provides alarm evaluation, fleet consensus, and history compression on top.

The ternary representation is the key insight: once sensor data is reduced to three states, an entire room (8 sensors) fits in 16 bits, room comparison becomes integer arithmetic, and history compresses to near-nothing.

### Data Flow

```
f64 sensor values
       │
       ▼
  Threshold Converter ──▶ trit values {-1, 0, +1}
       │
       ▼
  TernaryRoomState (Vec<i8> trits, packed u32)
       │
       ├──▶ TernaryAlarm (priority + alarm vector)
       ├──▶ FleetVote (consensus across rooms)
       └──▶ TernaryCompression (delta + RLE)
```

---

## Module-by-Module Walkthrough

### `threshold` — Sensor → Trit Conversion

The entry point. Three threshold strategies, each implementing different boundary semantics:

| Type | Constructor | Use When |
|------|------------|----------|
| `SingleThreshold` | `TernaryThreshold::single(value)` | One-sided checks (e.g., CO₂ above 1000ppm) |
| `RangeThreshold` | `TernaryThreshold::range(low, high)` | Band checks (e.g., temperature 18–26°C) |
| `DeadZone` | `TernaryThreshold::dead_zone(low, high, dead_low, dead_high, prev)` | Hysteresis needed (sensor oscillates near boundary) |

The core function: `to_trit(value: f64, threshold: &TernaryThreshold) -> i8`

Returns `-1`, `0`, or `+1`. The `DeadZone` variant requires an `Option<i8>` previous state for hysteresis — in the dead band, the previous trit is retained.

**Extension point:** Add new threshold types by extending the `TernaryThreshold` enum and adding a match arm in `to_trit()`. For example, a `MultiBandThreshold` could map sensor ranges to specific trit values.

### `room_state` — TernaryRoomState

Represents an entire room's sensor state as a ternary vector:

```rust
pub struct TernaryRoomState {
    trits: Vec<i8>,  // {-1, 0, +1} per sensor
}
```

Key operations:

| Method | Complexity | Description |
|--------|-----------|-------------|
| `new(trits)` | O(N) | Construct from raw trits |
| `from_sensors(values, thresholds)` | O(N) | Convert f64 values using per-sensor thresholds |
| `pack()` | O(N) | Encode into u32 (2 bits per trit) |
| `unpack(packed, len)` | O(N) | Decode from u32 |
| `hamming_distance(&other)` | O(N) | Count differing positions |
| `dot_product(&other)` | O(N) | Sum of (a_i × b_i) |

**Packing format:** Trits are 2-bit encoded: `-1 → 0b11`, `0 → 0b00`, `+1 → 0b01`. Packed LSB-first into u32. Up to 16 trits fit in a single u32.

**Extension point:** For rooms with >16 sensors, extend the packing to use `u64` or `Vec<u32>`. The current u32 gives 16 trits (enough for most rooms).

### `alarm_ternary` — Ternary Alarm Evaluation

Evaluates a `TernaryRoomState` and produces an alarm classification:

```rust
pub struct TernaryAlarm {
    non_zero_count: usize,
    alarm_trits: Vec<i8>,      // Only non-zero trits
    priority: AlarmPriority,
}
```

Priority levels based on non-zero trit count:

| Priority | Condition | Interpretation |
|----------|-----------|----------------|
| `None` | 0 non-zero | All sensors normal |
| `Low` | 1–2 non-zero | Minor anomaly |
| `Medium` | 3–4 non-zero | Multiple issues |
| `High` | 5+ non-zero | Critical |

The alarm vector preserves the direction of each anomaly (`-1` for under-threshold, `+1` for over-threshold), giving both *what* and *where*.

**Extension point:** Customize the priority thresholds by parameterizing `TernaryAlarm::evaluate()` or by implementing a trait. You could also weight sensors differently (e.g., temperature anomaly counts more than humidity).

### `fleet_vote` — Ternary Consensus

Aggregates votes from multiple rooms into a fleet-wide decision:

```rust
pub struct FleetVoteResult {
    pub consensus: i8,      // {-1, 0, +1}
    pub quorum_met: bool,
    pub pos_count: usize,
    pub neg_count: usize,
    pub zero_count: usize,
}
```

Three voting methods:

1. **`FleetVote::majority(votes, quorum)`** — Most common non-zero vote wins. Ties → 0.
2. **`FleetVote::weighted(votes, weights, quorum)`** — Weighted sum of votes. Highest absolute total wins.
3. **`FleetVote::consensus(votes, quorum)`** — Requires >60% supermajority of non-zero votes. Strictest method.

All methods require quorum: minimum number of non-zero votes before producing a non-zero result. If quorum isn't met, the result is 0 regardless.

**Extension point:** Add custom voting methods (e.g., veto-based, delegation) by implementing new functions on `FleetVote`. The quorum mechanism is generic and applies to any method.

### `compression` — Ternary History Compression

Two lossless compression strategies for tick history:

**Delta encoding:** Stores the initial state fully, then only changes (position, new_value) for subsequent ticks. No-change ticks produce empty entries. Extremely compact for histories where most ticks are identical.

**RLE (Run-Length Encoding):** Compresses consecutive identical states into `(state, count)` pairs. A room that's normal for 800 ticks becomes one run: `({0,0,...}, 800)`.

```rust
pub struct DeltaCompressed {
    pub initial: TernaryRoomState,
    pub deltas: Vec<Vec<(usize, i8)>>,  // Per-tick changes
}

pub struct RLECompressed {
    pub runs: Vec<(TernaryRoomState, usize)>,  // (state, count)
}
```

Both provide encode/decode pairs. Decompression always produces the exact original history.

**Extension point:** Add hybrid compression (delta + RLE combined) or entropy coding for histories with complex patterns. The compression module is self-contained and can be extended independently.

---

## Testing Strategy

The crate includes integration tests in `lib.rs` covering:

- **Full pipeline:** sensor → threshold → room state → alarm → consensus
- **Two-room voting:** majority, quorum enforcement, consensus supermajority
- **1000-tick compression:** delta and RLE compression with verification
- **DeadZone hysteresis:** oscillating values near boundaries
- **Room similarity:** Hamming distance and dot product correctness

Run with:

```bash
cargo test
```

---

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Threshold conversion | O(1) per sensor | Simple comparisons |
| Room state packing | O(N) trits | Bitwise operations only |
| Hamming distance | O(N) | Integer comparisons |
| Dot product | O(N) | Multiply-accumulate on i8 |
| Fleet vote | O(R) rooms | Single pass |
| Delta encode | O(T×N) | Most iterations are no-ops |
| RLE encode | O(T) | Single pass |

No heap allocations in hot paths beyond initial construction. All arithmetic is integer. The ternary domain keeps everything in L1 cache.

---

## Contributing Guide

### Adding a New Threshold Type

1. Add a variant to the `TernaryThreshold` enum in `threshold.rs`.
2. Implement the conversion logic in `to_trit()`.
3. Add a constructor method on `TernaryThreshold`.
4. Write tests: boundary cases, known values, hysteresis behavior.

### Adding a New Compression Method

1. Create new encode/decode functions in `compression.rs`.
2. Define a struct for the compressed representation.
3. Ensure lossless round-trip (encode → decode → compare).
4. Add benchmarks if compression ratio is a concern.

### Adding a New Voting Method

1. Add a new function to `FleetVote` in `fleet_vote.rs`.
2. Return `FleetVoteResult` with the consensus and quorum status.
3. Test with edge cases: single voter, all-zero votes, ties, quorum failure.

### Code Style

- All public types should derive `Debug, Clone, PartialEq`.
- Doc comments with examples on all public functions.
- Prefer `i8` for trit values — never use `u8` or `i32`.
- Keep `no_std` compatibility: avoid `std::` in core modules.
