//! Public repair-packet controls for the nested TypeScript owner follow-up (#16458).

use super::{run_command, workspace_root};
use serde_json::Value;
use std::fs;

#[test]
fn nested_owner_packet_refuses_unverified_calls() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = workspace_root().join("fixtures/typescript_vitest_wrapped_owner_call_boundary");
    let source = fs::read_to_string(fixture.join("input/src/price.ts"))?;
    let tests = fs::read_to_string(fixture.join("input/test/price.test.ts"))?;
    let diff = fs::read_to_string(fixture.join("diff.patch"))?;
    let original = "discount(90, discount(100))";
    assert_eq!(tests.matches(original).count(), 1);
    for (observed, signature, helper, ready_shape) in [
        (original, "discount(total: number)", "", None),
        ("price(90, price(100))", "discount(total: number)", "", None),
        (
            "Math.max(discount<number>(90, discount(100)), 90)",
            "discount<T = number>(total: number)",
            "",
            None,
        ),
        (
            "Math.max(price<number>(90, price(100)), 90)",
            "discount<T = number>(total: number)",
            "",
            None,
        ),
        (
            "discount(90, { deferred: () => discount(100) })",
            "discount(total: number, options?: { deferred: () => number })",
            "",
            Some("expect(discount(100, { deferred: () => discount(100) })).toBe(expected)"),
        ),
        (
            "discount(90, { immediate: (() => discount(100))() })",
            "discount(total: number, options?: { immediate: number })",
            "",
            None,
        ),
        (
            "price(90)",
            "discount(total: number)",
            "",
            Some("expect(price(90)).toBe(expected)"),
        ),
        (
            "Math.max(discount(90), 90)",
            "discount(total: number)",
            "",
            Some("expect(discount(100)).toBe(expected)"),
        ),
        (
            "discount(90, discount /* note */ (100))",
            "discount(total: number)",
            "",
            None,
        ),
        (
            "discount(90, discount<number>(100))",
            "discount<T = number>(total: number)",
            "",
            None,
        ),
        (
            "discount(90, discount<number, number>(100))",
            "discount<T = number, U = number>(total: number)",
            "",
            None,
        ),
        (
            "discount<number>(90, discount(100))",
            "discount<T = number>(total: number)",
            "",
            None,
        ),
        (
            "discount(90, () => discount(100))",
            "discount(total: number, deferred?: () => number)",
            "",
            Some("expect(discount(100, () => discount(100))).toBe(expected)"),
        ),
        (
            "discount(90, 10)",
            "discount(total: number, tier?: number)",
            "",
            Some("expect(discount(100, 10)).toBe(expected)"),
        ),
        (
            "discount(90, getTier())",
            "discount(total: number, tier?: number)",
            "const getTier = () => 10;\n",
            Some("expect(discount(100, getTier())).toBe(expected)"),
        ),
        (
            "discount(90, other.discount(100))",
            "discount(total: number, tier?: number)",
            "const other = { discount: (total: number) => total };\n",
            Some("expect(discount(100, other.discount(100))).toBe(expected)"),
        ),
    ] {
        let root = tempfile::tempdir()?;
        for file in ["package.json", "ripr.toml"] {
            fs::copy(fixture.join("input").join(file), root.path().join(file))?;
        }
        fs::create_dir(root.path().join("src"))?;
        fs::create_dir(root.path().join("test"))?;
        fs::write(
            root.path().join("src/price.ts"),
            source.replace("discount(total: number)", signature),
        )?;
        let test_source = tests
            .replace(original, observed)
            .replace("describe(", &format!("{helper}describe("));
        let test_source = if observed.contains("price(") {
            test_source
                .replace("import { discount }", "import { discount as price }")
                .replace("Math.max(discount(100), 100)", "Math.max(price(100), 100)")
        } else {
            test_source
        };
        fs::write(root.path().join("test/price.test.ts"), test_source)?;
        fs::write(
            root.path().join("diff.patch"),
            diff.replace("discount(total: number)", signature),
        )?;
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(root.path()),
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "diff.patch",
                "--mode",
                "fast",
                "--json",
            ],
        )?;
        assert!(
            output.status.success(),
            "{observed}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{observed}: {report}");
        let finding = &findings[0];
        assert_eq!(finding["classification"], "weakly_exposed", "{observed}");
        let evidence = finding["evidence"].as_array().ok_or("missing evidence")?;
        let expected_evidence = format!("typescript_oracle_observed: {observed}");
        assert!(
            evidence
                .iter()
                .any(|item| item.as_str() == Some(&expected_evidence)),
            "{observed}: {finding}"
        );
        let action = &finding["preview_actionability"];
        let expected_state = if ready_shape.is_some() {
            "actionable"
        } else {
            "unknown"
        };
        assert_eq!(action["gap_state"], expected_state, "{observed}: {action}");
        assert_eq!(
            action["repair_packet_ready"],
            ready_shape.is_some(),
            "{observed}: {action}"
        );
        let packet = &finding["typescript_repair_packet"];
        if let Some(shape) = ready_shape {
            assert_eq!(packet["assertion_shape"], shape, "{observed}: {finding}");
        } else {
            assert!(packet.is_null(), "{observed}: {packet}");
            if observed == original {
                for format in [&["--legend"][..], &["--format", "human-full"][..]] {
                    let scope = [
                        "check",
                        "--root",
                        ".",
                        "--diff",
                        "diff.patch",
                        "--mode",
                        "fast",
                    ];
                    let output = run_command(
                        env!("CARGO_BIN_EXE_ripr"),
                        Some(root.path()),
                        &[&scope[..], format].concat(),
                    )?;
                    assert!(
                        output.status.success(),
                        "{observed}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let stdout = String::from_utf8(output.stdout)?;
                    assert!(stdout.contains("do not replay the nested call"), "{stdout}");
                    assert!(
                        !stdout.contains("expect(discount(100, discount(100))).toBe(expected)"),
                        "{stdout}"
                    );
                }
            }
            assert!(
                action["why_not_actionable"].as_str().is_some_and(|reason| {
                    reason.contains("do not replay the nested call")
                        && reason.contains("total == 100")
                }),
                "{observed}: {action}"
            );
        }
    }
    Ok(())
}
