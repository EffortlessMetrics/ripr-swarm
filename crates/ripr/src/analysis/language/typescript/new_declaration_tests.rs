//! Tests for the new-declaration guard: the opening line of a NEW function
//! whose body carries its own added lines is not a behavior probe.

use super::*;

const PRICE_TS: &str = "export function loyaltyPrice(amount: number, years: number): number {\n  if (years >= 5) {\n    return amount - 100;\n  }\n  return amount;\n}\nexport const bump = (x: number) => x + 1;\n";

const PRICE_TEST_TS: &str = "import { loyaltyPrice, bump } from '../src/price';\ntest('loyalty boundary', () => {\n  expect(loyaltyPrice(1000, 5)).toBe(900);\n  expect(loyaltyPrice(1000, 4)).toBe(1000);\n});\ntest('bump', () => {\n  expect(bump(1)).toBe(2);\n});\n";

fn line(number: usize, text: &str) -> crate::analysis::diff::ChangedLine {
    crate::analysis::diff::ChangedLine {
        line: number,
        new_side_line: number,
        text: text.to_string(),
    }
}

/// A removed line recorded at new-side line 1, where its block's added run
/// starts.
fn comment(number: usize, text: &str) -> crate::analysis::diff::ChangedLine {
    let mut removed = line(number, text);
    removed.new_side_line = 1;
    removed
}

fn source_line(source: &str, number: usize) -> Result<&str, String> {
    source
        .lines()
        .nth(number - 1)
        .ok_or_else(|| format!("fixture has no line {number}"))
}

fn analyze(
    label: &str,
    source: &str,
    added: &[usize],
    removed: Vec<crate::analysis::diff::ChangedLine>,
) -> Result<Vec<Finding>, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-new-decl-{label}-{}-{nanos}",
        std::process::id()
    ));
    for (path, contents) in [
        ("src/price.ts", source),
        ("tests/price.test.ts", PRICE_TEST_TS),
    ] {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create_dir_all({}): {err}", parent.display()))?;
        }
        std::fs::write(&file, contents)
            .map_err(|err| format!("write({}): {err}", file.display()))?;
    }
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
    let mut added_lines = Vec::new();
    for number in added {
        added_lines.push(line(*number, source_line(source, *number)?));
    }
    let changed_files = vec![ChangedFile {
        path: PathBuf::from("src/price.ts"),
        added_lines,
        removed_lines: removed,
    }];
    let result = TypeScriptAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.changed_files, 1, "the changed file must be analyzed");
    Ok(result.findings)
}

fn classes_on(findings: &[Finding], number: usize) -> Vec<ExposureClass> {
    findings
        .iter()
        .filter(|finding| finding.probe.location.line == number)
        .map(|finding| finding.class.clone())
        .collect()
}

/// A new function with an exact boundary test: the declaration line yields no
/// probe, and the body predicate is exposed. Removing the guard in `mod.rs`
/// makes the first assertion fail with a weakly exposed declaration probe.
#[test]
fn new_function_declaration_line_is_not_probed_when_body_is_added() -> Result<(), String> {
    let findings = analyze("new-fn", PRICE_TS, &[1, 2, 3, 4, 5, 6], Vec::new())?;
    assert_eq!(
        classes_on(&findings, 1),
        Vec::<ExposureClass>::new(),
        "the declaration line of a new function has no behavior of its own"
    );
    assert_eq!(
        classes_on(&findings, 2),
        vec![ExposureClass::Exposed],
        "the body predicate carries the behavior and is exposed by the boundary test"
    );
    assert_eq!(classes_on(&findings, 3), vec![ExposureClass::Exposed]);
    Ok(())
}

/// A changed default value is runtime behavior: the paired signature line
/// keeps its probe even though body lines were added too.
#[test]
fn changed_default_value_on_signature_keeps_probe() -> Result<(), String> {
    let source = PRICE_TS.replacen("years: number)", "years = 3)", 1);
    let findings = analyze(
        "changed-default",
        &source,
        &[1, 2, 3],
        vec![line(
            1,
            "export function loyaltyPrice(amount: number, years = 5): number {",
        )],
    )?;
    assert_eq!(classes_on(&findings, 1).len(), 1);
    Ok(())
}

/// A NEW function whose signature carries a default value keeps its probe:
/// the default is behavior a test can discriminate.
#[test]
fn new_function_with_default_value_keeps_probe() -> Result<(), String> {
    let source = PRICE_TS.replacen("years: number)", "years = 3)", 1);
    let findings = analyze("new-default", &source, &[1, 2, 3, 4, 5, 6], Vec::new())?;
    assert_eq!(classes_on(&findings, 1).len(), 1);
    Ok(())
}

