//! #15358: module `const` chains of literal integer arithmetic
//! (`const m = s * 60; const h = m * 60;`) fold to a known boundary value, so
//! an exact test input at that value witnesses the changed comparison.

use super::*;

use crate::analysis::language::typescript::boundary_input::ts_module_folded_integer_constant;

fn fold(source: &str, name: &str) -> Option<i64> {
    ts_module_folded_integer_constant(Path::new("src/format.ts"), source, name)
}

#[test]
fn a_literal_chain_folds_through_arithmetic_and_separators() {
    let source = "const s = 1_000;\nconst m = s * 60;\nconst h = (m * 60) + 0;\nconst d = h - 1 + 1;\nexport function f(x: number) { return x > h; }\n";
    assert_eq!(fold(source, "s"), Some(1000));
    assert_eq!(fold(source, "m"), Some(60_000));
    assert_eq!(fold(source, "h"), Some(3_600_000));
    assert_eq!(fold(source, "d"), Some(3_600_000));
}

#[test]
fn every_unrecognised_shape_refuses_to_fold() {
    let cases: [(&str, &str); 14] = [
        // reassignable binding
        (
            "let h = 60 * 60;\nexport function f(x: number) { return x > h; }\n",
            "let",
        ),
        ("var h = 60 * 60;\n", "var"),
        // a link of the chain is not an immutable literal
        ("let m = 60;\nconst h = m * 60;\n", "let link"),
        // overflow past the safe-integer range, at i64 and at 2^53
        (
            "const a = 1000000000;\nconst h = a * a * a;\n",
            "i64 overflow",
        ),
        (
            "const a = 4503599627370496;\nconst h = a * 4;\n",
            "unsafe integer",
        ),
        // not integer / not one of * + -
        ("const h = 1000 * 60.5;\n", "float"),
        ("const h = 7200 / 2;\n", "division"),
        ("const h = 2 ** 10;\n", "exponent"),
        ("const h = -60 * -60;\n", "unary minus"),
        ("const h = 60 - 61 + 3661;\n", "negative intermediate"),
        ("const h = Number(60) * 60;\n", "call"),
        // cyclic and unresolved references
        ("const a = b * 2;\nconst b = a * 2;\n", "cycle"),
        ("const h = unknown * 60;\n", "unbound name"),
        // shadowed or rebound elsewhere in the module
        (
            "const h = 60 * 60;\nexport function f(h: number) { return h > 1; }\n",
            "parameter shadow",
        ),
    ];
    for (source, reason) in cases {
        assert_eq!(fold(source, "h"), None, "{reason}");
    }
}

#[test]
fn a_non_module_level_binding_or_second_declaration_refuses() {
    let nested = "export function f(x: number) {\n  const h = 60 * 60;\n  return x > h;\n}\n";
    assert_eq!(
        fold(nested, "h"),
        None,
        "a function-local const is not a module constant"
    );
    let shadowed =
        "const h = 60 * 60;\nexport function f(x: number) {\n  const h = 5;\n  return x > h;\n}\n";
    assert_eq!(fold(shadowed, "h"), None, "an inner redeclaration shadows");
    let twice = "const h = 60 * 60;\nconst h = 61;\n";
    assert_eq!(fold(twice, "h"), None, "a second declaration is ambiguous");
    let written = "const h = 60 * 60;\nexport function f() { h = 5; }\n";
    assert_eq!(fold(written, "h"), None, "a write to the name");
    let opaque = "const h = 60 * 60;\nexport function f(x: number) { return eval(\"x\") > h; }\n";
    assert_eq!(fold(opaque, "h"), None, "eval can rebind names");
}

