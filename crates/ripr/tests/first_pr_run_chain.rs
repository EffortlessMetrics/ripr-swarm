//! #11720: `ripr first-pr --run-chain` is the opt-in that runs the
//! missing-artifact chain in one invocation. The default (#11298) stays
//! analysis-free: one printed next command and nothing run.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};

const BASE_SRC: &str = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn adds() {\n        assert_eq!(add(1, 2), 3);\n    }\n}\n";
const CHANGED_SRC: &str = "pub fn add(a: i32, b: i32) -> i32 {\n    if a > 10 { a } else { a + b }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn adds() {\n        assert_eq!(add(1, 2), 3);\n    }\n}\n";

const DEFAULT_DISCLOSURE: &str = "ripr first-pr - side effects and cost disclosure\n  cost class:      artifact composition only; runs no analysis\n  writes to:       target/ripr/reports/\n  cache location:  none\n  git reads:       yes (base and head preflight)\n  network:         none\n  runtime hint:    seconds\n\n";

fn run(dir: &Path, program: &str, args: &[&str]) -> Result<Output, String> {
    Command::new(program)
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|err| format!("{program} {args:?}: {err}"))
}

fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let mut full = vec![
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@example.com",
        "-c",
        "commit.gpgSign=false",
    ];
    full.extend_from_slice(args);
    let output = run(dir, "git", &full)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn ripr(dir: &Path, args: &[&str]) -> Result<Output, String> {
    run(dir, env!("CARGO_BIN_EXE_ripr"), args)
}

/// A fresh Rust repository: `main` holds the base, `feature` holds one
/// committed change. No ripr artifact exists under `target/`.
fn fresh_repo(label: &str) -> Result<PathBuf, String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-11720-{label}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|err| err.to_string())?;
    fs::write(root.join(".gitignore"), "target\n").map_err(|err| err.to_string())?;
    fs::write(root.join("src/lib.rs"), BASE_SRC).map_err(|err| err.to_string())?;
    git(&root, &["init", "-q", "-b", "main"])?;
    git(&root, &["add", "-A"])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    git(&root, &["checkout", "-q", "-b", "feature"])?;
    fs::write(root.join("src/lib.rs"), CHANGED_SRC).map_err(|err| err.to_string())?;
    git(&root, &["commit", "-q", "-am", "change"])?;
    Ok(root)
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn packet(root: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(root.join("target/ripr/reports/start-here.json"))
        .map_err(|err| format!("start-here.json: {err}"))?;
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

#[test]
fn run_chain_takes_a_fresh_repo_to_a_non_missing_state_in_one_invocation() -> Result<(), String> {
    let root = fresh_repo("positive")?;
    let output = ripr(&root, &["first-pr", "--base", "main", "--run-chain"])?;
    let stdout = stdout_of(&output);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("run-chain step 1/3: repo exposure report:"),
        "{stdout}"
    );
    assert!(
        stdout.contains("run-chain step 2/3: gap ledger:"),
        "{stdout}"
    );
    assert!(
        stdout.contains("run-chain step 3/3: review cards: ripr "),
        "{stdout}"
    );
    // The disclosure is honest about the cost under the flag.
    assert!(
        stdout.contains("runs analysis for each missing artifact"),
        "{stdout}"
    );
    assert!(!stdout.contains("runs no analysis"), "{stdout}");
    assert!(stdout.contains("network:         none"), "{stdout}");
    // Every chain artifact was produced and the packet is a real state.
    for artifact in [
        "target/ripr/reports/repo-exposure.json",
        "target/ripr/reports/gap-decision-ledger.json",
        "target/ripr/review/comments.json",
    ] {
        assert!(root.join(artifact).is_file(), "missing {artifact}");
    }
    let packet = packet(&root)?;
    let state = packet["selected"]["state"].as_str().unwrap_or_default();
    assert_ne!(state, "missing_artifact", "{packet}");
    assert_ne!(
        packet["selected"]["output_state"], "missing_artifacts",
        "{packet}"
    );
    assert!(
        stdout.contains("State: top gap"),
        "the final composition reached a verdict state: {stdout}"
    );
    assert!(packet["selected"].get("also_missing").is_none(), "{packet}");
    // Scope: the review cards were rendered for the resolved base and head.
    let cards = fs::read_to_string(root.join("target/ripr/review/comments.json"))
        .map_err(|err| err.to_string())?;
    let cards: Value = serde_json::from_str(&cards).map_err(|err| err.to_string())?;
    assert_eq!(cards["base"], "main", "{cards}");
    assert_eq!(cards["head"], "HEAD", "{cards}");
    // A second run finds nothing missing and runs nothing.
    let rerun = ripr(&root, &["first-pr", "--base", "main", "--run-chain"])?;
    assert!(rerun.status.success());
    let rerun_stdout = stdout_of(&rerun);
    assert!(
        rerun_stdout.contains("run-chain: nothing to run"),
        "{rerun_stdout}"
    );
    assert!(!rerun_stdout.contains("run-chain step"), "{rerun_stdout}");
    Ok(())
}

