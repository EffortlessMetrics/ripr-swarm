//! #14968: an assertion that compares only literals (`assert!(true)`,
//! `assert!(false)`, `assert_eq!(1, 1)`) reads no runtime value, so it cannot
//! observe the changed owner a sibling statement of its test merely calls.
//! Such a test reads at most `reachable_unrevealed`. An assertion that reads
//! the owner's result, directly or through a binding, keeps its class.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

const OWNER: &str = "pub fn f(a: i32) -> i32 {\n    a + 2\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn f(a: i32) -> i32 {\n-    a + 1\n+    a + 2\n }\n";

/// The class of the candidate-current `return_value` finding on the changed
/// tail `a + 2`, for one test body.
fn return_value_class(key: &str, body: &str) -> Result<String, String> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("literal_only_assertion_credit")
        .join(key);
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"issue14968_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    let source = format!(
        "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {{\n        {body}\n    }}\n}}\n"
    );
    // The fixture is the subject the diff names: line 2 is the changed tail.
    assert_eq!(source.lines().nth(1), Some("    a + 2"), "{key}");
    std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
    std::fs::write(root.join("diff.patch"), DIFF).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings = report
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family.as_str() == "return_value"
                && finding.probe.location.line == 2
                && finding.probe.expression.trim() == "a + 2"
        })
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "{key}: {:#?}", report.findings);
    let class = findings[0].class.as_str().to_string();
    std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(class)
}

#[test]
fn a_literal_only_assertion_beside_a_bare_call_gives_no_weak_credit() -> Result<(), String> {
    for (key, body) in [
        ("assert_true_then_call", "assert!(true); let _ = f(5);"),
        ("assert_false_then_call", "assert!(false); let _ = f(5);"),
        ("call_then_assert_true", "let _ = f(5); assert!(true);"),
        ("assert_true_then_statement_call", "assert!(true); f(5);"),
        (
            "assert_eq_literals_then_call",
            "assert_eq!(2 + 2, 4); let _ = f(5);",
        ),
        (
            "assert_eq_strings_then_call",
            "assert_eq!(\"a\", \"a\"); let _ = f(5);",
        ),
        (
            "std_assert_true_then_call",
            "std::assert!(true); let _ = f(5);",
        ),
    ] {
        assert_eq!(
            return_value_class(key, body)?,
            "reachable_unrevealed",
            "{key}: a literal-only assertion observes nothing f returns"
        );
    }
    Ok(())
}

#[test]
fn assertions_that_read_the_owner_keep_their_class() -> Result<(), String> {
    for (key, body, expected) in [
        // No assertion at all: the call is reached, nothing reveals it.
        ("bare_call", "let _ = f(5);", "reachable_unrevealed"),
        // The exact pin of the new value.
        ("exact_pin", "assert_eq!(f(5), 7);", "exposed"),
        // A literal-only sibling neither adds nor removes the pin's credit.
        (
            "assert_true_beside_exact_pin",
            "assert!(true); assert_eq!(f(5), 7);",
            "exposed",
        ),
        // A read through a binding and a smoke check stay weak.
        (
            "bound_read",
            "let r = f(5); assert!(r == 7);",
            "weakly_exposed",
        ),
        (
            "assert_true_beside_bound_read",
            "let r = f(5); assert!(true); assert!(r == 7);",
            "weakly_exposed",
        ),
        ("smoke", "assert!(f(5) > 0);", "weakly_exposed"),
        // No call of the owner at all.
        ("assert_false_alone", "assert!(false);", "no_static_path"),
    ] {
        assert_eq!(return_value_class(key, body)?, expected, "{key}");
    }
    Ok(())
}
