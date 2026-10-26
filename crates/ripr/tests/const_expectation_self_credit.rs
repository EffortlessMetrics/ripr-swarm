//! A test that compares the owner's result with an expectation computed by the
//! owner itself cannot fail when the owner changes, so it earns no `exposed`
//! credit (#7260). A constant, an alias chain, a `const fn` and an associated
//! constant all hide the owner call behind a name; a literal constant is an
//! independent expectation and keeps its credit.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub const fn quota(n: u32) -> u32 {\n-    n + 1\n+    n + 2\n }\n";

fn return_value_class(name: &str, declarations: &str, assertion: &str) -> Result<String, String> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("const_expectation_self_credit")
        .join(name);
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"const_expectation_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    let source = format!(
        "pub const fn quota(n: u32) -> u32 {{\n    n + 2\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{declarations}\n    #[test]\n    fn quota_is_pinned() {{\n        {assertion}\n    }}\n}}\n"
    );
    std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    std::fs::write(root.join("diff.patch"), DIFF).map_err(|e| e.to_string())?;
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
    Ok(finding.class.as_str().to_string())
}

#[test]
fn owner_computed_expectations_behind_a_name_never_earn_exposed() -> Result<(), String> {
    let cases = [
        (
            "const",
            "    const EXPECTED: u32 = quota(2);",
            "assert_eq!(quota(2), EXPECTED);",
        ),
        (
            "const_reversed",
            "    const EXPECTED: u32 = quota(2);",
            "assert_eq!(EXPECTED, quota(2));",
        ),
        (
            "alias_chain",
            "    const A: u32 = quota(2);\n    const B: u32 = A;",
            "assert_eq!(quota(2), B);",
        ),
        (
            "const_fn",
            "    const fn expected() -> u32 { quota(2) }",
            "assert_eq!(quota(2), expected());",
        ),
        (
            "associated_const",
            "    struct K;\n    impl K { const EXPECTED: u32 = quota(2); }",
            "assert_eq!(quota(2), K::EXPECTED);",
        ),
    ];
    for (name, declarations, assertion) in cases {
        let class = return_value_class(name, declarations, assertion)?;
        assert_ne!(
            class, "exposed",
            "{name}: self-computed expectation credited"
        );
    }
    Ok(())
}

#[test]
fn an_independent_literal_constant_keeps_its_credit() -> Result<(), String> {
    let class = return_value_class(
        "literal_const",
        "    const EXPECTED: u32 = 4;",
        "assert_eq!(quota(2), EXPECTED);",
    )?;
    assert_eq!(class, "exposed");
    Ok(())
}