#[test]
fn run_chain_steps_are_exactly_the_commands_the_default_prints() -> Result<(), String> {
    let root = fresh_repo("same-commands")?;
    // Default run: prints one command and records the later ones.
    let default = ripr(&root, &["first-pr", "--base", "main"])?;
    assert!(default.status.success());
    let default_packet = packet(&root)?;
    let first = default_packet["selected"]["regeneration_command"]
        .as_str()
        .ok_or("default packet names no regeneration command")?
        .to_string();
    let later = default_packet["selected"]["also_missing"]
        .as_array()
        .ok_or("default packet names no later steps")?
        .iter()
        .map(|entry| {
            entry["regeneration_command"]
                .as_str()
                .map(str::to_string)
                .ok_or("later step without a command")
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(later.len(), 2, "{default_packet}");
    fs::remove_dir_all(root.join("target")).map_err(|err| err.to_string())?;

    let chain = ripr(&root, &["first-pr", "--base", "main", "--run-chain"])?;
    assert!(chain.status.success());
    let stdout = stdout_of(&chain);
    let steps = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("run-chain step "))
        .map(|line| line.splitn(3, ": ").nth(2).unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    assert_eq!(steps, vec![first, later[0].clone(), later[1].clone()]);
    Ok(())
}

#[test]
fn a_failing_step_stops_the_chain_and_writes_no_packet() -> Result<(), String> {
    let root = fresh_repo("negative")?;
    // The gap ledger's Markdown companion path is a directory: step 2 fails
    // after step 1 succeeded, and the cards step must never run.
    fs::create_dir_all(root.join("target/ripr/reports/gap-decision-ledger.md"))
        .map_err(|err| err.to_string())?;
    let output = ripr(&root, &["first-pr", "--base", "main", "--run-chain"])?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("first-pr --run-chain stopped at step 2/3 (gap ledger)"),
        "{stderr}"
    );
    // The reason is the step's own last diagnostic, never a stray "{".
    assert!(stderr.contains("failed (exit "), "{stderr}");
    assert!(!stderr.contains("failed (exit 2); {"), "{stderr}");
    assert!(
        stderr.contains("no start-here packet was written"),
        "{stderr}"
    );
    assert!(
        root.join("target/ripr/reports/repo-exposure.json")
            .is_file()
    );
    assert!(!root.join("target/ripr/review/comments.json").exists());
    assert!(!root.join("target/ripr/reports/start-here.json").exists());
    assert!(!root.join("target/ripr/reports/start-here.md").exists());
    Ok(())
}

#[test]
fn a_failing_first_step_runs_nothing_after_it() -> Result<(), String> {
    let root = fresh_repo("negative-first")?;
    // The repo-exposure report cannot be written: `reports` is a file.
    fs::create_dir_all(root.join("target/ripr")).map_err(|err| err.to_string())?;
    fs::write(root.join("target/ripr/reports"), "not a directory")
        .map_err(|err| err.to_string())?;
    let output = ripr(&root, &["first-pr", "--base", "main", "--run-chain"])?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("stopped at step 1/3 (repo exposure report)"),
        "{stderr}"
    );
    assert!(!root.join("target/ripr/review/comments.json").exists());
    assert!(!root.join("target/ripr/reports/repo-exposure.json").exists());
    Ok(())
}

