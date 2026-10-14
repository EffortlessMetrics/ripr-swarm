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
        "  subtotal=-subtotal;\n  return subtotal + fee;",
        "  if (fee) subtotal=+0;\n  return subtotal + fee;",
        "  subtotal >>>= 1;\n  return subtotal + fee;",
        "  for (subtotal of rows) {}\n  return subtotal + fee;",
        "  for (subtotal in table) {}\n  return subtotal + fee;",
        "  [subtotal] = pair;\n  return subtotal + fee;",
        "  ({ subtotal } = opts);\n  return subtotal + fee;",
        "  ({ cents: subtotal, fee: other } = opts);\n  return subtotal + fee;",
        "  [a, [subtotal]] = pair;\n  return subtotal + fee;",
        "  ({ a: { subtotal } } = opts);\n  return subtotal + fee;",
        "  (subtotal) = 0;\n  return subtotal + fee;",
        "  (subtotal as number) = 0;\n  return subtotal + fee;",
        "  subtotal! = 0;\n  return subtotal + fee;",
    ] {
        let source = format!("function quote(subtotal: number, fee: number) {{\n{body}\n}}");
        assert!(!ts_parameter_is_stable(&source, "subtotal"), "{body}");
    }
    let compared = "function quote(subtotal: number, fee: number) {\n  if (subtotal === 0 || subtotal >= 10 || subtotal != fee) { return 0; }\n  return subtotal + fee;\n}";
    assert!(ts_parameter_is_stable(compared, "subtotal"));
    // Reads, comparisons, members and object keys do not rebind it.
    let read = "function quote(subtotal: number, fee: number) {\n  log(subtotal, fee);\n  if (subtotal>=fee && subtotal!==0) { audit.subtotal = 1; }\n  const row = { subtotal: fee };\n  return [subtotal][0] + row.subtotal + fee;\n}";
    assert!(ts_parameter_is_stable(read, "subtotal"));
}

const PRICE: &str = "export const FLAT = 499;\n\nexport function shipping(subtotal: number): number {\n  return subtotal >= 5000 ? 0 : FLAT;\n}\n\nexport function price(subtotal: number, fee: number): number {\n  return subtotal + fee + shipping(subtotal) + FLAT;\n}\n";

fn price_finding(label: &str, test: &str, added: &str) -> Result<Finding, String> {
    price_finding_in(PRICE, label, test, added)
}

