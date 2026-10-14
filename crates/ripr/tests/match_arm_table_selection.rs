//! End-to-end witness for RIPR-SPEC-0229 decisions 6 and 7: a computed
//! integer scrutinee, a value-equal pattern respelling, and inputs lent by a
//! constant-row table.
//!
//! `http_class` matches `status / 100`, and the diff respells the `3` arm as
//! `0x3`, which takes exactly the same inputs. A test looping over a literal
//! table whose `301` row selects that arm, under an exact `assert_eq!`,
//! exposes it; a table with no 3xx row does not.

use ripr::{CheckInput, CheckOutput, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = r#"pub fn http_class(status: u16) -> &'static str {
    match status / 100 {
        2 => "success",
        3 => "redirect",
        4 => "client error",
        _ => "other",
    }
}
"#;

const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 0000000..1111111 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,8 +1,8 @@
 pub fn http_class(status: u16) -> &'static str {
     match status / 100 {
         2 => "success",
-        3 => "redirect",
+        0x3 => "redirect",
         4 => "client error",
         _ => "other",
     }
 }
"#;

const SELECTING_TABLE: &str = r#"use match_arm_table_selection::http_class;

#[test]
fn http_class_table() {
    let cases = [(200, "success"), (301, "redirect"), (404, "client error")];
    for (status, want) in cases {
        assert_eq!(http_class(status), want, "{status}");
    }
}
"#;

const OTHER_ROWS_TABLE: &str = r#"use match_arm_table_selection::http_class;

#[test]
fn http_class_table() {
    let cases = [(200, "success"), (404, "client error")];
    for (status, want) in cases {
        assert_eq!(http_class(status), want, "{status}");
    }
}
"#;

const COMPUTED_TABLE: &str = r#"use match_arm_table_selection::http_class;

fn statuses() -> Vec<(u16, &'static str)> {
    vec![(301, "redirect")]
}

#[test]
fn http_class_table() {
    let cases = statuses();
    for (status, want) in cases {
        assert_eq!(http_class(status), want, "{status}");
    }
}
"#;

const SHADOWED_TABLE: &str = r#"use match_arm_table_selection::http_class;

#[test]
fn http_class_table() {
    let http_class = |_status: u16| "redirect";
    let cases = [(301, "redirect")];
    for (status, want) in cases {
        assert_eq!(http_class(status), want, "{status}");
    }
}
"#;

const BOUND_RESULT_TABLE: &str = r#"use match_arm_table_selection::http_class;

#[test]
fn http_class_table() {
    let want = http_class(301);
    let cases = [(301, ())];
    for (status, _unit) in cases {
        assert_eq!(http_class(status), want, "{status}");
    }
}
"#;

const HELPER_NAMED_LIKE_THE_COLUMN: &str = r#"use match_arm_table_selection::http_class;

fn check(status: u16, want: &str) {
    assert_eq!(http_class(status), want);
}

#[test]
fn http_class_table() {
    for status in [301u16, 302] {
        check(status + 100, "client error");
    }
}
"#;

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test_source: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests on Windows
        // read the same `SystemTime`, share one root, and overwrite each
        // other's fixture.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-table-selection-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-table-selection\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/classes.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Every match-arm finding on the changed line. The line yields more than
/// one match-arm probe (the pattern and the whole arm); each must carry the
/// same verdict.
fn changed_arms(output: &CheckOutput) -> Result<Vec<&ripr::Finding>, String> {
    let findings = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ProbeFamily::MatchArm && finding.probe.location.line == 4
        })
        .collect::<Vec<_>>();
    if findings.is_empty() {
        let observed = output
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}:{}:{}",
                    finding.probe.family.as_str(),
                    finding.probe.location.line,
                    finding.probe.expression
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(format!("missing changed match arm; observed {observed}"));
    }
    Ok(findings)
}

#[test]
fn a_table_row_that_selects_the_respelled_arm_exposes_it() -> Result<(), String> {
    let repo = TempRepo::create(SELECTING_TABLE)?;
    let output = repo.check()?;
    for finding in changed_arms(&output)? {
        assert_eq!(
            finding.class,
            ripr::ExposureClass::Exposed,
            "the `301` row selects `status / 100 == 3` under an exact assert: {:?}",
            finding.ripr.reveal.discriminate
        );
    }
    Ok(())
}

#[test]
fn a_table_with_no_selecting_row_does_not_expose_the_arm() -> Result<(), String> {
    let repo = TempRepo::create(OTHER_ROWS_TABLE)?;
    let output = repo.check()?;
    for finding in changed_arms(&output)? {
        assert_ne!(
            finding.class,
            ripr::ExposureClass::Exposed,
            "no row of `200`, `404` selects the redirect arm: {:?}",
            finding.ripr.reveal.discriminate
        );
    }
    Ok(())
}

#[test]
fn a_table_built_at_run_time_does_not_expose_the_arm() -> Result<(), String> {
    let repo = TempRepo::create(COMPUTED_TABLE)?;
    let output = repo.check()?;
    for finding in changed_arms(&output)? {
        assert_ne!(
            finding.class,
            ripr::ExposureClass::Exposed,
            "a table a function returns is not a constant-row table: {:?}",
            finding.ripr.reveal.discriminate
        );
    }
    Ok(())
}

#[test]
fn a_table_test_that_shadows_binds_or_borrows_does_not_expose_the_arm() -> Result<(), String> {
    for (label, test) in [
        ("a closure shadowing the owner", SHADOWED_TABLE),
        (
            "an owner result bound on the expected side",
            BOUND_RESULT_TABLE,
        ),
        (
            "a helper whose parameter shares the column's name",
            HELPER_NAMED_LIKE_THE_COLUMN,
        ),
    ] {
        let repo = TempRepo::create(test)?;
        let output = repo.check()?;
        for finding in changed_arms(&output)? {
            assert_ne!(
                finding.class,
                ripr::ExposureClass::Exposed,
                "{label} must not credit the arm: {:?}",
                finding.ripr.reveal.discriminate
            );
        }
    }
    Ok(())
}
