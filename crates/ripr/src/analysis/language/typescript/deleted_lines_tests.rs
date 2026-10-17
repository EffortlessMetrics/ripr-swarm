//! Tests for removed-only TypeScript probes (#7689, RIPR-SPEC-0156): a
//! deleted behavior line no added line pairs with is base-side evidence on
//! its owner, never nothing and never a candidate edit target.

use super::*;
use crate::domain::SourceCurrentness;

const TEST_SOURCE: &str = "import { expect } from \"chai\";\nimport { cappedTotal, Cart } from \"../src/cart\";\n\ndescribe(\"cart\", () => {\n  it(\"caps the total\", () => {\n    expect(cappedTotal([80, 50])).to.equal(100);\n    expect(new Cart().capped(150)).to.equal(100);\n  });\n});\n";

/// One file's diff written as its lines, each opened by ` `, `-` or `+`.
/// The candidate source is the kept and added lines, so the diff and the
/// source on disk cannot disagree.
fn diff_and_source(lines: &[&str]) -> Result<(String, String), String> {
    let mut old_count = 0;
    let mut new_count = 0;
    let mut source = String::new();
    for line in lines {
        match line.chars().next() {
            Some(' ') => {
                old_count += 1;
                new_count += 1;
                source.push_str(&line[1..]);
                source.push('\n');
            }
            Some('-') => old_count += 1,
            Some('+') => {
                new_count += 1;
                source.push_str(&line[1..]);
                source.push('\n');
            }
            _ => {
                return Err(format!(
                    "diff line must open with ' ', '-' or '+': {line:?}"
                ));
            }
        }
    }
    let mut diff = format!(
        "diff --git a/src/cart.ts b/src/cart.ts\n--- a/src/cart.ts\n+++ b/src/cart.ts\n@@ -1,{old_count} +1,{new_count} @@\n"
    );
    for line in lines {
        diff.push_str(line);
        diff.push('\n');
    }
    Ok((diff, source))
}

/// Runs the adapter over the diff in a mocha package and returns every
/// finding.
fn analyze(label: &str, lines: &[&str]) -> Result<Vec<Finding>, String> {
    let (diff, source) = diff_and_source(lines)?;
    let root = super::tests::ts_unique_tempdir(label)?;
    let write =
        |path: &str, contents: &str| super::tests::ts_write_file(&root.join(path), contents);
    write(
        "package.json",
        r#"{"name":"pkg","scripts":{"test":"mocha"},"devDependencies":{"mocha":"^12.0.0","chai":"^6.0.0"}}"#,
    )?;
    write("src/cart.ts", &source)?;
    write("test/cart.test.ts", TEST_SOURCE)?;
    let changed_files = crate::analysis::diff::parse_unified_diff(&diff);
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let result = TypeScriptAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    Ok(result?.findings)
}

/// Each base-side finding as (line, owner name, currentness).
fn base_side(findings: &[Finding]) -> Vec<(usize, String, SourceCurrentness)> {
    findings
        .iter()
        .filter(|finding| finding.source_currentness.is_base_side())
        .map(|finding| {
            (
                finding.probe.location.line,
                finding
                    .probe
                    .owner
                    .as_ref()
                    .map(|owner| owner.0.rsplit("::").next().unwrap_or_default().to_string())
                    .unwrap_or_default(),
                finding.source_currentness,
            )
        })
        .collect()
}

fn record(line: usize, owner: &str) -> (usize, String, SourceCurrentness) {
    (line, owner.to_string(), SourceCurrentness::BaseDeleted)
}