fn price_finding_in(
    price_source: &str,
    label: &str,
    test: &str,
    added: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let source = price_source.replace(
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
    // The discriminate summary agrees with the withdrawn credit.
    let summary = &finding.ripr.reveal.discriminate.summary;
    assert!(
        summary.contains("withdrew the discrimination credit") && summary.contains("`fee`"),
        "{summary}"
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

    // The whole result of a renamed import, an outer call
    // around an inner one, and a fractional `fee` each witness the term.
    for (label, import, call) in [
        ("dropped-term-alias", "price as subject", "subject(6000, 7)"),
        ("dropped-term-outer", "price", "price(price(6000, 0), 7)"),
        ("dropped-term-half", "price", "price(6000, 0.5)"),
    ] {
        let test = format!(
            "import {{ {import} }} from \"../src/price\";\ntest(\"price\", () => {{\n  expect({call}).toBe(1);\n}});\n"
        );
        let finding = price_finding(
            label,
            &test,
            "  return subtotal + shipping(subtotal) + FLAT;",
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }

    // A local bound to a renamed import's call witnesses the term too.
    let local_alias = "import { price as subject } from \"../src/price\";\ntest(\"price\", () => {\n  const result = subject(6000, 7);\n  expect(result).toBe(1);\n});\n";
    let finding = price_finding(
        "dropped-term-alias-local",
        local_alias,
        "  return subtotal + shipping(subtotal) + FLAT;",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );

    // A double sum absorbs a term below 2^-52 of its magnitude, so a
    // `fee` under 1e-6 does not witness it.
    let finding = price_finding(
        "dropped-term-absorbed",
        &price_test("price(6000, 0.0000000000001)", "6499"),
        "  return subtotal + shipping(subtotal) + FLAT;",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // A non-zero call the assertion sees only through another expression
    // does not count: `Math.sign` hides the dropped `fee`.
    // Nor does an error matcher, which never sees the returned value, or
    // an inner call whose `fee` is not the asserted result's.
    for (label, other, kind) in [
        (
            "dropped-term-hidden",
            "  expect(Math.sign(price(6000, 7))).toBe(1);",
            OracleKind::ExactValue,
        ),
        (
            "dropped-term-rejects",
            "  await expect(price(0, 7)).rejects.toThrow(\"bad\");",
            OracleKind::ExactErrorVariant,
        ),
        (
            "dropped-term-nested",
            "  expect(price(price(6000, 7), 0)).toBe(13006);",
            OracleKind::ExactValue,
        ),
    ] {
        let test = format!(
            "import {{ price }} from \"../src/price\";\ntest(\"price\", async () => {{\n  expect(price(6000, 0)).toBe(6499);\n{other}\n}});\n"
        );
        let tests = extract_tests(Path::new("tests/price.test.ts"), &test);
        let grades: Vec<(OracleKind, OracleStrength)> = tests
            .iter()
            .flat_map(|test| test.assertions.iter())
            .map(|assertion| {
                (
                    assertion.oracle_kind.clone(),
                    assertion.oracle_strength.clone(),
                )
            })
            .collect();
        assert_eq!(
            grades,
            vec![
                (OracleKind::ExactValue, OracleStrength::Strong),
                (kind, OracleStrength::Strong)
            ],
            "{label}"
        );
        let finding = price_finding(
            label,
            &test,
            "  return subtotal + shipping(subtotal) + FLAT;",
        )?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("`fee`, which is 0 for every tested input")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn dropped_term_does_not_bind_a_rebound_parameter() -> Result<(), String> {
    for (label, statement, rebound) in [
        ("dropped-term-rebound", "  fee=fee-5;", true),
        ("dropped-term-read", "  audit(fee);", false),
    ] {
        let source = format!(
            "export const FLAT = 499;\n\nexport function shipping(subtotal: number): number {{\n  return subtotal >= 5000 ? 0 : FLAT;\n}}\n\nexport function price(subtotal: number, fee: number): number {{\n{statement}\n  return subtotal + shipping(subtotal) + FLAT;\n}}\n"
        );
        let root = ts_unique_tempdir(label)?;
        ts_write_file(&root.join("src/price.ts"), &source)?;
        ts_write_file(
            &root.join("tests/price.test.ts"),
            &price_test("price(6000, 5)", "6499"),
        )?;
        let changed = ChangedFile {
            path: PathBuf::from("src/price.ts"),
            added_lines: vec![crate::analysis::diff::ChangedLine {
                line: 9,
                text: "  return subtotal + shipping(subtotal) + FLAT;".to_string(),
                new_side_line: 9,
            }],
            removed_lines: vec![crate::analysis::diff::ChangedLine {
                line: 9,
                text: "  return subtotal + fee + shipping(subtotal) + FLAT;".to_string(),
                new_side_line: 9,
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
            .find(|finding| finding.probe.location.line == 9)
            .ok_or_else(|| format!("{label}: expected a finding on the changed line"))?;
        if rebound {
            // `fee` is 5 at the call but 0 on the changed line, so binding
            // it would credit a change the test cannot see.
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
            assert!(
                finding
                    .missing
                    .iter()
                    .any(|line| line.contains("`fee`, which is not shown to be non-zero")),
                "{:?}",
                finding.missing
            );
        } else {
            assert_eq!(finding.class, ExposureClass::Exposed, "{label}");
        }
    }
    Ok(())
}

/// Rule 1 reads value seams only: an error message the test pins with
/// `toThrow` keeps its error-path reading. `toThrow("limit ")` is a
/// substring match that both `"limit " + max` and `"limit "` satisfy, so
/// RIPR-SPEC-0243 rule 4 leaves it weak (corpus case
/// `jest-spec0248-throw-message-term`).
#[test]
fn dropped_term_leaves_an_error_seam_alone() -> Result<(), String> {
    let root = ts_unique_tempdir("dropped-term-error-seam")?;
    ts_write_file(
        &root.join("src/limit.ts"),
        "export function check(max: number): void {\n  throw new RangeError(\"limit \");\n}\n",
    )?;
    let test = "import { check } from \"../src/limit\";\ntest(\"limit\", () => {\n  expect(() => check(0)).toThrow(\"limit \");\n});\n";
    ts_write_file(&root.join("tests/limit.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/limit.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: "  throw new RangeError(\"limit \");".to_string(),
            new_side_line: 2,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            text: "  throw new RangeError(\"limit \" + max);".to_string(),
            new_side_line: 2,
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
        .find(|finding| finding.probe.location.line == 2)
        .ok_or("expected a finding on the changed line")?;
    assert_eq!(finding.probe.family, ProbeFamily::ErrorPath);
    assert!(
        !finding
            .missing
            .iter()
            .any(|line| line.contains("only adds or drops the additive term")),
        "{:?}",
        finding.missing
    );
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// RIPR-SPEC-0251 rule 1 reaches the RIPR-SPEC-0248 dropped-term witness:
/// `import cost from "../src/price"` calls the default-exported `price` as
/// `cost`, directly or through a local bound once to the call. A non-zero
/// `fee` keeps the credit; a zero `fee` withdraws it.
#[test]
fn dropped_term_reads_a_renamed_default_import() -> Result<(), String> {
    let default_price = PRICE.replace("export function price", "export default function price");
    let dropped = "  return subtotal + shipping(subtotal) + FLAT;";
    for (label, lines, observed, class) in [
        (
            "dropped-term-default-nonzero",
            "  expect(cost(6000, 7)).toBe(6506);",
            "cost(6000, 7)",
            ExposureClass::Exposed,
        ),
        (
            "dropped-term-default-zero",
            "  expect(cost(6000, 0)).toBe(6499);",
            "cost(6000, 0)",
            ExposureClass::WeaklyExposed,
        ),
        (
            "dropped-term-default-local-nonzero",
            "  const result = cost(6000, 7);\n  expect(result).toBe(6506);",
            "result",
            ExposureClass::Exposed,
        ),
        (
            "dropped-term-default-local-zero",
            "  const result = cost(6000, 0);\n  expect(result).toBe(6499);",
            "result",
            ExposureClass::WeaklyExposed,
        ),
    ] {
        let test = format!(
            "import cost from \"../src/price\";\ntest(\"price\", () => {{\n{lines}\n}});\n"
        );
        assert_eq!(
            extract_tests(Path::new("tests/price.test.ts"), &test)
                .iter()
                .flat_map(|test| test.assertions.iter())
                .map(|assertion| assertion.observed_expression.clone())
                .collect::<Vec<_>>(),
            vec![Some(observed.to_string())],
            "{label}"
        );
        let finding = price_finding_in(&default_price, label, &test, dropped)?;
        assert_eq!(finding.class, class, "{label}: {:?}", finding.missing);
        if class == ExposureClass::WeaklyExposed {
            assert!(
                finding
                    .missing
                    .iter()
                    .any(|line| line.contains("`fee`, which is 0 for every tested input")),
                "{label}: {:?}",
                finding.missing
            );
        }
    }
    Ok(())
}
