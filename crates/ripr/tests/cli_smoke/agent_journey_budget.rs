//! Whole-journey agent byte budget (#13673): the check -> explain ->
//! context -> verify loop an agent actually pays for, not one surface.
//!
//! `agent_output_budget` pins single outputs. This test pins the journey:
//! cold (first-run legend) and steady-state check bytes, one explain, one
//! context packet, and the final verify-bearing check, plus the exact
//! duplicate lines the agent reads twice. Bytes are UTF-8 stdout bytes
//! with the root replaced by `<root>`; the `bytes/4` column is a rough
//! reference estimate only — no tokenizer or provider billing is claimed.
//! Provider input/cached/output/reasoning tokens are host-recorded and
//! out of scope for this CLI test.
use super::{run_command, unique_external_workspace};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const BASE: &str = "pub fn can_vote(age: u32) -> bool { age > 18 }\n\
pub fn clamp(v: i32, lo: i32, hi: i32) -> i32 { if v < lo { lo } else if v > hi { hi } else { v } }\n\
pub fn discount(total: u32) -> u32 { if total > 100 { total - 10 } else { total } }\n\
pub fn parse(text: &str) -> Result<u32, String> { if text.len() > 5 { return Err(\"long\".into()); } text.parse::<u32>().map_err(|e| e.to_string()) }\n\
pub fn tier(n: u32) -> &'static str { if n >= 10 { \"gold\" } else { \"basic\" } }\n";

const TESTS: &str = "#[cfg(test)]\nmod tests {\n    use super::*;\n\
    #[test] fn vote() { assert!(can_vote(30)); }\n\
    #[test] fn clamp_ok() { assert_eq!(clamp(5, 0, 10), 5); }\n\
    #[test] fn disc() { assert!(discount(200) < 200); }\n\
    #[test] fn parses() { assert!(parse(\"12\").is_ok()); }\n\
    #[test] fn gold() { assert_eq!(tier(50), \"gold\"); }\n}\n";

fn workspace(label: &str) -> TestResult<std::path::PathBuf> {
    let project = unique_external_workspace(label)?;
    fs::create_dir_all(project.join("src"))?;
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let changed = BASE
        .replace("age > 18", "age >= 18")
        .replace("v > hi", "v >= hi")
        .replace("total > 100", "total >= 100")
        .replace("len() > 5", "len() >= 5")
        .replace("n >= 10", "n > 10");
    fs::write(project.join("src/lib.rs"), format!("{changed}{TESTS}"))?;
    let mut diff = String::from("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n");
    for (old, new) in BASE.lines().zip(changed.lines()) {
        if old == new {
            diff.push_str(&format!(" {old}\n"));
        } else {
            diff.push_str(&format!("-{old}\n+{new}\n"));
        }
    }
    fs::write(project.join("change.diff"), diff)?;
    Ok(project)
}

fn run(project: &Path, args: &[&str]) -> TestResult<String> {
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(project), args)?;
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = project.to_string_lossy().into_owned();
    Ok(String::from_utf8(output.stdout)?.replace(&root, "<root>"))
}

fn receipt(label: &str, text: &str, ceiling: usize) {
    eprintln!(
        "agent-journey-budget {label}: {} bytes (~{} ref-tokens at bytes/4, ceiling {ceiling})",
        text.len(),
        text.len() / 4
    );
    assert!(
        text.len() <= ceiling,
        "{label} grew to {} bytes, over its {ceiling}-byte journey budget:\n{text}",
        text.len()
    );
}

fn first_gap_id(check: &str) -> TestResult<String> {
    for line in check.lines() {
        if let Some(at) = line.find("|id=") {
            let id: String = line[at + 4..]
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '|')
                .collect();
            if !id.is_empty() {
                return Ok(id);
            }
        }
    }
    Err(format!("journey fixture must print a gap with |id=:\n{check}").into())
}

#[test]
fn agent_journey_stays_inside_byte_budget_and_keeps_verify_record() -> TestResult {
    let project = workspace("agent-journey-budget")?;
    let root = project.to_string_lossy().into_owned();

    // Cold: first run carries the one-time legend. Steady: budgeted rerun.
    let cold = run(
        &project,
        &[
            "check",
            "--quiet",
            "--diff",
            "change.diff",
            "--format",
            "agent",
        ],
    )?;
    let steady = run(
        &project,
        &[
            "check",
            "--quiet",
            "--diff",
            "change.diff",
            "--format",
            "agent",
        ],
    )?;
    let id = first_gap_id(&steady)?;
    let explained = run(
        &project,
        &[
            "explain",
            "--root",
            &root,
            "--diff",
            "change.diff",
            "--format",
            "agent",
            &id,
        ],
    )?;
    let context = run(
        &project,
        &[
            "context",
            "--root",
            &root,
            "--diff",
            "change.diff",
            "--at",
            &id,
        ],
    )?;
    // Final verify step reuses the same compact report (#13516).
    let verify = run(
        &project,
        &[
            "check",
            "--quiet",
            "--diff",
            "change.diff",
            "--format",
            "agent",
        ],
    )?;

    // Identity and completion authority survive every step.
    assert!(
        explained.contains(&id),
        "explain lost finding {id}:\n{explained}"
    );
    assert!(
        context.contains(&id),
        "context lost finding {id}:\n{context}"
    );
    let last = verify.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("verify=complete|") || last.starts_with("verify=refused|"),
        "final check must end with the #13516 verify= record, got: {last}"
    );

    // Ceilings sit just above the 2026-11-08 measured receipts on this
    // fixture (cold 1169, steady 1169, explain 577, context 4984,
    // verify 1169); cold equals steady here because the one-time legend
    // is a human-format surface, not an agent-format one.
    receipt("check cold (with legend)", &cold, 1_400);
    receipt("check steady", &steady, 1_400);
    receipt("explain --format agent", &explained, 800);
    receipt("context packet", &context, 6_000);
    receipt("final verify check", &verify, 1_400);

    // Redundancy the agent pays for twice: exact lines repeated across steps.
    let mut seen = BTreeSet::new();
    let mut dup_bytes = 0usize;
    for text in [&steady, &explained, &context, &verify] {
        for line in text.lines() {
            if line.len() >= 24 && !seen.insert(line.to_owned()) {
                dup_bytes += line.len() + 1;
            }
        }
    }
    let total: usize = [&steady, &explained, &context, &verify]
        .iter()
        .map(|t| t.len())
        .sum();
    eprintln!(
        "agent-journey-budget journey steady-total: {total} bytes, exact-duplicate lines >=24B: {dup_bytes} bytes"
    );
    assert!(
        dup_bytes <= 3_000,
        "journey repeats {dup_bytes} duplicate bytes over its 3000-byte redundancy budget"
    );
    Ok(())
}
