//! Narrow workflow-ledger applicability; uncertainty always retains wheel qualification.
use crate::{WorkflowBudget, parse_workflow_budgets};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

pub const LEDGER: &str = "policy/workflow_allowlist.txt";
pub const WORKFLOW: &str = ".github/workflows/python-wheel-qualification.yml";

/// Output is a skip proof only after every Git operation and record check succeeds.
pub fn qualifies(root: &Path, event: &str, base: &str, head: &str) -> Result<bool, String> {
    if event != "pull_request" {
        return Ok(true);
    }
    for sha in [base, head] {
        if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("event commit must be a full SHA-1 identity".into());
        }
    }
    let checked_out = git(root, &["rev-parse", "HEAD"])?;
    if checked_out.trim() != head.to_ascii_lowercase() {
        return Err("checkout differs from the event head".into());
    }
    let ancestors = git(root, &["merge-base", "--all", base, head])?;
    let ancestors = ancestors.lines().collect::<Vec<_>>();
    let [ancestor] = ancestors.as_slice() else {
        return Err("event history has no unique merge base".into());
    };
    let paths = git(
        root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--name-only",
            "-z",
            ancestor,
            head,
            "--",
        ],
    )?;
    // A successful empty diff still supplies no unrelated-record change proof.
    if paths != format!("{LEDGER}\0") {
        return Ok(true);
    }
    let before = git(root, &["show", &format!("{ancestor}:{LEDGER}")])?;
    let after = git(root, &["show", &format!("{head}:{LEDGER}")])?;
    Ok(!unchanged_wheel_record(&before, &after)?)
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("wheel applicability Git capture failed: {error}"))?;
    if !output.status.success() {
        // Never accept successful-looking stdout from a failed or partial diff.
        return Err(format!(
            "wheel applicability Git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("wheel applicability Git input is not UTF-8: {error}"))
}

fn unique_records(text: &str) -> Result<Vec<WorkflowBudget>, String> {
    let records = parse_workflow_budgets(LEDGER, text)?;
    let mut keys = BTreeSet::new();
    for record in &records {
        if !keys.insert(&record.path) {
            return Err(format!(
                "ambiguous normalized workflow key: {}",
                record.path
            ));
        }
    }
    Ok(records)
}

fn unchanged_wheel_record(before: &str, after: &str) -> Result<bool, String> {
    let before = unique_records(before)?;
    let after = unique_records(after)?;
    let before = before.iter().find(|record| record.path == WORKFLOW);
    let after = after.iter().find(|record| record.path == WORKFLOW);
    Ok(matches!((before, after), (Some(before), Some(after)) if before == after))
}

/// Invalid inputs and unavailable history qualify; a failed helper job also qualifies
/// through the workflow's explicit fallback. Existing policy remains independent.
pub fn command() -> Result<(), String> {
    crate::verify_executable_identity()?;
    let arguments = std::env::args().skip(2).collect::<Vec<_>>();
    let decision = match arguments.as_slice() {
        [event, base, head] => qualifies(Path::new("."), event, base, head),
        _ => Err(
            "usage: cargo policy python-wheel-applicability <event> <base-sha> <head-sha>".into(),
        ),
    };
    let qualify = match decision {
        Ok(qualify) => qualify,
        Err(error) => {
            eprintln!("wheel qualification retained: {error}");
            true
        }
    };
    println!("qualify={qualify}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossless_records_keep_ambiguity_and_existing_validator_policy() -> Result<(), String> {
        let wheel = format!("{WORKFLOW}|194|wheel contract\n");
        let unrelated = "other.yml|1|unrelated\n";
        assert!(unchanged_wheel_record(
            &wheel,
            &format!("{wheel}{unrelated}")
        )?);
        for changed in [
            String::new(),
            wheel.replace("194", "195"),
            wheel.replace("wheel contract", "changed"),
            wheel.replace(WORKFLOW, "renamed.yml"),
        ] {
            assert!(!unchanged_wheel_record(&wheel, &changed)?);
            assert!(!unchanged_wheel_record(&changed, &wheel)?);
        }
        for malformed in [
            format!("{wheel}{wheel}"),
            format!("{wheel}{}", wheel.replace('/', "\\")),
            format!("{wheel}{unrelated}{unrelated}"),
            format!("{wheel}bad|row\n"),
            format!("{wheel}bad|NaN|reason\n"),
            format!("{wheel}bad|1| \n"),
        ] {
            assert!(unchanged_wheel_record(&wheel, &malformed).is_err());
            assert!(unchanged_wheel_record(&malformed, &wheel).is_err());
        }
        let duplicate = parse_workflow_budgets(LEDGER, &format!("{wheel}{wheel}"))?;
        assert_eq!(
            duplicate.len(),
            2,
            "the shared parser must preserve every record"
        );
        Ok(())
    }
}
