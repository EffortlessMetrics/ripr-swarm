//! End-to-end witness for #9072: a `use <crate>::*;` at the top of an
//! integration test must not hide which module a qualified call names, so a
//! test of `b::value` is not credited to a changed `a::value`.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const LIB: &str = "pub mod a {\n    pub fn value(x: i32) -> i32 { x + 2 }\n}\n\npub mod b {\n    pub fn value(x: i32) -> i32 { x + 2 }\n}\n";
const OLD_LINE: &str = "    pub fn value(x: i32) -> i32 { x + 1 }";

/// The findings on line 2 of `src/lib.rs` (`a::value`, changed from `x + 1`
/// to `x + 2`) when the integration test is `test`.
fn findings(test: &str) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-glob-call-identity-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"globs\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write("src/lib.rs", LIB)?;
    write("tests/api.rs", test)?;
    let lines = LIB.lines().collect::<Vec<_>>();
    let new = lines
        .get(1)
        .ok_or_else(|| "lib has a second line".to_string())?;
    let rest = lines
        .iter()
        .skip(2)
        .map(|line| format!(" {line}\n"))
        .collect::<String>();
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n {first}\n-{OLD_LINE}\n+{new}\n{rest}",
        count = lines.len(),
        first = lines.first().copied().unwrap_or_default(),
    );
    write("diff.patch", &diff)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .collect())
}

fn return_value(findings: &[Finding]) -> Result<&Finding, String> {
    findings
        .iter()
        .find(|finding| finding.probe.family.as_str() == "return_value")
        .ok_or_else(|| format!("no return_value finding in {findings:?}"))
}

fn relates_a_test(finding: &Finding) -> bool {
    !finding.related_tests.is_empty()
}

#[test]
fn a_glob_import_does_not_credit_a_test_of_the_other_module() -> Result<(), String> {
    for call in ["b::value(1)", "globs::b::value(1)"] {
        let findings = findings(&format!(
            "use globs::*;\n\n#[test]\nfn other_module() {{\n    assert_eq!({call}, 3);\n}}\n"
        ))?;
        let finding = return_value(&findings)?;
        assert_eq!(
            finding.class,
            ExposureClass::NoStaticPath,
            "`{call}` names b::value: {finding:?}"
        );
        assert!(!relates_a_test(finding), "{finding:?}");
    }
    Ok(())
}

#[test]
fn a_glob_import_keeps_the_credit_for_a_test_of_the_changed_module() -> Result<(), String> {
    // Control: the same preamble and a call of the changed module.
    for call in ["a::value(1)", "globs::a::value(1)"] {
        let findings = findings(&format!(
            "use globs::*;\n\n#[test]\nfn changed_module() {{\n    assert_eq!({call}, 3);\n}}\n"
        ))?;
        let finding = return_value(&findings)?;
        assert!(relates_a_test(finding), "`{call}`: {finding:?}");
    }
    Ok(())
}

#[test]
fn a_dependency_glob_still_leaves_the_call_unsettled() -> Result<(), String> {
    // Control: `b` may come from the dependency's glob, so nothing is dropped.
    let findings = findings(
        "use other_dep::*;\n\n#[test]\nfn unknown() {\n    assert_eq!(b::value(1), 3);\n}\n",
    )?;
    assert!(relates_a_test(return_value(&findings)?), "{findings:?}");
    Ok(())
}
