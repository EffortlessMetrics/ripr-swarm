//! #14968: an assertion that compares only literals (`assert!(true)`,
//! `assert!(false)`, `assert_eq!(1, 1)`) reads no runtime value, so it cannot
//! observe the changed owner a sibling statement of its test merely calls.
//! Such a test reads at most `reachable_unrevealed`. An assertion that reads
//! the owner's result, directly or through a binding, keeps its class.
//!
//! Only an unconditional (top-level) literal-only assertion is dropped. It
//! still counts toward the single-assertion fallback, so it never hands that
//! fallback to an unrelated sibling, while an `unwrap()` / `unwrap_err()` of
//! the owner's own call keeps the reading it has without the literal. A
//! literal inside an `if`, `match` arm or other guard runs only on a path the
//! test's own read selects, so it keeps its reading.
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
        // An `unwrap_err()` on a binding reads nothing without the literal,
        // and the literal adds nothing to it.
        (
            "bound_unwrap_err_alone",
            "{ let r = g(2); r.unwrap_err(); }",
            UNREVEALED,
        ),
        (
            "bound_unwrap_err_assert_true",
            "{ let r = g(2); r.unwrap_err(); assert!(true); }",
            UNREVEALED,
        ),
    ])
}

/// The class of the candidate-current `family` finding on the changed line of
/// `template`, where `@@` reads `1` at base and `2` at head.
fn marked_class(key: &str, template: &str, family: &str) -> Result<String, String> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("literal_only_assertion_credit")
        .join(key);
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"issue14968_sibling_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    let marked = template
        .lines()
        .position(|line| line.contains("@@"))
        .ok_or_else(|| format!("{key}: no @@ marker"))?;
    let changed_line = marked + 1;
    let marked_text = template.lines().nth(marked).unwrap_or_default();
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{changed_line},1 +{changed_line},1 @@\n-{}\n+{}\n",
        marked_text.replace("@@", "1"),
        marked_text.replace("@@", "2"),
    );
    std::fs::write(root.join("src/lib.rs"), template.replace("@@", "2"))
        .map_err(|e| e.to_string())?;
    std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
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
            finding.probe.family.as_str() == family && finding.probe.location.line == changed_line
        })
        .collect::<Vec<_>>();
    let class = match findings.as_slice() {
        [finding] => finding.class.as_str().to_string(),
        _ => return Err(format!("{key}: {family}: {:#?}", report.findings)),
    };
    std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(class)
}

const SIDE_EFFECT_OWNER: &str = "pub struct S { pub v: Vec<i32> }\nimpl S {\n    pub fn add(&mut self, a: i32) {\n        self.v.push(a + @@);\n    }\n}\npub fn other() -> Vec<i32> { vec![1] }\n";
const RESULT_OWNER_WITH_OTHER: &str = "pub fn g(a: i32) -> Result<i32, String> {\n    if a > @@ { Ok(a) } else { Err(\"small\".into()) }\n}\npub fn other() -> i32 { 4 }\n";

fn with_test(owner: &str, body: &str) -> String {
    format!(
        "{owner}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {{\n        {body}\n    }}\n}}\n"
    )
}

#[test]
fn a_dropped_literal_never_hands_the_fallback_to_an_unrelated_sibling() -> Result<(), String> {
    let mut credited = Vec::new();
    for (key, owner, body, family) in [
        (
            "side_effect_literal_beside_unrelated_read",
            SIDE_EFFECT_OWNER,
            "let mut s = S { v: vec![] }; s.add(5); let o = other(); assert!(true); assert!(!o.is_empty());",
            "side_effect",
        ),
        (
            "side_effect_literal_eq_beside_unrelated_eq",
            SIDE_EFFECT_OWNER,
            "let mut s = S { v: vec![] }; s.add(5); assert_eq!(1, 1); assert_eq!(other(), vec![1]);",
            "side_effect",
        ),
        (
            "error_path_literal_beside_unrelated_eq",
            RESULT_OWNER_WITH_OTHER,
            "let _ = g(2); assert!(true); assert_eq!(other(), 4);",
            "error_path",
        ),
    ] {
        let class = marked_class(key, &with_test(owner, body), family)?;
        if class != "reachable_unrevealed" {
            credited.push(format!("{key}: {family} reads {class}"));
        }
    }
    // The unrelated sibling observes nothing the owner does.
    assert_eq!(credited, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_rebound_owner_binding_fails_closed_beside_a_dropped_literal() -> Result<(), String> {
    // The shared binding authority drops a name that any pattern rebinds,
    // even in a nested scope, so the surviving read no longer counts as an
    // owner read and the dropped literal keeps the fallback (fail closed).
    let without_literal = result_owner_classes(
        "scoped_rebind_alone",
        "{ let r = g(2); if let Some(r) = Some(1) { let _ = r; } assert!(r.is_err()); }",
    )?;
    let with_literal = result_owner_classes(
        "scoped_rebind_assert_true",
        "{ let r = g(2); if let Some(r) = Some(1) { let _ = r; } assert!(true); assert!(r.is_err()); }",
    )?;
    assert_eq!(
        (without_literal.0.as_str(), without_literal.1.as_str()),
        WEAK
    );
    assert_eq!(
        (with_literal.0.as_str(), with_literal.1.as_str()),
        ("weakly_exposed", "reachable_unrevealed")
    );
    Ok(())
}

#[test]
fn a_dropped_literal_never_lowers_an_assertion_that_reads_the_owner() -> Result<(), String> {
    let mut lowered = Vec::new();
    for (key, function, (error_path, predicate)) in [
        // Controls: the same reads without the literal.
        ("is_err_alone", "{ assert!(g(2).is_err()); }", WEAK),
        (
            "bound_is_err_alone",
            "{ let r = g(2); assert!(r.is_err()); }",
            WEAK,
        ),
        (
            "expect_err_format_alone",
            "{ g(2).expect_err(&format!(\"x\")); }",
            WEAK,
        ),
        // A macro assertion on the owner call keeps its reading beside a
        // dropped literal, on either side of it.
        (
            "is_err_then_assert_true",
            "{ assert!(g(2).is_err()); assert!(true); }",
            WEAK,
        ),
        (
            "assert_true_then_is_err",
            "{ assert!(true); assert!(g(2).is_err()); }",
            WEAK,
        ),
        (
            "assert_eq_literals_then_is_err",
            "{ assert_eq!(1, 1); assert!(g(2).is_err()); }",
            WEAK,
        ),
        // A read of a binding bound from an owner call.
        (
            "bound_assert_true_then_is_err",
            "{ let r = g(2); assert!(true); assert!(r.is_err()); }",
            WEAK,
        ),
        // An implicit observer whose text holds a macro call.
        (
            "expect_err_format_assert_true",
            "{ g(2).expect_err(&format!(\"x\")); assert!(true); }",
            WEAK,
        ),
    ] {
        let (actual_error_path, actual_predicate) = result_owner_classes(key, function)?;
        if (actual_error_path.as_str(), actual_predicate.as_str()) != (error_path, predicate) {
            lowered.push(format!(
                "{key}: (error_path, predicate) reads ({actual_error_path}, {actual_predicate})"
            ));
        }
    }
    // Each row reads as the same test without the literal.
    assert_eq!(lowered, Vec::<String>::new());
    Ok(())
}
