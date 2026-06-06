//! Ternary threshold conversion: sensor values → trits {-1, 0, +1}

/// A threshold strategy for converting a sensor value to a ternary trit.
#[derive(Debug, Clone, PartialEq)]
pub enum TernaryThreshold {
    /// Single threshold: value > threshold → +1, < threshold → -1, else 0
    Single { threshold: f64 },
    /// Range threshold: in [low, high] → 0, below → -1, above → +1
    Range { low: f64, high: f64 },
    /// Dead-zone with hysteresis: like Range but with a dead band to prevent oscillation.
    /// `prev` holds the previous trit state for hysteresis logic.
    DeadZone {
        low: f64,
        high: f64,
        dead_low: f64,
        dead_high: f64,
        prev: Option<i8>,
    },
}

impl TernaryThreshold {
    /// Create a single-point threshold.
    pub fn single(threshold: f64) -> Self {
        Self::Single { threshold }
    }

    /// Create a range threshold [low, high].
    pub fn range(low: f64, high: f64) -> Self {
        Self::Range { low, high }
    }

    /// Create a dead-zone threshold with hysteresis.
    ///
    /// - `[low, high]` is the safe zone → 0
    /// - `[dead_low, dead_high]` is the hysteresis dead band
    /// - Outside the dead band → -1 or +1
    /// - Inside the dead band → retain previous state
    pub fn dead_zone(low: f64, high: f64, dead_low: f64, dead_high: f64, prev: Option<i8>) -> Self {
        Self::DeadZone {
            low,
            high,
            dead_low,
            dead_high,
            prev,
        }
    }

    /// Create a dead-zone with updated previous state (for chained evaluation).
    pub fn with_prev(&self, prev: i8) -> Self {
        match self {
            Self::DeadZone {
                low,
                high,
                dead_low,
                dead_high,
                ..
            } => Self::DeadZone {
                low: *low,
                high: *high,
                dead_low: *dead_low,
                dead_high: *dead_high,
                prev: Some(prev),
            },
            other => other.clone(),
        }
    }
}

/// Convert a sensor value to a trit {-1, 0, +1} using the given threshold.
pub fn to_trit(value: f64, threshold: &TernaryThreshold) -> i8 {
    match threshold {
        TernaryThreshold::Single { threshold: t } => {
            if value > *t {
                1
            } else if value < *t {
                -1
            } else {
                0
            }
        }
        TernaryThreshold::Range { low, high } => {
            if value < *low {
                -1
            } else if value > *high {
                1
            } else {
                0
            }
        }
        TernaryThreshold::DeadZone {
            low,
            high,
            dead_low,
            dead_high,
            prev,
        } => {
            // Outside the dead band: definite state
            if value <= *dead_low {
                -1
            } else if value >= *dead_high {
                1
            }
            // Inside the safe zone: definitely normal
            else if value >= *low && value <= *high {
                0
            }
            // In the dead band but not in safe zone: hysteresis
            else {
                prev.unwrap_or(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_above() {
        let t = TernaryThreshold::single(25.0);
        assert_eq!(to_trit(30.0, &t), 1);
    }

    #[test]
    fn single_below() {
        let t = TernaryThreshold::single(25.0);
        assert_eq!(to_trit(20.0, &t), -1);
    }

    #[test]
    fn single_equal() {
        let t = TernaryThreshold::single(25.0);
        assert_eq!(to_trit(25.0, &t), 0);
    }

    #[test]
    fn range_in_range() {
        let t = TernaryThreshold::range(18.0, 26.0);
        assert_eq!(to_trit(22.0, &t), 0);
    }

    #[test]
    fn range_below() {
        let t = TernaryThreshold::range(18.0, 26.0);
        assert_eq!(to_trit(15.0, &t), -1);
    }

    #[test]
    fn range_above() {
        let t = TernaryThreshold::range(18.0, 26.0);
        assert_eq!(to_trit(30.0, &t), 1);
    }

    #[test]
    fn range_at_boundary_low() {
        let t = TernaryThreshold::range(18.0, 26.0);
        assert_eq!(to_trit(18.0, &t), 0);
    }

    #[test]
    fn range_at_boundary_high() {
        let t = TernaryThreshold::range(18.0, 26.0);
        assert_eq!(to_trit(26.0, &t), 0);
    }

    #[test]
    fn dead_zone_definite_below() {
        let t = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, None);
        assert_eq!(to_trit(15.0, &t), -1);
    }

    #[test]
    fn dead_zone_definite_above() {
        let t = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, None);
        assert_eq!(to_trit(29.0, &t), 1);
    }

    #[test]
    fn dead_zone_safe_zone() {
        let t = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, None);
        assert_eq!(to_trit(22.0, &t), 0);
    }

    #[test]
    fn dead_zone_hysteresis_retains_prev_positive() {
        // Value 27.0 is in dead band (26..28), prev = +1 → stays +1
        let t = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, Some(1));
        assert_eq!(to_trit(27.0, &t), 1);
    }

    #[test]
    fn dead_zone_hysteresis_retains_prev_negative() {
        // Value 17.0 is in dead band (16..18), prev = -1 → stays -1
        let t = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, Some(-1));
        assert_eq!(to_trit(17.0, &t), -1);
    }

    #[test]
    fn dead_zone_hysteresis_default_zero() {
        // No previous state, in dead band → default 0
        let t = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, None);
        assert_eq!(to_trit(17.0, &t), 0);
    }

    #[test]
    fn dead_zone_prevents_oscillation() {
        // Simulate value oscillating around the high boundary
        let t_base = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, Some(0));
        // 26.5 is in dead band, prev=0 → stays 0
        assert_eq!(to_trit(26.5, &t_base), 0);

        // Now with prev=1, same value → stays 1
        let t_pos = TernaryThreshold::dead_zone(18.0, 26.0, 16.0, 28.0, Some(1));
        assert_eq!(to_trit(26.5, &t_pos), 1);
    }
}
