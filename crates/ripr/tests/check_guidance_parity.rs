//! Check-versus-guidance parity for a changed fn no test calls (#13892).
//!
//! `ripr check --format missed-txt` lists every changed-line actionable gap.
//! `ripr review-comments` must carry each of those gaps to PR guidance, as a
//! card or as a `suppressed` entry with a named reason, so `ripr gate
//! evaluate --pr-guidance` evaluates it. Before #13892 an uncalled owner whose
//! boundary operand is computed (`cents % 100 > 49`) was listed by `check`
//! but reached guidance only as free warning text, and the gate never saw it.

#![cfg(feature = "lang-rust")]

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const COMMENTS_JSON: &str = "target/ripr/review/comments.json";
const GATE_JSON: &str = "target/ripr/reports/gate-decision.json";
const GATE_MD: &str = "target/ripr/reports/gate-decision.md";

/// Base source: one tested boundary and one owner no test calls.
const BASE_SOURCE: &str = r#"pub fn discount_percent(amount: u32) -> u32 {
    if amount >= 100 { 15 } else { 0 }
}

pub fn uncalled_rounding(cents: u32) -> u32 {
    if cents % 100 > 50 { cents / 100 + 1 } else { cents / 100 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discount_boundary() {
        assert_eq!(discount_percent(100), 15);
        assert_eq!(discount_percent(99), 0);
    }
}
"#;

/// Line of the `uncalled_rounding` predicate in [`BASE_SOURCE`].
const UNCALLED_LINE: u64 = 6;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unique_root(label: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    Ok(std::env::temp_dir().join(format!(
        "ripr-guidance-parity-{label}-{stamp}-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    )))
}

fn commit(root: &Path, message: &str) -> Result<(), String> {
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
        ],
    )
}

/// Temp git repo whose HEAD commit replaces `from` with `to` in `src/lib.rs`.
fn build_repo(label: &str, from: &str, to: &str) -> Result<Fixture, String> {
    let root = unique_root(label)?;
    std::fs::create_dir_all(root.join("src"))
        .map_err(|error| format!("create {}: {error}", root.display()))?;
    let fixture = Fixture(root);
    let root = fixture.0.as_path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"guidance-parity\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| format!("write Cargo.toml: {error}"))?;
    std::fs::write(root.join("src/lib.rs"), BASE_SOURCE)
        .map_err(|error| format!("write src/lib.rs: {error}"))?;
    fixture_git_ok(root, &["init", "-q"])?;
    fixture_git_ok(root, &["config", "core.autocrlf", "false"])?;
    fixture_git_ok(root, &["add", "Cargo.toml", "src/lib.rs"])?;
    commit(root, "base")?;
    let changed = BASE_SOURCE.replacen(from, to, 1);
    if changed == BASE_SOURCE {
        return Err(format!("fixture change {from:?} -> {to:?} altered nothing"));
    }
    std::fs::write(root.join("src/lib.rs"), changed)
        .map_err(|error| format!("write changed src/lib.rs: {error}"))?;
    fixture_git_ok(root, &["add", "src/lib.rs"])?;
    commit(root, "change")?;
    Ok(fixture)
}

fn run_ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?} in {}: {error}", root.display()))
}