#[test]
fn default_first_pr_stays_analysis_free_with_an_unchanged_disclosure() -> Result<(), String> {
    let root = fresh_repo("default")?;
    let output = ripr(&root, &["first-pr", "--base", "main"])?;
    assert!(output.status.success());
    let stdout = stdout_of(&output);
    assert!(stdout.starts_with(DEFAULT_DISCLOSURE), "{stdout}");
    assert!(!stdout.contains("run-chain"), "{stdout}");
    // Nothing was analyzed: no chain artifact exists, only the packet.
    assert!(!root.join("target/ripr/reports/repo-exposure.json").exists());
    assert!(
        !root
            .join("target/ripr/reports/gap-decision-ledger.json")
            .exists()
    );
    assert!(!root.join("target/ripr/review/comments.json").exists());
    let packet = packet(&root)?;
    assert_eq!(packet["selected"]["state"], "missing_artifact");
    Ok(())
}

#[test]
fn run_chain_is_refused_with_check_and_with_check_output() -> Result<(), String> {
    let root = fresh_repo("conflicts")?;
    let with_check = ripr(&root, &["first-pr", "--run-chain", "--check"])?;
    assert_eq!(with_check.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&with_check.stderr)
            .contains("--run-chain cannot be combined with --check"),
    );
    let with_output = ripr(
        &root,
        &["first-pr", "--run-chain", "--check-output", "check.json"],
    )?;
    assert_eq!(with_output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&with_output.stderr)
            .contains("--run-chain cannot be combined with --check-output"),
    );
    assert!(!root.join("target").exists());
    Ok(())
}

#[test]
fn run_chain_works_for_a_root_that_is_not_the_cwd_and_has_a_space() -> Result<(), String> {
    let root = fresh_repo("sp ace")?;
    let elsewhere = std::env::temp_dir();
    assert_ne!(elsewhere, root);
    let root_arg = root.to_string_lossy().to_string();
    let output = ripr(
        &elsewhere,
        &[
            "first-pr",
            "--root",
            &root_arg,
            "--base",
            "main",
            "--run-chain",
        ],
    )?;
    let stdout = stdout_of(&output);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("run-chain step 1/"), "{stdout}");
    for artifact in [
        "target/ripr/reports/repo-exposure.json",
        "target/ripr/reports/gap-decision-ledger.json",
        "target/ripr/reports/start-here.json",
    ] {
        assert!(
            root.join(artifact).is_file(),
            "missing {artifact}: {stdout}"
        );
    }
    // Nothing leaked into the cwd.
    assert!(!elsewhere.join("target/ripr").exists());
    Ok(())
}

#[test]
fn a_ledger_with_a_repair_start_skips_the_cards_and_ends_in_a_top_gap() -> Result<(), String> {
    let root = fresh_repo("repair-start")?;
    let ledger = root.join("target/ripr/reports/gap-decision-ledger.json");
    fs::create_dir_all(ledger.parent().ok_or("no parent")?).map_err(|err| err.to_string())?;
    let record = r#"{"schema_version":"0.1","kind":"gap_decision_ledger","records":[{
        "gap_id":"gap:pr:demo:add","canonical_gap_id":"gap:rust:demo:add",
        "kind":"MissingBoundaryAssertion","language":"rust","language_status":"stable",
        "scope":"pr_local","gap_state":"actionable","policy_state":"new","repairability":"repairable",
        "changed_behavior":"a > 10",
        "anchor":{"file":"src/lib.rs","line":2,"owner":"demo::add","dedupe_fingerprint":"gap:rust:demo:add"},
        "repair_route":{"route_kind":"AddBoundaryAssertion","target_file":"src/lib.rs","assertion_shape":"assert_eq!(add(11, 1), 11)"},
        "verification_commands":["cargo test"]}]}"#;
    fs::write(&ledger, record).map_err(|err| err.to_string())?;
    let output = ripr(&root, &["first-pr", "--base", "main", "--run-chain"])?;
    let stdout = stdout_of(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("run-chain step 1/1: review cards: not run;"),
        "{stdout}"
    );
    assert!(!root.join("target/ripr/review/comments.json").exists());
    assert!(stdout.contains("State: top gap"), "{stdout}");
    let packet = packet(&root)?;
    assert_eq!(packet["selected"]["gap_id"], "gap:pr:demo:add", "{packet}");
    Ok(())
}
