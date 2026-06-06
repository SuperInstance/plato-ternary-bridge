# plato-ternary-bridge

> **Every sensor reading is a ternary signal.**  
> An entire room's state — 8 sensors — becomes a single ternary vector. 8 trits packed into 2 bytes.

Bridges Plato room sensor values to ternary **{-1, 0, +1}** states, enabling ternary alarm evaluation, consensus voting across rooms, and compression of room state into compact ternary vectors.

---

## The Ternary Insight

Traditional sensor monitoring treats each reading as a continuous value — 22.3°C, 45% humidity, 1013 hPa — and compares it against thresholds using floating-point arithmetic. This works, but it misses a deeper structure:

**Every sensor reading can be reduced to exactly one of three states:**

| Trit | Meaning                     | Interpretation              |
|------|-----------------------------|-----------------------------|
| -1   | Under threshold             | Abnormal low                |
| 0    | Normal / in range           | No anomaly                  |
| +1   | Over threshold              | Abnormal high               |

This is the ternary insight. Once you make this reduction, powerful things happen:

- **An entire room's state** (8 sensors) becomes a single ternary vector — 8 trits packed into 2 bytes.
- **Room comparison** becomes a Hamming distance or dot product — trivial integer arithmetic.
- **Consensus across rooms** becomes a vote — each room casts {-1, 0, +1} — and you get fleet-wide decisions in O(N).
- **Alarm evaluation** becomes counting non-zero trits — the alarm vector tells you *what* and *where*.
- **History compression** becomes delta encoding on trits — most ticks are identical (all zeros), so deltas are tiny.

The ternary representation is the bridge between raw sensor data and the alarm/consensus/compression systems that act on it.

---

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│ Sensor Values│────▶│   Threshold   │────▶│ TernaryRoomState │────▶│ TernaryAlarm │
│ (f64 × N)   │     │  Converter    │     │  (i8 × N packed) │     │ (evaluation) │
└─────────────┘     └──────────────┘     └────────┬────────┘     └──────────────┘
                                                   │
                                          ┌────────▼────────┐
                                          │ FleetVote        │
                                          │ (consensus across│
                                          │  rooms)          │
                                          └──────────────────┘
                                                   │
                                          ┌────────▼────────┐
                                          │ TernaryCompression│
                                          │ (delta + RLE)    │
                                          └──────────────────┘
```

### Module Overview

| Module              | Purpose                                              |
|---------------------|------------------------------------------------------|
| `threshold`         | Convert sensor values → trits with various strategies |
| `room_state`        | Pack trits into compact u32, compare rooms           |
| `fleet_vote`        | Ternary consensus voting across rooms                |
| `alarm_ternary`     | Evaluate alarms from ternary state                   |
| `compression`       | Delta and RLE compression of tick history            |

---

## Threshold Types

### SingleThreshold

A single point threshold. The simplest conversion:

```
value > threshold → +1
value < threshold → -1
value == threshold → 0
```

Use when you only care about one direction of deviation (e.g., CO₂ above 1000ppm is bad, below is fine — but you still want the ternary distinction).

### RangeThreshold

A band `[low, high]`. Values inside the band are normal (0), outside are abnormal:

```
value < low  → -1
value > high → +1
otherwise   → 0
```

This is the most common threshold for environmental sensors — temperature between 18–26°C is comfortable, below is cold, above is hot.

### DeadZone (with Hysteresis)

Like RangeThreshold but with a **dead band** around the boundaries to prevent oscillation. When a sensor value is in the dead band, it retains its previous state instead of flipping:

```
value ≤ dead_low          → -1 (definite)
value ≥ dead_high         → +1 (definite)
low ≤ value ≤ high        → 0 (definite normal)
dead_low < value < low    → retain previous (hysteresis)
high < value < dead_high  → retain previous (hysteresis)
```

This prevents the rapid flapping that occurs when a value hovers right at a threshold boundary — a common problem with temperature and humidity sensors.

```rust
use plato_ternary_bridge::threshold::{to_trit, TernaryThreshold};

let threshold = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, None);
assert_eq!(to_trit(22.0, &threshold), 0);   // safe zone
assert_eq!(to_trit(15.0, &threshold), -1);  // definite below
assert_eq!(to_trit(27.0, &threshold), 0);   // dead band, no prev → 0

