//! Agent-facing output byte budgets (token-footprint regression guard).
//!
//! One five-gap diff is analyzed through the surfaces an agent actually reads.
//! Each ceiling sits a little above the measured size, so redundancy added to
//! any of these outputs fails here instead of silently taxing every agent turn.
//! A deliberate cut lowers the ceiling in the same change; a deliberate
//! addition must justify raising it.
use super::{printed_path, run_command, unique_external_workspace};
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const BASE: &str = "pub fn can_vote(age: u32) -> bool { age > 18 }\n\
pub fn clamp(v: i32, lo: i32, hi: i32) -> i32 { if v < lo { lo } else if v > hi { hi } else { v } }\n\
pub fn discount(total: u32) -> u32 { if total > 100 { total - 10 } else { total } }\n\
pub fn parse(text: &str) -> Result<u32, String> { if text.len() > 5 { return Err(\"long\".into()); } text.parse::<u32>().map_err(|e| e.to_string()) }\n\
pub fn tier(n: u32) -> &'static str { if n >= 10 { \"gold\" } else { \"basic\" } }\n";

/// Same-file tests, unchanged by the diff: one loose test per function.
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
    // Measure with the absolute root replaced by a fixed token, so the
    // budget does not depend on how long the checkout path is.
    // `printed_path` is the spelling the binary prints (verbatim prefix folded,
    // `/` separators on Windows), so the measurement is host-path independent.
    let root = printed_path(project);
    Ok(String::from_utf8(output.stdout)?.replace(&root, "<root>"))
}

fn within(label: &str, text: &str, ceiling: usize) {
    eprintln!(
        "agent-output-budget {label}: {} bytes (ceiling {ceiling})",
        text.len()
    );
    assert!(
        text.len() <= ceiling,
        "{label} grew to {} bytes, over its {ceiling}-byte agent budget:\n{text}",
        text.len()
    );
}

#[test]
fn agent_facing_outputs_stay_inside_their_byte_budgets() -> TestResult {
    let project = workspace("agent-output-budget")?;
    // First run prints the one-time legend; budget the steady-state rerun.
    run(&project, &["check", "--diff", "change.diff"])?;
    let human = run(&project, &["check", "--diff", "change.diff"])?;
    let json = run(
        &project,
        &["check", "--diff", "change.diff", "--format", "json"],
    )?;
    let terse = run(
        &project,
        &["check", "--diff", "change.diff", "--format", "json-terse"],
    )?;
    let github = run(
        &project,
        &["check", "--diff", "change.diff", "--format", "github"],
    )?;
    // Shape behind the human budget (#13673): hidden rows that can start a
    // test carry only their id; one shared command template follows them.
    let hidden = human
        .split(
            "
Hidden:
",
        )
        .nth(1)
        .ok_or_else(|| {
            format!(
                "fixture must hide lower-priority findings:
{human}"
            )
        })?;
    let ids = hidden
        .lines()
        .filter(|line| line.trim_start().starts_with("Test id: "))
        .count();
    assert!(
        ids >= 1,
        "fixture must list a hidden row with a test id:
{human}"
    );
    assert_eq!(
        hidden.matches("--seam-id").count(),
        hidden.matches("--seam-id TEST_ID").count(),
        "hidden rows must not repeat a full stub command:
{human}"
    );
    assert_eq!(
        hidden
            .lines()
            .filter(|line| line.trim_start().starts_with("ripr agent stub"))
            .count(),
        1,
        "one shared stub command template:
{human}"
    );
    within("check (default human)", &human, 3_900);
    within("check --format json", &json, 88_000);
    within("check --format json-terse", &terse, 49_000);
    within("check --format github", &github, 3_300);
    Ok(())
}

/// #13673: the brief states the before/after snapshot and verify commands once
/// under `next`; per-seam `verification` carries only the suggested test.
#[test]
fn agent_brief_states_workflow_commands_once() -> TestResult {
    let project = workspace("agent-brief-budget")?;
    let brief = run(
        &project,
        &["agent", "brief", "--diff", "change.diff", "--json"],
    )?;
    for command in ["before_snapshot_command", "after_snapshot_command"] {
        assert_eq!(
            brief.matches(command).count(),
            1,
            "{command} must appear once, not per seam:\n{brief}"
        );
    }
    assert!(!brief.contains("\"verify_command\""), "{brief}");
    within("agent brief --json", &brief, 8_900);
    Ok(())
}

/// #13803: the human `agent repair --phase before` start prints the next
/// command once (stdout), keeps the fields an agent acts on (target, edit
/// boundary with its terminality, redirect stop condition), and stays inside
/// a line ceiling. Stderr repeats none of stdout's result lines.
#[test]
fn agent_repair_start_prints_one_next_command_inside_a_line_budget() -> TestResult {
    let root = super::repair_route_workspace("agent repair start budget")?;
    let root_arg = root.to_string_lossy().into_owned();
    let output = super::run_ripr_with_env(
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--seam-id",
            super::REPAIR_ROUTE_SEAM,
            "--phase",
            "before",
        ],
        &[("RIPR_PROGRESS", "1"), ("RIPR_QUIET", "0")],
    );
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let next = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Next, after the test edit: "))
        .ok_or_else(|| format!("stdout must carry the next command:\n{stdout}"))?;
    assert!(
        next.starts_with("ripr continue ") && next.contains("--attempt repair-attempt-"),
        "{next}"
    );
    let combined = format!("{stdout}{stderr}");
    assert_eq!(
        combined.matches(next).count(),
        1,
        "the next command must print exactly once across stdout and stderr:\n{combined}"
    );
    for field in [
        "Repair prepared for seam ",
        "edit one test file: tests/pricing.rs",
        "fails the repair attempt terminally",
        "assertion shape: ",
    ] {
        assert!(stdout.contains(field), "stdout lost `{field}`:\n{stdout}");
    }
    assert!(
        stderr.contains("write this command's output outside the repo"),
        "the redirect stop condition must stay:\n{stderr}"
    );
    for repeated in [
        "repair started. Next:",
        "is waiting for your test",
        "attempt manifest: ",
        "attempt next command",
    ] {
        assert!(
            !stderr.contains(repeated),
            "stderr must not repeat `{repeated}` from stdout:\n{stderr}"
        );
    }
    let lines = combined.lines().count();
    eprintln!("agent-output-budget agent repair start: {lines} lines (ceiling 15)");
    assert!(
        lines <= 15,
        "agent repair start grew to {lines} lines, over its 15-line budget:\n{combined}"
    );
    Ok(())
}