fn diagnose(label: &str, output: &Output) -> String {
    format!(
        "{label} exited {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("parse {}: {error}\n{text}", path.display()))
}

/// `(file, line)` of every gap line `check --format missed-txt` lists.
fn missed_gaps(root: &Path) -> Result<Vec<(String, u64)>, String> {
    let output = run_ripr(
        root,
        &[
            "check",
            "--root",
            ".",
            "--base",
            "HEAD~1",
            "--format",
            "missed-txt",
        ],
    )?;
    if !output.status.success() {
        return Err(diagnose("check --format missed-txt", &output));
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let mut gaps = Vec::new();
    for line in text.lines().filter(|line| !line.starts_with("# ")) {
        let mut parts = line.splitn(3, ':');
        let (Some(file), Some(number)) = (parts.next(), parts.next()) else {
            return Err(format!("unparsable missed-txt line {line:?}"));
        };
        let number = number
            .parse::<u64>()
            .map_err(|error| format!("missed-txt line number in {line:?}: {error}"))?;
        gaps.push((file.to_string(), number));
    }
    Ok(gaps)
}

fn review_comments(root: &Path) -> Result<Value, String> {
    let output = run_ripr(
        root,
        &[
            "review-comments",
            "--root",
            ".",
            "--base",
            "HEAD~1",
            "--head",
            "HEAD",
            "--out",
            COMMENTS_JSON,
        ],
    )?;
    if !output.status.success() {
        return Err(diagnose("review-comments", &output));
    }
    read_json(&root.join(COMMENTS_JSON))
}

/// Every `(file, line)` guidance carries: cards by placement or seam
/// location, suppressed entries by their own location.
fn guidance_locations(comments: &Value) -> Vec<(String, u64)> {
    let mut locations = Vec::new();
    for lane in ["comments", "summary_only"] {
        for card in comments[lane].as_array().into_iter().flatten() {
            for (file, line) in [
                (&card["placement"]["path"], &card["placement"]["line"]),
                (&card["seam"]["file"], &card["seam"]["line"]),
            ] {
                if let (Some(file), Some(line)) = (file.as_str(), line.as_u64()) {
                    locations.push((file.to_string(), line));
                }
            }
        }
    }
    for item in comments["suppressed"].as_array().into_iter().flatten() {
        if let (Some(file), Some(line)) = (item["file"].as_str(), item["line"].as_u64()) {
            locations.push((file.to_string(), line));
        }
    }
    locations
}

fn evaluate_gate(root: &Path) -> Result<(Output, Value), String> {
    let output = run_ripr(
        root,
        &[
            "gate",
            "evaluate",
            "--root",
            ".",
            "--pr-guidance",
            COMMENTS_JSON,
            "--mode",
            "acknowledgeable",
            "--out",
            GATE_JSON,
        ],
    )?;
    let gate = read_json(&root.join(GATE_JSON))
        .map_err(|error| format!("{error}\n{}", diagnose("gate evaluate", &output)))?;
    Ok((output, gate))
}

#[test]
fn every_check_listed_gap_on_an_uncalled_changed_fn_reaches_guidance() -> Result<(), String> {
    let fixture = build_repo("uncalled", "cents % 100 > 50", "cents % 100 > 49")?;
    let gaps = missed_gaps(&fixture.0)?;
    // Fixture construction: check must list the uncalled owner's predicate,
    // or the parity claim has no subject.
    let uncalled = ("src/lib.rs".to_string(), UNCALLED_LINE);
    if !gaps.contains(&uncalled) {
        return Err(format!(
            "check --format missed-txt did not list the uncalled predicate at src/lib.rs:{UNCALLED_LINE}: {gaps:?}"
        ));
    }

    let comments = review_comments(&fixture.0)?;
    let locations = guidance_locations(&comments);
    let missing: Vec<_> = gaps.iter().filter(|gap| !locations.contains(gap)).collect();
    if !missing.is_empty() {
        return Err(format!(
            "check-listed gaps absent from review guidance: {missing:?}\ncomments={comments:#}"
        ));
    }

    // The withheld seam is disclosed with a named reason, never silently.
    let withheld = comments["suppressed"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["file"] == "src/lib.rs" && item["line"] == UNCALLED_LINE)
        .ok_or_else(|| format!("uncalled predicate is not itemized in suppressed: {comments:#}"))?;
    if withheld["reason"] != "static_limitation" {
        return Err(format!("withheld seam must name its reason: {withheld}"));
    }
    let message = withheld["message"].as_str().unwrap_or_default();
    if !message.contains("activation_boundary_input_unresolved")
        || !message.contains("not evidence")
    {
        return Err(format!(
            "withheld message must name the limitation and its absence semantics: {message}"
        ));
    }
    if comments["summary"]["suppressed"].as_u64()
        != comments["suppressed"]
            .as_array()
            .map(|items| items.len() as u64)
    {
        return Err(format!(
            "summary.suppressed disagrees with suppressed[]: {comments:#}"
        ));
    }
    Ok(())
}

#[test]
fn gate_evaluates_the_withheld_uncalled_gap_with_its_named_reason() -> Result<(), String> {
    let fixture = build_repo("gate", "cents % 100 > 50", "cents % 100 > 49")?;
    let comments = review_comments(&fixture.0)?;
    let seam_id = comments["suppressed"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["file"] == "src/lib.rs" && item["line"] == UNCALLED_LINE)
        .and_then(|item| item["seam_id"].as_str())
        .ok_or_else(|| format!("no suppressed entry for the uncalled predicate: {comments:#}"))?
        .to_string();

    let (output, gate) = evaluate_gate(&fixture.0)?;
    let decision = gate["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|decision| decision["source_id"] == seam_id.as_str())
        .ok_or_else(|| {
            format!(
                "gate did not evaluate the uncalled predicate {seam_id}: {gate:#}\n{}",
                diagnose("gate evaluate", &output)
            )
        })?;
    // A static limitation is disclosed, never a blocking repair claim.
    if decision["decision"] != "suppressed" {
        return Err(format!(
            "withheld seam must evaluate as suppressed: {decision:#}"
        ));
    }
    let reason = decision["gate_reason"].to_string();
    if !reason.contains("static_limitation") {
        return Err(format!(
            "gate reason must name static_limitation: {decision:#}"
        ));
    }
    if output.status.code() != Some(0) {
        return Err(format!(
            "a gate with only a disclosed static limitation must pass: {gate:#}\n{}",
            diagnose("gate evaluate", &output)
        ));
    }
    let markdown = std::fs::read_to_string(fixture.0.join(GATE_MD))
        .map_err(|error| format!("read {GATE_MD}: {error}"))?;
    if !markdown.contains("static_limitation") {
        return Err(format!(
            "gate-decision.md does not disclose the withheld seam:\n{markdown}"
        ));
    }
    Ok(())
}

#[test]
fn well_tested_change_lists_nothing_and_the_gate_passes() -> Result<(), String> {
    // Negative control: the changed boundary keeps both sides pinned by
    // exact assertions, so nothing is listed, withheld, or blocking.
    let fixture = build_repo("tested", "amount >= 100", "amount > 99")?;
    let gaps = missed_gaps(&fixture.0)?;
    if !gaps.is_empty() {
        return Err(format!("well-tested change listed gaps: {gaps:?}"));
    }
    let comments = review_comments(&fixture.0)?;
    let withheld: Vec<_> = comments["suppressed"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["reason"] == "static_limitation")
        .collect();
    if !withheld.is_empty() {
        return Err(format!("well-tested change withheld seams: {withheld:?}"));
    }
    let (output, gate) = evaluate_gate(&fixture.0)?;
    if output.status.code() != Some(0) || gate["status"] == "blocked" {
        return Err(format!(
            "well-tested change must not block: {gate:#}\n{}",
            diagnose("gate evaluate", &output)
        ));
    }
    Ok(())
}
