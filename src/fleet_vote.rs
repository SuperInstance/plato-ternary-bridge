//! FleetVote: ternary consensus voting across rooms.
//!
//! Each room votes {-1, 0, +1} on a question. Supports majority vote,
//! weighted vote, quorum requirements, and ternary consensus.

/// Result of a fleet vote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteResult {
    /// The consensus value: -1, 0, or +1.
    pub consensus: i8,
    /// Total votes cast.
    pub total_votes: usize,
    /// Votes for each option [-1, 0, +1].
    pub counts: [usize; 3],
    /// Whether quorum was met.
    pub quorum_met: bool,
}

impl VoteResult {
    fn new(consensus: i8, total_votes: usize, counts: [usize; 3], quorum_met: bool) -> Self {
        Self {
            consensus,
            total_votes,
            counts,
            quorum_met,
        }
    }
}

/// A room's vote with optional weight.
#[derive(Debug, Clone)]
pub struct RoomVote {
    /// The vote value: -1, 0, or +1.
    pub vote: i8,
    /// Weight of this room's vote (default 1.0).
    pub weight: f64,
}

impl RoomVote {
    pub fn new(vote: i8) -> Self {
        assert!(vote == -1 || vote == 0 || vote == 1);
        Self { vote, weight: 1.0 }
    }

    pub fn with_weight(vote: i8, weight: f64) -> Self {
        assert!(vote == -1 || vote == 0 || vote == 1);
        Self { vote, weight }
    }
}

/// Fleet voting engine.
pub struct FleetVote;

impl FleetVote {
    /// Simple majority vote. Ties → 0 (abstain).
    /// Requires quorum (minimum number of non-zero votes) to return non-zero.
    pub fn majority(votes: &[i8], quorum: usize) -> VoteResult {
        let mut counts = [0usize; 3]; // [-1, 0, +1]
        let mut non_zero = 0usize;

        for &v in votes {
            let idx = (v + 1) as usize;
            counts[idx] += 1;
            if v != 0 {
                non_zero += 1;
            }
        }

        let quorum_met = non_zero >= quorum;

        let consensus = if !quorum_met {
            0
        } else {
            let neg = counts[0];
            let zero = counts[1];
            let pos = counts[2];

            if pos > neg && pos > zero {
                1
            } else if neg > pos && neg > zero {
                -1
            } else {
                0 // tie or zero is most common
            }
        };

        VoteResult::new(consensus, votes.len(), counts, quorum_met)
    }

    /// Weighted vote. Each room has a weight. The option with the highest
    /// total weight wins. Ties → 0.
    pub fn weighted(votes: &[RoomVote], quorum: usize) -> VoteResult {
        let mut weights = [0.0f64; 3]; // [-1, 0, +1]
        let mut counts = [0usize; 3];
        let mut non_zero_count = 0usize;

        for rv in votes {
            let idx = (rv.vote + 1) as usize;
            weights[idx] += rv.weight;
            counts[idx] += 1;
            if rv.vote != 0 {
                non_zero_count += 1;
            }
        }

        let quorum_met = non_zero_count >= quorum;

        let consensus = if !quorum_met {
            0
        } else {
            let w_neg = weights[0];
            let w_zero = weights[1];
            let w_pos = weights[2];

            if w_pos > w_neg && w_pos > w_zero {
                1
            } else if w_neg > w_pos && w_neg > w_zero {
                -1
            } else {
                0
            }
        };

        VoteResult::new(consensus, votes.len(), counts, quorum_met)
    }

    /// Ternary consensus: require supermajority (>60%) of non-zero votes
    /// to agree. Otherwise → 0.
    pub fn consensus(votes: &[i8], quorum: usize) -> VoteResult {
        let mut counts = [0usize; 3];
        let mut non_zero = 0usize;

        for &v in votes {
            counts[(v + 1) as usize] += 1;
            if v != 0 {
                non_zero += 1;
            }
        }

        let quorum_met = non_zero >= quorum;

        let consensus = if !quorum_met || non_zero == 0 {
            0
        } else {
            let pos_count = counts[2];
            let neg_count = counts[0];

            if pos_count as f64 / non_zero as f64 > 0.6 {
                1
            } else if neg_count as f64 / non_zero as f64 > 0.6 {
                -1
            } else {
                0
            }
        };

        VoteResult::new(consensus, votes.len(), counts, quorum_met)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unanimous_agreement() {
        let votes = vec![1, 1, 1, 1];
        let result = FleetVote::majority(&votes, 1);
        assert_eq!(result.consensus, 1);
        assert!(result.quorum_met);
    }

    #[test]
    fn majority_with_minority() {
        let votes = vec![1, 1, 1, -1];
        let result = FleetVote::majority(&votes, 1);
        assert_eq!(result.consensus, 1);
    }

    #[test]
    fn tie_returns_zero() {
        let votes = vec![1, -1];
        let result = FleetVote::majority(&votes, 1);
        assert_eq!(result.consensus, 0);
    }

    #[test]
    fn quorum_not_met() {
        let votes = vec![1, 0, 0, 0];
        let result = FleetVote::majority(&votes, 3);
        assert_eq!(result.consensus, 0);
        assert!(!result.quorum_met);
    }

    #[test]
    fn weighted_vote() {
        let votes = vec![
            RoomVote::with_weight(-1, 5.0),
            RoomVote::with_weight(1, 1.0),
            RoomVote::with_weight(1, 1.0),
        ];
        let result = FleetVote::weighted(&votes, 1);
        // -1 has weight 5.0, +1 has weight 2.0 → -1 wins
        assert_eq!(result.consensus, -1);
    }

    #[test]
    fn weighted_tie_returns_zero() {
        let votes = vec![
            RoomVote::with_weight(-1, 2.0),
            RoomVote::with_weight(1, 2.0),
        ];
        let result = FleetVote::weighted(&votes, 1);
        assert_eq!(result.consensus, 0);
    }

    #[test]
    fn consensus_supermajority() {
        // 4 out of 5 non-zero = 80% → supermajority for +1
        let votes = vec![1, 1, 1, 1, -1];
        let result = FleetVote::consensus(&votes, 1);
        assert_eq!(result.consensus, 1);
    }

    #[test]
    fn consensus_no_supermajority() {
        // 3 out of 5 = 60%, not > 60%
        let votes = vec![1, 1, 1, -1, -1];
        let result = FleetVote::consensus(&votes, 1);
        assert_eq!(result.consensus, 0);
    }

    #[test]
    fn all_abstain() {
        let votes = vec![0, 0, 0, 0];
        let result = FleetVote::majority(&votes, 1);
        assert_eq!(result.consensus, 0);
        assert!(!result.quorum_met); // 0 non-zero votes
    }

    #[test]
    fn vote_result_counts() {
        let votes = vec![1, 1, -1, 0, 0];
        let result = FleetVote::majority(&votes, 1);
        assert_eq!(result.counts, [1, 2, 2]); // [-1, 0, +1]
        assert_eq!(result.total_votes, 5);
    }
}
