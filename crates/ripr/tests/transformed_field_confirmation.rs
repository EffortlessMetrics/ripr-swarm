use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::Path;

#[test]
fn transformed_field_keeps_non_injective_wrapper_uncredited() -> Result<(), String> {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-verdict-corpus");
    let original =
        std::fs::read_to_string(corpus.join("subjects/authored-spec-confirm/src/lib.rs.txt"))
            .map_err(|e| e.to_string())?
            .replace("total: items * PRICE,", "total: PRICE * items,");
    let manifest =
        std::fs::read_to_string(corpus.join("subjects/authored-spec-confirm/Cargo.toml"))
            .map_err(|e| e.to_string())?;
    let observed = "let q = with_tax(quote(3));\n        assert_eq!(q.total, 33);";
    for (label, before, after, exposed) in [
        ("real-transform", "", "", false),
        (
            "independent-direct-observer",
            "",
            "\n#[test] fn exact_quote_field() { assert_eq!(quote(3), Quote { total: 30, items: 3 }); }\n",
            true,
        ),
        (
            "overflow-expected",
            "assert_eq!(q.total, 33);",
            "assert_eq!(q.total, 4294967296);",
            false,
        ),
        (
            "overflow-input",
            "with_tax(quote(3))",
            "with_tax(quote(4294967296))",
            false,
        ),
        (
            "ignored-test",
            "fn taxed_quote_adds_a_tenth()",
            "#[ignore] fn taxed_quote_adds_a_tenth()",
            false,
        ),
        (
            "wrong-field",
            "assert_eq!(q.total, 33);",
            "assert_eq!(q.items, 3);",
            false,
        ),
        ("constant-wrapper", "q.total + q.total / 10", "33", false),
        (
            "cancelled-wrapper",
            "q.total + q.total / 10",
            "q.total - q.total + 33",
            false,
        ),
        (
            "zero-division",
            "q.total + q.total / 10",
            "q.total + q.total / 0",
            false,
        ),
        (
            "shadowed-result",
            observed,
            "let q = with_tax(quote(3));\n        let q = Quote { total: 33, items: 3 };\n        assert_eq!(q.total, 33);",
            false,
        ),
        (
            "mutated-result",
            observed,
            "let mut q = with_tax(quote(3));\n        q.total = 33;\n        assert_eq!(q.total, 33);",
            false,
        ),
        (
            "self-expected",
            "assert_eq!(q.total, 33);",
            "assert_eq!(q.total, with_tax(quote(3)).total);",
            false,
        ),
        (
            "early-return",
            observed,
            "return;\n        let q = with_tax(quote(3));\n        assert_eq!(q.total, 33);",
            false,
        ),
        (
            "unrelated-assertion",
            "assert_eq!(q.total, 33);",
            "assert_eq!(33, 33);",
            false,
        ),
        (
            "split-tests",
            observed,
            "let q = with_tax(quote(3));\n        assert_eq!(1, 1);\n    }\n    #[test]\n    fn unrelated_observer() {\n        let q = Quote { total: 33, items: 3 };\n        assert_eq!(q.total, 33);",
            false,
        ),
        (
            "rebound-wrapper",
            observed,
            "let with_tax = |_: Quote| Quote { total: 33, items: 3 };\n        let q = with_tax(quote(3));\n        assert_eq!(q.total, 33);",
            false,
        ),
    ] {
        let root =
            std::env::temp_dir().join(format!("ripr-transform-{}-{label}", std::process::id()));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let cleanup = Cleanup(root);
        std::fs::create_dir_all(cleanup.0.join("src")).map_err(|e| e.to_string())?;
        let source = if before.is_empty() {
            format!("{original}{after}")
        } else {
            assert_eq!(original.matches(before).count(), 1, "{label}");
            original.replace(before, after)
        };
        std::fs::write(cleanup.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        std::fs::write(cleanup.0.join("Cargo.toml"), &manifest).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: cleanup.0.clone(),
            diff_file: Some(corpus.join("cases/cell-0005-wrapper-field-binding.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let report: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|e| e.to_string())?;
        let findings = report["findings"].as_array().ok_or("findings missing")?;
        assert_eq!(findings.len(), 1, "{label}: {report}");
        if label == "real-transform" {
            assert_eq!(findings[0]["classification"], "propagation_unknown");
            assert_eq!(
                findings[0]["static_limit_kind"],
                "rust_value_propagation_unresolved"
            );
            let summary = findings[0]["ripr"]["propagate"]["summary"]
                .as_str()
                .ok_or("propagate summary missing")?;
            assert!(
                summary.contains("with_tax")
                    && summary.contains("q.total+q.total/10")
                    && summary.contains("assert_eq!(q.total, 33)"),
                "{summary}"
            );
            let step = findings[0]["recommended_next_step"]
                .as_str()
                .ok_or("next step missing")?;
            assert!(
                step.contains("actual overflow configuration")
                    && step.contains("no assertion repair is prescribed"),
                "{step}"
            );
            assert!(
                !step.starts_with("Add ") && !step.starts_with("Strengthen "),
                "{step}"
            );
        } else if label != "independent-direct-observer" {
            assert!(
                !findings[0]["ripr"]["propagate"]["summary"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("Transformed-field arithmetic unproved:"),
                "{label}: {}",
                findings[0]
            );
        }

        assert_eq!(
            findings[0]["classification"] == "exposed",
            exposed,
            "{label}: {}",
            findings[0]
        );
    }
    Ok(())
}

// This wrong field is a preimage of the independently pinned 33 when addition
// wraps. Valid test inputs and expected literals do not bound mutant output.
#[test]
fn transformed_field_wrapping_preimage_defeats_profile_independent_credit() {
    let correct = 30u32;
    let wrong = 3904515754u32;
    assert_ne!(correct, wrong);
    assert_eq!(correct.wrapping_add(correct / 10), 33);
    assert_eq!(wrong.wrapping_add(wrong / 10), 33);
    assert_eq!(correct.checked_add(correct / 10), Some(33));
    assert_eq!(wrong.checked_add(wrong / 10), None);
}
