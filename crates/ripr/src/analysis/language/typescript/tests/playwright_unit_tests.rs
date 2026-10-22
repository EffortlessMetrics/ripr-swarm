//! RIPR-SPEC-0243 rule 16: Playwright Test specs of pure functions. A soft
//! assertion, an awaited `test.step` body and an anchored `toThrow` regex
//! read like their plain forms; a moved threshold is witnessed by a call
//! between the old and new threshold.

use super::*;

/// Analyzes one edited line of `src/<module>.ts` against `test/<module>.test.ts`.
fn finding_for_edit(
    label: &str,
    module: &str,
    new_source: &str,
    line: usize,
    old: &str,
    test_source: &str,
) -> Result<Finding, String> {
    let new = new_source
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("{label}: no line {line}"))?
        .to_string();
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join(format!("src/{module}.ts")), new_source)?;
    ts_write_file(&root.join(format!("test/{module}.test.ts")), test_source)?;
    let changed = ChangedFile {
        path: PathBuf::from(format!("src/{module}.ts")),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: new,
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: old.to_string(),
            new_side_line: line,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on line {line}"))
}

fn playwright(imports: &str, body: &str) -> String {
    format!("import {{ expect, test }} from \"@playwright/test\";\n{imports}\n\n{body}\n")
}

const SHIPPING: &str = "export function shipping(weightKg: number): number {\n  return weightKg > 25 ? 900 : 400;\n}\n";
const SHIPPING_OLD: &str = "  return weightKg > 5 ? 900 : 400;";

fn shipping_finding(label: &str, source: &str, test_source: &str) -> Result<Finding, String> {
    finding_for_edit(label, "steps", source, 2, SHIPPING_OLD, test_source)
}

#[test]
fn a_soft_assertion_is_an_oracle() -> Result<(), String> {
    let source = "export function area(w: number, h: number): number {\n  return w * h * 2;\n}\n";
    for (label, call) in [
        ("hard", "expect(area(3, 4)).toBe(24);"),
        ("soft", "expect.soft(area(3, 4)).toBe(24);"),
    ] {
        let test = playwright(
            "import { area } from \"../src/area\";",
            &format!("test(\"area\", () => {{\n  {call}\n}});"),
        );
        let finding = finding_for_edit(label, "area", source, 2, "  return w * h;", &test)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn an_awaited_test_step_body_is_walked() -> Result<(), String> {
    let step = |call: &str| {
        playwright(
            "import { shipping } from \"../src/steps\";",
            &format!(
                "test(\"quotes\", async () => {{\n  {call}(\"heavy\", async () => {{\n    expect(shipping(20)).toBe(900);\n  }});\n}});"
            ),
        )
    };
    let credited = shipping_finding("step-awaited", SHIPPING, &step("await test.step"))?;
    assert_eq!(credited.class, ExposureClass::Exposed, "awaited step");
    let floating = shipping_finding("step-floating", SHIPPING, &step("test.step"))?;
    assert_ne!(floating.class, ExposureClass::Exposed, "floating step");
    // A `test` that is not Playwright's has no `step` that runs its body.
    let foreign = playwright(
        "import { shipping } from \"../src/steps\";\nconst test = { step: async (_name: string, body: () => Promise<void>) => { void body; } };",
        "it(\"quotes\", async () => {\n  await test.step(\"heavy\", async () => {\n    expect(shipping(20)).toBe(900);\n  });\n});",
    )
    .replace("import { expect, test }", "import { expect, test as it }");
    let foreign = shipping_finding("step-foreign", SHIPPING, &foreign)?;
    assert_ne!(foreign.class, ExposureClass::Exposed, "foreign test.step");
    Ok(())
}

#[test]
fn a_moved_threshold_is_witnessed_between_the_old_and_new_value() -> Result<(), String> {
    let spec = |body: &str| {
        playwright(
            "import { shipping } from \"../src/steps\";",
            &format!("test(\"quotes\", () => {{\n  {body}\n}});"),
        )
    };
    let between = shipping_finding(
        "threshold-between",
        SHIPPING,
        &spec("expect(shipping(20)).toBe(900);"),
    )?;
    assert_eq!(
        between.class,
        ExposureClass::Exposed,
        "20 is between 5 and 25"
    );
    for (label, body) in [
        ("above both", "expect(shipping(30)).toBe(900);"),
        ("below both", "expect(shipping(1)).toBe(400);"),
        ("dead expectation", "expect(shipping(20)).toBe(123);"),
        ("wrapper", "expect(shipping(20) > 0).toBe(true);"),
    ] {
        let finding = shipping_finding(label, SHIPPING, &spec(body))?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
    }
    // Equal arms: the threshold moves but no call can tell.
    let same = "export function shipping(weightKg: number): number {\n  return weightKg > 25 ? 900 : 900;\n}\n";
    let finding = finding_for_edit(
        "threshold-same-arms",
        "steps",
        same,
        2,
        "  return weightKg > 5 ? 900 : 900;",
        &spec("expect(shipping(20)).toBe(900);"),
    )?;
    assert_ne!(finding.class, ExposureClass::Exposed, "equal arms");
    Ok(())
}

#[test]
fn an_anchored_to_throw_regex_pins_the_message() -> Result<(), String> {
    let source = "export function parse(text: string): number {\n  if (text === \"\") {\n    throw new Error(\"not a number\");\n  }\n  return 1;\n}\n";
    let old = "    throw new Error(\"not a count\");";
    let spec = |matcher: &str| {
        playwright(
            "import { parse } from \"../src/parse\";",
            &format!(
                "test(\"refuses\", () => {{\n  expect(() => parse(\"\")).toThrow({matcher});\n}});"
            ),
        )
    };
    let anchored = finding_for_edit(
        "regex-anchored",
        "parse",
        source,
        3,
        old,
        &spec("/^not a count$/"),
    )?;
    assert_eq!(anchored.class, ExposureClass::Exposed, "anchored");
    for (label, matcher) in [
        ("unanchored", "/not a/"),
        ("case-insensitive", "/^not a count$/i"),
        ("alternation", "/^not a count$|^x$/"),
        ("other message", "/^something else$/"),
    ] {
        let finding = finding_for_edit(label, "parse", source, 3, old, &spec(matcher))?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}