fn folded_finding(label: &str, source: &str, argument: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/format.ts"), source)?;
    ts_write_file(
        &root.join("tests/format.test.ts"),
        &format!(
            "import {{ formatShort }} from \"../src/format\";\ntest(\"hour\", () => {{\n  expect(formatShort({argument})).toBe(\"1h\");\n}});\n"
        ),
    )?;
    let changed = "  if (msAbs >= h) {";
    let line = source
        .lines()
        .position(|text| text == changed)
        .ok_or("changed line not in source")?
        + 1;
    let file = ChangedFile {
        path: PathBuf::from("src/format.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: changed.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: "  if (msAbs > h) {".to_string(),
            new_side_line: line,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[file],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn duration_source(constants: &str) -> String {
    duration_source_with(constants, "ms: number", "const msAbs = Math.abs(ms);")
}

fn duration_source_with(constants: &str, signature: &str, local: &str) -> String {
    format!(
        "{constants}\nexport function formatShort({signature}): string {{\n  {local}\n  if (msAbs >= h) {{\n    return `${{Math.round(ms / h)}}h`;\n  }}\n  return `${{ms}}ms`;\n}}\n"
    )
}

#[test]
fn an_exact_input_at_the_folded_boundary_witnesses_the_changed_comparison() -> Result<(), String> {
    let source = duration_source("const s = 1000;\nconst m = s * 60;\nconst h = m * 60;");
    let hit = folded_finding("fold-hit", &source, "3_600_000")?;
    assert_eq!(hit.class, ExposureClass::Exposed, "{:?}", hit.evidence);
    let miss = folded_finding("fold-miss", &source, "3_599_999")?;
    assert_ne!(miss.class, ExposureClass::Exposed, "an off-boundary input");
    let miss = folded_finding("fold-miss-above", &source, "7_200_000")?;
    assert_ne!(miss.class, ExposureClass::Exposed, "an off-boundary input");
    Ok(())
}

#[test]
fn a_refused_chain_never_witnesses_the_boundary_input() -> Result<(), String> {
    let refused = [
        "const s = 1000;\nlet m = s * 60;\nconst h = m * 60;",
        "const s = 1000;\nconst m = s / 1;\nconst h = m * 3600;",
        "const s = 1000000000;\nconst h = s * s * s;",
    ];
    for (index, constants) in refused.into_iter().enumerate() {
        let finding = folded_finding(
            &format!("fold-refused-{index}"),
            &duration_source(constants),
            "3_600_000",
        )?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{constants}");
    }
    Ok(())
}

const CHAIN: &str = "const s = 1000;\nconst m = s * 60;\nconst h = m * 60;";

/// The derived local is credited only as an identity alias of exactly one
/// parameter, with the literal at that parameter's position (#15358).
#[test]
fn an_identity_alias_of_the_parameter_witnesses_at_its_own_position() -> Result<(), String> {
    for (index, local) in [
        "const msAbs = ms;",
        "const msAbs = Math.abs(ms);",
        "const msAbs: number = ms;",
        "const msAbs: number = Math.abs(ms);",
    ]
    .into_iter()
    .enumerate()
    {
        let source = duration_source_with(CHAIN, "ms: number, other: number = 0", local);
        let hit = folded_finding(&format!("alias-hit-{index}"), &source, "3_600_000")?;
        assert_eq!(
            hit.class,
            ExposureClass::Exposed,
            "{local}: {:?}",
            hit.evidence
        );
    }
    Ok(())
}

#[test]
fn a_literal_in_another_parameters_position_does_not_witness() -> Result<(), String> {
    let source = duration_source_with(
        CHAIN,
        "ms: number, other: number = 0",
        "const msAbs = Math.abs(ms);",
    );
    let finding = folded_finding("diffarg", &source, "5, 3_600_000")?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_local_that_transforms_the_parameter_does_not_witness() -> Result<(), String> {
    let cases = [
        ("div2", "const msAbs = Math.abs(ms) / 2;"),
        ("neg", "const msAbs = -ms;"),
        ("plus", "const msAbs = ms + other;"),
        ("written", "ms = ms / 2;\n  const msAbs = ms;"),
        ("redeclared", "const msAbs = ms;\n  { const msAbs = 1; }"),
        (
            "destructured-object",
            "({ ms } = other);\n  const msAbs = ms;",
        ),
        ("destructured-array", "[ms] = other;\n  const msAbs = ms;"),
        ("typed-transform", "const msAbs: number = ms / 2;"),
    ];
    for (label, local) in cases {
        let source = duration_source_with(CHAIN, "ms: number, other: number = 0", local);
        let finding = folded_finding(label, &source, "3_600_000")?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.evidence
        );
    }
    Ok(())
}
