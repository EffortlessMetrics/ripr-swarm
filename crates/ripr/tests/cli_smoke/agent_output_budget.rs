//! Agent-facing output byte budgets (token-footprint regression guard).
//!
//! One five-gap diff is analyzed through the surfaces an agent actually reads.
//! Each ceiling sits a little above the measured size, so redundancy added to
//! any of these outputs fails here instead of silently taxing every agent turn.
//! A deliberate cut lowers the ceiling in the same change; a deliberate
//! addition must justify raising it.
use super::{run_command, unique_external_workspace};
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const BASE: &str = "pub fn can_vote(age: u32) -> bool { age > 18 }\n\
pub fn clamp(v: i32, lo: i32, hi: i32) -> i32 { if v < lo { lo } else if v > hi { hi } else { v } }\n\
pub fn discount(total: u32) -> u32 { if total > 100 { total - 10 } else { total } }\n\
pub fn tier(n: u32) -> &'static str { if n >= 10 { \"gold\" } else { \"basic\" } }\n\
pub fn cap(n: u32) -> u32 { if n < 5 { n } else { 5 } }\n";

fn workspace(label: &str) -> TestResult<std::path::PathBuf> {
    let project = unique_external_workspace(label)?;
    fs::create_dir_all(project.join("src"))?;
    fs::create_dir_all(project.join("tests"))?;
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    let changed = BASE
        .replace("age > 18", "age >= 18")
        .replace("v > hi", "v >= hi")
        .replace("total > 100", "total >= 100")
        .replace("n >= 10", "n > 10")
        .replace("n < 5", "n <= 5");
    fs::write(project.join("src/lib.rs"), &changed)?;
    fs::write(
        project.join("tests/t.rs"),
        "use fx::*;\n#[test]\nfn vote() { assert!(can_vote(30)); }\n#[test]\nfn clamp_ok() { assert_eq!(clamp(5, 0, 10), 5); }\n#[test]\nfn disc() { assert!(discount(200) < 200); }\n#[test]\nfn t() { assert_eq!(tier(50), \"gold\"); }\n#[test]\nfn c() { assert_eq!(cap(1), 1); }\n",
    )?;
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
    Ok(String::from_utf8(output.stdout)?)
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
    let github = run(
        &project,
        &["check", "--diff", "change.diff", "--format", "github"],
    )?;
    within("check (default human)", &human, 3_500);
    within("check --format json", &json, 40_000);
    within("check --format github", &github, 2_000);
    Ok(())
}
