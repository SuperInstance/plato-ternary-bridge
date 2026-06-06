//! TernaryAlarm: alarm evaluation using ternary state.
//!
//! Non-zero trit = alarm condition, magnitude = severity.
//! Multiple sensor alarms → ternary alarm vector.

use crate::room_state::TernaryRoomState;

/// Priority level based on alarm count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlarmPriority {
    /// No alarms (all trits zero).
    None = 0,
    /// 1-2 non-zero trits (low severity).
    Low = 1,
    /// 3-4 non-zero trits (medium severity).
    Medium = 2,
    /// 5+ non-zero trits (high severity).
    High = 3,
}

impl AlarmPriority {
    fn from_count(count: usize) -> Self {
        match count {
            0 => Self::None,
            1..=2 => Self::Low,
            3..=4 => Self::Medium,
            _ => Self::High,
        }
    }
}

/// An alarm derived from ternary room state.
#[derive(Debug, Clone)]
pub struct TernaryAlarm {
    /// The room state that triggered this alarm.
    state: TernaryRoomState,
    /// Indices of sensors with non-zero trits.
    alarmed_indices: Vec<usize>,
    /// Priority based on alarm count.
    priority: AlarmPriority,
}

impl TernaryAlarm {
    /// Evaluate a room state and produce an alarm.
    pub fn evaluate(state: TernaryRoomState) -> Self {
        let alarmed_indices: Vec<usize> = state
            .trits()
            .iter()
            .enumerate()
            .filter(|(_, &t)| t != 0)
            .map(|(i, _)| i)
            .collect();

        let priority = AlarmPriority::from_count(alarmed_indices.len());

        Self {
            state,
            alarmed_indices,
            priority,
        }
    }

    /// Whether any alarm is active.
    pub fn is_alarm(&self) -> bool {
        !self.alarmed_indices.is_empty()
    }

    /// Number of alarmed sensors.
    pub fn alarm_count(&self) -> usize {
        self.alarmed_indices.len()
    }

    /// Get the alarm priority.
    pub fn priority(&self) -> AlarmPriority {
        self.priority
    }

    /// Get the alarmed sensor indices.
    pub fn alarmed_indices(&self) -> &[usize] {
        &self.alarmed_indices
    }

    /// Get the trit value for an alarmed sensor.
    pub fn severity_at(&self, index: usize) -> Option<i8> {
        if self.alarmed_indices.contains(&index) {
            Some(self.state.trits()[index])
        } else {
            None
        }
    }

    /// Get the underlying room state.
    pub fn state(&self) -> &TernaryRoomState {
        &self.state
    }

    /// Create an alarm vector: the trit values for all alarmed sensors.
    pub fn alarm_vector(&self) -> Vec<i8> {
        self.alarmed_indices
            .iter()
            .map(|&i| self.state.trits()[i])
            .collect()
    }
}

impl std::fmt::Display for TernaryAlarm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.is_alarm() {
            write!(f, "OK")
        } else {
            write!(
                f,
                "ALARM {:?} [{}] {:?}",
                self.priority,
                self.alarmed_indices
                    .iter()
                    .map(|&i| format!("{}:{}", i, if self.state.trits()[i] == 1 { "+1" } else { "-1" }))
                    .collect::<Vec<_>>()
                    .join(","),
                self.alarm_vector()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_alarm_all_zero() {
        let state = TernaryRoomState::new(vec![0, 0, 0, 0]);
        let alarm = TernaryAlarm::evaluate(state);
        assert!(!alarm.is_alarm());
        assert_eq!(alarm.priority(), AlarmPriority::None);
        assert_eq!(alarm.alarm_count(), 0);
    }

    #[test]
    fn single_sensor_alarm_positive() {
        let state = TernaryRoomState::new(vec![0, 1, 0, 0]);
        let alarm = TernaryAlarm::evaluate(state);
        assert!(alarm.is_alarm());
        assert_eq!(alarm.priority(), AlarmPriority::Low);
        assert_eq!(alarm.alarm_count(), 1);
        assert_eq!(alarm.severity_at(1), Some(1));
    }

    #[test]
    fn single_sensor_alarm_negative() {
        let state = TernaryRoomState::new(vec![0, 0, -1, 0]);
        let alarm = TernaryAlarm::evaluate(state);
        assert!(alarm.is_alarm());
        assert_eq!(alarm.alarm_count(), 1);
        assert_eq!(alarm.severity_at(2), Some(-1));
    }

    #[test]
    fn multi_sensor_alarm() {
        let state = TernaryRoomState::new(vec![-1, 0, 1, -1]);
        let alarm = TernaryAlarm::evaluate(state);
        assert_eq!(alarm.alarm_count(), 3);
        assert_eq!(alarm.priority(), AlarmPriority::Medium);
        assert_eq!(alarm.alarm_vector(), vec![-1, 1, -1]);
    }

    #[test]
    fn high_priority_alarm() {
        let state = TernaryRoomState::new(vec![-1, 1, -1, 1, -1]);
        let alarm = TernaryAlarm::evaluate(state);
        assert_eq!(alarm.alarm_count(), 5);
        assert_eq!(alarm.priority(), AlarmPriority::High);
    }

    #[test]
    fn display_ok() {
        let state = TernaryRoomState::new(vec![0, 0]);
        let alarm = TernaryAlarm::evaluate(state);
        assert_eq!(format!("{}", alarm), "OK");
    }

    #[test]
    fn display_alarm() {
        let state = TernaryRoomState::new(vec![1, 0, -1]);
        let alarm = TernaryAlarm::evaluate(state);
        let s = format!("{}", alarm);
        assert!(s.contains("ALARM"));
        assert!(s.contains("Low"));
    }
}
