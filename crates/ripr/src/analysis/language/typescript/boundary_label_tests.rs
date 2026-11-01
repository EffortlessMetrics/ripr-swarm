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
    typescript_changed_boundary_text(Path::new("src/lib.ts"), &ProbeFamily::Predicate, old, new)
        .into_owned()
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
    let old = "  if (n.length > 0 && ((x) => x) && identity<Array<string>>(xs)) {";
    let new = "  if (n.length >= 0 && ((x) => x) && identity<Array<string>>(xs)) {";
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
    // A shift is not another comparison, but its compound operand is not
    // a simple boundary that the label reader can represent.
    assert_eq!(boundary_label(Some(old), new), None);
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
        typescript_changed_boundary_text(
            Path::new("src/lib.ts"),
            &ProbeFamily::ReturnValue,
            Some("x"),
            line
        ),
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

#[test]
fn literal_only_edits_and_token_identical_edits_refuse_boundaries() {
    for (old, new) in [
        ("if (ready || /a>b/) {", "if (ready || /a>=b/) {"),
        ("if (ready || \"a>b\") {", "if (ready || \"a>=b\") {"),
        (
            "if (ready || total > limit) {",
            "if (ready || total > limit /* why */) {",
        ),
        (
            "if (ready || total > limit) {",
            "if (ready || total  > limit) {",
        ),
        (
            "if (total > limit || 'unterminated) {",
            "if (total >= limit) {",
        ),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{old} -> {new}");
    }
}

#[test]
fn sole_changed_comparison_ignores_literal_and_comment_operators() {
    for (old, new) in [
        (
            "if (a > b || flags[\"x>=y\"]) {",
            "if (a < b || flags[\"x>=y\"]) {",
        ),
        ("if (a > b || /x>=y/) {", "if (a < b || /x>=y/) {"),
        ("if (a > b) { // x>=y", "if (a < b) { // x>=y"),
        ("if (a /* x>=y */ > b) {", "if (a /* x>=y */ < b) {"),
        (
            "if (a > b || xs.some(x => x >= y)) {",
            "if (a < b || xs.some(x => x >= y)) {",
        ),
    ] {
        assert_eq!(
            boundary_label(Some(old), new).as_deref(),
            Some("a == b"),
            "{old} -> {new}"
        );
    }
}

#[test]
fn nested_syntax_edits_do_not_name_an_outer_predicate_boundary() {
    for (old, new) in [
        (
            "if (xs.some(x => x >= y) || a > b) {",
            "if (xs.some(x => x > y) || a > b) {",
        ),
        (
            "if (check(x >= y) || a > b) {",
            "if (check(x > y) || a > b) {",
        ),
        (
            "if (flags[x >= y] || a > b) {",
            "if (flags[x > y] || a > b) {",
        ),
        (
            "if ([x >= y].length || a > b) {",
            "if ([x > y].length || a > b) {",
        ),
        (
            "if (identity<Array<string>>(xs) || a > b) {",
            "if (identity<Array<number>>(xs) || a > b) {",
        ),
        ("if (a > b || c > d) {", "if (a < b || c < d) {"),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{old} -> {new}");
    }
}

#[test]
fn grouped_changed_comparison_keeps_its_boundary() {
    assert_eq!(
        boundary_label(Some("if ((a > b) || c > d) {"), "if ((a >= b) || c > d) {").as_deref(),
        Some("a == b")
    );
}

#[test]
fn finding_refuses_literal_and_nested_edits_even_with_a_strong_assertion() -> Result<(), String> {
    for (old, new) in [
        ("if (ready || /a>b/) {", "if (ready || /a>=b/) {"),
        (
            "if (xs.some(a => a >= b) || total > limit) {",
            "if (xs.some(a => a > b) || total > limit) {",
        ),
        ("if (ready || a > b) {", "if (ready || a > b /* why */) {"),
    ] {
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
            new,
            Some(old),
            &[owner],
            &[test],
            None,
            &ReExportIndex::empty(),
            None,
            None,
        )
        .ok_or_else(|| "expected a finding".to_string())?;
        assert!(!matches!(finding.class, ExposureClass::Exposed), "{new}");
        assert!(
            missing_discriminator_values(&finding).is_empty(),
            "{new}: {:?}",
            missing_discriminator_values(&finding)
        );
        assert!(
            format!("{:?} {:?}", finding.missing, finding.evidence)
                .contains("no statically comparable predicate boundary"),
            "{new}"
        );
    }
    Ok(())
}

