//! #15272: a related test that asserts the owner only at inputs where the
//! changed owner and its base side return the same value is no discriminator,
//! whatever the changed syntax is.
//!
//! `largest(2, 2)` cannot tell `if a > b { a } else { b }` from the swapped
//! arms; `pair(5) == (5, true)` cannot tell `(a, a > 0)` from `(a, a >= 0)`.
//! The predicate lens credits a literal that sits on the changed boundary and
//! the return-value lens credits any reaching exact assertion, so both read
//! `exposed` on a test that passes identically on the two versions.
//!
//! Like the unary-tail (#14815) and literal-condition (#14816) readings, this
//! one reads the diff itself: inside the owner's span the diff must replace
//! exactly the probe's line, and the base-side item
//! [`crate::analysis::diff::verified_base_owner`] establishes must be the
//! current item with that line restored. The infection stage then folds both
//! versions at the related tests' asserted literal calls
//! ([`crate::analysis::test_grip_evidence::owner_change_visibility`]).
//!
//! Withholding only: the reading can turn a credited infection weak, never
//! the reverse, and anything the fold cannot read keeps today's reading. An
//! equivalent rewrite, which no input separates, is not claimed, since the
//! gap statement would send the reader after a test that cannot exist.

use super::unary_tail::{
    SoleReplacedLine, another_definition_may_answer_the_name, base_item_is_the_restored_owner,
    sole_replaced_line,
};
use crate::analysis::diff::{ChangedFile, ModuleDeclarations};
use crate::analysis::facts::TestFact as TestSummary;
use crate::analysis::facts::{FunctionImplContext, FunctionSummary};
use crate::analysis::rust_index::RustIndex;
use crate::analysis::test_grip_evidence::{RangeChangeVisibility, owner_change_visibility};
use crate::domain::{Confidence, Probe, ProbeFamily, StageEvidence, StageState};

/// The base-side text of `owner`, a free function or a method of a named
/// self type, when `probe` is a predicate or return-value change on the one
/// line the diff replaces inside it and the related `tests` assert the owner
/// only where both versions agree. `None` for every other shape.
///
/// The fold runs first, on the unverified line pairing: establishing the
/// base-side item parses the whole file on both sides, and almost no probe
/// has agreeing inputs to withhold.
pub(in crate::analysis) fn sole_line_owner_baseline(
    probe: &Probe,
    owner: &FunctionSummary,
    tests: &[&TestSummary],
    changed: &ChangedFile,
    index: &RustIndex,
    modules: &ModuleDeclarations,
) -> Option<String> {
    if !matches!(
        probe.family,
        ProbeFamily::Predicate | ProbeFamily::ReturnValue
    ) || matches!(owner.impl_context, FunctionImplContext::Unknown)
        || tests.is_empty()
    {
        return None;
    }
    let line = probe.location.line;
    let SoleReplacedLine {
        current,
        old,
        baseline,
        ..
    } = sole_replaced_line(owner, changed, line)?;
    if !matches!(
        owner_change_visibility(owner, &baseline, tests, false),
        RangeChangeVisibility::EveryAssertedInputAgrees(_)
    ) {
        return None;
    }
    base_item_is_the_restored_owner(owner, changed, index, modules, line, &current, &old)
        .then_some(baseline)
}

/// The weak infection that replaces a credited one when every related test
/// asserts `owner` only at literal calls where it and `baseline` fold to the
/// same value, while some small input does separate the two. `None` keeps
/// the credited reading.
pub(in crate::analysis) fn agreeing_inputs_infection(
    owner: &FunctionSummary,
    baseline: &str,
    tests: &[&TestSummary],
    owner_has_other_callers: bool,
    index: &RustIndex,
) -> Option<StageEvidence> {
    let RangeChangeVisibility::EveryAssertedInputAgrees(call) =
        owner_change_visibility(owner, baseline, tests, owner_has_other_callers)
    else {
        return None;
    };
    // Last, since it scans the index: the bare call must name this owner.
    if another_definition_may_answer_the_name(owner, index) {
        return None;
    }
    Some(StageEvidence::new(
        StageState::Weak,
        Confidence::Medium,
        format!(
            "Every related test asserts the owner only at literal inputs where the changed owner and its baseline evaluate to the same value (for example `{call}`), so no test input reaches a difference"
        ),
    ))
}
