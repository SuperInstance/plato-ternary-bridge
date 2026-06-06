# Plug & Play — plato-ternary-bridge

> Copy these templates. Change the thresholds and sensor names. You're converting.

---

## Pattern 1: Sensor → Trit Conversion

Convert raw sensor values to ternary signals with threshold-based mapping.

```rust
use plato_ternary_bridge::threshold::{to_trit, TernaryThreshold};

fn main() {
    // ↓ Change these thresholds for your sensors ↓
    let thresholds = vec![
        ("coolant_temp", TernaryThreshold::range(80.0, 100.0)),
        ("oil_pressure", TernaryThreshold::single(20.0)),
        ("battery_v",    TernaryThreshold::range(11.5, 14.5)),
    ];

    // ↓ Change these sensor values (from your hardware) ↓
    let readings = vec![92.0, 55.0, 12.8];

    for ((name, threshold), value) in thresholds.iter().zip(&readings) {
        let trit = to_trit(*value, threshold);
        let status = match trit { -1 => "LOW", 0 => "OK", 1 => "HIGH", _ => "?" };
        println!("{}: {:.1} → {} ({})", name, value, trit, status);
    }
}
```

**Change:** threshold values, sensor names, reading sources.

---

## Pattern 2: Full Room State + Alarm Evaluation

Convert an entire room's sensors to a packed ternary state and evaluate alarms.

```rust
use plato_ternary_bridge::room_state::TernaryRoomState;
use plato_ternary_bridge::threshold::TernaryThreshold;
use plato_ternary_bridge::alarm_ternary::TernaryAlarm;

fn main() {
    // ↓ Sensor values (replace with real reads) ↓
    let sensor_values: Vec<f64> = vec![92.0, 25.0, 55.0, 12.8, 1800.0, 35.0, 1013.0, 0.5];

    // ↓ Per-sensor thresholds ↓
    let thresholds = vec![
        TernaryThreshold::range(80.0, 100.0),   // coolant_temp
        TernaryThreshold::range(18.0, 26.0),    // ambient_temp
        TernaryThreshold::single(20.0),          // oil_pressure
        TernaryThreshold::range(11.5, 14.5),    // battery_voltage
        TernaryThreshold::range(600.0, 2000.0), // engine_rpm
        TernaryThreshold::range(30.0, 70.0),    // humidity
        TernaryThreshold::range(980.0, 1040.0), // barometric
        TernaryThreshold::range(0.0, 1.0),      // bilge_level
    ];

    // Convert → pack → evaluate
    let state = TernaryRoomState::from_sensors(&sensor_values, &thresholds);
    println!("Ternary state: {}", state);
    println!("Packed: 0x{:08X}", state.pack());

    let alarm = TernaryAlarm::evaluate(state);
    if alarm.is_alarm() {
        println!("⚠️  Alarm! {} sensors anomalous, priority: {:?}",
            alarm.alarm_count(), alarm.priority());
    } else {
        println!("✓ All sensors normal");
    }
}
```

**Change:** sensor values (from hardware), thresholds (for your environment), sensor count.

---

## Pattern 3: Fleet Consensus + History Compression

Multiple rooms vote on fleet-wide status and compress tick history.

```rust
use plato_ternary_bridge::fleet_vote::FleetVote;
use plato_ternary_bridge::room_state::TernaryRoomState;
use plato_ternary_bridge::compression::TernaryCompression;

fn main() {
    // ↓ Each room votes based on its dominant ternary state ↓
    let room_votes: Vec<i8> = vec![1, 1, 0, 1, -1];  // 5 rooms

    // Majority vote (quorum = 3)
    let result = FleetVote::majority(&room_votes, 3);
    println!("Fleet consensus: {} (quorum: {})",
        match result.consensus { 1 => "ALERT", -1 => "UNDER", _ => "NORMAL" },
        if result.quorum_met { "met" } else { "NOT met" },
    );

    // Compress tick history
    let normal = TernaryRoomState::new(vec![0, 0, 0, 0]);
    let alert  = TernaryRoomState::new(vec![1, 0, -1, 0]);

    // ↓ Replace with your actual history ↓
    let history: Vec<TernaryRoomState> = (0..1000).map(|i| {
        if i % 300 == 100 { alert.clone() } else { normal.clone() }
    }).collect();

    let rle = TernaryCompression::rle_encode(&history);
    println!("Compressed: {} runs from {} ticks ({:.1}% reduction)",
        rle.runs.len(),
        history.len(),
        (1.0 - rle.runs.len() as f64 / history.len() as f64) * 100.0,
    );
}
```

**Change:** room votes, quorum threshold, history source, sensor count.

---

## Quick Reference

| What | API | Example |
|------|-----|---------|
| Single threshold | `TernaryThreshold::single(val)` | `single(100.0)` — above = +1 |
| Range threshold | `TernaryThreshold::range(lo, hi)` | `range(18.0, 26.0)` — in range = 0 |
| Dead zone | `TernaryThreshold::dead_zone(lo, hi, dlo, dhi, prev)` | Hysteresis near boundaries |
| Convert value | `to_trit(value, &threshold)` | Returns `-1`, `0`, or `+1` |
| Room from sensors | `TernaryRoomState::from_sensors(&vals, &thresholds)` | Batch conversion |
| Pack room state | `state.pack()` | Returns `u32` (16 trits max) |
| Unpack room state | `TernaryRoomState::unpack(packed, len)` | Round-trip |
| Compare rooms | `room_a.hamming_distance(&room_b)` | Count differences |
| Evaluate alarm | `TernaryAlarm::evaluate(state)` | Priority + alarm vector |
| Fleet majority | `FleetVote::majority(&votes, quorum)` | Most common wins |
| Fleet consensus | `FleetVote::consensus(&votes, quorum)` | >60% supermajority |
| Delta compress | `TernaryCompression::delta_encode(&history)` | Changes only |
| RLE compress | `TernaryCompression::rle_encode(&history)` | Run-length pairs |
