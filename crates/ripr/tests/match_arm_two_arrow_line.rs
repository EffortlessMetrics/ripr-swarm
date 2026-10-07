//! End-to-end witness for RIPR-SPEC-0122 `before` cutting (#7037).
//!
//! A changed match arm's `before` is cut to its old head only when the old
//! line holds one `=>`. With two arms on one line, or a nested match in the
//! arm body, the whole old line stays, and arm selection cannot tell which
//! arm changed. The verdict must then match the uncut behavior: no
//! "No related test call selects arm" signal.

use ripr::{CheckInput, CheckOutput, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const UNSELECTED_PREFIX: &str = "No related test call selects arm";

const TEST_SOURCE: &str = r#"use match_arm_two_arrow_line::route;

#[test]
fn other_kind_routes_to_nine() {
    assert_eq!(route("x"), 9);
}
"#;

fn source(arms: &str) -> String {
    format!(
        "pub fn route(kind: &str) -> u32 {{\n    match kind {{\n{arms}\n        _ => 9,\n    }}\n}}\n"
    )
}

fn diff(old_arms: &str, new_arms: &str) -> String {
    let count = 4 + old_arms.lines().count();
    let old = old_arms
        .lines()
        .map(|line| format!("-{line}\n"))
        .collect::<String>();
    let new = new_arms
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n pub fn route(kind: &str) -> u32 {{\n     match kind {{\n{old}{new}         _ => 9,\n     }}\n }}\n"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(old_arms: &str, new_arms: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests share one
        // `SystemTime` on some platforms.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-two-arrow-line-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-two-arrow-line\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source(new_arms))
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/route.rs"), TEST_SOURCE)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff(old_arms, new_arms))
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

fn changed_arm<'a>(output: &'a CheckOutput, head: &str) -> Result<&'a ripr::Finding, String> {
    output
        .findings
        .iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::MatchArm
                && finding.probe.expression.starts_with(head)
        })
        .ok_or_else(|| {
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
            format!("missing changed `{head}` arm; observed {observed}")
        })
}

fn changed_b_arm(old_arms: &str, new_arms: &str) -> Result<ripr::Finding, String> {
    let repo = TempRepo::create(old_arms, new_arms)?;
    let output = repo.check()?;
    changed_arm(&output, "\"b\" | \"d\" =>").cloned()
}

fn names_unselected_arm(finding: &ripr::Finding) -> bool {
    finding.ripr.infect.summary.contains(UNSELECTED_PREFIX)
        || finding
            .evidence
            .iter()
            .any(|line| line.contains(UNSELECTED_PREFIX))
}

#[test]
fn a_one_arm_line_cuts_before_and_names_the_unselected_arm() -> Result<(), String> {
    // The control: one `=>` on the old line, so `before` is the old head and
    // arm selection reads it. `route("x")` selects no changed alternative.
    let finding = changed_b_arm("        \"b\" | \"c\" => 2,", "        \"b\" | \"d\" => 2,")?;
    assert_eq!(finding.probe.before.as_deref(), Some("\"b\" | \"c\" =>"));
    assert!(
        names_unselected_arm(&finding),
        "the control must name the unselected arm: {:?}",
        finding.ripr.infect
    );
    Ok(())
}

#[test]
fn two_arms_on_one_line_keep_the_whole_line_and_name_no_unselected_arm() -> Result<(), String> {
    let old = "        \"a\" => 1, \"b\" | \"c\" => 2,";
    let finding = changed_b_arm(old, "        \"a\" => 1, \"b\" | \"d\" => 2,")?;
    assert_eq!(finding.probe.before.as_deref(), Some(old.trim()));
    assert!(
        !names_unselected_arm(&finding),
        "which arm changed is not known on a two-arm line: {:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_nested_match_in_the_body_keeps_the_whole_line_and_names_no_unselected_arm()
-> Result<(), String> {
    let old = "        \"b\" | \"c\" => match kind.len() { 0 => 3, _ => 2 },";
    let finding = changed_b_arm(
        old,
        "        \"b\" | \"d\" => match kind.len() { 0 => 3, _ => 2 },",
    )?;
    assert_eq!(finding.probe.before.as_deref(), Some(old.trim()));
    assert!(
        !names_unselected_arm(&finding),
        "a nested match's arrows keep the old line whole: {:?}",
        finding.evidence
    );
    Ok(())
}