/// A one-line arrow function carries its body on the declaration line.
#[test]
fn one_line_arrow_function_keeps_probe() -> Result<(), String> {
    let findings = analyze("arrow", PRICE_TS, &[7], Vec::new())?;
    assert_eq!(classes_on(&findings, 7).len(), 1);
    Ok(())
}

#[test]
fn signature_opening_line_shapes() {
    let file = Path::new("src/t.ts");
    for opening in [
        "export function f(a: number, b: number): number {",
        "function f(a) {",
        "export default async function f(a: string): Promise<void> {",
        "export const f = (a: number): number => {",
        "const f = async (a) => {",
        "const f = function (a) {",
        "total(a: number): number {",
        "get total(): number {",
        "function f(...rest: number[]) {",
    ] {
        assert!(
            is_signature_opening_line(file, opening),
            "expected an opening line: `{opening}`"
        );
    }
    for behavior in [
        "export const f = (x: number) => x + 1;",
        "function f(a = 1) {",
        "function f({ a = 1 }) {",
        "function f(a) { return a;",
        "export const f = (a) =>",
        "constructor(private readonly a: number) {",
        "[key](a) {",
        "@decorate() total(a) {",
        "if (a > 1) {",
        "export function f(",
    ] {
        assert!(
            !is_signature_opening_line(file, behavior),
            "expected a behavior-bearing line: `{behavior}`"
        );
    }
}

/// Review of #4428: a function made `async` while a helper is inserted above
/// it. Git can pair the removed old signature with the inserted helper, so
/// the signature has no in-place removed partner; the old line naming the
/// same function still marks it as a changed signature that keeps its probe.
#[test]
fn changed_signature_paired_elsewhere_by_git_keeps_probe() -> Result<(), String> {
    let source = PRICE_TS.replacen("export function", "export async function", 1);
    let mut removed = line(
        7,
        "export function loyaltyPrice(amount: number, years: number): number {",
    );
    removed.new_side_line = 7;
    let findings = analyze("paired-elsewhere", &source, &[1, 2, 3], vec![removed])?;
    assert_eq!(classes_on(&findings, 1).len(), 1, "{findings:?}");
    Ok(())
}

/// #7524: a new function replacing three comment lines, an even block. The
/// opening line pairs by position with the first comment, which does not
/// look like it, so the opening line still reads as new and yields no
/// probe; the body line keeps its own. A guard that treated any counterpart
/// as an old signature would probe the opening line.
#[test]
fn new_function_replacing_an_unrelated_line_is_not_probed() -> Result<(), String> {
    let source =
        format!("export function helper(a: number): number {{\n  return a * 2;\n}}\n{PRICE_TS}");
    let findings = analyze(
        "new-fn-replacing-comment",
        &source,
        &[1, 2, 3],
        vec![
            line(1, "// pricing rules"),
            comment(2, "// apply the loyalty discount"),
            comment(3, "// in whole currency units"),
        ],
    )?;
    assert_eq!(
        classes_on(&findings, 1),
        Vec::<ExposureClass>::new(),
        "{findings:?}"
    );
    assert_eq!(classes_on(&findings, 2).len(), 1, "{findings:?}");
    Ok(())
}

/// The control: a renamed function whose body grows in the same block. Its
/// old signature looks like the new one, so the opening line is a changed
/// signature and keeps its probe.
#[test]
fn renamed_function_in_an_uneven_block_keeps_probe() -> Result<(), String> {
    let findings = analyze(
        "renamed-fn-uneven",
        PRICE_TS,
        &[1, 2, 3],
        vec![line(
            1,
            "export function oldPrice(amount: number, years: number): number {",
        )],
    )?;
    assert_eq!(classes_on(&findings, 1).len(), 1, "{findings:?}");
    Ok(())
}

/// #7524: the annotation-only guard reads the same pairing. In a two-line
/// block whose second line only changes a type, that line pairs with the
/// second removed line by position and yields no probe; before, it found
/// no old line and was probed as new code.
#[test]
fn annotation_only_change_on_a_later_line_of_a_block_is_not_probed() -> Result<(), String> {
    let mut closing = line(6, "};");
    closing.new_side_line = 6;
    let mut bump = line(7, "export const bump = (x: any) => x + 1;");
    bump.new_side_line = 6;
    let findings = analyze(
        "annotation-later-line",
        PRICE_TS,
        &[6, 7],
        vec![closing, bump],
    )?;
    assert_eq!(
        classes_on(&findings, 7),
        Vec::<ExposureClass>::new(),
        "{findings:?}"
    );
    Ok(())
}

