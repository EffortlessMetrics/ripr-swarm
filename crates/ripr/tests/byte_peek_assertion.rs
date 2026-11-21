//! #5329: a literal byte-peek change and the test that compares its result.

use ripr::domain::{ProbeFamily, StageState};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str =
    "pub fn accepts_ge(bytes: &[u8]) -> bool {\n    Some(&b'=') == bytes.get(1)\n}\n";
const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn accepts_ge(bytes: &[u8]) -> bool {
-    Some(&b'>') == bytes.get(1)
+    Some(&b'=') == bytes.get(1)
 }
"#;
const UNRELATED: &str = r#"#[test]
fn unrelated_test() {
    assert_eq!(b">=".len(), 2);
}
"#;

fn findings(tests: &str) -> Result<Vec<Finding>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-byte-peek-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed),
    ));
    let result = (|| {
        for directory in ["src", "tests"] {
            std::fs::create_dir_all(root.join(directory)).map_err(|error| error.to_string())?;
        }
        let write = |path: &str, text: &str| {
            std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"comparator\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )?;
        write("src/lib.rs", SOURCE)?;
        write(
            "tests/comparator.rs",
            &format!("use comparator::accepts_ge;\n{tests}\n{UNRELATED}"),
        )?;
        write("change.diff", DIFF)?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("change.diff")),
            include_unchanged_tests: true,
            mode: Mode::Ready,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        let findings: Vec<_> = report
            .findings
            .into_iter()
            .filter(|finding| finding.probe.expression == "Some(&b'=') == bytes.get(1)")
            .collect();
        for family in [ProbeFamily::Predicate, ProbeFamily::ReturnValue] {
            assert_eq!(
                findings
                    .iter()
                    .filter(|finding| finding.probe.family == family)
                    .count(),
                1,
                "{findings:#?}"
            );
        }
        Ok(findings)
    })();
    let _ = std::fs::remove_dir_all(root);
    result
}

#[test]
fn comparison_of_the_changed_byte_peek_credits_the_asserting_test() -> Result<(), String> {
    let tests = r#"
#[test]
fn accepts_greater_equal() {
    assert_eq!(accepts_ge(b">="), true);
}
"#;
    for finding in findings(tests)? {
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:#?}");
        assert_eq!(finding.ripr.infect.state, StageState::Yes);
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Yes);
        assert_eq!(finding.related_tests.len(), 1);
        let test = &finding.related_tests[0];
        assert_eq!(test.name, "accepts_greater_equal");
        assert!(test.miss.is_none(), "{test:?}");
    }
    Ok(())
}

#[test]
fn an_unasserted_byte_peek_cannot_borrow_an_unrelated_assertion() -> Result<(), String> {
    let tests = r#"
#[test]
fn calls_without_asserting() {
    let _ = accepts_ge(b">=");
}

#[test]
fn checks_unchanged_input() {
    assert_eq!(accepts_ge(b">x"), false);
}
"#;
    for finding in findings(tests)? {
        assert_ne!(finding.class, ExposureClass::Exposed, "{finding:#?}");
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name == "calls_without_asserting")
        );
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name == "checks_unchanged_input")
        );
        assert!(
            finding
                .related_tests
                .iter()
                .all(|test| test.name != "unrelated_test")
        );
    }
    Ok(())
}
