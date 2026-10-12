//! RIPR-SPEC-0248 rule 1: a change that only adds or drops one additive term
//! keeps `exposed` only when a tested input makes the term non-zero.

use super::*;

#[test]
fn dropped_additive_term_reads_both_directions_and_forms() {
    for (old, new, term) in [
        (
            "return { tier, totalCents: total + shippingFee(subtotalCents) };",
            "return { tier, totalCents: total };",
            Some("shippingFee(subtotalCents)"),
        ),
        ("return total;", "return total + fee;", Some("fee")),
        ("return total - fee;", "return total;", Some("fee")),
        ("return fee + total;", "return total;", Some("fee")),
        ("const t = a + b * c;", "const t = a;", Some("b * c")),
        ("return a + b + c;", "return a + c;", Some("b")),
        ("return f(a + rate);", "return f(a);", Some("rate")),
        ("return a * b + fee;", "return a * b;", Some("fee")),
        ("return a + b - c;", "return a;", Some("b - c")),
        ("return total + 0x10;", "return total;", None),
        ("return total + 0;", "return total;", None),
        ("return total + '';", "return total;", None),
        ("return a + b * c;", "return a * c;", None),
        ("return a * b + c;", "return a * c;", None),
        ("return x - a + b;", "return x - b;", None),
        ("return a - b + c;", "return a;", None),
        ("return +x;", "return;", None),
        ("total += fee;", "total;", None),
        ("return total + fee;", "return total + tax;", None),
        ("return !a + b;", "return !b;", None),
        ("return f(-fee);", "return f();", None),
        ("return a + (b;", "return a;", None),
        ("return a + b ? c : d;", "return a;", None),
    ] {
        assert_eq!(
            ts_dropped_additive_term(old, new).as_deref(),
            term,
            "{old} -> {new}"
        );
    }
}

#[test]
fn parameter_is_stable_only_when_the_body_never_rebinds_it() {
    let stable = "function quote(subtotal: number, points: number) {\n  const tier = rank(points);\n  return subtotal + fee(subtotal, tier);\n}";
    assert!(ts_parameter_is_stable(stable, "subtotal"));
    assert!(ts_parameter_is_stable(stable, "points"));
    for body in [
        "  subtotal = subtotal * 2;\n  return subtotal + fee;",
        "  subtotal += 1;\n  return subtotal + fee;",
        "  subtotal++;\n  return subtotal + fee;",
        "  --subtotal;\n  return subtotal + fee;",
        "  subtotal ??= 0;\n  return subtotal + fee;",
        "  let subtotal = 3;\n  return subtotal + fee;",
        "  const { subtotal } = opts;\n  return subtotal + fee;",
        "  return [1].map((subtotal) => subtotal + fee)[0];",
        "  function inner(subtotal: number) { return subtotal; }\n  return inner(2) + fee;",
    ] {
        let source = format!("function quote(subtotal: number, fee: number) {{\n{body}\n}}");
        assert!(!ts_parameter_is_stable(&source, "subtotal"), "{body}");
    }
    let compared = "function quote(subtotal: number, fee: number) {\n  if (subtotal === 0 || subtotal >= 10 || subtotal != fee) { return 0; }\n  return subtotal + fee;\n}";
    assert!(ts_parameter_is_stable(compared, "subtotal"));
}

const PRICE: &str = "export const FLAT = 499;\n\nexport function shipping(subtotal: number): number {\n  return subtotal >= 5000 ? 0 : FLAT;\n}\n\nexport function price(subtotal: number, fee: number): number {\n  return subtotal + fee + shipping(subtotal) + FLAT;\n}\n";

