//! Agent-in-the-loop trials on the agent-eval scenarios.
//!
//! `cargo xtask agent-eval trial-setup <dir>` writes one fresh repository per
//! gap scenario (no answer key inside), for a coding agent to work in with or
//! without ripr. `cargo xtask agent-eval trial-score <dir>` then judges what
//! the agent left behind, independently of ripr:
//!
//! - `head_pass`: the whole test suite passes on the changed code;
//! - per keyed gap, `caught`: putting that one line back to its base text
//!   makes the suite fail (a one-line revert of the change, not a mutation
//!   engine);
//! - `production_untouched`: `src/lib.rs` still equals the head text, so a
//!   trial cannot pass by editing the code under test.
//!
//! A gap counts as caught only when all three hold.

use super::scenarios::{SCENARIOS, Scenario};
use super::{capture, materialize, strings, write_file};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

/// The lib text with keyed `line` (1-based) put back to its base text.
pub(crate) fn revert_line(scenario: &Scenario, line: u32) -> Option<String> {
    let base: Vec<&str> = scenario.base_lib.lines().collect();
    let mut head: Vec<&str> = scenario.head_lib.lines().collect();
    let index = usize::try_from(line).ok()?.checked_sub(1)?;
    if base.len() != head.len() {
        return None;
    }
    *head.get_mut(index)? = base.get(index)?;
    let mut text = head.join("\n");
    text.push('\n');
    Some(text)
}

fn gap_scenarios() -> impl Iterator<Item = &'static Scenario> {
    SCENARIOS
        .iter()
        .filter(|scenario| !scenario.gaps.is_empty())
}

pub(crate) fn trial_setup(dir: &Path) -> Result<(), String> {
    if dir.exists()
        && fs::read_dir(dir)
            .map_err(|err| format!("failed to read {}: {err}", dir.display()))?
            .next()
            .is_some()
    {
        return Err(format!(
            "{} is not empty; pick a fresh directory",
            dir.display()
        ));
    }
    fs::create_dir_all(dir).map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    for scenario in gap_scenarios() {
        let workspace = materialize(dir, scenario, "trial")?;
        println!("agent-eval trial-setup: {}", workspace.root.display());
    }
    Ok(())
}

/// What one `cargo test` run established.
#[derive(Debug, PartialEq)]
pub(crate) enum Suite {
    Pass,
    /// The tests compiled, ran, and at least one failed.
    TestFailed,
    /// Ran but gave no test result (for example a compile error): no
    /// evidence either way. A head that does not compile because of the
    /// agent's own tests is a legitimate zero.
    NotEstablished(String),
    /// The run itself could not be measured (cargo missing, timeout, a
    /// signal, an output budget): the trial score is incomplete.
    InstrumentError(String),
}

/// Classify a finished `cargo test` run. Only a run that compiled and
/// reported a failing test counts as a failure; a revert that stops the
/// crate compiling must not credit the agent's tests.
pub(crate) fn classify_suite(exit: Option<i32>, output: &str) -> Suite {
    let compile_error = output.contains("could not compile") || output.contains("error[E");
    match exit {
        Some(0) if !compile_error => Suite::Pass,
        Some(101) if !compile_error && output.contains("test result: FAILED") => Suite::TestFailed,
        None => Suite::InstrumentError("cargo test ended without an exit code".to_string()),
        Some(code) => {
            Suite::NotEstablished(format!("cargo test exit {code} without a test result"))
        }
    }
}

fn run_suite(root: &Path, target_dir: &Path) -> Suite {
    let target = target_dir.display().to_string();
    match capture(
        Path::new("cargo"),
        &strings(&["test", "--offline", "-q"]),
        root,
        None,
        &[("CARGO_TARGET_DIR", target.as_str())],
    ) {
        Ok(output) => classify_suite(
            output.exit,
            &format!("{}\n{}", output.stdout, output.stderr),
        ),
        Err(error) => Suite::InstrumentError(error),
    }
}

fn not_established(suite: &Suite) -> Option<&str> {
    match suite {
        Suite::NotEstablished(reason) | Suite::InstrumentError(reason) => Some(reason.as_str()),
        Suite::Pass | Suite::TestFailed => None,
    }
}

/// Run the suite; an instrument failure aborts scoring this repository, so
/// it shows up as an incomplete score rather than a low one.
fn measured_suite(root: &Path, target_dir: &Path) -> Result<Suite, String> {
    match run_suite(root, target_dir) {
        Suite::InstrumentError(reason) => Err(format!("instrument failure: {reason}")),
        suite => Ok(suite),
    }
}

