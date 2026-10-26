//! An owner pin must not credit an expected value that a test-file helper
//! computes from the owner (#7714): the helper moves with a wrong owner, so
//! the equality still holds. A helper that never reaches the owner keeps the
//! credit, as does a literal.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

#[test]
fn helper_computed_expected_values_do_not_credit_the_owner_pin() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("helper_computed_expected");
    let production = "pub fn compute_score(base: i32) -> i32 {\n    base * 2\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn compute_score(base: i32) -> i32 {\n-    base + 1\n+    base * 2\n }\n";
    // (name, helper items in the test module, test body, credited)
    let cases = [
        (
            "literal_baseline",
            "",
            "assert_eq!(compute_score(4), 8);",
            true,
        ),
        (
            "unrelated_helper_keeps_credit",
            "fn eight() -> i32 { 4 + 4 }",
            "let e = eight();\nassert_eq!(compute_score(4), e);",
            true,
        ),
        (
            "helper_let_binding",
            "fn expected() -> i32 { compute_score(4) }",
            "let e = expected();\nassert_eq!(compute_score(4), e);",
            false,
        ),
        (
            "helper_direct_operand",
            "fn expected() -> i32 { compute_score(4) }",
            "assert_eq!(compute_score(4), expected());",
            false,
        ),
        (
            "helper_chain_through_second_helper",
            "fn inner() -> i32 { compute_score(4) }\nfn expected() -> i32 { inner() + 0 }",
            "let e = expected();\nassert_eq!(compute_score(4), e);",
            false,
        ),
        (
            "helper_through_let_chain",
            "fn expected() -> i32 { compute_score(4) }",
            "let a = expected();\nlet b = a;\nassert_eq!(compute_score(4), b);",
            false,
        ),
        (
            "helper_computed_constant",
            "fn normalize(v: i32) -> i32 { v + compute_score(0) }\nconst EXPECTED: i32 = normalize(8);",
            "assert_eq!(compute_score(4), EXPECTED);",
            false,
        ),
        (
            "unrelated_helper_constant_keeps_credit",
            "fn normalize(v: i32) -> i32 { v }\nconst EXPECTED: i32 = normalize(8);",
            "assert_eq!(compute_score(4), EXPECTED);",
            true,
        ),
    ];
    for (name, items, body, credited) in cases {
        let source = format!(
            "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n{items}\n#[test]\nfn compute_score_observed() {{\n{body}\n}}\n}}\n"
        );
        let root = root_dir.join(name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"helper_expected_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let finding = report
            .findings
            .iter()
            .find(|f| f.probe.family.as_str() == "return_value")
            .ok_or_else(|| format!("{name}: no return_value finding"))?;
        assert_eq!(
            finding.class.as_str() == "exposed",
            credited,
            "{name}: {}",
            finding.class.as_str()
        );
    }
    Ok(())
}
