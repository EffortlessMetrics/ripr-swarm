//! Family-relevant assertion selection for TypeScript related tests (#5525,
//! RIPR-SPEC-0224).
//!
//! A TypeScript test usually carries several assertions. The classifier
//! already judges a change at the assertion level through
//! [`ts_oracle_kind_matches_seam`] (RIPR-SPEC-0104), but the public
//! `RelatedTest` row used to collapse each test to its strongest assertion
//! overall, so a stronger wrong-family assertion (a `.toThrow(DiscountError)`
//! for a changed `return`, a `.toBe(90)` for a changed `throw`) became the
//! displayed oracle while classification judged another assertion. This
//! module is the one selector the row projection reads.
//!
//! Selection law:
//!
//! 1. keep only assertions whose oracle kind can observe the changed
//!    [`ProbeFamily`], through the same [`ts_oracle_kind_matches_seam`]
//!    authority the classifier uses — no second family table;
//! 2. among those, the strongest one wins;
//! 3. equal strength is broken by the later source line, then the rendered
//!    oracle text, the observed expression, the expected value, the matcher
//!    and the oracle kind — never by the order of the assertion inventory.
//!    Preferring the later line keeps the pre-#5525 projection for equal
//!    candidates (the strength-only `max_by_key` kept the last maximum, and
//!    the extractor records assertions in source order);
//! 4. the selected assertion is returned by reference, so its kind,
//!    strength, rendered call, observed expression, expected value, dynamic
//!    argument state, confidence and payloads all come from one assertion —
//!    facts are never merged across assertions;
//! 5. when assertions exist but none matches the family, the result is the
//!    explicit [`TypeScriptAssertionSelection::NoFamilyRelevant`] — never a
//!    stronger wrong-family fallback.
//!
//! Assertion admission (#5524) stays a separate gate: a test with no
//! extracted assertion selects [`TypeScriptAssertionSelection::NoAssertion`],
//! and its admission state says whether that is established or unresolved.

use super::{
    TypeScriptAssertion, assertion_oracle_text, strongest_assertion, ts_oracle_kind_matches_seam,
};
use crate::domain::ProbeFamily;
use std::cmp::Ordering;

/// The selected assertion of one test, or why there is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptAssertionSelection<'a> {
    Selected(&'a TypeScriptAssertion),
    /// The test has assertions, but none can observe the changed family.
    NoFamilyRelevant,
    /// The test has no extracted assertion.
    NoAssertion,
}

impl<'a> TypeScriptAssertionSelection<'a> {
    pub(crate) fn assertion(self) -> Option<&'a TypeScriptAssertion> {
        match self {
            Self::Selected(assertion) => Some(assertion),
            Self::NoFamilyRelevant | Self::NoAssertion => None,
        }
    }
}

/// Select the assertion of one test relevant to `probe_family`.
///
/// `probe_family` is `None` only where no changed line exists (test-only
/// owner-level relation checks); then every assertion applies and the
/// strongest one is chosen with the same deterministic tie-break.
pub(crate) fn select_family_relevant_assertion<'a>(
    assertions: &'a [TypeScriptAssertion],
    probe_family: Option<&ProbeFamily>,
) -> TypeScriptAssertionSelection<'a> {
    if assertions.is_empty() {
        return TypeScriptAssertionSelection::NoAssertion;
    }
    assertions
        .iter()
        .filter(|assertion| {
            probe_family
                .is_none_or(|family| ts_oracle_kind_matches_seam(&assertion.oracle_kind, family))
        })
        .min_by(|left, right| selection_order(left, right))
        .map_or(
            TypeScriptAssertionSelection::NoFamilyRelevant,
            TypeScriptAssertionSelection::Selected,
        )
}

/// How a test's family-relevant row differs from the pre-#5525 strength-only
/// row, when it differs in a way a delegation consumer reads.
///
/// The repair-packet projection, the verify-command inference and the preview
/// card pick their target row by row strength. A row whose strength moved can
/// therefore move the target test and the inferred verify command, so the
/// classifier discloses the move and the packet projection keeps that finding
/// non-delegatable (RIPR-SPEC-0224): no card becomes agent-packet eligible
/// because of this selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeScriptRowProjectionMove {
    /// Every assertion observes another behavior family; the row shows none.
    NoFamilyRelevant,
    /// A stronger assertion of another behavior family was passed over; the
    /// row shows a weaker family-relevant one.
    OtherBehaviorAssertionPassedOver,
}

impl TypeScriptRowProjectionMove {
    /// The `typescript_assertion_selection:` evidence value for this move.
    pub(crate) fn evidence_value(self, probe_family: &ProbeFamily) -> String {
        match self {
            Self::NoFamilyRelevant => format!("no_{}_relevant_assertion", probe_family.as_str()),
            Self::OtherBehaviorAssertionPassedOver => {
                "other_behavior_assertion_passed_over".to_string()
            }
        }
    }
}

/// Compare the family-relevant row with the strength-only row of one test.
///
/// Returns `None` when both rows carry the same strength: the strength-keyed
/// target choice is then unchanged.
pub(crate) fn row_projection_move(
    assertions: &[TypeScriptAssertion],
    probe_family: &ProbeFamily,
) -> Option<TypeScriptRowProjectionMove> {
    let strength_only = strongest_assertion(assertions)?;
    match select_family_relevant_assertion(assertions, Some(probe_family)) {
        TypeScriptAssertionSelection::Selected(selected) => (selected.oracle_strength.rank()
            != strength_only.oracle_strength.rank())
        .then_some(TypeScriptRowProjectionMove::OtherBehaviorAssertionPassedOver),
        TypeScriptAssertionSelection::NoFamilyRelevant => {
            Some(TypeScriptRowProjectionMove::NoFamilyRelevant)
        }
        TypeScriptAssertionSelection::NoAssertion => None,
    }
}

/// Total order where the selected assertion sorts first: strength, then the
/// later source line, then rendered oracle text, observed expression,
/// expected value, matcher and oracle kind.
fn selection_order(left: &TypeScriptAssertion, right: &TypeScriptAssertion) -> Ordering {
    right
        .oracle_strength
        .rank()
        .cmp(&left.oracle_strength.rank())
        .then_with(|| right.line.cmp(&left.line))
        .then_with(|| assertion_oracle_text(left).cmp(&assertion_oracle_text(right)))
        .then_with(|| left.observed_expression.cmp(&right.observed_expression))
        .then_with(|| {
            left.expected_value_or_variant
                .cmp(&right.expected_value_or_variant)
        })
        .then_with(|| left.matcher.cmp(&right.matcher))
        .then_with(|| left.oracle_kind.as_str().cmp(right.oracle_kind.as_str()))
}