/// Score one repository. `src/lib.rs` is put back to what the agent left on
/// every path, including errors, so a rerun sees the same input.
fn score_one(scenario: &Scenario, root: &Path, target_dir: &Path) -> Result<Value, String> {
    let lib_path = root.join("src/lib.rs");
    let current = fs::read_to_string(&lib_path)
        .map_err(|err| format!("failed to read {}: {err}", lib_path.display()))?;
    let scored = score_against_head(scenario, root, target_dir, current == scenario.head_lib);
    let restored = write_file(&lib_path, &current);
    match (scored, restored) {
        (Ok(row), Ok(())) => Ok(row),
        (Ok(_), Err(restore)) => Err(restore),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(restore)) => Err(format!("{error}; also {restore}")),
    }
}

fn score_against_head(
    scenario: &Scenario,
    root: &Path,
    target_dir: &Path,
    production_untouched: bool,
) -> Result<Value, String> {
    let lib_path = root.join("src/lib.rs");
    // Score the tests against the real head, never an agent's edit.
    write_file(&lib_path, scenario.head_lib)?;
    let head = measured_suite(root, target_dir)?;
    let head_pass = head == Suite::Pass;
    let mut gaps = Vec::new();
    for gap in scenario.gaps {
        let reverted = revert_line(scenario, gap.line)
            .ok_or_else(|| format!("{}: cannot revert line {}", scenario.id, gap.line))?;
        write_file(&lib_path, &reverted)?;
        let suite = measured_suite(root, target_dir)?;
        let fails = suite == Suite::TestFailed;
        gaps.push(json!({
            "owner": gap.owner,
            "line": gap.line,
            "revert_fails_suite": fails,
            "not_established": not_established(&suite),
            "caught": fails && head_pass && production_untouched,
        }));
    }
    let caught = gaps.iter().filter(|gap| gap["caught"] == true).count();
    Ok(json!({
        "id": scenario.id,
        "head_pass": head_pass,
        "head_not_established": not_established(&head),
        "production_untouched": production_untouched,
        "keyed_gaps": scenario.gaps.len(),
        "caught": caught,
        "gaps": gaps,
    }))
}

/// Score every gap scenario under `dir`. A missing repository or a scoring
/// error stays in the denominator as zero caught and is listed, so a broken
/// or partial trial can never read as a better score.
pub(crate) fn trial_score(dir: &Path) -> Result<Value, String> {
    let target_dir = dir.join(".cargo-target");
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    for scenario in gap_scenarios() {
        let root = dir.join(format!("{}-trial", scenario.id));
        let row = if root.is_dir() {
            score_one(scenario, &root, &target_dir)
        } else {
            Err(format!("{} is missing", root.display()))
        };
        rows.push(row.unwrap_or_else(|error| {
            problems.push(format!("{}: {error}", scenario.id));
            json!({"id": scenario.id, "keyed_gaps": scenario.gaps.len(), "caught": 0, "error": error})
        }));
    }
    let caught: u64 = rows.iter().filter_map(|row| row["caught"].as_u64()).sum();
    let keyed: u64 = gap_scenarios()
        .map(|scenario| scenario.gaps.len() as u64)
        .sum();
    Ok(json!({
        "schema_version": "ripr-agent-eval-trial-score-v1",
        "caught": caught,
        "keyed_gaps": keyed,
        "complete": problems.is_empty(),
        "problems": problems,
        "scenarios": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_line_restores_only_the_keyed_line() {
        for scenario in gap_scenarios() {
            for gap in scenario.gaps {
                let reverted = revert_line(scenario, gap.line);
                let reverted = reverted.unwrap_or_default();
                assert_ne!(
                    reverted, scenario.head_lib,
                    "{} line {}",
                    scenario.id, gap.line
                );
                let changed: Vec<usize> = reverted
                    .lines()
                    .zip(scenario.head_lib.lines())
                    .enumerate()
                    .filter(|(_, (left, right))| left != right)
                    .map(|(index, _)| index + 1)
                    .collect();
                assert_eq!(changed, vec![gap.line as usize], "{}", scenario.id);
            }
        }
        assert_eq!(revert_line(&SCENARIOS[0], 0), None);
    }

    #[test]
    fn only_a_compiled_failing_run_counts_as_a_failure() {
        assert_eq!(classify_suite(Some(0), "test result: ok."), Suite::Pass);
        assert_eq!(
            classify_suite(Some(101), "test result: FAILED. 0 passed; 1 failed"),
            Suite::TestFailed
        );
        // A revert that breaks compilation exits 101 too; it is no catch.
        let compile = "error[E0308]: mismatched types\nerror: could not compile `shop`";
        assert!(matches!(
            classify_suite(Some(101), compile),
            Suite::NotEstablished(_)
        ));
        assert!(matches!(
            classify_suite(None, ""),
            Suite::InstrumentError(_)
        ));
        assert!(matches!(
            classify_suite(Some(101), ""),
            Suite::NotEstablished(_)
        ));
        assert_eq!(revert_line(&SCENARIOS[0], 999), None);
    }
}
