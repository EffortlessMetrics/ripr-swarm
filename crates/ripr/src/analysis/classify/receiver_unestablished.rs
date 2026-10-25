//! RIPR-SPEC-0352: a receiver blocked only by an unverifiable macro is not a
//! gap.
//!
//! An inherent method owner relates to a test as `direct_owner_call` only when
//! the receiver's type is established (#9611). A call written inside a standard
//! assertion (`std::assert_eq!(Widget::new().read(), 4)`) counts only when the
//! assertion macro binding is verified, and that check needs target-root
//! placement: which Cargo target compiles the test file, and so which macros
//! that target can see. An unresolved `include!` or `#[path]` anywhere in the
//! workspace (a build-script output, `include!(concat!(env!("OUT_DIR"), ..))`)
//! may compile any file into another crate, so placement is unavailable and the
//! binding cannot be verified.
//!
//! The refusal then used to read as a receiver mismatch: the relation dropped to
//! `weak_token_substring` and the finding became a gap. That is a claim ripr
//! cannot make. The call resolves if the macro can be trusted, and whether it
//! can is exactly what ripr cannot read. This rule withholds the gap under
//! `rust_inherent_receiver_unestablished` and credits nothing:
//!
//! - the relation stays `weak_token_substring`, and no test gains
//!   `direct_owner_call` or grip from this rule;
//! - a receiver that would not resolve even with the macro trusted (a wrong
//!   type, a message-only operand, a shadowed or rebound local, a shadowed
//!   type) keeps its gap;
//! - a refusal while placement IS available (a real source-level rebinding of
//!   `std` or the macro) keeps its gap.
//!
//! The classifier decides per related test while the run-shared syntax memo is
//! in scope and leaves a marker; the post-pass here consumes it. Like every
//! RIPR-SPEC-0240 rule it only withholds, so the actionability flip stays
//! fail-closed.

use super::OwnerPinSyntax;
use super::gap_admission::{is_ignored, withhold_gap};
use super::related_tests::{
    inherent_owner_call_resolves_if_macros_trusted, requires_inherent_receiver_identity,
};
use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustInherentReceiverUnestablished;
const FIRST_UNRESOLVED_EDGE: &str = "whether the standard assertion macro around the owner call is the one the test's target compiles";
const JUDGED_TEST_LIMIT: usize = 3;

/// Classifier-to-post-pass marker prefix: a related test's owner call resolves
/// if the standard assertion macro were trusted, and placement is unavailable.
/// The rest of the line names the judged tests.
pub(in crate::analysis) const RECEIVER_UNVERIFIABLE_MACRO_PREFIX: &str = "receiver_unestablished: the owner call resolves only if the standard assertion macro is trusted: ";

/// Tests whose owner call is blocked only by the unverifiable macro binding,
/// formatted for the next step. Empty when the rule does not apply.
fn blocked_tests(
    finding: &Finding,
    owner: &FunctionSummary,
    index: &RustIndex,
    pin_syntax: &OwnerPinSyntax,
) -> Vec<String> {
    if !matches!(
        finding.class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    ) || finding.static_limit_kind.is_some()
        || !requires_inherent_receiver_identity(owner)
    {
        return Vec::new();
    }
    index
        .tests()
        .iter()
        .filter(|test| {
            !is_ignored(test)
                && !test.assertions.is_empty()
                && test.body.contains(owner.name.as_str())
                && inherent_owner_call_resolves_if_macros_trusted(test, owner, index, pin_syntax)
        })
        .take(JUDGED_TEST_LIMIT)
        .map(|test| {
            format!(
                "`{}` ({}:{})",
                test.name,
                test.file.display().to_string().replace('\\', "/"),
                test.start_line
            )
        })
        .collect()
}

/// Classifier side: record that a related test is blocked only by the
/// unverifiable macro binding. Returns `true` when the marker was added.
pub(in crate::analysis) fn mark_receiver_unestablished(
    finding: &mut Finding,
    owner: Option<&FunctionSummary>,
    index: &RustIndex,
    pin_syntax: &OwnerPinSyntax,
) -> bool {
    let Some(owner) = owner else {
        return false;
    };
    let judged = blocked_tests(finding, owner, index, pin_syntax);
    if judged.is_empty() {
        return false;
    }
    finding.evidence.push(format!(
        "{RECEIVER_UNVERIFIABLE_MACRO_PREFIX}{}",
        judged.join(", ")
    ));
    true
}

/// Withhold a gap the classifier marked. Returns `true` when the finding was
/// rewritten to `static_unknown`. The marker never survives the pass.
pub(in crate::analysis) fn withhold_receiver_unestablished_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    let marker = finding
        .evidence
        .iter()
        .find_map(|line| line.strip_prefix(RECEIVER_UNVERIFIABLE_MACRO_PREFIX))
        .map(str::to_string);
    finding
        .evidence
        .retain(|line| !line.starts_with(RECEIVER_UNVERIFIABLE_MACRO_PREFIX));
    let Some(judged) = marker else {
        return false;
    };
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
    {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    let next_step = format!(
        "{} calls `{}` on a receiver that would resolve if the standard assertion macro is the one its target compiles, and ripr cannot place the test in a Cargo target here (an unresolved `include!` or `#[path]`), so it does not report a gap. Check that the assertion would fail if `{}` returned the wrong value; add a test only if none does.",
        judged, owner.name, owner.name,
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &[judged]);
    true
}