let with_prev = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, Some(1));
assert_eq!(to_trit(27.0, &with_prev), 1);   // dead band, prev=+1 → stays +1
```

---

## TernaryRoomState

An entire room's sensor state as a ternary vector. Each sensor contributes one trit {-1, 0, +1}, and the whole vector packs into a `u32` (16 trits, 2 bits each).

### Packing

Trits are encoded as 2-bit values:

| Trit | Bits  |
|------|-------|
| -1   | 0b11  |
| 0    | 0b00  |
| +1   | 0b01  |

Bits are packed LSB-first: bits 0-1 = trit[0], bits 2-3 = trit[1], etc. This means a room with 8 sensors fits in just 16 bits — the lower half of a u32.

### Comparison

Two key metrics for comparing room states:

- **Hamming distance**: count of positions where trits differ. Quick measure of how different two rooms are.
- **Ternary dot product**: sum of (a_i × b_i). Ranges from -N to +N. High positive = strong agreement, high negative = strong disagreement, near zero = unrelated.

```rust
use plato_ternary_bridge::room_state::TernaryRoomState;

let room_a = TernaryRoomState::new(vec![1, 0, -1, 1, 0, -1, 1, 0]);
let room_b = TernaryRoomState::new(vec![1, 0, 1, -1, 0, -1, 1, 0]);

assert_eq!(room_a.hamming_distance(&room_b), 2);  // 2 positions differ
assert_eq!(room_a.dot_product(&room_b), 1);        // net agreement = 1

// Pack and round-trip
let packed = room_a.pack();
let unpacked = TernaryRoomState::unpack(packed, 8);
assert_eq!(room_a, unpacked);
```

### Display

Room states display as `{-1,0,+1}` strings:

```rust
let state = TernaryRoomState::new(vec![-1, 0, 1, 0]);
assert_eq!(format!("{}", state), "{-1,0,+1,0}");
```

---

## FleetVote: Ternary Consensus Across Rooms

Each room votes {-1, 0, +1} on a question (typically derived from its dominant trit). The fleet voting system aggregates these into a consensus.

### Voting Methods

1. **Majority vote**: The most common non-zero vote wins. Ties → 0. Requires quorum.
2. **Weighted vote**: Each room has a weight (e.g., proportional to sensor count or room importance). Highest total weight wins.
3. **Ternary consensus**: Requires >60% supermajority of non-zero votes. This is the strictest method — useful for alarm decisions where you want strong agreement.

### Quorum

All voting methods support a quorum parameter — the minimum number of non-zero votes required to produce a non-zero result. If quorum isn't met, the result is 0 (abstain), regardless of what the votes say.

```rust
use plato_ternary_bridge::fleet_vote::FleetVote;

// Majority: 3 votes +1, 1 vote -1
let result = FleetVote::majority(&[1, 1, 1, -1], 1);
assert_eq!(result.consensus, 1);

// Quorum not met: only 1 non-zero vote, need 3
let result = FleetVote::majority(&[1, 0, 0, 0], 3);
assert_eq!(result.consensus, 0);
assert!(!result.quorum_met);

// Consensus: 4 out of 5 = 80% → supermajority for +1
let result = FleetVote::consensus(&[1, 1, 1, 1, -1], 1);
assert_eq!(result.consensus, 1);
```

---

## TernaryAlarm: Alarm Evaluation

The ternary alarm system evaluates a room's ternary vector and produces an alarm with priority:

| Priority | Non-zero trits | Interpretation   |
|----------|----------------|------------------|
| None     | 0              | All normal       |
| Low      | 1–2            | Minor anomaly    |
| Medium   | 3–4            | Multiple issues  |
| High     | 5+             | Critical         |

Each alarmed sensor has a **severity** — its trit value (-1 or +1) — indicating the direction of the anomaly. The full alarm vector captures all alarmed positions.

```rust
use plato_ternary_bridge::alarm_ternary::{TernaryAlarm, AlarmPriority};
use plato_ternary_bridge::room_state::TernaryRoomState;

let state = TernaryRoomState::new(vec![-1, 0, 1, -1]);
let alarm = TernaryAlarm::evaluate(state);