/// A statement deleted from a live function, with nothing added, is a
/// `base_deleted` record on that function at its projected line.
#[test]
fn pure_deletion_is_base_side_evidence_on_its_owner() -> Result<(), String> {
    let findings = analyze(
        "pure-deletion",
        &[
            " const CAP = 100;",
            " ",
            " export function cappedTotal(items: number[]): number {",
            "   let total = items.reduce((sum, item) => sum + item, 0);",
            "-  total = Math.min(total, CAP);",
            "   return total;",
            " }",
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![record(5, "cappedTotal")],
        "{findings:?}"
    );
    let finding = findings
        .iter()
        .find(|finding| finding.source_currentness.is_base_side())
        .ok_or("no base-side finding")?;
    assert_eq!(
        finding.probe.before.as_deref().map(str::trim),
        Some("total = Math.min(total, CAP);")
    );
    assert_eq!(finding.probe.after, None);
    assert!(!finding.is_candidate_actionable());
    Ok(())
}

/// #7587's shape in TypeScript: the comment above the deleted line is
/// re-added at column 0 and pairs with the old comment by content, so no
/// added line pairs with the deleted clamp.
#[test]
fn deleted_line_beside_a_moved_comment_is_base_side_evidence() -> Result<(), String> {
    let findings = analyze(
        "moved-comment",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   let total = items.reduce((sum, item) => sum + item, 0);",
            "-  // clamp to the cap",
            "-  total = Math.min(total, 100);",
            "+// clamp to the cap",
            "   return total;",
            " }",
        ],
    )?;
    // Every removed line of a block shares the new-side line where its added
    // run starts, here the re-added comment.
    assert_eq!(
        base_side(&findings),
        vec![record(3, "cappedTotal")],
        "{findings:?}"
    );
    Ok(())
}

/// An even block pairs every removed line with an added one, so the old
/// line is read through its replacement and leaves no base-side record.
#[test]
fn even_block_leaves_no_base_side_record() -> Result<(), String> {
    let findings = analyze(
        "even-block",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   let total = items.reduce((sum, item) => sum + item, 0);",
            "-  total = Math.min(total, 100);",
            "+  total = Math.min(total, 99);",
            "   return total;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|finding| finding.probe.location.line == 3),
        "the replacement keeps its candidate finding: {findings:?}"
    );
    Ok(())
}

