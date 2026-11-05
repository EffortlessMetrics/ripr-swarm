//! #13103: run both presentations through the real CLI on the same owned
//! checkout. Equality of parsed values is necessary but not sufficient: each
//! case also requires the intended finding, omission, policy or refusal.

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Output;

const SOURCE: &str = r#"pub fn discounted_total(amount: i32, threshold: i32) -> i32 {
    if amount >= threshold {
        amount - 10
    } else {
        amount
    }
}

pub fn can_ship(weight: i32, limit: i32) -> bool {
    weight <= limit
}
"#;

const TESTS: &str = r#"use compact_json_fixture::{can_ship, discounted_total};

#[test]
fn below_threshold_has_no_discount() {
    assert_eq!(discounted_total(50, 100), 50);
}

#[test]
fn far_above_threshold_discounts() {
    assert_eq!(discounted_total(10_000, 100), 9_990);
}

#[test]
fn ordinary_weight_can_ship() {
    assert!(can_ship(5, 10));
}
"#;

struct Fixture {
    home: PathBuf,
    root: PathBuf,
    decoy: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "ripr-json-compact-{label}-{}-{stamp}",
            std::process::id()
        ));
        // Claim the directory exclusively before constructing its cleanup
        // owner. A collision must never remove another test's existing tree.
        std::fs::create_dir(&home)
            .map_err(|error| format!("create owned fixture {}: {error}", home.display()))?;
        let fixture = Self {
            root: home.join("repo with spaces"),
            decoy: home.join("foreign cwd"),
            home,
        };
        std::fs::create_dir_all(fixture.root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(fixture.root.join("tests")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&fixture.decoy).map_err(|e| e.to_string())?;
        fixture.write(
            "Cargo.toml",
            "[package]\nname = \"compact_json_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )?;
        fixture.write(".gitignore", "/target/\n")?;
        fixture.write("src/lib.rs", &SOURCE.replace(">=", ">").replace("<=", "<"))?;
        fixture.write("tests/pricing.rs", TESTS)?;
        fixture_git_ok(&fixture.root, &["init", "-b", "main"])?;
        fixture_git_ok(&fixture.root, &["config", "core.autocrlf", "false"])?;
        fixture_git_ok(&fixture.root, &["add", "."])?;
        fixture_git_ok(
            &fixture.root,
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "--no-gpg-sign",
                "-qm",
                "base",
            ],
        )?;
        fixture.write("src/lib.rs", SOURCE)?;
        Ok(fixture)
    }

    fn write(&self, relative: &str, text: &str) -> Result<(), String> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn check(&self, format: &str, extra: &[&str], budget: Option<&str>) -> Result<Output, String> {
        let root = self.root.to_string_lossy();
        let mut args = vec![
            "check",
            "--root",
            root.as_ref(),
            "--base",
            "HEAD",
            "--worktree",
            "--quiet",
            "--timeout",
            "30",
            "--format",
            format,
        ];
        args.extend_from_slice(extra);
        run_cli(&self.decoy, &args, budget)
    }

    fn pair(
        &self,
        extra: &[&str],
        budget: Option<&str>,
        expected_exit: i32,
    ) -> Result<(Output, Output, Value), String> {
        let pretty = self.check("json", extra, budget)?;
        let compact = self.check("json-compact", extra, budget)?;
        let value = equivalent(&pretty, &compact, expected_exit)?;
        Ok((pretty, compact, value))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn run_cli(cwd: &Path, args: &[&str], budget: Option<&str>) -> Result<Output, String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .current_dir(cwd)
        .args(args)
        .env_remove("RIPR_CHECK_FINDINGS_BYTES")
        .env_remove("RIPR_CHECK_TIMEOUT")
        .env_remove("RIPR_GIT_TIMEOUT")
        .env_remove("RIPR_PROGRESS")
        .env_remove("RIPR_CACHE_DIR");
    if let Some(budget) = budget {
        command.env("RIPR_CHECK_FINDINGS_BYTES", budget);
    }
    command
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

fn equivalent(pretty: &Output, compact: &Output, expected_exit: i32) -> Result<Value, String> {
    if pretty.status.code() != Some(expected_exit) || compact.status.code() != Some(expected_exit) {
        return Err(format!(
            "expected exit {expected_exit}; pretty={:?} ({}) compact={:?} ({})",
            pretty.status.code(),
            String::from_utf8_lossy(&pretty.stderr),
            compact.status.code(),
            String::from_utf8_lossy(&compact.stderr)
        ));
    }
    let expected: Value = serde_json::from_slice(&pretty.stdout)
        .map_err(|error| format!("legacy stdout must be one JSON document: {error}"))?;
    let actual: Value = serde_json::from_slice(&compact.stdout)
        .map_err(|error| format!("compact stdout must be one JSON document: {error}"))?;
    if expected != actual || pretty.stderr != compact.stderr {
        return Err("presentation changed report values or stderr".to_string());
    }
    let text = std::str::from_utf8(&compact.stdout).map_err(|error| error.to_string())?;
    if !text.ends_with('\n') || text.lines().count() != 1 {
        return Err("compact stdout must be one JSON line with a final newline".to_string());
    }
    if compact.stdout.len() >= pretty.stdout.len() {
        return Err("formatted stdout must become smaller without losing its document".to_string());
    }
    Ok(actual)
}

fn findings(value: &Value) -> Result<&Vec<Value>, String> {
    value["findings"]
        .as_array()
        .ok_or_else(|| "missing findings array".to_string())
}

fn nonempty_analysis(value: &Value) -> Result<(), String> {
    if findings(value)?.len() < 2
        || value["summary"]["findings"]
            .as_u64()
            .is_none_or(|count| count < 2)
        || value["analysis_outcome"]["analysis_complete"] != true
        || value["canonical_next_action"].is_null()
    {
        return Err(
            "fixture must produce at least two findings, complete analysis and a canonical action"
                .to_string(),
        );
    }
    Ok(())
}

#[test]
fn json_compact_cli_live_report_preserves_evidence_and_measures_bytes() -> Result<(), String> {
    let fixture = Fixture::new("live")?;
    let (pretty, compact, value) = fixture.pair(&[], None, 0)?;
    nonempty_analysis(&value)?;
    if value["head"]["source"] != "working_tree" {
        return Err(format!(
            "fixture must retain its actual source identity: {}",
            value["head"]
        ));
    }
    let repeated = fixture.check("json", &[], None)?;
    if repeated.status.code() != Some(0) || repeated.stdout != pretty.stdout {
        return Err("selecting compact output must not change later legacy JSON bytes".to_string());
    }
    // Native execution witness, not a tokenizer benchmark or an efficacy claim.
    eprintln!(
        "json-compact live comparison: findings={} pretty_bytes={} compact_bytes={} pretty_sha256={:x} compact_sha256={:x} source_sha256={:x}",
        findings(&value)?.len(),
        pretty.stdout.len(),
        compact.stdout.len(),
        Sha256::digest(&pretty.stdout),
        Sha256::digest(&compact.stdout),
        Sha256::digest(SOURCE.as_bytes())
    );
    Ok(())
}

#[test]
fn json_compact_cli_preserves_real_omissions_and_full_counts() -> Result<(), String> {
    let fixture = Fixture::new("bounded")?;
    let (_, _, complete) = fixture.pair(&[], Some("0"), 0)?;
    nonempty_analysis(&complete)?;
    let (_, _, bounded) = fixture.pair(&[], Some("1"), 0)?;
    let retained = findings(&bounded)?.len();
    if retained == 0
        || retained >= findings(&complete)?.len()
        || bounded["summary"] != complete["summary"]
        || bounded["analysis_outcome"]["document_complete"] != false
        || bounded["canonical_next_action"] != complete["canonical_next_action"]
    {
        return Err(
            "bounded output must retain a genuine subset, full counts and the same primary action"
                .to_string(),
        );
    }
    let limits = bounded["run_limitations"]
        .as_array()
        .ok_or("missing budget disclosure")?;
    if !limits
        .iter()
        .any(|limit| limit["category"] == "limited_findings_bound")
    {
        return Err("compact output must retain the findings-bound limitation".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_cli_applies_root_relative_policy_and_keeps_implicit_policy_notice()
-> Result<(), String> {
    let fixture = Fixture::new("policy")?;
    let (_, _, original) = fixture.pair(&[], None, 0)?;
    nonempty_analysis(&original)?;
    let first = findings(&original)?.first().ok_or("missing policy subject")?;
    let id = first["id"].as_str().ok_or("finding must have an id")?;
    let quoted_id = toml::Value::String(id.to_string()).to_string();
    fixture.write(
        ".ripr/suppressions.toml",
        &format!(
            "schema_version = 1\n[[suppressions]]\nkind = \"exposure_gap\"\nfinding_id = {quoted_id}\nowner = \"compact-json-test\"\nreason = \"reviewed fixture exception\"\n"
        ),
    )?;
    let (implicit, _, unchanged) = fixture.pair(&[], None, 0)?;
    if unchanged != original
        || !String::from_utf8_lossy(&implicit.stderr).contains("it was not applied to this check")
    {
        return Err("an implicit policy must be disclosed, not silently applied".to_string());
    }
    let (applied, _, filtered) = fixture.pair(
        &["--suppression-policy", ".ripr/suppressions.toml"],
        None,
        0,
    )?;
    if filtered["suppression_policy"]["suppressed"] != 1
        || findings(&filtered)?.len() != findings(&original)?.len()
        || String::from_utf8_lossy(&applied.stderr).contains("it was not applied")
    {
        return Err(
            "explicit policy must suppress exactly its real finding without dropping evidence"
                .to_string(),
        );
    }
    let subject = findings(&filtered)?
        .iter()
        .find(|finding| finding["id"] == id)
        .ok_or("suppressed subject must remain visible")?;
    if subject["suppressed"] != true {
        return Err("the policy subject must be marked suppressed".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_cli_preserves_policy_and_invalid_budget_refusals() -> Result<(), String> {
    let fixture = Fixture::new("failure")?;
    for policy in ["missing.toml", "malformed.toml"] {
        if policy == "malformed.toml" {
            fixture.write(policy, "this is not a suppression policy\n")?;
        }
        let (_, _, value) = fixture.pair(&["--suppression-policy", policy], None, 2)?;
        if value["analysis_scope"]["downstream_consumable"] != false
            || value["analysis_scope"]["run_status"] != "suppression_policy_invalid"
        {
            return Err(format!("{policy} must retain its typed refusal, got {value}"));
        }
    }
    let (pretty, _, value) = fixture.pair(&[], Some("not-a-budget"), 2)?;
    if value["analysis_scope"]["downstream_consumable"] != false
        || !String::from_utf8_lossy(&pretty.stderr).contains("RIPR_CHECK_FINDINGS_BYTES")
    {
        return Err("an invalid output budget must not become a successful report".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_cli_preserves_structured_refusal_exit_and_recovery() -> Result<(), String> {
    let fixture = Fixture::new("missing-root")?;
    let missing = fixture.home.join("does not exist");
    let root = missing.to_string_lossy();
    let pretty = run_cli(
        &fixture.decoy,
        &["check", "--root", &root, "--format", "json"],
        None,
    )?;
    let compact = run_cli(
        &fixture.decoy,
        &["check", "--root", &root, "--format", "json-compact"],
        None,
    )?;
    let value = equivalent(&pretty, &compact, 2)?;
    if value["analysis_scope"]["run_status"] != "repository_root_unusable"
        || value["analysis_scope"]["downstream_consumable"] != false
    {
        return Err("missing root must remain a non-consumable typed refusal".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_cli_rejects_conflicting_selection_before_analysis() -> Result<(), String> {
    let fixture = Fixture::new("argv")?;
    for argv in [
        ["check", "--json", "--format", "json-compact"],
        ["check", "--format", "json-compact", "--json"],
    ] {
        let output = run_cli(&fixture.decoy, &argv, None)?;
        if output.status.code() != Some(2)
            || !output.stdout.is_empty()
            || !String::from_utf8_lossy(&output.stderr)
                .contains("select different output formats; pass one")
        {
            return Err("argv conflicts must remain prose-only usage failures".to_string());
        }
    }
    let repeated = fixture.check("json-compact", &["--format", "json-compact"], None)?;
    let ordinary = fixture.check("json-compact", &[], None)?;
    if repeated.status.code() != Some(0) || repeated.stdout != ordinary.stdout {
        return Err("repeating the same compact selection must be harmless".to_string());
    }
    Ok(())
}