assert!(alarm.is_alarm());
assert_eq!(alarm.alarm_count(), 3);
assert_eq!(alarm.priority(), AlarmPriority::Medium);
assert_eq!(alarm.alarm_vector(), vec![-1, 1, -1]);
```

---

## TernaryCompression: Tick History Compression

Room state histories are highly compressible because:
- Most ticks are normal (all zeros)
- Changes are rare — a sensor might flip once every few hundred ticks
- The ternary domain (3 values) is much smaller than the original sensor domain

### Delta Encoding

Stores only the *changes* between consecutive ticks. For N ticks, the encoding is:
1. Initial state (full)
2. For each subsequent tick: list of (position, new_value) pairs

Ticks with no changes produce empty delta entries (no-ops). This is extremely compact for histories where most ticks are identical.

### Run-Length Encoding (RLE)

Compresses consecutive identical states into (state, count) pairs. A room that's normal for 800 consecutive ticks becomes a single run: `({0,0,0,0}, 800)`.

Both methods are **lossless** — decompression produces the exact original history.

```rust
use plato_ternary_bridge::compression::TernaryCompression;
use plato_ternary_bridge::room_state::TernaryRoomState;

let normal = TernaryRoomState::new(vec![0, 0, 0, 0]);
let anomaly = TernaryRoomState::new(vec![1, 0, -1, 0]);

let history = vec![
    normal.clone(), normal.clone(), normal.clone(),
    anomaly.clone(),
    normal.clone(), normal.clone(),
];

// Delta compression
let compressed = TernaryCompression::delta_encode(&history).unwrap();
let decompressed = TernaryCompression::delta_decode(&compressed);
assert_eq!(decompressed, history);

// RLE compression
let rle = TernaryCompression::rle_encode(&history);
// 3 runs: (normal×3), (anomaly×1), (normal×2)
assert_eq!(rle.runs.len(), 3);
```

---

## Connection to the Ternary Ecosystem

`plato-ternary-bridge` fits into a larger ternary monitoring ecosystem:

- **`ternary-types`** — Core ternary types and abstractions
- **`ternary-consensus`** — Consensus algorithms for ternary voting
- **`plato-engine-block`** — The Plato monitoring engine that consumes ternary state
- **`plato-ternary-bridge`** — *This crate* — the bridge between raw sensor data and the ternary world

The flow is:

```
Plato Sensors → plato-ternary-bridge → ternary-types → plato-engine-block
                                      ↘ ternary-consensus (fleet voting)
```

`plato-ternary-bridge` is the entry point — it's where continuous sensor values become discrete ternary signals, and where the simplicity of the ternary model starts paying dividends in compression, comparison, and consensus.

---

## API Quick Reference

```rust
// Threshold conversion
use plato_ternary_bridge::threshold::{to_trit, TernaryThreshold};
let t = TernaryThreshold::range(18.0, 26.0);
let trit = to_trit(30.0, &t); // → +1

// Room state
use plato_ternary_bridge::room_state::TernaryRoomState;
let state = TernaryRoomState::from_sensors(&[22.0, 30.0], &[t.clone(), t.clone()]);
let packed = state.pack();

// Alarm
use plato_ternary_bridge::alarm_ternary::TernaryAlarm;
let alarm = TernaryAlarm::evaluate(state);

// Fleet voting
use plato_ternary_bridge::fleet_vote::FleetVote;
let result = FleetVote::majority(&[1, 1, -1, 0], 2);

// Compression
use plato_ternary_bridge::compression::TernaryCompression;
let compressed = TernaryCompression::delta_encode(&history).unwrap();
let restored = TernaryCompression::delta_decode(&compressed);
```

---

## Performance

The ternary representation is designed for speed:

- **Threshold conversion**: O(1) per sensor, simple comparisons
- **Packing**: O(N) where N = trit count, bitwise operations only
- **Hamming distance**: O(N) integer comparisons
- **Dot product**: O(N) multiply-accumulate on i8
- **Fleet vote**: O(R) where R = number of rooms
- **Delta encoding**: O(T×N) where T = ticks, N = trits — but most iterations are no-ops
- **RLE encoding**: O(T) single pass

No allocations in hot paths beyond the initial construction. All arithmetic is integer. The ternary domain keeps everything in L1 cache.

---

## License

MIT