/// A whole deleted function deletes its owner; its body is not given to the
/// function that now stands where it was.
#[test]
fn deleted_function_body_is_not_given_to_a_neighbour() -> Result<(), String> {
    let findings = analyze(
        "deleted-function",
        &[
            "-export function clamp(total: number): number {",
            "-  total = Math.min(total, 100);",
            "-  return total;",
            "-}",
            "-",
            " export function cappedTotal(items: number[]): number {",
            "   const total = items.reduce((sum, item) => sum + item, 0);",
            "   return total;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A deleted method inside a class that lives on is lost with its header;
/// a line deleted from a live method stays that method's.
#[test]
fn deleted_method_is_lost_and_live_method_keeps_its_line() -> Result<(), String> {
    let findings = analyze(
        "class-methods",
        &[
            " export class Cart {",
            "-  clamp(total: number): number {",
            "-    total = Math.min(total, 100);",
            "-    return total;",
            "-  }",
            "   capped(total: number): number {",
            "-    total = Math.min(total, 100);",
            "     return total;",
            "   }",
            " }",
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![record(3, "capped")],
        "{findings:?}"
    );
    Ok(())
}

/// An added line interrupting a deleted function's body does not hide the
/// deleted header: the walk follows old-file order.
#[test]
fn interrupted_deleted_function_body_is_not_given_to_a_neighbour() -> Result<(), String> {
    let findings = analyze(
        "interrupted-deleted",
        &[
            " export function cappedTotal(items: number[]): number {",
            "-  return items.length;",
            "-}",
            "-function clamp(total: number): number {",
            "-  total = Math.min(total, 100);",
            "+  const size = items.length;",
            "-  return total * 2;",
            "+  return size;",
            " }",
        ],
    )?;
    assert!(
        !findings
            .iter()
            .any(|finding| finding.source_currentness.is_base_side()
                && finding
                    .probe
                    .before
                    .as_deref()
                    .is_some_and(|text| text.contains("Math.min") || text.contains("total * 2"))),
        "the deleted clamp body has no live owner: {findings:?}"
    );
    Ok(())
}

/// A callback deleted from a live function is not an owner: its body lines
/// belong to the function that held the call.
#[test]
fn deleted_callback_body_stays_with_the_live_function() -> Result<(), String> {
    let findings = analyze(
        "deleted-callback",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   let total = 0;",
            "-  items.forEach((item) => {",
            "-    total += item;",
            "-  });",
            "   return Math.min(total, 100);",
            " }",
        ],
    )?;
    let owners: Vec<String> = base_side(&findings)
        .into_iter()
        .map(|(_, owner, _)| owner)
        .collect();
    assert!(!owners.is_empty(), "{findings:?}");
    assert!(
        owners.iter().all(|owner| owner == "cappedTotal"),
        "{findings:?}"
    );
    Ok(())
}

/// A header whose types changed pairs with its replacement, so the function
/// is live and its deleted body line keeps its record.
#[test]
fn deleted_body_line_under_a_type_only_header_edit_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "type-only-header",
        &[
            "-export function cappedTotal(items: number[]): number {",
            "+export function cappedTotal(items: readonly number[]): number {",
            "   let total = items.reduce((sum, item) => sum + item, 0);",
            "-  total = Math.min(total, 100);",
            "   return total;",
            " }",
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![record(3, "cappedTotal")],
        "{findings:?}"
    );
    Ok(())
}

/// The same text re-added elsewhere in the file is movement evidence.
#[test]
fn deleted_line_re_added_elsewhere_is_moved_or_renamed() -> Result<(), String> {
    let findings = analyze(
        "moved-line",
        &[
            " export function cappedTotal(items: number[]): number {",
            "-  const cap = 100;",
            "   let total = items.reduce((sum, item) => sum + item, 0);",
            "+  const cap = 100;",
            "   return Math.min(total, cap);",
            " }",
        ],
    )?;
    let moved: Vec<_> = base_side(&findings)
        .into_iter()
        .filter(|(_, _, currentness)| *currentness == SourceCurrentness::MovedOrRenamed)
        .collect();
    assert_eq!(moved.len(), 1, "{findings:?}");
    Ok(())
}

/// A module statement has no owner, as the same line added there has none.
#[test]
fn module_level_deletion_leaves_no_record() -> Result<(), String> {
    let findings = analyze(
        "module-level",
        &[
            "-const FLOOR = 10;",
            " export function cappedTotal(items: number[]): number {",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// Two identical deleted lines in one owner get distinct ids.
#[test]
fn repeated_deleted_line_gets_an_ordinal_id() -> Result<(), String> {
    let findings = analyze(
        "repeated",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   let total = items.reduce((sum, item) => sum + item, 0);",
            "-  total = Math.min(total, 100);",
            "   total = total + 1;",
            "-  total = Math.min(total, 100);",
            "   return total;",
            " }",
        ],
    )?;
    let ids: Vec<&str> = findings
        .iter()
        .filter(|finding| finding.source_currentness.is_base_side())
        .map(|finding| finding.probe.id.0.as_str())
        .collect();
    assert_eq!(ids.len(), 2, "{findings:?}");
    assert_ne!(ids[0], ids[1]);
    assert!(ids[1].ends_with(".2"), "{ids:?}");
    Ok(())
}

/// The owner of every base-side record, in order.
fn base_side_owners(findings: &[Finding]) -> Vec<String> {
    base_side(findings)
        .into_iter()
        .map(|(_, owner, _)| owner)
        .collect()
}

/// An assignment whose value passes a callback is a behavior line, not a
/// named arrow function's header.
#[test]
fn deleted_assignment_holding_a_callback_is_base_side_evidence() -> Result<(), String> {
    let findings = analyze(
        "assignment-with-callback",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   let total = 0;",
            "-  total = items.reduce((sum, item) => sum + item, 0);",
            "   return Math.min(total, 100);",
            " }",
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![record(3, "cappedTotal")],
        "{findings:?}"
    );
    Ok(())
}

/// A deleted `const x = items.map((item) => {` block is a callback inside
/// the live function: every line of it is that function's.
#[test]
fn deleted_assigned_callback_block_stays_with_the_live_function() -> Result<(), String> {
    let findings = analyze(
        "assigned-callback",
        &[
            " export function cappedTotal(items: number[]): number {",
            "-  const doubled = items.map((item) => {",
            "-    return item * 2;",
            "-  });",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(
        base_side_owners(&findings),
        vec!["cappedTotal".to_string(), "cappedTotal".to_string()],
        "{findings:?}"
    );
    Ok(())
}

/// A call whose object argument opens on the call's line is not a method
/// header.
#[test]
fn deleted_call_with_an_object_argument_stays_with_the_live_function() -> Result<(), String> {
    let findings = analyze(
        "call-with-object",
        &[
            " export function cappedTotal(items: number[]): number {",
            "-  emit(\"total\", {",
            "-    size: items.length,",
            "-  });",
            "   return items.length;",
            " }",
        ],
    )?;
    let owners = base_side_owners(&findings);
    assert!(!owners.is_empty(), "{findings:?}");
    assert!(
        owners.iter().all(|owner| owner == "cappedTotal"),
        "{findings:?}"
    );
    Ok(())
}

/// A deleted `if` block leads on to the function that held it.
#[test]
fn deleted_control_block_stays_with_the_live_function() -> Result<(), String> {
    let findings = analyze(
        "control-block",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   let total = items.length;",
            "-  if (total > 100) {",
            "-    total = 100;",
            "-  }",
            "   return total;",
            " }",
        ],
    )?;
    let owners = base_side_owners(&findings);
    assert!(
        base_side(&findings)
            .iter()
            .any(|(_, owner, _)| owner == "cappedTotal"),
        "the clamp inside the deleted block is the function's: {findings:?}"
    );
    assert!(
        owners.iter().all(|owner| owner == "cappedTotal"),
        "{findings:?}"
    );
    Ok(())
}

/// A method whose parameter list Prettier wrapped is still a header: when
/// it is deleted, its lines are not given to the object that held it.
#[test]
fn deleted_method_with_a_wrapped_signature_records_nothing() -> Result<(), String> {
    let findings = analyze(
        "wrapped-signature",
        &[
            " export const api = {",
            "   size: 100,",
            "-  save(",
            "-    id: string,",
            "-  ) {",
            "-    persist(id);",
            "-  },",
            " };",
            " export function cappedTotal(items: number[]): number {",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// Git lists a block's removed lines before its added ones, so a header
/// whose types changed and the body line deleted under it both project to
/// the replacement header's line. The function lives on and keeps the line.
#[test]
fn deleted_line_under_a_replaced_header_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "replaced-header",
        &[
            "-export function cappedTotal(items: number[]): number {",
            "-  audit(items);",
            "+export function cappedTotal(items: readonly number[]): number {",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(
        base_side(&findings),
        vec![record(1, "cappedTotal")],
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn owner_headers_are_told_from_lines_that_only_hold_a_callback() {
    for header in [
        "export function cappedTotal(items: number[]): number {",
        "  async *pages() {",
        "  capped(total: number): number {",
        "  subscribe(listener: (state: State) => void): () => void {",
        "export const cappedTotal = (items: number[]): number => {",
        "const load = async (id: string) => {",
        "  handler = (event: Event) => {",
        "  onClick: (event) => {",
        "const parse: Parser = (text) => {",
        "export class Cart {",
        "  save({ id }: { id: string }) {",
        "  configure(opts = {}) {",
        "  onSave: (opts = {}) => {",
        "export const pick = <T extends Record<string, unknown>>(obj: T) => {",
        "const handler = function (request) {",
        "  save: function (id) {",
        "const f: <T = string>(a: T) => void = (a) => {",
        "export default function (",
        "export default class {",
        "export default (x) => {",
        "  get = (id) => {",
        "  set = (id) => {",
        "  get total() {",
        "  set total(value) {",
    ] {
        assert!(super::deleted_lines::opens_an_owner(header), "{header}");
    }
    for line in [
        "  total = items.reduce((sum, item) => sum + item, 0);",
        "  const doubled = items.map((item) => {",
        "  let found = items.find((item) => item.id === id);",
        "  const message = \"a => b\";",
        "  items.forEach((item) => {",
        "  setState({",
        "  post(url, {",
        "  setTimeout(function () {",
        "  const listener: (state: State) => void = noop;",
        "  if (total > 100) {",
        "  doThing(); {",
    ] {
        assert!(!super::deleted_lines::opens_an_owner(line), "{line}");
    }
}

/// A method whose parameters hold a brace (a destructured parameter, an
/// object default, an inline object type) is still a header: deleted, its
/// body is not given to the object that held it.
#[test]
fn deleted_method_with_a_brace_in_its_parameters_records_nothing() -> Result<(), String> {
    let findings = analyze(
        "brace-parameters",
        &[
            " export const api = {",
            "   size: 100,",
            "-  save({ id }: { id: string }) {",
            "-    persist(id);",
            "-  },",
            " };",
            " export function cappedTotal(items: number[]): number {",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A comment inside a wrapped signature does not hide the header.
#[test]
fn deleted_wrapped_signature_with_a_comment_records_nothing() -> Result<(), String> {
    let findings = analyze(
        "wrapped-signature-comment",
        &[
            " export const api = {",
            "   size: 100,",
            "-  save(",
            "-    id: string, // record id",
            "-  ) {",
            "-    persist(id);",
            "-  },",
            " };",
            " export function cappedTotal(items: number[]): number {",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A header replaced by a differently named one is a different owner: the
/// old function's deleted body is not the new function's evidence.
#[test]
fn deleted_body_under_a_renamed_header_is_not_the_new_owners() -> Result<(), String> {
    let findings = analyze(
        "renamed-header",
        &[
            "-export function oldTotal(items: number[]): number {",
            "-  audit(items);",
            "+export function cappedTotal(items: number[]): number {",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// #7705 shape 3: a typed binding whose type is itself a generic function
/// (`const f: <T = string>(a: T) => void = (a) => {`) still opens an owner.
/// `plain_assignment` must skip the `=` inside `<T = string>`, or the header
/// is missed and a type-only header edit drops the deleted body line.
#[test]
fn deleted_body_under_a_type_parameter_default_header_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "type-param-default",
        &[
            "-export const cappedTotal: <T = number[]>(items: T) => number = (items) => {",
            "-  audit(items);",
            "+export const cappedTotal: <T = readonly number[]>(items: T) => number = (items) => {",
            "   return items.length;",
            " };",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(1, "cappedTotal")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// #7705 shape 9: an anonymous default function has no `owner_name`, but a
/// type-only header edit still keeps the owner live so the deleted body
/// line is recorded on it. Both headers being nameless is the same owner.
#[test]
fn deleted_body_under_an_anonymous_default_function_header_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "anon-default-function",
        &[
            "-export default function (items: number[]): number {",
            "-  audit(items);",
            "+export default function (items: readonly number[]): number {",
            "   return items.length;",
            " }",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(1, "default")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// #7705 shape 9: an anonymous default arrow is the same owner across a
/// type-only header edit, so its deleted body line is kept.
#[test]
fn deleted_body_under_an_anonymous_default_arrow_header_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "anon-default-arrow",
        &[
            "-export default (items: number[]): number => {",
            "-  audit(items);",
            "+export default (items: readonly number[]): number => {",
            "   return items.length;",
            " };",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(1, "default")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// #7705 shape 10: a class field named `get` is an owner header, not a
/// getter modifier. Deleting it with its body does not give those lines to
/// the function that held the class.
#[test]
fn deleted_class_field_named_get_is_not_given_to_the_enclosing_function() -> Result<(), String> {
    let findings = analyze(
        "class-field-named-get",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   class Cart {",
            "-    get = (id: string) => {",
            "-      audit(id);",
            "-    };",
            "   }",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// #7705 shape 1: a replacement header Prettier-wrapped so no single added
/// line opens an owner (`save(` / `id: readonly string,` / `) {`). The
/// method still lives on; the unpaired body line stays base-side on it.
#[test]
fn deleted_body_under_a_wrapped_replacement_header_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "wrapped-replacement-header",
        &[
            " export class Cart {",
            "-  save(id: string) {",
            "-    audit(id);",
            "+  save(",
            "+    id: string | number,",
            "+  ) {",
            "     persist(id);",
            "   }",
            " }",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(2, "save")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// #7705 shape 4: a wrapped return type (`): Promise<` / `string` / `> {`)
/// still joins as one header, so a type-only header edit keeps the deleted
/// body line on the live method.
#[test]
fn deleted_body_under_a_wrapped_return_type_header_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "wrapped-return-type",
        &[
            " export class Cart {",
            "-  save(id: string): Promise<",
            "-    string",
            "-  > {",
            "-    audit(id);",
            "+  save(id: string | number): Promise<",
            "+    string",
            "+  > {",
            "     return Promise.resolve(id);",
            "   }",
            " }",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(2, "save")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// #7705 shape 8: a wrapped header whose first line ends in `{` (`save({` /
/// `id,` / `}) {`) is still a header. Deleting it does not give its body to
/// the enclosing live function.
#[test]
fn deleted_wrapped_header_opening_with_a_brace_is_not_given_to_the_enclosing_function()
-> Result<(), String> {
    let findings = analyze(
        "wrapped-header-opens-with-brace",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   const api = {",
            "-    save({",
            "-      id,",
            "-    }) {",
            "-      audit(id);",
            "-    },",
            "   };",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A wrapped replacement whose parameter list contains `>=` must not let
/// that comparison cancel the open `(`, or the unpaired body line is lost.
#[test]
fn deleted_body_under_a_wrapped_header_with_a_comparison_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "wrapped-header-comparison",
        &[
            " export class Cart {",
            "-  save(",
            "-    id: string,",
            "-    limit = a >= b,",
            "-  ) {",
            "-    audit(id);",
            "+  save(",
            "+    id: string | number,",
            "+    limit = a >= b,",
            "+  ) {",
            "     persist(id);",
            "   }",
            " }",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(2, "save")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// A comparison `>` on the same line as `Promise<` must not cancel that
/// type-open, or the wrapped `Result` / `> {` close is lost and the body
/// line is dropped.
#[test]
fn deleted_body_under_a_header_with_a_comparison_then_a_type_list_keeps_its_owner()
-> Result<(), String> {
    let findings = analyze(
        "comparison-then-type-list",
        &[
            " export class Cart {",
            "-  save(limit = a > b): Promise<",
            "-    string",
            "-  > {",
            "-    audit(limit);",
            "+  save(limit = a > b): Promise<",
            "+    string | number",
            "+  > {",
            "     return Promise.resolve(limit);",
            "   }",
            " }",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(2, "save")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// A wrap that closes one type list and opens another on the same line
/// (`> | Array<`) must net to zero, or the later `> {` leaves depth open
/// and the unpaired body line is swallowed as header text.
#[test]
fn deleted_body_under_a_wrapped_union_type_header_keeps_its_owner() -> Result<(), String> {
    let findings = analyze(
        "wrapped-union-type",
        &[
            " export class Cart {",
            "-  save(): Promise<",
            "-    string",
            "-  > | Array<",
            "-    string",
            "-  > {",
            "-    audit();",
            "+  save(): Promise<",
            "+    string | number",
            "+  > | Array<",
            "+    string | number",
            "+  > {",
            "     return Promise.resolve(\"ok\");",
            "   }",
            " }",
        ],
    )?;
    let base = base_side(&findings);
    assert_eq!(base, vec![record(2, "save")], "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// A line deleted from the body of a kept block comment is not code.
#[test]
fn deleted_comment_body_line_records_nothing() -> Result<(), String> {
    let findings = analyze(
        "comment-body",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   /*",
            "-  total = 100;",
            "   */",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// Type-only declarations are erased: deleting one changes no behavior.
#[test]
fn deleted_type_only_declaration_records_nothing() -> Result<(), String> {
    let findings = analyze(
        "type-only",
        &[
            " export function cappedTotal(items: number[]): number {",
            "-  type Total = number;",
            "-  interface Shape {",
            "-    size: number;",
            "-  }",
            "-  type Point = {",
            "-    x: number;",
            "-  };",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A whole deleted function of thousands of lines is read once: every line
/// resolves to the deleted owner, so none is recorded. The work is one pass
/// over the run, not a walk back from each line.
#[test]
fn long_deleted_function_records_nothing() -> Result<(), String> {
    let body: Vec<String> = (0..20_000)
        .map(|index| format!("-  total = total + {index};"))
        .collect();
    let mut lines: Vec<&str> = vec!["-export function clamp(total: number): number {"];
    lines.extend(body.iter().map(String::as_str));
    lines.extend([
        "-  return total;",
        "-}",
        " export function cappedTotal(items: number[]): number {",
        "   return items.length;",
        " }",
    ]);
    let started = std::time::Instant::now();
    let findings = analyze("long-deleted-function", &lines)?;
    assert_eq!(base_side(&findings), Vec::new());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "a 20,000-line deletion took {:?}",
        started.elapsed()
    );
    Ok(())
}

/// Unspaced `length<3` is a comparison, not a type list: a later deleted
/// method in the same object stays a removed header, not body of `api`.
#[test]
fn deleted_method_after_an_unspaced_length_comparison_is_not_stolen() -> Result<(), String> {
    let findings = analyze(
        "unspaced-length-comparison",
        &[
            " const items = [1, 2, 3];",
            " export const api = {",
            "-  cap: items.length<3,",
            "-  save(id: string) {",
            "-    audit(id);",
            "-  },",
            " };",
        ],
    )?;
    let stolen = findings.iter().any(|finding| {
        finding.source_currentness.is_base_side()
            && finding
                .probe
                .before
                .as_deref()
                .is_some_and(|text| text.contains("save(id") || text.contains("audit(id)"))
    });
    assert!(!stolen, "save/audit joined into api: {findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}

/// #7705 shape 5: a removed behavior line followed by a removed bare `{`
/// joins as `doThing(); {` and must not be read as a method header. The
/// statement stays base-side evidence on the live owner. The inner block
/// line is also live-enclosing once the false header is refused.
#[test]
fn deleted_statement_then_bare_brace_is_not_swallowed_as_a_header() -> Result<(), String> {
    let findings = analyze(
        "statement-then-bare-brace",
        &[
            " export function cappedTotal(items: number[]): number {",
            "-  doThing();",
            "-  {",
            "-    audit(items);",
            "-  }",
            "   return items.length;",
            " }",
        ],
    )?;
    let dos = findings
        .iter()
        .filter(|finding| {
            finding.source_currentness.is_base_side()
                && finding.probe.before.as_deref().map(str::trim) == Some("doThing();")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        dos.len(),
        1,
        "doThing(); must not be swallowed as a header: {findings:?}"
    );
    assert_eq!(
        dos[0]
            .probe
            .owner
            .as_ref()
            .map(|owner| owner.0.rsplit("::").next().unwrap_or_default()),
        Some("cappedTotal"),
        "{findings:?}"
    );
    assert!(
        findings.iter().any(|finding| {
            finding.source_currentness.is_base_side()
                && finding
                    .probe
                    .before
                    .as_deref()
                    .is_some_and(|text| text.contains("audit(items)"))
        }),
        "the block body stays on the live owner: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .all(|finding| !finding.is_candidate_actionable()),
        "no kept record may become actionable: {findings:?}"
    );
    Ok(())
}

/// An Allman-style method header (`save()` then `{` on the next line) is
/// still a header: deleting it does not give its body to the enclosing
/// live function. Shape 5 must not refuse this join.
#[test]
fn deleted_allman_method_header_is_not_given_to_the_enclosing_function() -> Result<(), String> {
    let findings = analyze(
        "allman-method-header",
        &[
            " export function cappedTotal(items: number[]): number {",
            "   const api = {",
            "-    save()",
            "-    {",
            "-      audit(id);",
            "-    },",
            "   };",
            "   return items.length;",
            " }",
        ],
    )?;
    assert_eq!(base_side(&findings), Vec::new(), "{findings:?}");
    Ok(())
}

/// A nested function header inside a deleted `forEach(() => {` callback is
/// its own removed owner: wrap-join must not swallow it into the live
/// enclosing function.
#[test]
fn deleted_nested_header_inside_a_callback_is_not_stolen() -> Result<(), String> {
    let findings = analyze(
        "nested-header-in-callback",
        &[
            " export function cappedTotal(items: number[]): number {",
            "-  items.forEach((item) => {",
            "-    function helper() {",
            "-      audit(item);",
            "-    }",
            "-  });",
            "   return items.length;",
            " }",
        ],
    )?;
    let stolen = findings.iter().any(|finding| {
        finding.source_currentness.is_base_side()
            && finding.probe.before.as_deref().is_some_and(|text| {
                text.contains("function helper") || text.contains("audit(item)")
            })
    });
    assert!(
        !stolen,
        "nested helper/audit joined into cappedTotal: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .filter(|finding| finding.source_currentness.is_base_side())
            .all(|finding| !finding.is_candidate_actionable()),
        "base-side records must stay non-actionable: {findings:?}"
    );
    Ok(())
}
