//! Files the operator left out on purpose are requested scope (#13894): a
//! fully tested change stays a complete analysis with `--path`, `--exclude`
//! or a configured `generated_file_patterns`, and `missed-txt` stays empty.
//! A genuinely skipped file the operator did not ask for stays incomplete.
use super::{assert_success, run_ripr_with_env, unique_temp_workspace, workspace_root};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COVERED_TESTS: &str = "use scoped_complete_fixture::discounted_total;\n\
#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n\
#[test]\nfn far_above_threshold_discounts() {\n    assert_eq!(discounted_total(10_000, 100), 9_990);\n}\n\
#[test]\nfn at_threshold_discounts() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

/// A boundary edit whose threshold is pinned by a test, plus one extra
/// changed file at `extra` that the scope under test leaves out.
fn fixture(
    label: &str,
    extra: &str,
    config: Option<&str>,
) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let dir = unique_temp_workspace(label);
    let input = workspace_root().join("fixtures/boundary_gap/input");
    fs::create_dir_all(dir.join("src/gen"))?;
    fs::create_dir_all(dir.join("tests"))?;
    fs::write(
        dir.join("Cargo.toml"),
        fs::read_to_string(input.join("Cargo.toml"))?
            .replace("boundary_gap_fixture", "scoped_complete_fixture"),
    )?;
    fs::write(
        dir.join("src/lib.rs"),
        fs::read_to_string(input.join("src/lib.rs"))?,
    )?;
    fs::write(dir.join("tests/discount.rs"), COVERED_TESTS)?;
    if let Some(parent) = dir.join(extra).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(dir.join(extra), "pub fn extra() -> u32 { 1 }\n")?;
    if let Some(config) = config {
        fs::write(dir.join("ripr.toml"), config)?;
    }
    let mut diff = fs::read_to_string(workspace_root().join("fixtures/boundary_gap/diff.patch"))?;
    diff.push_str(&format!(
        "diff --git a/{extra} b/{extra}\n--- /dev/null\n+++ b/{extra}\n@@ -0,0 +1 @@\n+pub fn extra() -> u32 {{ 1 }}\n"
    ));
    let diff_path = dir.join("change.diff");
    fs::write(&diff_path, diff)?;
    Ok((dir, diff_path))
}

fn check(dir: &Path, diff: &Path, format: &str, extra: &[&str]) -> std::process::Output {
    let root = dir.display().to_string();
    let diff = diff.display().to_string();
    let mut args = vec![
        "check", "--root", &root, "--diff", &diff, "--format", format,
    ];
    args.extend_from_slice(extra);
    run_ripr_with_env(&args, &[])
}

fn outcome(dir: &Path, diff: &Path, extra: &[&str]) -> Result<Value, Box<dyn std::error::Error>> {
    let output = check(dir, diff, "json", extra);
    assert_success(&output);
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn assert_complete_with_scope_note(value: &Value, label: &str) {
    let analysis = &value["analysis_outcome"];
    assert_eq!(analysis["analysis_complete"], true, "{label}: {analysis}");
    let kind = analysis["outcome"]["kind"].as_str().unwrap_or_default();
    assert!(
        kind.starts_with("complete_") || kind == "no_behavioral_candidates",
        "{label}: {kind}"
    );
    let limitations = analysis["outcome"]["limitations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !limitations.is_empty(),
        "{label}: scope note missing: {analysis}"
    );
    for limitation in &limitations {
        assert_eq!(
            limitation["kind"], "requested_scope_excluded",
            "{label}: {limitation}"
        );
    }
}

fn assert_quiet_missed_txt(dir: &Path, diff: &Path, extra: &[&str], label: &str) {
    let output = check(dir, diff, "missed-txt", extra);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "",
        "{label}: a fully analyzed scoped run is zero bytes, got {stdout:?}"
    );
}