/// The control: the same block with a runtime change on the later line
/// keeps that line's probe.
#[test]
fn behavior_change_on_a_later_line_of_a_block_keeps_probe() -> Result<(), String> {
    let mut closing = line(6, "};");
    closing.new_side_line = 6;
    let mut bump = line(7, "export const bump = (x: number) => x + 2;");
    bump.new_side_line = 6;
    let findings = analyze(
        "behavior-later-line",
        PRICE_TS,
        &[6, 7],
        vec![closing, bump],
    )?;
    assert_eq!(classes_on(&findings, 7).len(), 1, "{findings:?}");
    Ok(())
}

/// #14097: the same new function with its signature wrapped over four lines,
/// as a formatter writes it past the line width.
const WRAPPED_TS: &str = "export function loyaltyPrice(\n  amount: number,\n  years: number,\n): number {\n  if (years >= 5) {\n    return amount - 100;\n  }\n  return amount;\n}\n";

/// A wrapped signature of a new function yields no probe on any of its
/// lines, and the body reads exactly as under the one-line spelling. Without
/// the guard in `mod.rs` lines 1 to 4 each carry a probe (date-fns `18cbd43`:
/// nine signature probes for a one-line body).
#[test]
fn wrapped_signature_of_a_new_function_is_not_probed() -> Result<(), String> {
    let findings = analyze(
        "wrapped-new-fn",
        WRAPPED_TS,
        &[1, 2, 3, 4, 5, 6, 7, 8, 9],
        Vec::new(),
    )?;
    for header_line in 1..=4 {
        assert_eq!(
            classes_on(&findings, header_line),
            Vec::<ExposureClass>::new(),
            "signature line {header_line} has no behavior of its own: {findings:?}"
        );
    }
    assert_eq!(
        classes_on(&findings, 5),
        vec![ExposureClass::Exposed],
        "the body predicate keeps the verdict it has under a one-line signature"
    );
    assert_eq!(classes_on(&findings, 6), vec![ExposureClass::Exposed]);
    Ok(())
}

/// A default value anywhere in the wrapped signature is behavior a test can
/// discriminate, so the header fails closed and its lines keep their probes.
#[test]
fn wrapped_signature_with_a_default_value_keeps_probes() -> Result<(), String> {
    let source = WRAPPED_TS.replacen("  years: number,", "  years = 3,", 1);
    let findings = analyze(
        "wrapped-default",
        &source,
        &[1, 2, 3, 4, 5, 6, 7, 8, 9],
        Vec::new(),
    )?;
    assert_eq!(classes_on(&findings, 3).len(), 1, "{findings:?}");
    assert_eq!(classes_on(&findings, 1).len(), 1, "{findings:?}");
    Ok(())
}

/// A parameter line added to the wrapped signature of an EXISTING function:
/// the header is not all new, so the added line keeps its probe.
#[test]
fn parameter_added_to_an_existing_wrapped_signature_keeps_probe() -> Result<(), String> {
    let findings = analyze("wrapped-existing", WRAPPED_TS, &[3, 5], Vec::new())?;
    assert_eq!(classes_on(&findings, 3).len(), 1, "{findings:?}");
    Ok(())
}

/// A renamed function whose wrapped signature was rewritten in place: the
/// removed opening line looks like the new one, so this is a changed
/// signature and every header line keeps its probe.
#[test]
fn renamed_function_with_a_wrapped_signature_keeps_probes() -> Result<(), String> {
    let mut removed = Vec::new();
    for (number, text) in [
        (1, "export function oldPrice("),
        (2, "  amount: number,"),
        (3, "  months: number,"),
        (4, "): number {"),
    ] {
        let mut old = line(number, text);
        old.new_side_line = 1;
        removed.push(old);
    }
    let findings = analyze("wrapped-renamed", WRAPPED_TS, &[1, 2, 3, 4, 5], removed)?;
    assert_eq!(classes_on(&findings, 1).len(), 1, "{findings:?}");
    assert_eq!(classes_on(&findings, 3).len(), 1, "{findings:?}");
    Ok(())
}

/// A wrapped signature whose body adds nothing to probe (an empty function)
/// keeps its opening probe: something must stand for the new owner.
#[test]
fn wrapped_signature_without_an_added_body_line_keeps_probe() -> Result<(), String> {
    let source = "export function noop(\n  amount: number,\n  years: number,\n): void {\n}\n";
    let findings = analyze("wrapped-empty-body", source, &[1, 2, 3, 4, 5], Vec::new())?;
    assert_eq!(classes_on(&findings, 1).len(), 1, "{findings:?}");
    Ok(())
}

