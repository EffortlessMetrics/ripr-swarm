//! Consumer-facing evidence state for a canonical review gap.
//!
//! This is a presentation decision over analyzer facts, separate from the
//! producer's `AnalysisOutcome` and its limitation taxonomy.

use serde::{Deserialize, Serialize};

/// How a consumer should present one review gap. Serialized as the
/// `snake_case` word that [`EvidenceState::as_str`] returns. Each producer
/// (the Rust evidence record, preview language adapters) chooses the state;
/// only [`EvidenceState::Actionable`] marks a repair target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceState {
    /// The gap is a repair target consumers may act on. The only state that
    /// [`EvidenceState::is_actionable`] accepts.
    Actionable,
    /// Existing tests already observe the behavior. For Rust seams this is
    /// the `strongly_gripped` grip class.
    AlreadyObserved,
    /// Shown for context but not a repair target. For Rust seams this is the
    /// `intentional` grip class.
    InternalOnly,
    /// Not actionable: the producer cannot back a repair packet. This covers
    /// analyzer limits, and also real gaps that name a test file or a missing
    /// test but lack the verification, receipt or edit constraints a repair
    /// needs.
    StaticLimitation,
    /// Not actionable, and no other state applies. A preview producer may
    /// still attach a suggested test or assertion as advice.
    Unknown,
}

impl EvidenceState {
    /// Returns the snake_case wire word, such as `"already_observed"`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Actionable => "actionable",
            Self::AlreadyObserved => "already_observed",
            Self::InternalOnly => "internal_only",
            Self::StaticLimitation => "static_limitation",
            Self::Unknown => "unknown",
        }
    }

    /// Returns `true` only for [`EvidenceState::Actionable`].
    pub const fn is_actionable(self) -> bool {
        matches!(self, Self::Actionable)
    }

    /// Parse a serialized gap state. An unrecognized word is `None`, never a
    /// guessed state, so consumers stay fail-closed.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "actionable" => Some(Self::Actionable),
            "already_observed" => Some(Self::AlreadyObserved),
            "internal_only" => Some(Self::InternalOnly),
            "static_limitation" => Some(Self::StaticLimitation),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// True only when `word` is the serialized actionable state.
    pub fn is_actionable_word(word: &str) -> bool {
        Self::parse(word).is_some_and(Self::is_actionable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_states_keep_the_existing_words_and_actionability_boundary() {
        let cases = [
            (EvidenceState::Actionable, "actionable"),
            (EvidenceState::AlreadyObserved, "already_observed"),
            (EvidenceState::InternalOnly, "internal_only"),
            (EvidenceState::StaticLimitation, "static_limitation"),
            (EvidenceState::Unknown, "unknown"),
        ];
        for (state, wire) in cases {
            assert_eq!(state.as_str(), wire);
            assert_eq!(EvidenceState::parse(wire), Some(state));
        }
        assert_eq!(EvidenceState::parse("Actionable"), None);
        assert_eq!(EvidenceState::parse(""), None);
        assert!(EvidenceState::is_actionable_word("actionable"));
        assert!(!EvidenceState::is_actionable_word("already_observed"));
        assert!(!EvidenceState::is_actionable_word("actionable "));
        assert!(EvidenceState::Actionable.is_actionable());
        assert!(!EvidenceState::Unknown.is_actionable());
        assert!(!EvidenceState::StaticLimitation.is_actionable());
    }
}
