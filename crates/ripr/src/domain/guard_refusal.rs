//! Read-back helpers for conditional-return assertion refusals (#6541, #7974).
//!
//! The classifier writes the typed reason on the `assertion not credited:` evidence
//! line; renderers and gap withholding must not fall back to standard-macro wording
//! when that line names a guard instead.

use super::ASSERTION_NOT_CREDITED_PREFIX;

/// Why the related assertion was not credited, when the classifier recorded one.
pub(crate) fn assertion_not_credited_reason(evidence: &[String]) -> Option<&str> {
    assertion_not_credited_entry(evidence).and_then(|entry| {
        entry
            .rsplit_once(": ")
            .map(|(_, reason)| reason.trim())
            .filter(|reason| !reason.is_empty())
    })
}

/// Full `assertion not credited:` evidence line when it names the given test.
pub(crate) fn assertion_not_credited_entry_for_test<'a>(
    evidence: &'a [String],
    test_name: &str,
) -> Option<&'a str> {
    let needle = format!(" in {test_name} at ");
    evidence.iter().find_map(|line| {
        let body = line.strip_prefix(ASSERTION_NOT_CREDITED_PREFIX)?;
        body.contains(&needle).then_some(line.as_str())
    })
}

fn assertion_not_credited_entry(evidence: &[String]) -> Option<&str> {
    evidence
        .iter()
        .find_map(|entry| entry.strip_prefix(ASSERTION_NOT_CREDITED_PREFIX))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
}

/// Digest `Why …` hint from a guard-shaped refusal (#6541).
///
/// Only when the guard explains the finding class; never override `no_static_path`
/// and similar exposure explanations with an unrelated test's guard refusal.
pub(crate) fn guard_refusal_digest_classification_hint(
    finding: &crate::domain::Finding,
) -> Option<String> {
    use crate::domain::{ASSERTION_CONTEXT_UNESTABLISHED, ExposureClass, StageState};
    let reason = assertion_not_credited_reason(&finding.evidence)?;
    guard_refusal_kind(reason)?;
    let applies = match finding.class {
        ExposureClass::StaticUnknown => true,
        ExposureClass::ReachableUnrevealed => {
            finding.ripr.reveal.observe.summary == ASSERTION_CONTEXT_UNESTABLISHED
                && finding.ripr.reach.state != StageState::Weak
                && !finding
                    .ripr
                    .reach
                    .summary
                    .starts_with(crate::domain::CALLEE_ONLY_REACH_PREFIX)
        }
        _ => false,
    };
    if !applies {
        return None;
    }
    guard_refusal_classification_hint(reason)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardRefusalKind {
    /// `AssertionRefusal::ValueGuard` (RIPR-SPEC-0240 trap-tally control).
    ValueGuard,
    /// `AssertionContextRefusal::GuardedReturn` (trap-steps and certain gaps).
    GuardedReturn,
}

pub(crate) fn guard_refusal_kind(reason: &str) -> Option<GuardRefusalKind> {
    if reason.contains("spares the value the assertion expects") {
        Some(GuardRefusalKind::ValueGuard)
    } else if reason.contains("earlier `if") && reason.contains("can skip") {
        Some(GuardRefusalKind::GuardedReturn)
    } else {
        None
    }
}

/// Human `Why …` hint when the not-credited note names a conditional return.
pub(crate) fn guard_refusal_classification_hint(reason: &str) -> Option<String> {
    Some(match guard_refusal_kind(reason)? {
        GuardRefusalKind::ValueGuard => "a related test asserts here, but whether the assertion runs depends on a conditional return and a value ripr does not evaluate".into(),
        GuardRefusalKind::GuardedReturn => "a related test asserts here, but a conditional return before the assertion may skip it on the executed path".into(),
    })
}

/// First unresolved edge for `withhold_unsupported_gap` when refusals are guard-shaped.
pub(crate) fn guard_refusal_limitation_edge(reason: &str) -> Option<String> {
    Some(match guard_refusal_kind(reason)? {
        GuardRefusalKind::ValueGuard => "whether an earlier conditional return skips the assertion depends on a value ripr does not evaluate".into(),
        GuardRefusalKind::GuardedReturn => "whether an earlier conditional return skips the assertion on the executed path".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_guard_reason_is_recognized() {
        let reason = "an earlier `if weight(4) > 1000 { return; }` spares the value the assertion expects, so whether the assertion runs depends on the value of `weight(4)`, which ripr does not evaluate";
        assert_eq!(
            guard_refusal_kind(reason),
            Some(GuardRefusalKind::ValueGuard)
        );
        assert!(
            guard_refusal_classification_hint(reason)
                .is_some_and(|hint| !hint.contains("standard"))
        );
    }

    #[test]
    fn guarded_return_reason_is_recognized() {
        let reason = "the assertion is inside a block that an earlier `if weight(4) < 1000 { return; }` can skip";
        assert_eq!(
            guard_refusal_kind(reason),
            Some(GuardRefusalKind::GuardedReturn)
        );
        assert!(
            guard_refusal_classification_hint(reason)
                .is_some_and(|hint| !hint.contains("standard"))
        );
    }

    #[test]
    fn not_credited_entry_matches_test_name_in_location() {
        let evidence = vec![format!(
            "{ASSERTION_NOT_CREDITED_PREFIX}`assert_eq!` in weighs_two at src/lib.rs:12: guard reason"
        )];
        assert!(assertion_not_credited_entry_for_test(&evidence, "weighs_two").is_some());
        assert!(assertion_not_credited_entry_for_test(&evidence, "other_test").is_none());
    }
}