/// Lines of an all-new `source` that carry at least one probe.
fn probed_lines(label: &str, source: &str) -> Result<Vec<usize>, String> {
    let all: Vec<usize> = (1..=source.lines().count()).collect();
    let mut lines: Vec<usize> = analyze(label, source, &all, Vec::new())?
        .iter()
        .map(|finding| finding.probe.location.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    Ok(lines)
}

/// The wrapped shapes the guard reads, through the production path: generics
/// with a default TYPE, a destructured parameter, an object-typed parameter,
/// a wrapped arrow, and a wrapped method. Only body lines are probed.
#[test]
fn wrapped_header_shapes_leave_only_body_probes() -> Result<(), String> {
    for (label, source, body) in [
        (
            "generics",
            "export function f<\n  T extends Date,\n  R extends Date = T,\n>(\n  date: T,\n): R {\n  return go(date);\n}\n",
            vec![7],
        ),
        (
            "destructured",
            "export function f({\n  a,\n  b,\n}: Opts): number {\n  return a + b;\n}\n",
            vec![5],
        ),
        (
            "object-type",
            "export function f(\n  opts: {\n    a: number;\n  },\n): number {\n  return opts.a;\n}\n",
            vec![6],
        ),
        (
            "arrow",
            "export const f = (\n  a: number,\n): number => {\n  return a + 1;\n};\n",
            vec![4],
        ),
        (
            "method",
            "export class Cart {\n  total(\n    a: number,\n  ): number {\n    return a + 1;\n  }\n}\n",
            vec![5],
        ),
    ] {
        assert_eq!(probed_lines(label, source)?, body, "{label}");
    }
    Ok(())
}

/// Wrapped headers that carry runtime behavior, or are not new function
/// headers at all, keep a probe on the line that holds it.
#[test]
fn wrapped_header_shapes_that_keep_their_probes() -> Result<(), String> {
    for (label, source, probed) in [
        (
            "destructuring-default",
            "export function f({\n  a = 1,\n}: Opts): number {\n  return a;\n}\n",
            2,
        ),
        (
            "rest-default",
            "export function f(\n  ...[a = 1]: number[]\n): number {\n  return a;\n}\n",
            2,
        ),
        (
            "parameter-property",
            "export class Cart {\n  constructor(\n    private readonly a: number,\n  ) {\n    log(a);\n  }\n}\n",
            3,
        ),
        (
            "decorated-parameter",
            "export class Cart {\n  total(\n    @Inject() a: number,\n  ): number {\n    return a + 1;\n  }\n}\n",
            3,
        ),
        (
            "call-not-a-header",
            "export function f(a: number): number {\n  return go(\n    a,\n    {\n      b: 1,\n    });\n}\n",
            3,
        ),
    ] {
        let lines = probed_lines(label, source)?;
        assert!(
            lines.contains(&probed),
            "{label}: line {probed} of {lines:?}"
        );
    }
    Ok(())
}

/// A bracket inside a header comment or string throws the round-bracket
/// count off. The count only picks the line where the parse is tried and the
/// parse decides, so the cost is a missed header whose lines keep their
/// probes. A body line is never taken for part of the header.
#[test]
fn bracket_in_a_header_comment_or_string_never_hides_a_body_line() -> Result<(), String> {
    // An unclosed `(` in a comment: the count never returns to zero on the
    // closer, so no header is read and every line keeps its probe.
    let lines = probed_lines(
        "comment-open",
        "export function f(\n  a: number, // threshold (inclusive\n): number {\n  return a + 1;\n}\n",
    )?;
    assert!(lines.contains(&2) && lines.contains(&4), "{lines:?}");

    // The same, with a body comment that brings the count back to zero on a
    // body line ending in `{`. The text joined up to that line is not a
    // signature with an empty body, so the parse refuses it.
    let lines = probed_lines(
        "comment-open-then-closed-in-body",
        "export function f(\n  a: number, // (open\n): number {\n  // closes) here\n  if (a > 1) {\n    return 1;\n  }\n  return a;\n}\n",
    )?;
    for body_line in [5, 6, 8] {
        assert!(
            lines.contains(&body_line),
            "body line {body_line} of {lines:?}"
        );
    }

    // A stray `)` in a comment: the count reaches zero early, but only a line
    // ending in `{` is tried, and that is the real closer. The header is read.
    assert_eq!(
        probed_lines(
            "comment-close",
            "export function f(\n  a: number, // see note)\n  b: number,\n): number {\n  return a + b;\n}\n",
        )?,
        vec![5]
    );

    // A `)` inside a default string: the closer is tried, and the default
    // value keeps the header probed.
    let lines = probed_lines(
        "string-close",
        "export function f(\n  sep = ')',\n): string {\n  return sep;\n}\n",
    )?;
    assert!(lines.contains(&2) && lines.contains(&4), "{lines:?}");
    Ok(())
}
