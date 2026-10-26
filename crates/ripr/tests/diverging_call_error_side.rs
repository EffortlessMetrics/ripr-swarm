//! A call that can end the test run before the assertion (`process::exit`,
//! `abort`, `exec`) must keep the `?` side-flip oracle from crediting
//! `exposed`, wherever the call sits: in an owner-call argument, a check
//! helper or a table loop body (#8015).
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

#[test]
fn diverging_calls_before_an_err_side_assertion_never_credit_exposed() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("diverging_call_error_side");
    let production = "#[derive(Debug)]\npub struct E;\nfn digit(c: char) -> Result<u32, E> {\n    c.to_digit(10).ok_or(E)\n}\npub fn total(s: &str) -> Result<u32, E> {\n    let mut sum = 0;\n    for c in s.chars() {\n        let d = digit(c)?;\n        sum += d;\n    }\n    Ok(sum)\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -8,3 +8,3 @@ pub fn total(s: &str) -> Result<u32, E> {\n     for c in s.chars() {\n-        let d = (digit(c))?;\n+        let d = digit(c)?;\n         sum += d;\n";
    // (name, production tail, test-module helpers, test body, credited)
    let cases = [
        ("control", "", "", "assert!(total(\"1x\").is_err());", true),
        (
            "table_control",
            "",
            "",
            "for (s, want) in [(\"1x\", None)] {\n assert_eq!(total(s).ok(), want);\n }",
            true,
        ),
        (
            "production_exit_elsewhere_in_the_file",
            "pub fn quit() -> ! {\n    std::process::exit(2)\n}\n",
            "",
            "assert!(total(\"1x\").is_err());",
            true,
        ),
        (
            "check_helper_control",
            "",
            "fn check(s: &str, want: Option<u32>) {\n        assert_eq!(total(s).ok(), want);\n    }\n",
            "check(\"1x\", None);",
            true,
        ),
        (
            "exit_in_owner_call_argument",
            "",
            "fn skip() -> &'static str {\n        std::process::exit(0)\n    }\n",
            "assert_eq!(total(skip()).ok(), None);",
            false,
        ),
        (
            "exit_in_check_helper",
            "",
            "fn check(s: &str, want: Option<u32>) {\n        std::process::exit(0);\n        assert_eq!(total(s).ok(), want);\n    }\n",
            "check(\"1x\", None);",
            false,
        ),
        (
            "abort_before_assertion",
            "",
            "",
            "std::process::abort();\n        assert!(total(\"1x\").is_err());",
            false,
        ),
        (
            "exec_before_assertion",
            "",
            "",
            "let _ = std::process::Command::new(\"true\").exec();\n        assert!(total(\"1x\").is_err());",
            false,
        ),
        (
            "exit_in_table_loop",
            "",
            "",
            "for (s, want) in [(\"1x\", None)] {\n std::process::exit(0);\n assert_eq!(total(s).ok(), want);\n }",
            false,
        ),
    ];
    let mut wrong = Vec::new();
    for (name, production_tail, helpers, body, credited) in cases {
        let source = format!(
            "{production}{production_tail}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    {helpers}    #[test]\n    fn total_rejects_bad_digit() {{\n        {body}\n    }}\n}}\n"
        );
        let root = root_dir.join(name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"total_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let classes: Vec<String> = report
            .findings
            .iter()
            .map(|f| format!("{}={}", f.probe.family.as_str(), f.class.as_str()))
            .collect();
        let error_path_exposed = report
            .findings
            .iter()
            .any(|f| f.probe.family.as_str() == "error_path" && f.class.as_str() == "exposed");
        if report.findings.is_empty() || error_path_exposed != credited {
            wrong.push(format!("{name}: {classes:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}