#[test]
fn exclude_flag_is_requested_scope_and_stays_complete() -> TestResult {
    let (dir, diff) = fixture("scope-exclude", "src/gen/api.rs", None)?;
    let flags = ["--exclude", "src/gen/**"];
    let value = outcome(&dir, &diff, &flags)?;
    assert_complete_with_scope_note(&value, "exclude");
    assert_quiet_missed_txt(&dir, &diff, &flags, "exclude");
    let human = String::from_utf8_lossy(&check(&dir, &diff, "human", &flags).stdout).into_owned();
    assert!(human.contains("Scope note"), "{human}");
    assert!(!human.contains("language_scope_unsupported"), "{human}");
    assert!(!human.contains("Limitation:"), "{human}");
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn path_flag_is_requested_scope_and_stays_complete() -> TestResult {
    let (dir, diff) = fixture("scope-path", "crates/other/src/lib.rs", None)?;
    let flags = ["--path", "src"];
    let value = outcome(&dir, &diff, &flags)?;
    assert_complete_with_scope_note(&value, "path");
    assert_quiet_missed_txt(&dir, &diff, &flags, "path");
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn configured_generated_pattern_is_requested_scope_and_stays_complete() -> TestResult {
    let (dir, diff) = fixture(
        "scope-configured",
        "src/ffi_bindings.rs",
        Some("[languages.rust]\ngenerated_file_patterns = [\"src/ffi_bindings.rs\"]\n"),
    )?;
    let value = outcome(&dir, &diff, &[])?;
    assert_complete_with_scope_note(&value, "configured");
    assert_quiet_missed_txt(&dir, &diff, &[], "configured");
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

/// Every changed Rust file matches a configured pattern: nothing is left to
/// analyze and nothing was skipped unasked, so the run is complete with the
/// note and `missed-txt` is zero bytes (RIPR-SPEC-0309).
#[test]
fn every_changed_file_matching_a_configured_pattern_is_complete_and_quiet() -> TestResult {
    let (dir, diff) = fixture(
        "scope-all-configured",
        "src/ffi_bindings.rs",
        Some(
            "[languages.rust]\ngenerated_file_patterns = [\"src/lib.rs\", \"src/ffi_bindings.rs\"]\n",
        ),
    )?;
    let value = outcome(&dir, &diff, &[])?;
    assert_complete_with_scope_note(&value, "all-configured");
    assert_eq!(
        value["analysis_outcome"]["outcome"]["kind"], "no_behavioral_candidates",
        "{value}"
    );
    assert_quiet_missed_txt(&dir, &diff, &[], "all-configured");
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

/// Negative control: the same file excluded only by ripr's built-in
/// generated-name convention is not something the operator asked for, so the
/// run stays incomplete and `missed-txt` says so.
#[test]
fn built_in_generated_skip_stays_incomplete() -> TestResult {
    let (dir, diff) = fixture("scope-builtin", "src/gen/api.rs", None)?;
    let value = outcome(&dir, &diff, &[])?;
    let analysis = &value["analysis_outcome"];
    assert_eq!(analysis["analysis_complete"], false, "{analysis}");
    assert_eq!(
        analysis["outcome"]["kind"], "partial_with_limitations",
        "{analysis}"
    );
    assert!(
        analysis["outcome"]["limitations"]
            .as_array()
            .is_some_and(|items| items
                .iter()
                .any(|item| item["kind"] == "language_scope_unsupported")),
        "{analysis}"
    );
    let missed = check(&dir, &diff, "missed-txt", &[]);
    assert_success(&missed);
    let stdout = String::from_utf8_lossy(&missed.stdout);
    assert!(stdout.contains("partial_with_limitations"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

/// Negative control: a requested exclusion does not launder a file ripr
/// cannot analyze. The excluded file is a note; the unreadable kept file
/// keeps the outcome incomplete.
#[test]
fn requested_exclusion_does_not_hide_an_unanalyzed_kept_file() -> TestResult {
    let (dir, diff) = fixture("scope-mixed", "src/gen/api.rs", None)?;
    let mut patch = fs::read_to_string(&diff)?;
    patch.push_str(
        "diff --git a/src/absent.rs b/src/absent.rs\n--- a/src/absent.rs\n+++ b/src/absent.rs\n@@ -0,0 +1 @@\n+pub fn absent() -> u8 { 1 }\n",
    );
    fs::write(&diff, patch)?;
    let value = outcome(&dir, &diff, &["--exclude", "src/gen/**"])?;
    let analysis = &value["analysis_outcome"];
    assert_eq!(analysis["analysis_complete"], false, "{analysis}");
    assert_eq!(
        analysis["outcome"]["kind"], "partial_with_limitations",
        "{analysis}"
    );
    let kinds: Vec<&str> = analysis["outcome"]["limitations"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["kind"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        kinds.contains(&"changed_file_absent_from_worktree"),
        "{kinds:?}"
    );
    assert!(kinds.contains(&"requested_scope_excluded"), "{kinds:?}");
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}
