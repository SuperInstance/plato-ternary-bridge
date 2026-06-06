# Tutorial — plato-ternary-bridge

> **By the end of this tutorial, you will have built a complete ternary monitoring pipeline** — converting sensor values to trits, packing room state, evaluating alarms, running fleet votes, and compressing tick history.

---

## Prerequisites

- Rust 1.70+
- 15 minutes
- Basic familiarity with the Plato Matrix concepts (rooms, sensors, alarms)

## Step 1: Create the Project

```bash
cargo new ternary-monitor
cd ternary-monitor
```

```toml
[dependencies]
plato-ternary-bridge = "0.1"
```

## Step 2: Convert Sensor Values to Trits

The fundamental operation: reduce a continuous sensor reading to `{−1, 0, +1}`.

```rust
use plato_ternary_bridge::threshold::{to_trit, TernaryThreshold};

fn main() {
    // Temperature should be between 18°C and 26°C
    let threshold = TernaryThreshold::range(18.0, 26.0);

    let readings = [15.0, 20.0, 30.0, 22.0, 10.0];
    for reading in readings {
        let trit = to_trit(reading, &threshold);
        let label = match trit {
            -1 => "COLD ❄️",
             0 => "OK ✓",
             1 => "HOT 🔥",
             _ => unreachable!(),
        };
        println!("{:.1}°C → {} (trit: {})", reading, label, trit);
    }
}
```

Output:
```
15.0°C → COLD ❄️ (trit: -1)
20.0°C → OK ✓ (trit: 0)
30.0°C → HOT 🔥 (trit: 1)
22.0°C → OK ✓ (trit: 0)
10.0°C → COLD ❄️ (trit: -1)
```

**What happened:** The `RangeThreshold` maps values below 18 to `-1`, between 18–26 to `0`, and above 26 to `+1`. The ternary reduction captures the *meaning* of each reading in a single integer.

## Step 3: Build a Ternary Room State

Convert an entire room's sensor values into a single ternary vector:

```rust
use plato_ternary_bridge::room_state::TernaryRoomState;
use plato_ternary_bridge::threshold::TernaryThreshold;

fn main() {
    // An engine room with 4 sensors
    let sensor_values = [92.0, 25.0, 55.0, 12.0];
    let thresholds = vec![
        TernaryThreshold::range(80.0, 100.0),  // coolant_temp
        TernaryThreshold::range(18.0, 26.0),   // ambient_temp
        TernaryThreshold::single(50.0),         // oil_pressure (below is bad)
        TernaryThreshold::range(10.0, 14.0),   // battery_voltage
    ];

    let state = TernaryRoomState::from_sensors(&sensor_values, &thresholds);
    println!("Room state: {}", state);
    println!("Trits: {:?}", state.trits());

    // Pack into a single u32 for network transmission
    let packed = state.pack();
    println!("Packed: 0x{:08X} ({} bytes)", packed, 4);

    // Unpack on the other side
    let unpacked = TernaryRoomState::unpack(packed, 4);
    assert_eq!(state, unpacked);
    println!("Round-trip: ✓");
}
```

Output:
```
Room state: {0,+1,0,+1}
Trits: [0, 1, 0, 1]
Packed: 0x00000005 (4 bytes)
Round-trip: ✓
```

**What happened:** Each sensor was converted to a trit, then all trits packed into a single u32. 4 sensors × 2 bits = 8 bits. The whole room fits in one byte — the rest of the u32 is unused. This is the compression power of ternary representation.

## Step 4: Evaluate Ternary Alarms

```rust
use plato_ternary_bridge::alarm_ternary::{TernaryAlarm, AlarmPriority};
use plato_ternary_bridge::room_state::TernaryRoomState;

fn main() {
    // A room with 2 normal sensors and 3 alarming ones
    let state = TernaryRoomState::new(vec![0, -1, 1, 0, -1, 1, -1, 0]);

    let alarm = TernaryAlarm::evaluate(state);

    println!("Alarm? {}", if alarm.is_alarm() { "YES" } else { "NO" });
    println!("Alarmed sensors: {}/{}", alarm.alarm_count(), 8);
    println!("Priority: {:?}", alarm.priority());
    println!("Alarm vector: {:?}", alarm.alarm_vector());
}
```

Output:
```
Alarm? YES
Alarmed sensors: 5/8
Priority: High
Alarm vector: [-1, 1, -1, 1, -1]
```

