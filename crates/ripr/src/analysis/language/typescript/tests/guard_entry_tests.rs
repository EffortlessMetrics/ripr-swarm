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
