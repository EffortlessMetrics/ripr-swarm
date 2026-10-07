//! Family-relevant assertion selection for Python related tests (#5572,
//! RIPR-SPEC-0224).
//!
//! A Python test usually carries several assertions. The row a finding
//! displays, the oracle the classifier judges, the sink alignment reads, and
//! the strong-row gates in `boundary.rs` / `no_behavior.rs` must all name the
//! *same* assertion: the one relevant to the changed behavior. Ranking by
//! oracle strength alone let a stronger wrong-family assertion (a
//! `pytest.raises(..., match=...)` for a changed `return`, an exact value
//! assertion for a changed `raise`) become the row's oracle while the
//! behavior-specific evidence judged something else.
//!
//! Selection law:
//!
//! 1. keep only assertions applicable to the changed [`ProbeFamily`];
//! 2. among those, prefer an assertion relevant to the changed sink — for a
//!    field change whose changed field is known, a field observer that reads
//!    only a sibling field ranks below every other applicable assertion — and
//!    then the strongest one. A sibling-field observer is the same family on
//!    a different sink, so it stays selectable when nothing relevant exists
//!    (sink alignment then reports it `orthogonal`, which keeps its repair
//!    card out of agent packets);
//! 3. break equal strength by a semantic family preference, whole-value over
//!    aggregate observation and the oracle shape, then the source line and
//!    text — never by extractor traversal order;
//! 4. the selected assertion carries its own strength, kind, shape, text and
//!    line, so strength and sink evidence always come from one assertion;
//! 5. when assertions exist but none is applicable, the result is the explicit
//!    [`PythonAssertionSelection::NoFamilyRelevant`] — never a stronger
//!    wrong-family fallback.
//!
//! Why a Python shape table rather than the Rust `ORACLE_FAMILY_MATCHES` table
//! (`analysis/classify/reveal.rs`) or the TypeScript
//! `ts_oracle_kind_matches_seam` (RIPR-SPEC-0104): both key on `OracleKind`
//! alone, but Python's extractor already records the observed surface in
//! `PythonOracleShape`. The kind cannot tell `response.status_code == 404`
//! (an error path's visible effect) from `total == 3` (a normal value) — both
//! are `ExactValue` — while the shape can. The cross-domain rule is the same
//! as SPEC-0104: value families never credit an exception observer, and an
//! error path never credits a normal-value observer or a call expectation.
//! Python differs in two places. A mock expectation stays applicable to value
//! families, because a Python `return` / assignment line that changes is very
//! often a call whose arguments the expectation observes (and a `Medium`
//! mock never credits `exposed` alone). `Predicate` is also
//! `classify_probe_shape`'s fallback for unrecognized lines, guards raises
//! and calls as often as values, and so admits every observer.
//!
//! An `UnknownCustomHelper` is applicable to every family but carries
//! `Unknown` strength, so it is selected only when nothing graded applies; it
//! is never credited as evidence, and the test's #5571 admission state stays
//! the authority on whether the test asserts at all.

use super::sink_alignment::{
    dict_changed_keys_and_values, oracle_is_pure_len_aggregate,
    oracle_observes_changed_dict_element, oracle_text_observes_token,
};
use super::{PythonAssertion, PythonOracleShape, parse_attribute_assignment};
use crate::domain::ProbeFamily;
use std::cmp::Ordering;

/// The changed behavior an assertion must be relevant to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonAssertionFocus {
    family: ProbeFamily,
    changed_field: Option<PythonChangedField>,
    /// The changed line opens a `try:` / `except` / `finally:` block. An
    /// `ErrorPath` change there alters what the handler does — often the value
    /// it returns — so normal-value, mock and output observers apply too.
    error_handler_line: bool,
}

/// The field a `FieldConstruction` change writes, when the changed line names
/// it statically.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PythonChangedField {
    /// `recv.attr = value`.
    Attribute(String),
    /// A dict literal whose changed keys and new values are localized from the
    /// paired old line (`sink_alignment::dict_changed_keys_and_values`).
    DictElements {
        keys: Vec<String>,
        values: Vec<String>,
    },
}

impl PythonAssertionFocus {
    /// The focus for one changed line. `family` is the line's
    /// `classify_probe_shape` family; the changed field is read only for a
    /// `FieldConstruction` change.
    pub(super) fn for_change(
        family: ProbeFamily,
        line_text: &str,
        old_line_text: Option<&str>,
    ) -> Self {
        let changed_field = matches!(family, ProbeFamily::FieldConstruction)
            .then(|| changed_field(line_text, old_line_text))
            .flatten();
        let error_handler_line =
            matches!(family, ProbeFamily::ErrorPath) && is_error_handler_line(line_text);
        Self {
            family,
            changed_field,
            error_handler_line,
        }
    }

