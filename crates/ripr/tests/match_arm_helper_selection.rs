//! End-to-end witness for RIPR-SPEC-0229 decision 5: a check helper's
//! literal arguments select a changed arm.
//!
//! A test that calls `check_color('S', "black")`, where the helper asserts
//! `suit_color(suit) == want`, lends the literal `'S'` to the owner call. The
//! literal selects the changed `'C' | 'S'` arm and the helper's exact
//! assertion pins that call's result, so the arm reads exposed. A literal
//! that selects another arm, or a variable argument, lends nothing.

use ripr::{CheckInput, CheckOutput, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = r#"pub fn suit_color(suit: char) -> &'static str {
    match suit {
        'H' | 'D' => "red",
        'C' | 'S' => "black",
        _ => "unknown",
    }
}
"#;

const DIFF: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 0000000..1111111 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,7 +1,7 @@
 pub fn suit_color(suit: char) -> &'static str {
     match suit {
         'H' | 'D' => "red",
-        'C' | 'S' => "black",
+        'S' | 'C' => "black",
         _ => "unknown",
     }
 }
"#;

const SELECTING_TEST: &str = r#"use match_arm_helper_selection::suit_color;

fn check_color(suit: char, want: &str) {
    assert_eq!(suit_color(suit), want);
}

#[test]
fn suits_have_colors() {
    check_color('H', "red");
    check_color('S', "black");
}
"#;

const OTHER_ARM_TEST: &str = r#"use match_arm_helper_selection::suit_color;

fn check_color(suit: char, want: &str) {
    assert_eq!(suit_color(suit), want);
}

#[test]
fn hearts_are_red() {
    check_color('H', "red");
}
"#;

const VARIABLE_TEST: &str = r#"use match_arm_helper_selection::suit_color;

fn check_color(suit: char, want: &str) {
    assert_eq!(suit_color(suit), want);
}

#[test]
fn spades_are_black() {
    let suit = 'S';
    check_color(suit, "black");
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
            "ripr-match-arm-helper-selection-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-helper-selection\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/suits.rs"), test_source)
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
fn a_helper_lent_literal_that_selects_the_arm_exposes_it() -> Result<(), String> {
    let repo = TempRepo::create(SELECTING_TEST)?;
    let output = repo.check()?;
    for finding in changed_arms(&output)? {
        assert_eq!(
            finding.class,
            ripr::ExposureClass::Exposed,
            "`check_color('S', \"black\")` selects and pins the changed arm: {:?}",
            finding.ripr.reveal.discriminate
        );
    }
    Ok(())
}

#[test]
fn a_helper_lent_literal_for_another_arm_does_not_expose_it() -> Result<(), String> {
    let repo = TempRepo::create(OTHER_ARM_TEST)?;
    let output = repo.check()?;
    for finding in changed_arms(&output)? {
        assert_ne!(
            finding.class,
            ripr::ExposureClass::Exposed,
            "`'H'` selects the red arm, not the changed one: {:?}",
            finding.ripr.reveal.discriminate
        );
        // Decision 5 credits only: a helper call never names the arm as
        // unselected, even when its lent literal selects another arm.
        assert!(
            !finding
                .ripr
                .infect
                .summary
                .contains("No related test call selects arm"),
            "a helper-lent literal must not name the arm as unselected: {:?}",
            finding.ripr.infect
        );
    }
    Ok(())
}

#[test]
fn a_variable_helper_argument_does_not_expose_the_arm() -> Result<(), String> {
    let repo = TempRepo::create(VARIABLE_TEST)?;
    let output = repo.check()?;
    for finding in changed_arms(&output)? {
        assert_ne!(
            finding.class,
            ripr::ExposureClass::Exposed,
            "a variable argument is not a lent literal: {:?}",
            finding.ripr.reveal.discriminate
        );
    }
    Ok(())
}