fn price_finding(label: &str, test: &str, added: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let source = PRICE.replace(
        "  return subtotal + fee + shipping(subtotal) + FLAT;",
        added,
    );
    ts_write_file(&root.join("src/price.ts"), &source)?;
    ts_write_file(&root.join("tests/price.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/price.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 8,
            text: added.to_string(),
            new_side_line: 8,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 8,
            text: "  return subtotal + fee + shipping(subtotal) + FLAT;".to_string(),
            new_side_line: 8,
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
        .find(|finding| finding.probe.location.line == 8)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn price_test(call: &str, expected: &str) -> String {
    format!(
        "import {{ price }} from \"../src/price\";\ntest(\"price\", () => {{\n  expect({call}).toBe({expected});\n}});\n"
    )
}

#[test]
fn dropped_term_keeps_credit_only_with_a_non_zero_tested_input() -> Result<(), String> {
    let test = price_test("price(6000, 5)", "6504");
    assert_eq!(
        extract_tests(Path::new("tests/price.test.ts"), &test)
            .iter()
            .flat_map(|test| test.assertions.iter())
            .map(|assertion| assertion.observed_expression.clone())
            .collect::<Vec<_>>(),
        vec![Some("price(6000, 5)".to_string())]
    );
    // `fee` is 5 for the tested call: dropping it is noticed.
    let finding = price_finding(
        "dropped-term-nonzero",
        &test,
        "  return subtotal + shipping(subtotal) + FLAT;",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // A module constant folds too.
    let finding = price_finding(
        "dropped-term-constant",
        &test,
        "  return subtotal + fee + shipping(subtotal);",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);

    // `fee` is 0 for the only tested call: both versions return 6499.
    let finding = price_finding(
        "dropped-term-zero",
        &price_test("price(6000, 0)", "6499"),
        "  return subtotal + shipping(subtotal) + FLAT;",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("`fee`, which is 0 for every tested input")),
        "{:?}",
        finding.missing
    );

    // A call term does not fold: `shipping(6000)` is 0, and ripr cannot
    // show otherwise.
    let finding = price_finding(
        "dropped-term-call",
        &test,
        "  return subtotal + fee + FLAT;",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("`shipping(subtotal)`, which is not shown to be non-zero")),
        "{:?}",
        finding.missing
    );

    // One non-zero call among several is enough.
    let two = "import { price } from \"../src/price\";\ntest(\"price\", () => {\n  expect(price(6000, 0)).toBe(6499);\n  expect(price(6000, 7)).toBe(6506);\n});\n";
    let finding = price_finding(
        "dropped-term-one-of-two",
        two,
        "  return subtotal + shipping(subtotal) + FLAT;",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn dropped_term_does_not_bind_a_rebound_parameter() -> Result<(), String> {
    let root_source = "export const FLAT = 499;\n\nexport function shipping(subtotal: number): number {\n  return subtotal >= 5000 ? 0 : FLAT;\n}\n\nexport function price(subtotal: number, fee: number): number {\n  fee = fee - 5; return subtotal + fee + shipping(subtotal) + FLAT;\n}\n";
    let root = ts_unique_tempdir("dropped-term-rebound")?;
    ts_write_file(
        &root.join("src/price.ts"),
        &root_source.replace(
            "  fee = fee - 5; return subtotal + fee + shipping(subtotal) + FLAT;",
            "  fee = fee - 5; return subtotal + shipping(subtotal) + FLAT;",
        ),
    )?;
    ts_write_file(
        &root.join("tests/price.test.ts"),
        &price_test("price(6000, 5)", "6499"),
    )?;
    let changed = ChangedFile {
        path: PathBuf::from("src/price.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 8,
            text: "  fee = fee - 5; return subtotal + shipping(subtotal) + FLAT;".to_string(),
            new_side_line: 8,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 8,
            text: "  fee = fee - 5; return subtotal + fee + shipping(subtotal) + FLAT;".to_string(),
            new_side_line: 8,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == 8)
        .ok_or("expected a finding on the changed line")?;
    // `fee` is 5 at the call but 0 on the changed line, so binding it would
    // credit a change the test cannot see.
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}