#[test]
fn computed_comparison_operands_refuse_truncated_labels() {
    for (old, new) in [
        ("if (arr[i] >= n || c > d) {", "if (arr[i] > n || c > d) {"),
        ("if (a >= arr[i] || c > d) {", "if (a > arr[i] || c > d) {"),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{new}");
    }
}

#[test]
fn changed_comparison_beside_await_keeps_its_boundary() {
    assert_eq!(
        boundary_label(
            Some("if (await ready() || a > b) {"),
            "if (await ready() || a >= b) {"
        )
        .as_deref(),
        Some("a == b")
    );
}

#[test]
fn selected_comparison_preserves_dead_expected_side_refusal() -> Result<(), String> {
    let old = "  if ((total > 100) || ready) {";
    let new = "  if ((total >= 100) || ready) {";
    let mut owner = test_owner("applyDiscount", "src/lib.ts");
    owner.params = vec!["total".to_string(), "ready".to_string()];
    owner.arity = Some(2);
    owner.source_text = Some(format!(
        "export function applyDiscount(total: number, ready: boolean) {{\n{new}\n    return total * 0.9;\n  }}\n  return total;\n}}"
    ));
    for (expected, exposed) in [("90", true), ("999", false)] {
        let mut test = direct_test_with_assertion(
            "discount test",
            "applyDiscount",
            format!("expect(applyDiscount(100, false)).toBe({expected})"),
            "toBe",
            1,
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        test.assertions[0].observed_expression = Some("applyDiscount(100, false)".to_string());
        test.assertions[0].expected_value_or_variant = Some(expected.to_string());
        let finding = classify_change_with_alias_state(
            Path::new("src/lib.ts"),
            2,
            new,
            Some(old),
            &[owner.clone()],
            &[test],
            None,
            &ReExportIndex::empty(),
            None,
            None,
        )
        .ok_or_else(|| "expected a finding".to_string())?;
        assert_eq!(
            matches!(finding.class, ExposureClass::Exposed),
            exposed,
            "expected {expected}: {:?} {:?}",
            finding.missing,
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn edits_around_an_unchanged_comparison_do_not_select_it() -> Result<(), String> {
    for (old, new) in [
        (
            "if (leftFlag || a > b || rightFlag) {",
            "if (newLeftFlag || a > b || newRightFlag) {",
        ),
        (
            "if (leftFlag || a>b || rightFlag) {",
            "if (newLeftFlag || a > b || newRightFlag) {",
        ),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{new}");
        let finding = classify_with_old(old, new)?;
        assert!(missing_discriminator_values(&finding).is_empty());
    }
    assert_eq!(
        boundary_label(Some("if (a > b || a >= b) {"), "if (a >= b || a >= b) {").as_deref(),
        Some("a == b")
    );
    Ok(())
}

#[test]
fn optional_static_member_comparison_keeps_its_boundary() {
    assert_eq!(
        boundary_label(
            Some("if (order?.amount >= 50) {"),
            "if (order?.amount > 50) {"
        )
        .as_deref(),
        Some("order.amount == 50")
    );
}

/// A `typeof` operand is a comparable kind boundary: the whole-line reader
/// labeled it `typeof input == "function"` and the parser-owned selector must
/// keep naming it, not refuse the boundary (the typescript_typeof_kind_*
/// fixtures pin the credited classification, #10399 qualification).
#[test]
fn typeof_comparison_edit_names_its_kind_boundary() {
    assert_eq!(
        boundary_label(
            Some("  return typeof input === \"function\" ? a : b;"),
            "  return typeof input !== \"function\" ? a : b;"
        )
        .as_deref(),
        Some("typeof input == \"function\"")
    );
    // A nested typeof operand still refuses; only a plain admitted operand
    // under typeof keeps its site.
    assert_eq!(
        boundary_label(
            Some("  if (typeof order.total === \"number\") {"),
            "  if (typeof order.total !== \"number\") {"
        )
        .as_deref(),
        Some("typeof order.total == \"number\"")
    );
}

/// The module-bound boundary-input fact reads the full changed source line,
/// not the narrowed label text: the fact gates' whole-side and verbatim-line
/// rules need the source line's statement context, which the selected
/// comparison alone does not carry. Without the source-line identity the fact
/// is silently inert on exactly the compound lines this PR labels (#10399
/// sweep review on `classifier.rs`).
#[test]
fn compound_line_boundary_fact_reads_the_source_line_identity() -> Result<(), String> {
    let old = "  if (total >= 100 && ready) {";
    let new = "  if (total > 100 && ready) {";
    let root = super::tests::ts_unique_tempdir("boundary-fact-compound")?;
    super::tests::ts_write_file(
        &root.join("src/lib.ts"),
        "export function applyDiscount(total: number, ready: boolean): number {\n  if (total > 100 && ready) {\n    return total * 0.9;\n  }\n  return total;\n}\n",
    )?;
    let mut owner = test_owner("applyDiscount", "src/lib.ts");
    owner.params = vec!["total".to_string(), "ready".to_string()];
    owner.arity = Some(2);
    // A weak assertion cannot witness the boundary row, so the row stays
    // unresolved and the module-derived fact is what the evidence line reads.
    let test = weak_direct_test_for("applyDiscount");
    let finding = classify_change_with_alias_state(
        Path::new("src/lib.ts"),
        2,
        new,
        Some(old),
        &[owner],
        &[test],
        Some(&root),
        &ReExportIndex::empty(),
        None,
        None,
    )
    .ok_or_else(|| "expected TypeScript preview finding".to_string())?;
    assert_eq!(
        missing_discriminator_values(&finding),
        vec!["total == 100".to_string()]
    );
    assert!(
        finding.evidence.iter().any(|line| line
            == "typescript_boundary_input: parameter=total;index=0;operand=100;value=100"),
        "boundary-input evidence line missing on the compound line: {:?}",
        finding.evidence
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

// Clause-level matrix over compound `&&` / `||` conditions (#11196). The
// selector reads parser-owned comparison leaves, so precedence is decided by
// the parse tree, never by token order. A named clause is the comparison the
// edit changed; it is not claimed to be independently steerable, because under
// `&&` it is observable only when the other clauses hold.

#[test]
fn mixed_logical_operators_name_the_changed_leaf_by_parse_tree_precedence() {
    // `a && b > 1 || c` parses as `(a && (b > 1)) || c`; `b > 1` is still one
    // comparison leaf whichever way the operators group.
    for (old, new) in [
        ("  if (ok && b > 1 || c) {", "  if (ok && b >= 1 || c) {"),
        ("  if (ok || b > 1 && c) {", "  if (ok || b >= 1 && c) {"),
        (
            "  if ((ok || b > 1) && c) {",
            "  if ((ok || b >= 1) && c) {",
        ),
    ] {
        assert_eq!(
            boundary_label(Some(old), new).as_deref(),
            Some("b == 1"),
            "{new}"
        );
    }
}

#[test]
fn clause_reorder_names_no_boundary() {
    let old = "  if (a > 1 && b < 2) {";
    let new = "  if (b < 2 && a > 1) {";
    assert_eq!(boundary_label(Some(old), new), None);
}

#[test]
fn removed_clause_names_no_boundary() {
    assert_eq!(
        boundary_label(Some("  if (a > 1 && b < 2) {"), "  if (a > 1) {"),
        None
    );
    assert_eq!(
        boundary_label(Some("  if (a > 1 || b < 2) {"), "  if (b < 2) {"),
        None
    );
}

#[test]
fn moved_parenthesis_names_no_boundary() {
    // No comparison changed; only the grouping of `b || c` did.
    let old = "  if ((a > 1 && b) || c) {";
    let new = "  if (a > 1 && (b || c)) {";
    assert_eq!(boundary_label(Some(old), new), None);
}

#[test]
fn logical_operator_swap_names_no_boundary() {
    // `&&` -> `||` changes how the clauses combine, not a comparison.
    assert_eq!(
        boundary_label(Some("  if (a > 1 && b) {"), "  if (a > 1 || b) {"),
        None
    );
}

#[test]
fn non_comparison_clause_edit_names_no_boundary() {
    assert_eq!(
        boundary_label(Some("  if (a > 1 && ok) {"), "  if (a > 1 && ready) {"),
        None
    );
}

#[test]
fn edit_inside_a_string_that_looks_like_a_clause_names_no_boundary() {
    let old = "  log(\"a > 1 && b\");";
    let new = "  log(\"a >= 1 && b\");";
    assert_eq!(boundary_label(Some(old), new), None);
}

#[test]
fn non_simple_operand_in_the_changed_clause_names_no_boundary() {
    for (old, new) in [
        ("  if (f(a) > 1 && b) {", "  if (f(a) >= 1 && b) {"),
        ("  if (xs[i] > 1 && b) {", "  if (xs[i] >= 1 && b) {"),
        ("  if ((a ?? 0) > 1 && b) {", "  if ((a ?? 0) >= 1 && b) {"),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{new}");
    }
}

#[test]
fn nullish_chain_without_parentheses_names_no_boundary() {
    // `a ?? b > 1` groups as `a ?? (b > 1)`: the changed comparison sits under
    // a nullish operand the label reader cannot represent.
    assert_eq!(
        boundary_label(Some("  if (a ?? b > 1) {"), "  if (a ?? b >= 1) {"),
        None
    );
}

#[test]
fn comparison_inside_a_template_literal_names_no_boundary() {
    let old = "  const s = `${a > 1 ? 'x' : 'y'}`;";
    let new = "  const s = `${a >= 1 ? 'x' : 'y'}`;";
    assert_eq!(boundary_label(Some(old), new), None);
}

#[test]
fn assigned_boolean_expression_names_no_boundary() {
    // Not a predicate line: the declaration is not classified as a predicate,
    // so no clause-level label is produced for it (non-goal of #11196).
    let old = "  const ok = a > 1 && b < 2;";
    let new = "  const ok = a >= 1 && b < 2;";
    assert_eq!(boundary_label(Some(old), new), None);
}

// #11442: two over-claims are withheld. Everything else keeps the edit-envelope
// behavior above.

#[test]
fn single_token_substitutions_keep_their_label() {
    for (old, new, label) in [
        ("  if (x > 5) {", "  if (x >= 5) {", "x == 5"),
        ("  if (x === 'a') {", "  if (x !== 'a') {", "x == 'a'"),
        ("  if (x > 5) {", "  if (x > 6) {", "x == 6"),
        ("  if (x > y) {", "  if (x > z) {", "x == z"),
        ("  if (a > 1 && ok) {", "  if (a >= 1 && ok) {", "a == 1"),
        (
            "  if (ok && a > 1 && b < 2) {",
            "  if (ok && a > 1 && b <= 2) {",
            "b == 2",
        ),
    ] {
        assert_eq!(
            boundary_label(Some(old), new).as_deref(),
            Some(label),
            "{old} -> {new}"
        );
    }
}

#[test]
fn pure_operand_swaps_keep_their_label() {
    for (old, new, label) in [
        (
            "  if (total >= 100) {",
            "  if (100 <= total) {",
            "100 == total",
        ),
        (
            "  if (spend > 500) {",
            "  if (500 < spend) {",
            "500 == spend",
        ),
        (
            "  if (tier === \"gold\") {",
            "  if (\"gold\" === tier) {",
            "\"gold\" == tier",
        ),
        ("  if (a !== 1) {", "  if (1 !== a) {", "1 == a"),
        ("  return age >= 18;", "  return 18 <= age;", "18 == age"),
        ("  if (a.x > b.y) {", "  if (b.y < a.x) {", "b.y == a.x"),
    ] {
        assert_eq!(
            boundary_label(Some(old), new).as_deref(),
            Some(label),
            "{old} -> {new}"
        );
    }
}

#[test]
fn swaps_that_change_the_predicate_name_no_boundary() {
    for (old, new) in [
        // Operands exchanged without mirroring the operator flips the test.
        ("  if (a > b) {", "  if (b > a) {"),
        ("  if (a >= b) {", "  if (b >= a) {"),
        // Mirror operator but the operator also strengthens or a literal moves.
        ("  if (a >= b) {", "  if (b < a) {"),
        ("  if (a > 1) {", "  if (2 < a) {"),
        ("  if (a > b) {", "  if (c < a) {"),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{old} -> {new}");
    }
}

#[test]
fn two_tokens_changed_in_one_clause_name_no_boundary() {
    for (old, new) in [
        ("  if (a > 1 && ok) {", "  if (a >= 2 && ok) {"),
        ("  if (a > 1) {", "  if (a >= 2) {"),
        ("  if (a > b) {", "  if (c >= b) {"),
        ("  if (a.x > 1) {", "  if (b.y > 1) {"),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{old} -> {new}");
    }
}

#[test]
fn added_comparison_clause_names_no_boundary() {
    for (old, new) in [
        ("  if (a > 1) {", "  if (a > 1 && b < 2) {"),
        ("  if (a > 1) {", "  if (b < 2 && a > 1) {"),
        ("  if (a > 1 && ok) {", "  if (a > 1 && ok && b < 2) {"),
        ("  if (a > 1 || ok) {", "  if (a > 1 || b < 2) {"),
        ("  if (a > 1 && ok) {", "  if (a > 1 && ok < b) {"),
        ("  if (a && b) {", "  if (a < b) {"),
        ("  if (a > 1) {", "  if (a >= 1 && b < 2) {"),
    ] {
        assert_eq!(boundary_label(Some(old), new), None, "{old} -> {new}");
    }
}
