//! A sibling oracle cannot turn an opaque inline-snapshot helper into a gap.

#![cfg(feature = "lang-rust")]

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Finding, Mode, OutputFormat, check_workspace,
    render_check,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = r#"mod snap {
    pub fn check(actual: &str, expected: &str) { assert_eq!(actual, expected); }
}
macro_rules! assert_data_eq {
    ($a:expr, $e:expr) => { $crate::snap::check(&$a, $e) };
}
macro_rules! snap_str {
    [[$expected:literal]] => { $expected };
}
pub fn render_float(value: f64) -> String {
    if value == 0.0 && value.is_sign_negative() {
        "-0.0".to_string()
    } else {
        value.to_string()
    }
}
pub fn separate_owner() -> i32 { 7 }
pub fn separate_float(_: f64) -> String { "-0.0".to_string() }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_zero_snapshot() {
        assert_data_eq!(render_float(-0.0), snap_str![["-0.0"]]);
    }
    #[test]
    fn separate_exact_oracle() { assert_eq!(separate_owner(), 7); }
}
"#;

struct Fixture(PathBuf);

fn replace_once(source: &str, before: &str, after: &str) -> String {
    assert_eq!(
        source.matches(before).count(),
        1,
        "control must change its subject"
    );
    source.replacen(before, after, 1)
}

impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-local-snapshot-helper-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname = \"snapshot-helper-control\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
        let lines: Vec<_> = source.lines().collect();
        let positions: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.trim() == "\"-0.0\".to_string()")
            .collect();
        assert_eq!(positions.len(), 1, "one changed production return");
        let (offset, after) = positions[0];
        let line = offset + 1;
        let before = after.replace("\"-0.0\"", "\"-0\"");
        std::fs::write(
            fixture.0.join("diff.patch"),
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{before}\n+{after}\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn checked_output(&self) -> Result<CheckOutput, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert_eq!(output.findings.len(), 1, "nonempty changed-owner subject");
        let finding = &output.findings[0];
        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| { test.name == "negative_zero_snapshot" && test.oracle.is_none() })
        );
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| { test.name == "separate_exact_oracle" && test.oracle.is_some() })
        );
        Ok(output)
    }

    fn finding(&self) -> Result<Finding, String> {
        Ok(self.checked_output()?.findings.remove(0))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn limited(source: &str) -> Result<(), String> {
    let output = Fixture::new(source)?.checked_output()?;
    let finding = &output.findings[0];
    assert_eq!(
        finding.static_limit_kind.map(|kind| kind.as_str()),
        Some("rust_macro_wrapped_assertion_unresolved")
    );
    let next_step = "Inspect the custom assertion macro's definition and run the existing test; its spelling does not establish assertion semantics.";
    assert_eq!(finding.recommended_next_step.as_deref(), Some(next_step));
    assert_eq!(finding.canonical_gap, None);
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&output, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    assert_eq!(json["analysis_outcome"]["analysis_complete"], false);
    assert_eq!(
        json["analysis_outcome"]["outcome"]["kind"],
        "partial_with_limitations"
    );
    assert!(
        json["analysis_outcome"]["outcome"]["limitations"]
            .as_array()
            .ok_or("outcome limitations must be an array")?
            .iter()
            .any(|item| item["bounded_detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("rust_macro_wrapped_assertion_unresolved")))
    );
    assert_eq!(json["findings"][0]["recommended_next_step"], next_step);
    assert_eq!(json["findings"][0]["suggested_next_action"], next_step);
    assert_eq!(
        json["findings"][0]["static_limitation"]["kind"],
        "rust_macro_wrapped_assertion_unresolved"
    );
    assert!(json["findings"][0]["canonical_gap"].is_null());
    let human = render_check(&output, &OutputFormat::HumanFull)?;
    assert!(human.contains(next_step.trim_end_matches('.')));
    assert!(human.contains("assertion semantics could not be resolved for some findings"));
    assert!(!human.contains("some changed files were not analyzed"));
    assert!(
        human.contains("Some findings depend on assertion semantics that could not be resolved")
    );
    assert!(!human.contains("behavior outside it has no finding"));
    let github = render_check(&output, &OutputFormat::Github)?;
    assert!(
        github.contains("Some findings depend on assertion semantics that could not be resolved.")
    );
    assert!(!github.contains("behavior outside it has no finding"));
    assert!(!human.contains("Replace broad assertions"));
    Ok(())
}

fn keeps_gap(body: &str) -> Result<(), String> {
    let source = replace_once(
        SOURCE,
        "assert_data_eq!(render_float(-0.0), snap_str![[\"-0.0\"]]);",
        body,
    );
    let output = Fixture::new(&source)?.checked_output()?;
    assert_eq!(output.findings[0].static_limit_kind, None);
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&output, &OutputFormat::Json)?)
            .map_err(|error| error.to_string())?;
    assert_eq!(json["analysis_outcome"]["analysis_complete"], true);
    Ok(())
}

#[test]
fn nested_expected_macro_keeps_the_local_helper_limit() -> Result<(), String> {
    limited(SOURCE)
}

#[test]
fn helper_and_expected_names_cannot_supply_exposed_credit() -> Result<(), String> {
    for source in [
        replace_once(
            SOURCE,
            "pub fn check(actual: &str, expected: &str) { assert_eq!(actual, expected); }",
            "pub fn check(_actual: &str, _expected: &str) {}",
        ),
        replace_once(
            &replace_once(
                SOURCE,
                "pub fn check(actual: &str, expected: &str) { assert_eq!(actual, expected); }",
                "pub fn check(_actual: &str, _expected: &str) {}",
            ),
            "    use super::*;",
            "    use super::*;\n    mod snap { pub fn check(a: &str, e: &str) { assert_eq!(a, e); } }",
        ),
        replace_once(SOURCE, "snap_str![[\"-0.0\"]]", "snap_str![[\"0.0\"]]"),
        replace_once(
            SOURCE,
            "assert_eq!(actual, expected);",
            "macro_rules! assert_eq { ($a:expr, $e:expr) => {}; } assert_eq!(actual, expected);",
        ),
    ] {
        limited(&source)?;
    }
    Ok(())
}

#[test]
fn only_owner_calls_outside_nested_macro_inputs_supply_the_limit() -> Result<(), String> {
    for body in [
        "let _ = render_float(-0.0); assert_data_eq!(separate_float(-0.0), snap_str![[\"-0.0\"]]);",
        "let _ = render_float(-0.0); assert_data_eq!(discard!(render_float(-0.0)), snap_str![[\"-0.0\"]]);",
        "let value = render_float(-0.0); assert_data_eq!(value, snap_str![[\"-0.0\"]]);",
    ] {
        keeps_gap(body)?;
    }
    Ok(())
}

#[test]
fn a_literal_expected_operand_retains_its_existing_limit() -> Result<(), String> {
    limited(&replace_once(SOURCE, "snap_str![[\"-0.0\"]]", "\"-0.0\""))
}

#[test]
fn complete_proximity_evidence_survives_the_display_cap() -> Result<(), String> {
    let assertions = (0..12)
        .map(|n| format!("assert_eq!(separate_owner(), 7, \"oracle {n}\");"))
        .collect::<Vec<_>>()
        .join("\n");
    let source = replace_once(SOURCE, "assert_eq!(separate_owner(), 7);", &assertions);
    let finding = Fixture::new(&source)?.finding()?;
    assert!(finding.related_tests_total() > finding.related_tests.len());
    limited(&source)?;
    for body in [
        "let _ = render_float(-0.0); assert_data_eq!(separate_float(-0.0), snap_str![[\"-0.0\"]]);",
        "let _ = render_float(-0.0); assert_data_eq!(discard!(render_float(-0.0)), snap_str![[\"-0.0\"]]);",
    ] {
        let control = replace_once(
            &source,
            "assert_data_eq!(render_float(-0.0), snap_str![[\"-0.0\"]]);",
            body,
        );
        let finding = Fixture::new(&control)?.finding()?;
        assert!(finding.related_tests_total() > finding.related_tests.len());
        assert_eq!(finding.static_limit_kind, None);
    }
    Ok(())
}

#[test]
fn capped_cli_and_verify_refuse_an_incomplete_after_snapshot() -> Result<(), String> {
    use std::process::{Command, Output};

    fn run(root: &std::path::Path, program: &str, args: &[&str]) -> Result<Output, String> {
        Command::new(program)
            .current_dir(root)
            .args(args)
            .output()
            .map_err(|e| e.to_string())
    }
    fn success(output: Output) -> Result<Output, String> {
        if output.status.success() {
            Ok(output)
        } else {
            Err(format!(
                "command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
    fn commit(root: &std::path::Path, message: &str) -> Result<(), String> {
        success(run(
            root,
            "git",
            &["add", "Cargo.toml", "src/lib.rs", "diff.patch"],
        )?)?;
        success(run(
            root,
            "git",
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                message,
            ],
        )?)?;
        Ok(())
    }
    let assertions = (0..12)
        .map(|n| format!("assert_eq!(separate_owner(), 7, \"oracle {n}\");"))
        .collect::<Vec<_>>()
        .join("\n");
    let limited = replace_once(SOURCE, "assert_eq!(separate_owner(), 7);", &assertions);
    let control = replace_once(
        &limited,
        "assert_data_eq!(render_float(-0.0), snap_str![[\"-0.0\"]]);",
        "let _ = render_float(-0.0); assert_data_eq!(separate_float(-0.0), snap_str![[\"-0.0\"]]);",
    );
    let fixture = Fixture::new(&control)?;
    let root = &fixture.0;
    success(run(root, "git", &["init", "-q"])?)?;
    commit(root, "control before")?;
    let check = |format| {
        run(
            root,
            env!("CARGO_BIN_EXE_ripr"),
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "diff.patch",
                "--mode",
                "ready",
                "--format",
                format,
                "--quiet",
            ],
        )
    };
    let before = success(check("check-findings-json")?)?;
    let before_json: serde_json::Value =
        serde_json::from_slice(&before.stdout).map_err(|e| e.to_string())?;
    assert_eq!(before_json["analysis_outcome"]["analysis_complete"], true);
    assert!(
        before_json["findings"][0]["canonical_gap_id"]
            .as_str()
            .is_some()
    );
    std::fs::create_dir_all(root.join("target/ripr")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("target/ripr/before.json"), before.stdout)
        .map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/lib.rs"), &limited).map_err(|e| e.to_string())?;
    commit(root, "limited after")?;
    let json: serde_json::Value =
        serde_json::from_slice(&success(check("json")?)?.stdout).map_err(|e| e.to_string())?;
    assert_eq!(json["analysis_outcome"]["analysis_complete"], false);
    assert_eq!(
        json["analysis_outcome"]["outcome"]["kind"],
        "partial_with_limitations"
    );
    assert_eq!(
        json["findings"][0]["static_limitation"]["kind"],
        "rust_macro_wrapped_assertion_unresolved"
    );
    let finding = &json["findings"][0];
    assert!(
        finding["related_tests_total"]
            .as_u64()
            .ok_or("missing related total")?
            > finding["related_tests"]
                .as_array()
                .ok_or("missing related rows")?
                .len() as u64
    );
    let after = success(check("check-findings-json")?)?;
    std::fs::write(root.join("target/ripr/after.json"), after.stdout).map_err(|e| e.to_string())?;
    let verify = |after| {
        run(
            root,
            env!("CARGO_BIN_EXE_ripr"),
            &[
                "agent",
                "verify",
                "--root",
                ".",
                "--before",
                "target/ripr/before.json",
                "--after",
                after,
                "--json",
            ],
        )
    };
    let refused = verify("target/ripr/after.json")?;
    assert_eq!(refused.status.code(), Some(2));
    assert!(refused.stdout.is_empty());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("after artifact is incomplete"));
    std::fs::write(root.join("src/lib.rs"), &control).map_err(|e| e.to_string())?;
    commit(root, "control after")?;
    let control_after = success(check("check-findings-json")?)?;
    std::fs::write(
        root.join("target/ripr/control-after.json"),
        control_after.stdout,
    )
    .map_err(|e| e.to_string())?;
    success(verify("target/ripr/control-after.json")?)?;
    Ok(())
}

#[test]
fn github_keeps_scope_warning_when_macro_limit_is_mixed_with_missing_file() -> Result<(), String> {
    let fixture = Fixture::new(SOURCE)?;
    let diff_path = fixture.0.join("diff.patch");
    let mut diff = std::fs::read_to_string(&diff_path).map_err(|e| e.to_string())?;
    diff.push_str("diff --git a/src/missing.rs b/src/missing.rs\n--- a/src/missing.rs\n+++ b/src/missing.rs\n@@ -1 +1 @@\n-old\n+new\n");
    std::fs::write(&diff_path, diff).map_err(|e| e.to_string())?;
    let output = fixture.checked_output()?;
    assert_eq!(
        output.findings[0]
            .static_limit_kind
            .map(|kind| kind.as_str()),
        Some("rust_macro_wrapped_assertion_unresolved")
    );
    let json: serde_json::Value =
        serde_json::from_str(&render_check(&output, &OutputFormat::Json)?)
            .map_err(|e| e.to_string())?;
    let limitations = json["analysis_outcome"]["outcome"]["limitations"]
        .as_array()
        .ok_or("missing limitations")?;
    assert!(
        limitations
            .iter()
            .any(|item| item["kind"] == "changed_file_absent_from_worktree")
    );
    assert!(limitations.iter().any(|item| {
        item["bounded_detail"]
            .as_str()
            .is_some_and(|s| s.contains("rust_macro_wrapped_assertion_unresolved"))
    }));
    let github = render_check(&output, &OutputFormat::Github)?;
    assert!(
        github.contains(
            "The findings cover only the analyzed scope; behavior outside it has no finding."
        ),
        "{github}"
    );
    assert!(
        !github.contains("Some findings depend on assertion semantics"),
        "{github}"
    );
    Ok(())
}
