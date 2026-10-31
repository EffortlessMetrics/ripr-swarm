//! RIPR-SPEC-0352: a receiver blocked only by an unverifiable macro is not a
//! gap.
//!
//! A call inside a standard assertion (`std::assert_eq!(Widget::new().read(), 4)`)
//! counts as a `direct_owner_call` only when the assertion macro binding is
//! verified, which needs target-root placement. Where an unresolved `include!`
//! or `#[path]` makes placement unavailable, the refusal used to read as a
//! receiver mismatch and the finding became a gap ripr cannot claim. This rule
//! withholds that gap under `rust_inherent_receiver_unestablished` and credits
//! nothing. Only the placement input is lifted: a receiver that would not
//! resolve with the macro trusted, a real source-level rebinding, and a
//! non-`strong` oracle over the owner all keep their gap.
//!
//! The classifier decides per related test while the run-shared syntax memo is
//! in scope and leaves a marker; the post-pass here consumes it. Like every
//! RIPR-SPEC-0240 rule it only withholds, so the actionability flip stays
//! fail-closed.

use super::gap_admission::{is_ignored, withhold_gap};
use super::related_tests::{
    inherent_owner_call_resolves_if_macros_trusted, requires_inherent_receiver_identity,
};
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
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
fn blocked_tests(finding: &Finding, owner: &FunctionSummary, index: &RustIndex) -> Vec<String> {
    if !matches!(
        finding.class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    ) || finding.static_limit_kind.is_some()
        || !requires_inherent_receiver_identity(owner)
    {
        return Vec::new();
    }
    // Only the finding's own related tests are judged: a same-named type in
    // another crate whose test the finding never related must not trigger.
    index
        .tests()
        .iter()
        .filter(|test| {
            finding
                .related_tests
                .iter()
                .any(|row| is_test(&row.name, &row.file, row.line, test))
                && !is_ignored(test)
                && assertions_over_owner_are_strong(test, owner)
                && inherent_owner_call_resolves_if_macros_trusted(test, owner, index)
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

/// Whether a related-test row names `test`. Name and file alone cannot tell
/// two same-named tests in sibling modules apart, so the start line is part of
/// the identity.
fn is_test(name: &str, file: &std::path::Path, line: usize, test: &TestSummary) -> bool {
    name == test.name && file == test.file && line == test.start_line
}

/// Whether the oracle is not what keeps this test from catching the change: at
/// least one assertion of the test names the owner, and every assertion that
/// does is `strong` under the shared oracle strength owner. A weak or medium
/// check (`assert!(w.read() > 0)`, `assert_ne!(w.read(), 0)`) is a real gap
/// whether or not the macro is trusted, so it never reaches the withhold. An
/// unreadable test keeps the gap. The assertion text, not the test body, is the
/// subject, so an unrelated strong assertion elsewhere in the test cannot lend
/// its strength to a weak one over the owner.
fn assertions_over_owner_are_strong(test: &TestSummary, owner: &FunctionSummary) -> bool {
    let mut over_owner = test
        .assertions
        .iter()
        .filter(|assertion| assertion.text.contains(owner.name.as_str()))
        .peekable();
    over_owner.peek().is_some() && over_owner.all(|a| a.strength == OracleStrength::Strong)
}

/// Classifier side: record that a related test is blocked only by the
/// unverifiable macro binding. Returns `true` when the marker was added.
pub(in crate::analysis) fn mark_receiver_unestablished(
    finding: &mut Finding,
    owner: Option<&FunctionSummary>,
    index: &RustIndex,
) -> bool {
    let Some(owner) = owner else {
        return false;
    };
    let judged = blocked_tests(finding, owner, index);
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

#[cfg(test)]
mod tests {
    use super::is_test;
    use crate::analysis::rust_index::summarize_file;
    use std::path::PathBuf;

    /// Two same-named tests in sibling modules share name and file; only the
    /// start line says which one a related-test row is.
    #[test]
    fn a_related_row_names_one_of_two_same_named_tests_by_its_start_line() -> Result<(), String> {
        let file = PathBuf::from("src/lib.rs");
        let source =
            "mod first {\n#[test] fn checks() {}\n}\nmod second {\n#[test] fn checks() {}\n}\n";
        let facts = summarize_file(file.clone(), source.to_string());
        let [first, second] = facts.tests.as_slice() else {
            return Err(format!("premise: two tests, got {}", facts.tests.len()));
        };
        assert_eq!(first.name, second.name);
        assert_ne!(first.start_line, second.start_line);
        assert!(is_test("checks", &file, first.start_line, first));
        assert!(!is_test("checks", &file, first.start_line, second));
        assert!(!is_test("checks", &file, second.start_line, first));
        Ok(())
    }
}
