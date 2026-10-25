//! The predicate boundary label comes from the changed comparison (#10399).
//!
//! On a compound condition the boundary discriminator used to take the first
//! comparison operator it found, so it named a neighbouring comparison the
//! diff never touched, or invented a boundary for an edit that touched no
//! comparison. A wrong label steers a repair agent to the wrong test, so when
//! the changed operator cannot be located unambiguously no label is named.

use super::tests::{missing_discriminator_values, test_owner, weak_direct_test_for};
use super::*;

const TWO_COMPARISONS_OLD: &str = "  if (isDone || index > stopIndex || readingCount >= concurrency || total - headIndex >= backpressure) {";

fn boundary_text(old: Option<&str>, new: &str) -> String {
    typescript_changed_boundary_text(&ProbeFamily::Predicate, old, new).into_owned()
}

fn boundary_label(old: Option<&str>, new: &str) -> Option<String> {
    typescript_boundary_discriminator(&boundary_text(old, new))
}

#[test]
fn second_comparison_edit_names_the_changed_comparison_not_the_neighbour() -> Result<(), String> {
    let new =
        TWO_COMPARISONS_OLD.replace("readingCount >= concurrency", "readingCount > concurrency");
    // Fixture: the line really holds a neighbouring `>=` comparison, and the
    // whole-line reading is the neighbour the issue reported.
    assert_eq!(
        typescript_boundary_discriminator(&new).as_deref(),
        Some("headIndex == backpressure")
    );
    assert_eq!(
        boundary_label(Some(TWO_COMPARISONS_OLD), &new).as_deref(),
        Some("readingCount == concurrency")
    );
    Ok(())
}

#[test]
fn first_comparison_edit_names_the_changed_comparison() {
    let new = TWO_COMPARISONS_OLD.replace("index > stopIndex", "index >= stopIndex");
    assert_eq!(
        boundary_label(Some(TWO_COMPARISONS_OLD), &new).as_deref(),
        Some("index == stopIndex")
    );
}

#[test]
fn operand_edit_inside_one_comparison_names_that_comparison() {
    let new = TWO_COMPARISONS_OLD.replace("readingCount >= concurrency", "readingCount >= limit");
    assert_eq!(
        boundary_label(Some(TWO_COMPARISONS_OLD), &new).as_deref(),
        Some("readingCount == limit")
    );
}

#[test]
fn removed_non_comparison_operand_invents_no_boundary() {
    // Negative control: the edit drops `isDone ||` and touches no comparison.
    let new = TWO_COMPARISONS_OLD.replace("isDone || ", "");
    assert!(typescript_boundary_discriminator(&new).is_some());
    assert_eq!(boundary_label(Some(TWO_COMPARISONS_OLD), &new), None);
}

#[test]
fn single_comparison_line_with_untouched_comparison_invents_no_boundary() {
    let old = "  if (isDone || total >= limit) {";
    let new = "  if (total >= limit) {";
    assert_eq!(boundary_label(Some(old), new), None);
}

#[test]
fn single_comparison_edit_keeps_its_label() {
    assert_eq!(
        boundary_label(Some("  if (total >= limit) {"), "  if (total > limit) {").as_deref(),
        Some("total == limit")
    );
    // An added line has no old text to localize against.
    assert_eq!(
        boundary_label(None, "  if (total > limit) {").as_deref(),
        Some("total == limit")
    );
}

#[test]
fn edit_touching_two_comparisons_names_neither() {
    let new = TWO_COMPARISONS_OLD
        .replace("index > stopIndex", "index >= stopIndex")
        .replace("readingCount >= concurrency", "readingCount > concurrency");
    assert_eq!(boundary_label(Some(TWO_COMPARISONS_OLD), &new), None);
}

#[test]
fn added_line_with_two_comparisons_names_neither() {
    assert_eq!(boundary_label(None, TWO_COMPARISONS_OLD), None);
}

#[test]
fn unterminated_literal_names_no_boundary() {
    assert_eq!(
        boundary_label(
            Some("  if (a > b || s === 'x) {"),
            "  if (a >= b || s === 'x) {"
        ),
        None
    );
}

#[test]
fn comparison_inside_a_string_is_not_a_site() {
    let old = "  if (label === \"a >= b\" || total >= limit) {";
    let new = "  if (label === \"a >= b\" || total > limit) {";
    assert_eq!(
        boundary_label(Some(old), new).as_deref(),
        Some("total == limit")
    );
}

#[test]
fn returned_ternary_arm_edit_keeps_its_one_condition_boundary() {
    let old = "  return level === Level.High ? 10 : 10;";
    let new = "  return level === Level.High ? 11 : 10;";
    assert_eq!(
        boundary_label(Some(old), new).as_deref(),
        Some("level == Level.High")
    );
}

#[test]
fn returned_ternary_arm_edit_with_two_condition_comparisons_names_neither() {
    let old = "  return a > b && c >= d ? 10 : 10;";
    let new = "  return a > b && c >= d ? 11 : 10;";
    assert_eq!(boundary_label(Some(old), new), None);
}

#[test]
fn returned_ternary_condition_edit_names_the_changed_comparison() {
    let old = "  return a > b && c >= d ? 10 : 10;";
    let new = "  return a > b && c > d ? 10 : 10;";
    assert_eq!(boundary_label(Some(old), new).as_deref(), Some("c == d"));
}

#[test]
fn non_predicate_family_reads_the_whole_line() {
    let line = "  return a >= b || c >= d;";
    assert_eq!(
        typescript_changed_boundary_text(&ProbeFamily::ReturnValue, Some("x"), line),
        line
    );
}

fn classify_with_old(old: &str, new: &str) -> Result<Finding, String> {
    let owner = test_owner("applyDiscount", "src/lib.ts");
    let test = weak_direct_test_for("applyDiscount");
    classify_change_with_alias_state(
        Path::new("src/lib.ts"),
        2,
        new,
        Some(old),
        &[owner],
        &[test],
        None,
        &ReExportIndex::empty(),
        None,
        None,
    )
    .ok_or_else(|| "expected TypeScript preview finding".to_string())
}

#[test]
fn finding_names_the_changed_comparison_in_discriminator_and_limitation() -> Result<(), String> {
    let new =
        TWO_COMPARISONS_OLD.replace("readingCount >= concurrency", "readingCount > concurrency");
    let finding = classify_with_old(TWO_COMPARISONS_OLD, &new)?;
    assert_eq!(
        missing_discriminator_values(&finding),
        vec!["readingCount == concurrency".to_string()]
    );
    let text = format!("{:?} {:?}", finding.missing, finding.evidence);
    assert!(
        !text.contains("index == stopIndex") && !text.contains("headIndex == backpressure"),
        "neighbouring comparison leaked into the finding: {text}"
    );
    Ok(())
}

#[test]
fn finding_for_non_comparison_edit_omits_the_boundary_label() -> Result<(), String> {
    let new = TWO_COMPARISONS_OLD.replace("isDone || ", "");
    let finding = classify_with_old(TWO_COMPARISONS_OLD, &new)?;
    assert_eq!(missing_discriminator_values(&finding), Vec::<String>::new());
    let text = format!("{:?} {:?}", finding.missing, finding.evidence);
    assert!(
        !text.contains("== stopIndex")
            && !text.contains("== concurrency")
            && !text.contains("== backpressure"),
        "invented boundary leaked into the finding: {text}"
    );
    Ok(())
}
