//! The predicate boundary label comes from the changed comparison (#10399).
//!
//! On a compound condition the boundary discriminator used to take the first
//! comparison operator it found, so it named a neighbouring comparison the
//! diff never touched, or invented a boundary for an edit that touched no
//! comparison. A wrong label steers a repair agent to the wrong test, so when
//! the changed operator cannot be located unambiguously no label is named.

use super::tests::{
    direct_test_with_assertion, missing_discriminator_values, test_owner, weak_direct_test_for,
};
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
fn trailing_operand_deletion_keeps_the_label() {
    assert_eq!(
        boundary_label(Some("  if (total > limit + 1) {"), "  if (total > limit) {").as_deref(),
        Some("total == limit")
    );
}

#[test]
fn leading_operand_deletion_keeps_the_label() {
    assert_eq!(
        boundary_label(Some("  if (x + a > b) {"), "  if (a > b) {").as_deref(),
        Some("a == b")
    );
}

#[test]
fn operand_deletion_in_a_compound_condition_names_that_comparison() {
    let old = "  if (c || total > limit + 1 || d >= e) {";
    let new = "  if (c || total > limit || d >= e) {";
    assert_eq!(
        boundary_label(Some(old), new).as_deref(),
        Some("total == limit")
    );
}

#[test]
fn generics_and_arrows_are_not_comparison_sites() {
    let old = "  if (n.length > 0 && ((x) => x) && y: Array<Array<string>>) {";
    let new = "  if (n.length >= 0 && ((x) => x) && y: Array<Array<string>>) {";
    // The generic `<`/`>` and the arrow `=>` add no sites the edit touched.
    assert_eq!(
        boundary_label(Some(old), new).as_deref(),
        Some("n.length == 0")
    );
}

#[test]
fn call_or_paren_operand_names_no_boundary() {
    let old = "  if (xs.map((x) => x).length > 0 && a > b) {";
    let new = "  if (xs.map((x) => x).length >= 0 && a > b) {";
    assert_eq!(boundary_label(Some(old), new), None);
    let old = "  if (a > f(b) && c > d) {";
    let new = "  if (a >= f(b) && c > d) {";
    assert_eq!(boundary_label(Some(old), new), None);
}

#[test]
fn shift_operator_is_not_a_comparison_site() {
    let old = "  if (a >> 1 > b) {";
    let new = "  if (a >> 1 >= b) {";
    assert!(boundary_text(Some(old), new).contains(">= b"));
}

#[test]
fn trailing_comment_is_not_code() {
    let old = "  if (a > b || c >= d) { // c > d";
    let new = "  if (a >= b || c >= d) { // c > d";
    assert_eq!(boundary_label(Some(old), new).as_deref(), Some("a == b"));
}

#[test]
fn nullish_coalesce_is_a_site() {
    let old = "  if ((a ?? 0) > 5 && b > c) {";
    let new = "  if ((a ?? 1) > 5 && b > c) {";
    // The edit touches the `??` comparison only; `b > c` is not named.
    assert_ne!(boundary_label(Some(old), new).as_deref(), Some("b == c"));
}

#[test]
fn non_returned_ternary_condition_edit_names_the_changed_comparison() {
    let old = "  x > 1 && y >= 2 ? foo() : bar();";
    let new = "  x > 1 && y > 2 ? foo() : bar();";
    assert_eq!(boundary_label(Some(old), new).as_deref(), Some("y == 2"));
}

#[test]
fn non_ascii_identifiers_tokenize_as_words() {
    let old = "  if (größe > a || b >= c) {";
    let new = "  if (größe >= a || b >= c) {";
    // The discriminator keeps ASCII operands only, so no label is named, but
    // the changed comparison is still located whole.
    assert!(boundary_text(Some(old), new).starts_with("größe >= a"));
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

#[test]
fn regex_literal_contents_are_not_comparison_sites() {
    let old = "  if (/a>b/.test(tag) || total > limit) {";
    let new = "  if (/a>=b/.test(tag) || total > limit) {";
    // The edit is inside the regex; no comparison changed.
    assert_eq!(boundary_label(Some(old), new), None);
    // A changed comparison beside a regex is still named.
    let old2 = "  if (/a>b/.test(tag) || x > y || total > limit) {";
    let new2 = "  if (/a>b/.test(tag) || x > y || total >= limit) {";
    assert_eq!(
        boundary_label(Some(old2), new2).as_deref(),
        Some("total == limit")
    );
    // Character classes and escapes do not end the literal early.
    let old = "  if (/[/>]\\/x/.test(t) || a > b) {";
    let new = "  if (/[/>=]\\/x/.test(t) || a > b) {";
    assert_eq!(boundary_label(Some(old), new), None);
    // Division after an operand is not a regex.
    let text = boundary_text(Some("  if (a / 2 > b / 3) {"), "  if (a / 2 >= b / 3) {");
    assert!(text.contains(">= b / 3"), "{text}");
}

#[test]
fn finding_for_regex_edit_names_no_boundary_and_is_not_exposed() -> Result<(), String> {
    let owner = test_owner("applyDiscount", "src/lib.ts");
    let test = direct_test_with_assertion(
        "discount test",
        "applyDiscount",
        "expect(applyDiscount(a, b)).toBe(90)",
        "toBe",
        1,
        OracleKind::ExactValue,
        OracleStrength::Strong,
    );
    let finding = classify_change_with_alias_state(
        Path::new("src/lib.ts"),
        2,
        "  if (/a>=b/.test(tag) || total > limit) {",
        Some("  if (/a>b/.test(tag) || total > limit) {"),
        &[owner],
        &[test],
        None,
        &ReExportIndex::empty(),
        None,
        None,
    )
    .ok_or_else(|| "expected a finding".to_string())?;
    assert!(!matches!(finding.class, ExposureClass::Exposed));
    assert_eq!(missing_discriminator_values(&finding), Vec::<String>::new());
    let text = format!("{:?} {:?}", finding.missing, finding.evidence);
    assert!(!text.contains("a == b"), "regex content leaked: {text}");
    Ok(())
}