    #[cfg(test)]
    pub(super) fn for_family(family: ProbeFamily) -> Self {
        Self {
            family,
            changed_field: None,
            error_handler_line: false,
        }
    }

    pub(super) fn family(&self) -> &ProbeFamily {
        &self.family
    }

    /// Whether `assertion` can observe this change at all (the family
    /// filter). Static-limit suppression reads the selection built on this
    /// filter, so a wrong-family strong assertion never hides a limit.
    pub(super) fn admits(&self, assertion: &PythonAssertion) -> bool {
        self.error_handler_line || shape_matches_family(assertion.oracle_shape, &self.family)
    }

    /// Sink relevance: 0 for a sibling-field observer, 1 otherwise.
    fn sink_relevance(&self, assertion: &PythonAssertion) -> u8 {
        u8::from(!self.reads_only_a_sibling_field(assertion))
    }

    /// A field observer that reads none of the changed field's evidence
    /// observes a sibling field, not the changed one. Whole-object and
    /// non-field assertions are never siblings.
    fn reads_only_a_sibling_field(&self, assertion: &PythonAssertion) -> bool {
        if assertion.oracle_shape != PythonOracleShape::FieldAssertion {
            return false;
        }
        match &self.changed_field {
            None => false,
            Some(PythonChangedField::Attribute(attr)) => {
                !oracle_text_observes_token(&assertion.text, attr)
            }
            Some(PythonChangedField::DictElements { keys, values }) => {
                !oracle_observes_changed_dict_element(&assertion.text, keys, values)
            }
        }
    }

    fn preference(&self, shape: PythonOracleShape) -> u8 {
        let preferred = match self.family {
            ProbeFamily::ErrorPath => shape == PythonOracleShape::ExceptionAssertion,
            ProbeFamily::Predicate => shape == PythonOracleShape::BoundaryAssertion,
            ProbeFamily::FieldConstruction => shape == PythonOracleShape::FieldAssertion,
            ProbeFamily::ReturnValue => shape == PythonOracleShape::ExactAssertion,
            ProbeFamily::SideEffect | ProbeFamily::CallDeletion => matches!(
                shape,
                PythonOracleShape::MockExpectation | PythonOracleShape::OutputAssertion
            ),
            ProbeFamily::MatchArm | ProbeFamily::StaticUnknown => false,
        };
        u8::from(preferred)
    }
}

/// The `try:` / `except` / `except*` / `finally:` lines `classify_probe_shape`
/// files under `ErrorPath`. A `raise` or `with ... raises(` line is not one.
fn is_error_handler_line(line_text: &str) -> bool {
    let trimmed = line_text.trim_start();
    trimmed.starts_with("try:")
        || trimmed.starts_with("except ")
        || trimmed.starts_with("except:")
        || trimmed.starts_with("except* ")
        || trimmed.starts_with("finally:")
}

fn changed_field(line_text: &str, old_line_text: Option<&str>) -> Option<PythonChangedField> {
    if let Some((_, attr, _)) = parse_attribute_assignment(line_text) {
        return Some(PythonChangedField::Attribute(attr.to_string()));
    }
    let (keys, values) = dict_changed_keys_and_values(old_line_text, line_text)?;
    Some(PythonChangedField::DictElements { keys, values })
}

/// Whether an assertion of `shape` can observe a change of `family`.
fn shape_matches_family(shape: PythonOracleShape, family: &ProbeFamily) -> bool {
    use PythonOracleShape as Shape;
    match family {
        // An error path is observed by the raised exception or its visible
        // effect (status code, captured output); a normal-value observer
        // never triggers the changed raise. (Handler lines are admitted
        // wholesale in `admits`.) A field assertion on a captured exception
        // (`with pytest.raises(E) as excinfo:` then `assert
        // excinfo.value.code == 3`) is not recognized here: telling it from a
        // normal-value field assertion needs the `as` binding, which the
        // assertion inventory does not carry, so it stays a value observer
        // and fails closed for a changed raise.
        ProbeFamily::ErrorPath => matches!(
            shape,
            Shape::ExceptionAssertion
                | Shape::StatusCodeAssertion
                | Shape::OutputAssertion
                | Shape::BroadSmokeAssertion
                | Shape::UnknownCustomHelper
        ),
        // A produced value is never observed by an exception observer. A
        // call expectation stays applicable: a Python `return client.post(..)`
        // or `self.x = svc.call(..)` line is also a call whose arguments a
        // mock expectation observes, and a `Medium` mock never credits
        // `exposed` on its own.
        ProbeFamily::ReturnValue | ProbeFamily::FieldConstruction => {
            shape != Shape::ExceptionAssertion
        }
        // A predicate's outcome may be a value, a raise, or whether a call
        // happens, so every observer stays applicable; the boundary observer
        // is preferred only among equal-strength candidates.
        ProbeFamily::Predicate => true,
        // Effects, unmodelled families, and unknown shapes cannot be ruled out.
        ProbeFamily::SideEffect
        | ProbeFamily::CallDeletion
        | ProbeFamily::MatchArm
        | ProbeFamily::StaticUnknown => true,
    }
}

