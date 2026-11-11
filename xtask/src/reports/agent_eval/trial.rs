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

fn suite_passes(root: &Path, target_dir: &Path) -> Result<bool, String> {
    let target = target_dir.display().to_string();
    let output = capture(
        Path::new("cargo"),
        &strings(&["test", "--offline", "-q"]),
        root,
        None,
        &[("CARGO_TARGET_DIR", target.as_str())],
    )?;
    Ok(output.exit == Some(0))
}

fn score_one(scenario: &Scenario, root: &Path, target_dir: &Path) -> Result<Value, String> {
    let lib_path = root.join("src/lib.rs");
    let current = fs::read_to_string(&lib_path)
        .map_err(|err| format!("failed to read {}: {err}", lib_path.display()))?;
    let production_untouched = current == scenario.head_lib;
    if !production_untouched {
        // Score the tests against the real head, never the agent's edit.
        write_file(&lib_path, scenario.head_lib)?;
    }
    let head_pass = suite_passes(root, target_dir)?;
    let mut gaps = Vec::new();
    for gap in scenario.gaps {
        let reverted = revert_line(scenario, gap.line)
            .ok_or_else(|| format!("{}: cannot revert line {}", scenario.id, gap.line))?;
        write_file(&lib_path, &reverted)?;
        let fails = !suite_passes(root, target_dir)?;
        gaps.push(json!({
            "owner": gap.owner,
            "line": gap.line,
            "revert_fails_suite": fails,
            "caught": fails && head_pass && production_untouched,
        }));
    }
    write_file(&lib_path, &current)?;
    let caught = gaps.iter().filter(|gap| gap["caught"] == true).count();
    Ok(json!({
        "id": scenario.id,
        "head_pass": head_pass,
        "production_untouched": production_untouched,
        "keyed_gaps": scenario.gaps.len(),
        "caught": caught,
        "gaps": gaps,
    }))
}

pub(crate) fn trial_score(dir: &Path) -> Result<Value, String> {
    let target_dir = dir.join(".cargo-target");
    let mut rows = Vec::new();
    for scenario in gap_scenarios() {
        let root = dir.join(format!("{}-trial", scenario.id));
        if !root.is_dir() {
            rows.push(json!({"id": scenario.id, "missing": true}));
            continue;
        }
        rows.push(score_one(scenario, &root, &target_dir)?);
    }
    let caught: u64 = rows.iter().filter_map(|row| row["caught"].as_u64()).sum();
    let keyed: u64 = rows
        .iter()
        .filter_map(|row| row["keyed_gaps"].as_u64())
        .sum();
    Ok(json!({
        "schema_version": "ripr-agent-eval-trial-score-v1",
        "caught": caught,
        "keyed_gaps": keyed,
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
        assert_eq!(revert_line(&SCENARIOS[0], 999), None);
    }
}