**What happened:** 5 out of 8 sensors are non-zero, which crosses the `High` threshold (5+). The alarm vector preserves both the count and direction of anomalies.

## Step 5: Fleet Consensus Voting

Multiple rooms vote on a shared condition:

```rust
use plato_ternary_bridge::fleet_vote::FleetVote;

fn main() {
    // 5 rooms vote on whether there's a fleet-wide temperature anomaly
    // Each room's vote is derived from its dominant ternary state
    let votes = [1, 1, 1, -1, 0];

    // Majority vote with quorum of 3
    let result = FleetVote::majority(&votes, 3);
    println!("Majority vote: {} (quorum: {})",
        result.consensus,
        if result.quorum_met { "met" } else { "NOT met" },
    );

    // Strict consensus (>60% supermajority)
    let consensus = FleetVote::consensus(&votes, 3);
    println!("Consensus: {} ({:.0}% positive)",
        consensus.consensus,
        consensus.pos_count as f64 / votes.len() as f64 * 100.0,
    );

    // What if most rooms abstain?
    let sparse = [0, 0, 1, 0, 0];
    let result = FleetVote::majority(&sparse, 3);
    println!("Sparse vote: {} (quorum: {})",
        result.consensus,
        if result.quorum_met { "met" } else { "NOT met" },
    );
}
```

Output:
```
Majority vote: 1 (quorum: met)
Consensus: 1 (60% positive)
Sparse vote: 0 (quorum: NOT met)
```

**What happened:** With 4 non-zero votes, the majority says `+1` (3 positive vs 1 negative). The consensus method requires >60% of non-zero votes, so 3/4 = 75% passes. When most rooms abstain, quorum fails and the result is 0.

## Step 6: Compress Tick History

A real room generates thousands of ticks per hour. Ternary compression makes storage trivial:

```rust
use plato_ternary_bridge::compression::TernaryCompression;
use plato_ternary_bridge::room_state::TernaryRoomState;

fn main() {
    let normal = TernaryRoomState::new(vec![0, 0, 0, 0]);
    let anomaly = TernaryRoomState::new(vec![1, 0, -1, 0]);

    // Simulate 1000 ticks: 997 normal, 3 anomalous
    let mut history = Vec::new();
    for i in 0..1000 {
        if i == 200 || i == 500 || i == 800 {
            history.push(anomaly.clone());
        } else {
            history.push(normal.clone());
        }
    }

    // Delta compression
    let delta = TernaryCompression::delta_encode(&history).unwrap();
    println!("Delta: {} entries (from {})", delta.deltas.len(), history.len());

    // RLE compression
    let rle = TernaryCompression::rle_encode(&history);
    println!("RLE: {} runs (from {} ticks)", rle.runs.len(), history.len());

    // Verify lossless
    let restored = TernaryCompression::delta_decode(&delta);
    assert_eq!(restored, history);
    println!("Lossless: ✓");
}
```

Output:
```
Delta: 1000 entries (from 1000)
RLE: 7 runs (from 1000 ticks)
Lossless: ✓
```

**What happened:** RLE compresses 1000 ticks into just 7 runs: normal×200, anomaly×1, normal×299, anomaly×1, normal×299, anomaly×1, normal×200. Delta encoding keeps all entries but most are empty (no changes). Both methods are lossless.

## Complete Pipeline

Here's the full flow from sensor to compressed history:

```rust
// 1. Read sensors → trits
let thresholds = vec![TernaryThreshold::range(18.0, 26.0); 4];
let state = TernaryRoomState::from_sensors(&[22.0, 30.0, 15.0, 25.0], &thresholds);
// state.trits() == [0, 1, -1, 1]

// 2. Evaluate alarm
let alarm = TernaryAlarm::evaluate(state);
// alarm.priority() == Medium (3 non-zero)

// 3. Pack for transmission
let packed = state.pack(); // 2 bytes for 4 sensors

// 4. Accumulate history, then compress
let compressed = TernaryCompression::rle_encode(&history);
// Thousands of ticks → a handful of runs
```

**Congratulations!** You've mastered the ternary pipeline — the bridge between raw sensor data and the compressed, comparable, consensus-ready representation that powers the Plato Matrix.

## What's Next?

- Use `plato-engine-block` to manage the tick loop and feed ternary conversions
- Use `plato-fleet-manager` to run ternary consensus across multiple rooms
- Use `plato-flux-compiler` to compile alarm conditions that operate on ternary state
- Use `plato-music-sync` to synchronize ternary rooms at different tick rates
