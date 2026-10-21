//! #8569: a changed line under an `if` arm no related test call enters keeps
//! `exposed` only when some call's inputs can enter the arm.

use super::*;

const PRICE: &str = "export function price(qty: number, coupon?: string): number {\n  let total = qty * 10;\n  if (coupon === 'HALF') {\n    total = total / 3;\n  }\n  return total;\n}\n";
const ELSE_PRICE: &str = "export function price(qty: number, coupon?: string): number {\n  let total = qty * 10;\n  if (coupon) {\n    total = total / 3;\n  } else {\n    total = total + 7;\n  }\n  return total;\n}\n";

fn finding_at(
    label: &str,
    source: &str,
    line: usize,
    removed: &str,
    added: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/price.ts"), source)?;
    ts_write_file(&root.join("tests/price.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/price.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: added.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: removed.to_string(),
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
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn then_arm(label: &str, test: &str) -> Result<Finding, String> {
    let source = PRICE.replace("total / 3", "total / 2");
    finding_at(
        label,
        &source,
        4,
        "    total = total / 3;",
        "    total = total / 2;",
        test,
    )
}

fn test_with(calls: &[(&str, &str)]) -> String {
    let mut body = String::new();
    for (call, expected) in calls {
        body.push_str(&format!("  expect({call}).toBe({expected});\n"));
    }
    format!("import {{ price }} from \"../src/price\";\ntest(\"price\", () => {{\n{body}}});\n")
}

#[test]
fn a_guard_no_test_input_enters_withdraws_exposed() -> Result<(), String> {
    for (label, calls) in [
        ("guard-missing-arg", vec![("price(2)", "20")]),
        (
            "guard-other-literal",
            vec![("price(2)", "20"), ("price(3, 'FULL')", "30")],
        ),
    ] {
        let finding = then_arm(label, &test_with(&calls))?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("`coupon === 'HALF'`")
                    && line.contains("no related test call enters")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn a_guard_some_test_input_can_enter_keeps_exposed() -> Result<(), String> {
    // Positive control: the entering call keeps the credit.
    let entering = then_arm("guard-entered", &test_with(&[("price(3, 'HALF')", "15")]))?;
    assert_eq!(
        entering.class,
        ExposureClass::Exposed,
        "{:?}",
        entering.missing
    );
    let mixed = then_arm(
        "guard-mixed",
        &test_with(&[("price(2)", "20"), ("price(3, 'HALF')", "15")]),
    )?;
    assert_eq!(mixed.class, ExposureClass::Exposed, "{:?}", mixed.missing);
    // An argument the reader cannot evaluate may enter the guard.
    let computed = then_arm("guard-computed", &test_with(&[("price(2, pick())", "20")]))?;
    assert_eq!(
        computed.class,
        ExposureClass::Exposed,
        "{:?}",
        computed.missing
    );
    Ok(())
}

#[test]
fn an_else_arm_is_entered_by_the_inputs_that_fail_the_guard() -> Result<(), String> {
    let source = ELSE_PRICE.replace("total + 7", "total + 8");
    let run = |label: &str, calls: &[(&str, &str)]| {
        finding_at(
            label,
            &source,
            6,
            "    total = total + 7;",
            "    total = total + 8;",
            &test_with(calls),
        )
    };
    // Every call is truthy, so the `else` arm never runs.
    let weak = run("else-unentered", &[("price(2, 'A')", "6")])?;
    assert_eq!(
        weak.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        weak.missing
    );
    assert!(
        weak.missing.iter().any(|line| line.contains("!(coupon)")),
        "{:?}",
        weak.missing
    );
    // Omitting the coupon enters the `else` arm.
    let strong = run("else-entered", &[("price(2)", "28")])?;
    assert_eq!(strong.class, ExposureClass::Exposed, "{:?}", strong.missing);
    Ok(())
}

#[test]
fn unreadable_entries_keep_exposed() -> Result<(), String> {
    // A spread may carry the coupon at index 1.
    let spread = "import { price } from \"../src/price\";\ntest(\"price\", () => {\n  const args = [3, 'HALF'];\n  expect(price(...args)).toBe(15);\n});\n";
    let finding = then_arm("guard-spread", spread)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );

    // A file-local helper enters the arm; the direct call alone would not.
    let helper = "import { price } from \"../src/price\";\nconst check = (c: string) => price(3, c);\ntest(\"price\", () => {\n  expect(price(2)).toBe(20);\n  expect(check('HALF')).toBe(10);\n});\n";
    let finding = then_arm("guard-helper", helper)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );

    // A member call on a helper object may enter the arm too.
    let member = "import { price } from \"../src/price\";\nconst h = { check: (c: string) => price(3, c) };\ntest(\"price\", () => {\n  expect(price(2)).toBe(20);\n  expect(h.check('HALF')).toBe(10);\n});\n";
    let finding = then_arm("guard-member-helper", member)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );

    // An explicit `undefined` applies the parameter default.
    let source = PRICE
        .replace("coupon?: string", "coupon = 'HALF'")
        .replace("total / 3", "total / 2");
    let test = test_with(&[("price(3, undefined)", "15")]);
    let finding = finding_at(
        "guard-default",
        &source,
        4,
        "    total = total / 3;",
        "    total = total / 2;",
        &test,
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );

    // CRLF sources are not read.
    let crlf = PRICE
        .replace("total / 3", "total / 2")
        .replace('\n', "\r\n");
    let finding = finding_at(
        "guard-crlf",
        &crlf,
        4,
        "    total = total / 3;",
        "    total = total / 2;",
        &test_with(&[("price(2)", "20")]),
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn file_level_and_hook_calls_keep_exposed() -> Result<(), String> {
    // A file-level call enters the arm even though the test body does not.
    let top = "import { price } from \"../src/price\";\nconst entered = price(3, 'HALF');\ntest(\"price\", () => {\n  expect(price(2)).toBe(20);\n  expect(entered).toBe(15);\n});\n";
    let finding = then_arm("guard-file-level", top)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let hook = "import { price } from \"../src/price\";\nlet entered: number;\nbeforeAll(() => {\n  entered = price(3, 'HALF');\n});\ntest(\"price\", () => {\n  expect(price(2)).toBe(20);\n  expect(entered).toBe(15);\n});\n";
    let finding = then_arm("guard-hook", hook)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    // An indexed or parenthesized receiver is not a framework object.
    let indexed = "import { price } from \"../src/price\";\nconst helpers = [{ run: (c: string) => price(3, c) }];\ntest(\"price\", () => {\n  expect(price(2)).toBe(20);\n  expect(helpers[0].run('HALF')).toBe(15);\n});\n";
    let finding = then_arm("guard-indexed", indexed)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_deleted_line_is_not_located_by_content() -> Result<(), String> {
    // The deleted `return null;` is gone from the new side; the identical
    // line in the `b` arm must not stand in for it.
    let source = "export function f(mode: string): null | number {\n  if (mode === 'a') {\n    // deleted line was here\n  } else if (mode === 'b') {\n    return null;\n  }\n  return 1;\n}\n";
    let test = "import { f } from \"../src/price\";\ntest(\"f\", () => {\n  expect(f('a')).toBe(1);\n});\n";
    let root = ts_unique_tempdir("guard-deleted")?;
    ts_write_file(&root.join("src/price.ts"), source)?;
    ts_write_file(&root.join("tests/price.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/price.ts"),
        added_lines: Vec::new(),
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 3,
            text: "    return null;".to_string(),
            new_side_line: 3,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    for finding in result?.findings {
        assert!(
            !finding
                .missing
                .iter()
                .any(|line| line.contains("no related test call enters")),
            "{:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn arrow_owner_locates_by_position_or_fails_open() -> Result<(), String> {
    let source = "export const price = (qty: number, coupon?: string): number => {\n  let total = qty * 10;\n  if (coupon === 'HALF') {\n    total = total / 2;\n  }\n  return total;\n};\n";
    let finding = finding_at(
        "guard-arrow",
        source,
        4,
        "    total = total / 3;",
        "    total = total / 2;",
        &test_with(&[("price(2)", "20")]),
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.missing
    );
    // An entering call still keeps the credit for an arrow owner.
    let entered = finding_at(
        "guard-arrow-entered",
        source,
        4,
        "    total = total / 3;",
        "    total = total / 2;",
        &test_with(&[("price(3, 'HALF')", "15")]),
    )?;
    assert_eq!(
        entered.class,
        ExposureClass::Exposed,
        "{:?}",
        entered.missing
    );
    Ok(())
}

#[test]
fn a_file_level_getter_that_calls_the_owner_keeps_exposed() -> Result<(), String> {
    let getter = "import { price } from \"../src/price\";\nconst h = { get value() { return price(3, 'HALF'); } };\ntest(\"price\", () => {\n  void h.value;\n  expect(price(2)).toBe(20);\n});\n";
    let finding = then_arm("guard-getter", getter)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn text_outside_the_first_arrow_block_is_read() -> Result<(), String> {
    let head = "import { price } from \"../src/price\";\n";
    for (label, body) in [
        (
            "guard-callback-after",
            "test(\"price\", function () {\n  expect(price(3, \"HALF\")).toBe(15);\n  [1].forEach(() => { expect(price(2)).toBe(20); });\n});\n",
        ),
        (
            "guard-each",
            "test.each([price(3, \"HALF\")])(\"n\", (v) => {\n  expect(price(2)).toBe(20);\n});\n",
        ),
        (
            "guard-title-arrow",
            "test(\"a => { b\", function () {\n  expect(price(3, \"HALF\")).toBe(15);\n  expect(price(2)).toBe(20);\n});\n",
        ),
    ] {
        let finding = then_arm(label, &format!("{head}{body}"))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn comments_before_an_if_keep_the_real_condition() -> Result<(), String> {
    // A comment before the guard once shifted the condition text to `0`,
    // which reads false for every call and demoted a change an entering call
    // covers.
    let source = PRICE
        .replace("  if (coupon", "  // 0\n  if (coupon")
        .replace("total / 3", "total / 2");
    let run = |label: &str, calls: &[(&str, &str)]| {
        finding_at(
            label,
            &source,
            5,
            "    total = total / 3;",
            "    total = total / 2;",
            &test_with(calls),
        )
    };
    let entered = run("guard-comment-entered", &[("price(3, 'HALF')", "15")])?;
    assert_eq!(
        entered.class,
        ExposureClass::Exposed,
        "{:?}",
        entered.missing
    );
    let missed = run("guard-comment-missed", &[("price(2)", "20")])?;
    assert_eq!(
        missed.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        missed.missing
    );
    Ok(())
}

#[test]
fn bracket_calls_and_loaders_keep_exposed() -> Result<(), String> {
    let head =
        "import { price } from \"../src/price\";\nimport * as pricing from \"../src/price\";\n";
    for (label, body) in [
        (
            "guard-bracket-literal",
            "test(\"p\", () => {\n  expect(pricing.price(2)).toBe(20);\n  expect(pricing['price'](3, 'HALF')).toBe(15);\n});\n",
        ),
        (
            "guard-bracket-key",
            "test(\"p\", () => {\n  const k = 'price';\n  expect(price(2)).toBe(20);\n  expect(pricing[k](3, 'HALF')).toBe(15);\n});\n",
        ),
        (
            "guard-require-actual",
            "test(\"p\", () => {\n  expect(price(2)).toBe(20);\n  expect(jest.requireActual('../src/checkout').half()).toBe(15);\n});\n",
        ),
    ] {
        let finding = then_arm(label, &format!("{head}{body}"))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    // A method shorthand's own parameter is not the owner's, annotated or not.
    for (label, method) in [
        ("guard-shadow", "run(coupon: string): void {"),
        (
            "guard-shadow-async",
            "async run(coupon: string): Promise<void> {",
        ),
        ("guard-shadow-plain", "run(coupon: string) {"),
        ("guard-shadow-fn-type", "run(coupon: string): () => void {"),
    ] {
        let source = format!(
            "export function price(qty: number, coupon?: string): number {{\n  let total = qty * 10;\n  const o = {{\n    {method}\n      if (coupon === 'HALF') {{\n        total = total / 2;\n      }}\n    }},\n  }};\n  o.run('HALF');\n  return total;\n}}\n"
        );
        let finding = finding_at(
            label,
            &source,
            6,
            "        total = total / 3;",
            "        total = total / 2;",
            &test_with(&[("price(2)", "20")]),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

const OPTIONS_PRICE: &str = "export function price(qty: number, { preserveOrder = true } = {}): string {\n  let out = \"\";\n  if (preserveOrder) {\n    out = \"ordered\";\n  } else {\n    out = \"unordered\";\n  }\n  return out + qty;\n}\n";

fn options_else_arm(label: &str, test: &str) -> Result<Finding, String> {
    let source = OPTIONS_PRICE.replace("\"unordered\"", "\"loose\"");
    finding_at(
        label,
        &source,
        6,
        "    out = \"unordered\";",
        "    out = \"loose\";",
        test,
    )
}

#[test]
fn an_options_object_guard_no_test_input_enters_withdraws_exposed() -> Result<(), String> {
    for (label, calls) in [
        ("options-omitted", vec![("price(2)", "\"ordered2\"")]),
        (
            "options-other-key",
            vec![
                ("price(2)", "\"ordered2\""),
                ("price(3, { other: 1 })", "\"ordered3\""),
            ],
        ),
        (
            "options-explicit-true",
            vec![("price(2, { preserveOrder: true })", "\"ordered2\"")],
        ),
    ] {
        let finding = options_else_arm(label, &test_with(&calls))?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("!(preserveOrder)")
                    && line.contains("no related test call enters")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn an_options_object_guard_some_test_input_can_enter_keeps_exposed() -> Result<(), String> {
    // Positive control: the call that turns the option off keeps the credit.
    let entering = options_else_arm(
        "options-entered",
        &test_with(&[("price(2, { preserveOrder: false })", "\"unordered2\"")]),
    )?;
    assert_eq!(
        entering.class,
        ExposureClass::Exposed,
        "{:?}",
        entering.missing
    );
    let mixed = options_else_arm(
        "options-mixed",
        &test_with(&[
            ("price(2)", "\"ordered2\""),
            ("price(3, { preserveOrder: false })", "\"unordered3\""),
        ]),
    )?;
    assert_eq!(mixed.class, ExposureClass::Exposed, "{:?}", mixed.missing);
    // Options the reader cannot read may turn the option off.
    for (label, call) in [
        ("options-variable", "price(2, opts)"),
        ("options-spread", "price(2, { ...base })"),
        (
            "options-computed-value",
            "price(2, { preserveOrder: pick() })",
        ),
    ] {
        let finding = options_else_arm(label, &test_with(&[(call, "\"x\"")]))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}