/// The selected assertion of one test, or why there is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PythonAssertionSelection<'a> {
    Selected(&'a PythonAssertion),
    /// The test has assertions, but none is relevant to the changed family.
    NoFamilyRelevant,
    /// The test has no extracted assertion; its admission state says whether
    /// that is established or unresolved (#5571).
    NoAssertion,
}

impl<'a> PythonAssertionSelection<'a> {
    pub(super) fn assertion(self) -> Option<&'a PythonAssertion> {
        match self {
            Self::Selected(assertion) => Some(assertion),
            Self::NoFamilyRelevant | Self::NoAssertion => None,
        }
    }
}

/// The pre-#5572 strength-only projection: the strongest assertion, the last
/// one in extractor order on ties. It is not a selector; the classifier
/// compares it with the family selection only to keep repair delegation
/// fail-closed (a row whose projection moved never becomes newly
/// delegatable, RIPR-SPEC-0224).
pub(super) fn strength_only_assertion(assertions: &[PythonAssertion]) -> Option<&PythonAssertion> {
    assertions
        .iter()
        .max_by_key(|assertion| assertion.oracle_strength.rank())
}

/// Select the assertion of one test relevant to `focus`.
///
/// `focus` is `None` only where no changed line exists (owner-level related
/// test ordering and the repo owner inventory); then every assertion applies
/// and the strongest one is chosen with the same deterministic tie-break.
pub(super) fn select_relevant_assertion<'a>(
    assertions: &'a [PythonAssertion],
    focus: Option<&PythonAssertionFocus>,
) -> PythonAssertionSelection<'a> {
    if assertions.is_empty() {
        return PythonAssertionSelection::NoAssertion;
    }
    assertions
        .iter()
        .filter(|assertion| focus.is_none_or(|focus| focus.admits(assertion)))
        .min_by(|left, right| selection_order(left, right, focus))
        .map_or(
            PythonAssertionSelection::NoFamilyRelevant,
            PythonAssertionSelection::Selected,
        )
}

/// Total order where the selected assertion sorts first: sink relevance, then
/// strength, then the
/// family preference, then whole-value over a pure `len(...)` aggregate (the
/// sink owner's aggregate rule), then the oracle shape (declaration order of
/// `PythonOracleShape`: exact before boundary before exception ...), then the
/// later source line, then the text.
///
/// The source line is a source identity, not traversal order: the extractor
/// visits `with` items before their body and handlers after it, while the
/// line is fixed by the source. Equal candidates that remain at that point
/// share strength, family preference, aggregate class and shape, so their
/// semantic result (kind, strength, shape) does not depend on which one is shown;
/// preferring the later line keeps the pre-#5572 projection for them.
fn selection_order(
    left: &PythonAssertion,
    right: &PythonAssertion,
    focus: Option<&PythonAssertionFocus>,
) -> Ordering {
    let preference = |assertion: &PythonAssertion| {
        focus.map_or(0, |focus| focus.preference(assertion.oracle_shape))
    };
    let sink_relevance =
        |assertion: &PythonAssertion| focus.map_or(1, |focus| focus.sink_relevance(assertion));
    let observes_whole_value =
        |assertion: &PythonAssertion| !oracle_is_pure_len_aggregate(&assertion.text, "");
    sink_relevance(right)
        .cmp(&sink_relevance(left))
        .then_with(|| {
            right
                .oracle_strength
                .rank()
                .cmp(&left.oracle_strength.rank())
        })
        .then_with(|| preference(right).cmp(&preference(left)))
        .then_with(|| observes_whole_value(right).cmp(&observes_whole_value(left)))
        .then_with(|| left.oracle_shape.cmp(&right.oracle_shape))
        .then_with(|| right.line.cmp(&left.line))
        .then_with(|| left.text.cmp(&right.text))
}
