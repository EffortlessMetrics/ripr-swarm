//! #14968: an assertion that compares only literals (`assert!(true)`,
//! `assert!(false)`, `assert_eq!(1, 1)`) reads no runtime value, so it cannot
//! observe the changed owner a sibling statement of its test merely calls.
//! Such a test reads at most `reachable_unrevealed`. An assertion that reads
//! the owner's result, directly or through a binding, keeps its class.
//!
//! Only an unconditional (top-level) literal-only assertion is dropped, and it
//! is dropped as if absent: adding one never lowers a test's class below the
//! same test without it. A literal inside an `if`, `match` arm or other guard
//! runs only on a path the test's own read selects, so it keeps its reading.
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

const RESULT_OWNER: &str = "pub fn g(a: i32) -> Result<i32, String> {\n    if a > 2 { Ok(a) } else { Err(\"small\".into()) }\n}\n";
const RESULT_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn g(a: i32) -> Result<i32, String> {\n-    if a > 1 { Ok(a) } else { Err(\"small\".into()) }\n+    if a > 2 { Ok(a) } else { Err(\"small\".into()) }\n }\n";

/// The classes of the candidate-current `error_path` and `predicate` findings
/// on the changed guard `a > 2`, for one test function (signature tail and
/// body, braces included).
fn result_owner_classes(key: &str, function: &str) -> Result<(String, String), String> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("literal_only_assertion_credit")
        .join(key);
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"issue14968_result_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    let source = format!(
        "{RESULT_OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {function}\n}}\n"
    );
    // The fixture is the subject the diff names: line 2 is the changed guard.
    assert_eq!(
        source.lines().nth(1),
        Some("    if a > 2 { Ok(a) } else { Err(\"small\".into()) }"),
        "{key}"
    );
    std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
    std::fs::write(root.join("diff.patch"), RESULT_DIFF).map_err(|e| e.to_string())?;
    let report = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let class_of = |family: &str| -> Result<String, String> {
        let findings = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family.as_str() == family && finding.probe.location.line == 2
            })
            .collect::<Vec<_>>();
        match findings.as_slice() {
            [finding] => Ok(finding.class.as_str().to_string()),
            _ => Err(format!("{key}: {family}: {:#?}", report.findings)),
        }
    };
    let classes = (class_of("error_path")?, class_of("predicate")?);
    std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(classes)
}

const WEAK: (&str, &str) = ("weakly_exposed", "weakly_exposed");
const UNREVEALED: (&str, &str) = ("reachable_unrevealed", "reachable_unrevealed");

fn assert_result_classes(rows: &[(&str, &str, (&str, &str))]) -> Result<(), String> {
    for (key, function, (error_path, predicate)) in rows {
        let (actual_error_path, actual_predicate) = result_owner_classes(key, function)?;
        assert_eq!(
            (actual_error_path.as_str(), actual_predicate.as_str()),
            (*error_path, *predicate),
            "{key}: (error_path, predicate)"
        );
    }
    Ok(())
}

#[test]
fn a_guarded_literal_assertion_keeps_its_guards_reading() -> Result<(), String> {
    assert_result_classes(&[
        (
            "if_let_ok_assert_false",
            "{ if let Ok(_) = g(2) { assert!(false, \"2 must be rejected\"); } }",
            WEAK,
        ),
        (
            "match_err_arm_assert_false",
            "{ match g(3) { Ok(_) => {} Err(_) => assert!(false), } }",
            WEAK,
        ),
        (
            "is_err_guard_assert_false",
            "{ let r = g(3); if r.is_err() { assert!(false) } }",
            WEAK,
        ),
        (
            "if_let_err_assert_false_multiline",
            "{\n        if let Err(e) = g(3) {\n            assert!(false, \"expected ok: {e}\");\n        }\n    }",
            WEAK,
        ),
        // Control: the same read as a direct assertion.
        ("assert_is_err", "{ assert!(g(2).is_err()); }", WEAK),
    ])
}

#[test]
fn an_unconditional_literal_assertion_never_lowers_the_implicit_reading() -> Result<(), String> {
    assert_result_classes(&[
        // Controls: the implicit observer alone.
        ("unwrap_err_alone", "{ g(2).unwrap_err(); }", WEAK),
        (
            "let_unwrap_err_alone",
            "{ let _e = g(2).unwrap_err(); }",
            WEAK,
        ),
        ("unwrap_alone", "{ g(3).unwrap(); }", WEAK),
        (
            "question_mark_alone",
            "-> Result<(), String> { g(3)?; Ok(()) }",
            UNREVEALED,
        ),
        ("bare_call_alone", "{ let _ = g(3); }", UNREVEALED),
        // The same tests with a literal-only assertion added read the same.
        (
            "unwrap_err_assert_true",
            "{ g(2).unwrap_err(); assert!(true); }",
            WEAK,
        ),
        (
            "let_unwrap_err_assert_eq_literals",
            "{ let _e = g(2).unwrap_err(); assert_eq!(1, 1); }",
            WEAK,
        ),
        (
            "unwrap_assert_true",
            "{ g(3).unwrap(); assert!(true); }",
            WEAK,
        ),
        (
            "unwrap_assert_true_tail",
            "{ g(3).unwrap(); assert!(true) }",
            WEAK,
        ),
        (
            "question_mark_assert_true",
            "-> Result<(), String> { g(3)?; assert!(true); Ok(()) }",
            UNREVEALED,
        ),
        (
            "assert_false_then_bare_call",
            "{ assert!(false); let _ = g(3); }",
            UNREVEALED,
        ),
        (
            "assert_true_then_bare_call",
            "{ assert!(true); let _ = g(3); }",
            UNREVEALED,
        ),
    ])
}
