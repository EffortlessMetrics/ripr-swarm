//! RIPR-SPEC-0248 rules 2 and 3: an exact matcher that cannot tell the
//! changed return value apart keeps no `exposed` credit.

use super::*;

#[test]
fn neutral_operand_change_reads_one_bounded_shape() {
    for (line, want) in [
        ("  return unit * qty;", Some(('*', "qty", "1"))),
        ("return 2 / qty;", Some(('/', "qty", "1"))),
        ("return base + tip;", None),
        ("return base - tip;", None),
        ("return unit * qty + fee;", None),
        ("return unit ** qty;", None),
        ("return unit * 1;", None),
        ("return f(unit) * qty;", None),
        ("return unit *= qty;", None),
        ("const total = unit * qty;", None),
        ("returnunit * qty;", None),
    ] {
        let got = ts_neutral_operand_change(line)
            .map(|change| (change.operator, change.parameter, change.neutral));
        assert_eq!(
            got.as_ref().map(|(o, p, n)| (*o, p.as_str(), *n)),
            want,
            "{line}"
        );
    }
}

const SCALED: &str =
    "export function scaled(qty: number, unit: number): number {\n  return qty * unit;\n}\n";

fn finding_for(
    label: &str,
    owner_source: &str,
    old_line: &str,
    new_line: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let source = owner_source.replace(old_line, new_line);
    ts_write_file(&root.join("src/lib.ts"), &source)?;
    ts_write_file(&root.join("tests/lib.test.ts"), test)?;
    let line = source
        .lines()
        .position(|text| text == new_line)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: changed line absent"))?;
    let changed = ChangedFile {
        path: PathBuf::from("src/lib.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: new_line.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: old_line.to_string(),
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

fn scaled_finding(label: &str, body: &str) -> Result<Finding, String> {
    let test = format!(
        "import {{ scaled }} from \"../src/lib\";\ntest(\"scaled\", () => {{\n{body}\n}});\n"
    );
    finding_for(
        label,
        SCALED,
        "  return qty * unit;",
        "  return unit * qty;",
        &test,
    )
}

#[test]
fn identity_operand_in_every_tested_call_withdraws_credit() -> Result<(), String> {
    let property =
        "  for (let unit = 1; unit < 5; unit++) {\n    expect(scaled(1, unit)).toBe(unit);\n  }";
    let finding = scaled_finding("neutral-property", property)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("passes `1` for `qty`")),
        "{:?}",
        finding.missing
    );

    // One call with another quantity separates `*` from `/`.
    let with_second = format!("{property}\n  expect(scaled(3, 7)).toBe(21);");
    let finding = scaled_finding("neutral-second-call", &with_second)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // The identity on the other operand does not hide the inverse:
    // `1 / qty` differs from `qty`.
    let left = "  expect(scaled(4, 1)).toBe(4);";
    let finding = scaled_finding("neutral-left-operand", left)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // A call the reader cannot resolve keeps the credit.
    let opaque = "  expect(scaled(1, 1)).toBe(1);\n  expect(scaled(pick(), 2)).toBe(6);";
    let finding = scaled_finding("neutral-opaque-call", opaque)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn additive_identity_is_not_read_because_plus_concatenates_strings() -> Result<(), String> {
    let owner =
        "export function label(base: string, tip: number): string {\n  return tip + base;\n}\n";
    let test = "import { label } from \"../src/lib\";\ntest(\"label\", () => {\n  expect(label(\"x\", 0)).toBe(\"x0\");\n});\n";
    // `tip + base` against `base + tip` concatenates in another order.
    let finding = finding_for(
        "plus-string",
        owner,
        "  return tip + base;",
        "  return base + tip;",
        test,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn nested_owner_call_keeps_credit() -> Result<(), String> {
    let finding = scaled_finding("nested-call", "  expect(scaled(1, scaled(3, 4))).toBe(12);")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

const NORMALIZE: &str =
    "export function normalize(text: string): string {\n  return text.trim();\n}\n";

fn normalize_finding(label: &str, body: &str) -> Result<Finding, String> {
    let test = format!(
        "import {{ normalize }} from \"../src/lib\";\ntest(\"normalize\", () => {{\n{body}\n}});\n"
    );
    finding_for(
        label,
        NORMALIZE,
        "  return text.trim();",
        "  return text.replace(/^\\s+|\\s+$/g, \"\");",
        &test,
    )
}

#[test]
fn owner_only_relation_holds_for_a_constant_and_withdraws_credit() -> Result<(), String> {
    let idempotent = "  for (const text of [\"  a \", \"b\", \"\"]) {\n    expect(normalize(normalize(text))).toEqual(normalize(text));\n  }";
    let finding = normalize_finding("relation-idempotent", idempotent)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("compares one `normalize(...)` call with another")),
        "{:?}",
        finding.missing
    );

    // An independent expected value pins the result.
    let pinned = format!("{idempotent}\n  expect(normalize(\"  a \")).toBe(\"a\");");
    let finding = normalize_finding("relation-pinned", &pinned)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // The owner against another function is a different implementation, not
    // a relation among the owner's own outputs.
    let other = "  expect(normalize(\" a \")).toBe(reference(\" a \"));";
    let finding = normalize_finding("relation-other-function", other)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // A member of the result is read outside the relation.
    let member = format!("{idempotent}\n  expect(normalize(\" ab \").length).toBe(2);");
    let finding = normalize_finding("relation-member", &member)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn identity_operand_rule_needs_an_in_place_operand_rewrite() {
    for (old, new, want) in [
        ("return qty * unit;", "return unit * qty;", true),
        ("return qty * unit;", "return qty * unit;", true),
        ("return qty + unit;", "return unit + qty;", false),
        ("return a - b;", "return b - a;", false),
        ("return a - b;", "return a / b;", false),
        ("return a + b;", "return a * b;", false),
        ("return a * b + 5;", "return a * b;", false),
        ("return x * q;", "return y * q;", false),
        ("return a / b;", "return a * b;", true),
        ("return a * b;", "return a / b;", true),
        ("return a - b;", "return a + b;", false),
        ("return a", "return a * b", true),
        ("return a / b", "return a * b", true),
        ("return a;", "return a * b;", true),
        ("return b;", "return a * b;", false),
        ("return a;", "return b * a;", false),
    ] {
        assert_eq!(ts_identity_hides_edit(old, new), want, "{old} -> {new}");
    }
}

#[test]
fn identity_operand_rule_keeps_credit_for_calls_outside_the_assertions() -> Result<(), String> {
    // The dropped `+ 5` is pinned by the identity input.
    let owner = "export function f(a: number, b: number): number {\n  return a * b + 5;\n}\n";
    let test = "import { f } from \"../src/lib\";\ntest(\"f\", () => {\n  expect(f(10, 1)).toBe(10);\n});\n";
    let finding = finding_for(
        "neutral-dropped-term",
        owner,
        "  return a * b + 5;",
        "  return a * b;",
        test,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // An owner call in setup, asserted through a local, also pins `*`.
    let setup = "  expect(scaled(1, 2)).toBe(2);\n  const out = [3, 4].map((n) => scaled(n, 5));\n  expect(out).toEqual([15, 20]);";
    let finding = scaled_finding("neutral-setup-call", setup)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // A weak assertion that names the owner is not accounted for either.
    let weak = "  expect(scaled(1, 2)).toBe(2);\n  expect(scaled(3, 7)).toBeGreaterThan(20);";
    let finding = scaled_finding("neutral-weak-assertion", weak)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // The same holds for the owner-only relation (rule 3).
    let relation = "  expect(normalize(normalize(\" a\"))).toEqual(normalize(\" a\"));\n  expect(normalize(\" a \")).toMatch(/^a$/);";
    let finding = normalize_finding("relation-weak-pin", relation)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    let setup = "  const out = normalize(\" a \");\n  expect(out).toBe(\"a\");\n  expect(normalize(normalize(\"b\"))).toEqual(normalize(\"b\"));";
    let finding = normalize_finding("relation-setup-call", setup)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn identity_input_also_hides_an_added_factor_and_an_inverse_swap() -> Result<(), String> {
    let test = |body: &str| {
        format!(
            "import {{ scaled }} from \"../src/lib\";\ntest(\"scaled\", () => {{\n{body}\n}});\n"
        )
    };
    // `qty` becomes `qty * unit`; every tested call passes `unit == 1`.
    let owner = "export function scaled(qty: number, unit: number): number {\n  return qty;\n}\n";
    let hidden = test("  expect(scaled(5, 1)).toBe(5);");
    let finding = finding_for(
        "added-factor",
        owner,
        "  return qty;",
        "  return qty * unit;",
        &hidden,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    let pinned = test("  expect(scaled(5, 3)).toBe(15);");
    let finding = finding_for(
        "added-factor-pinned",
        owner,
        "  return qty;",
        "  return qty * unit;",
        &pinned,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // `qty / unit` becomes `qty * unit`.
    let owner =
        "export function scaled(qty: number, unit: number): number {\n  return qty / unit;\n}\n";
    let finding = finding_for(
        "inverse-swap",
        owner,
        "  return qty / unit;",
        "  return qty * unit;",
        &hidden,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

#[test]
fn owner_mentions_the_call_counter_cannot_read_keep_the_credit() -> Result<(), String> {
    let run = |label: &str, body: &str| {
        let test = format!(
            "import {{ scaled }} from \"../src/lib\";\ntest(\"scaled\", () => {{\n{body}\n}});\n"
        );
        finding_for(
            label,
            SCALED,
            "  return qty * unit;",
            "  return unit * qty;",
            &test,
        )
    };
    let property = "  expect(scaled(1, 2)).toBe(2);";
    let finding = run("mention-baseline", property)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    for (label, extra) in [
        (
            "mention-map",
            "  expect([3, 4].map(scaled)).toEqual([3, 4]);",
        ),
        (
            "mention-generic",
            "  expect(scaled<number>(3, 7)).toBe(21);",
        ),
        (
            "mention-alias",
            "  const g = scaled;\n  expect(g(3, 7)).toBe(21);",
        ),
        (
            "mention-qualified",
            "  expect(other.scaled(3, 7)).toBe(21);",
        ),
    ] {
        let finding = run(label, &format!("{property}\n{extra}"))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn owner_only_relation_reads_node_assert_and_object_returns() -> Result<(), String> {
    let node = "import assert from \"node:assert/strict\";\nimport { normalize } from \"../src/lib\";\nimport { test } from \"node:test\";\ntest(\"idempotent\", () => {\n  assert.equal(normalize(normalize(\" a \")), normalize(\" a \"));\n});\n";
    let finding = finding_for(
        "relation-node-assert",
        NORMALIZE,
        "  return text.trim();",
        "  return text.replace(/^\\s+|\\s+$/g, \"\");",
        node,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");

    let owner = "export function clean(text: string) {\n  return { text: text.trim() };\n}\n";
    let test = "import { clean } from \"../src/lib\";\ntest(\"idempotent\", () => {\n  expect(clean(clean(\" a \"))).toEqual(clean(\" a \"));\n});\n";
    let finding = finding_for(
        "relation-object",
        owner,
        "  return { text: text.trim() };",
        "  return { text: text.replace(/^\\s+|\\s+$/g, \"\") };",
        test,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    Ok(())
}

#[test]
fn one_hop_local_idempotence_is_read_and_its_controls_keep_credit() -> Result<(), String> {
    let one_hop = "  const once = normalize(\"  a \");\n  expect(normalize(once)).toEqual(once);";
    let finding = normalize_finding("onehop-idempotent", one_hop)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");

    // An independent expected value pins the result.
    let pinned = format!("{one_hop}\n  expect(normalize(\"  a \")).toBe(\"a\");");
    let finding = normalize_finding("onehop-pinned", &pinned)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // The local is read again outside the relation.
    let reused = format!("{one_hop}\n  expect(once).toBe(\"a\");");
    let finding = normalize_finding("onehop-reused", &reused)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // A `let` can be reassigned between the call and the assertion.
    let mutable = "  let once = normalize(\"  a \");\n  once = \"a\";\n  expect(normalize(once)).toEqual(once);";
    let finding = normalize_finding("onehop-let", mutable)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // The local is bound to an independent value, not an owner call.
    let literal = "  const once = \"a\";\n  expect(normalize(once)).toEqual(once);";
    let finding = normalize_finding("onehop-literal", literal)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // The local is bound to a different function.
    let other = "  const once = reference(\"  a \");\n  expect(normalize(once)).toEqual(once);";
    let finding = normalize_finding("onehop-other-function", other)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
