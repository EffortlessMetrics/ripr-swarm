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
use super::{Suite, capture, classify_suite, materialize, strings, write_file};
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

/// Where `score_one` keeps the agent's `src/lib.rs` while it swaps in head
/// and reverted text. It sits outside `src/` so an interrupted score (Ctrl-C,
/// a terminated process) can be recovered: the next run restores from it instead
/// of mistaking the head text it left behind for the agent's own file.
const AGENT_LIB_COPY: &str = ".trial-score-agent-lib.rs";

/// Score one repository. `src/lib.rs` is put back to what the agent left on
/// every path, including errors and an interrupted earlier run, so a rerun
/// sees the same input.
fn score_one(scenario: &Scenario, root: &Path, target_dir: &Path) -> Result<Value, String> {
    let current = hold_agent_lib(root)?;
    let scored = score_against_head(scenario, root, target_dir, current == scenario.head_lib);
    match (scored, release_agent_lib(root, &current)) {
        (Ok(row), Ok(())) => Ok(row),
        (Ok(_), Err(restore)) => Err(restore),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(restore)) => Err(format!("{error}; also {restore}")),
    }
}

/// The `src/lib.rs` the agent left, kept in `AGENT_LIB_COPY` before scoring
/// touches the file. A copy left by an interrupted run wins over the file.
fn hold_agent_lib(root: &Path) -> Result<String, String> {
    let copy_path = root.join(AGENT_LIB_COPY);
    if copy_path.is_file() {
        return fs::read_to_string(&copy_path)
            .map_err(|err| format!("failed to read {}: {err}", copy_path.display()));
    }
    let lib_path = root.join("src/lib.rs");
    let current = fs::read_to_string(&lib_path)
        .map_err(|err| format!("failed to read {}: {err}", lib_path.display()))?;
    // Publish the copy whole or not at all: a copy truncated by an interrupt
    // or a full disk would win on the next run and overwrite the agent's
    // intact file. Only a renamed, complete copy is ever trusted.
    let partial = root.join(format!("{AGENT_LIB_COPY}.partial"));
    write_file(&partial, &current)?;
    fs::rename(&partial, &copy_path)
        .map_err(|err| format!("failed to move {} into place: {err}", partial.display()))?;
    Ok(current)
}

/// Put the agent's `src/lib.rs` back, then drop the copy.
fn release_agent_lib(root: &Path, current: &str) -> Result<(), String> {
    write_file(&root.join("src/lib.rs"), current)?;
    let copy_path = root.join(AGENT_LIB_COPY);
    fs::remove_file(&copy_path)
        .map_err(|err| format!("failed to remove {}: {err}", copy_path.display()))
}

/// A gap is caught only when the revert makes a compiled suite fail, the
/// suite passes on head, and the agent left the code under test alone.
fn gap_caught(revert_fails_suite: bool, head_pass: bool, production_untouched: bool) -> bool {
    revert_fails_suite && head_pass && production_untouched
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
            "caught": gap_caught(fails, head_pass, production_untouched),
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

    #[test]
    fn a_catch_needs_a_failing_revert_a_passing_head_and_untouched_code() {
        assert!(gap_caught(true, true, true));
        assert!(!gap_caught(false, true, true));
        assert!(!gap_caught(true, false, true));
        assert!(!gap_caught(true, true, false));
    }

    fn error_of<E: std::fmt::Display>(result: Result<(), E>) -> Option<String> {
        result.err().map(|error| error.to_string())
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ripr-agent-eval-trial-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_repositories_score_zero_and_leave_the_score_incomplete() {
        let dir = scratch("missing");
        let score = trial_score(&dir).unwrap_or_default();
        let keyed: usize = gap_scenarios().map(|scenario| scenario.gaps.len()).sum();
        assert_eq!(score["caught"], 0);
        assert_eq!(score["keyed_gaps"], keyed);
        assert_eq!(score["complete"], false);
        assert_eq!(
            score["problems"].as_array().map(Vec::len),
            Some(gap_scenarios().count())
        );
    }

    #[test]
    fn setup_refuses_a_directory_that_is_not_empty() {
        let dir = scratch("nonempty");
        assert_eq!(error_of(fs::create_dir_all(&dir)), None);
        assert_eq!(error_of(fs::write(dir.join("keep.txt"), "mine")), None);
        assert!(trial_setup(&dir).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("keep.txt")).ok().as_deref(),
            Some("mine")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_interrupted_score_recovers_the_agents_lib_from_the_copy() {
        // A previous run died after swapping in head text: the copy holds
        // the agent's edit, so it must win over the file and come back.
        let dir = scratch("interrupted");
        let root = dir.join("repo");
        assert_eq!(error_of(fs::create_dir_all(root.join("src"))), None);
        let head = SCENARIOS[0].head_lib;
        let agent_lib = format!("{head}// agent edit\n");
        assert_eq!(error_of(fs::write(root.join("src/lib.rs"), head)), None);
        assert_eq!(
            error_of(fs::write(root.join(AGENT_LIB_COPY), &agent_lib)),
            None
        );
        assert_eq!(
            hold_agent_lib(&root).ok().as_deref(),
            Some(agent_lib.as_str())
        );
        assert_eq!(error_of(release_agent_lib(&root, &agent_lib)), None);
        assert_eq!(
            fs::read_to_string(root.join("src/lib.rs")).ok().as_deref(),
            Some(agent_lib.as_str())
        );
        assert!(!root.join(AGENT_LIB_COPY).exists());

        // Without a copy, the file itself is held and a copy is written
        // before anything changes it.
        assert_eq!(
            hold_agent_lib(&root).ok().as_deref(),
            Some(agent_lib.as_str())
        );
        assert_eq!(
            fs::read_to_string(root.join(AGENT_LIB_COPY))
                .ok()
                .as_deref(),
            Some(agent_lib.as_str())
        );

        // A staging file left by an interrupted copy write is never trusted:
        // the intact file wins, and the copy is published whole.
        assert_eq!(error_of(release_agent_lib(&root, &agent_lib)), None);
        let partial = root.join(format!("{AGENT_LIB_COPY}.partial"));
        assert_eq!(error_of(fs::write(&partial, "")), None);
        assert_eq!(
            hold_agent_lib(&root).ok().as_deref(),
            Some(agent_lib.as_str())
        );
        assert_eq!(
            fs::read_to_string(root.join(AGENT_LIB_COPY))
                .ok()
                .as_deref(),
            Some(agent_lib.as_str())
        );
        assert!(!partial.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
