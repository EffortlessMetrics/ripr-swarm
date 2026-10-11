//! Git-index executable policy, shared by the lightweight and xtask front doors.
use crate::{FixKind, PolicyReportSpec, finish_policy_report, normalize_slashes};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn check() -> Result<(), String> {
    let allowlist = read_allowlist_optional("policy/executable_allowlist.txt")?;
    let output = git_index_modes()?;
    let violations = executable_file_violations(&allowlist, &output);

    finish_policy_report(
        PolicyReportSpec {
            report_file: "executable-files.md",
            check: "check-executable-files",
            why_it_matters: "Checked-in executable scripts make automation drift away from the Rust-first xtask surface.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Remove the executable bit from ordinary files.",
                "Move script behavior into xtask.",
                "If an executable file is truly required, add a reviewed allowlist entry.",
                "If an allowlist path is missing or no longer mode 100755, remove the entry.",
            ],
            rerun_command: "cargo xtask check-executable-files",
            exception_template: Some("policy/executable_allowlist.txt entry:\npath/to/file"),
        },
        &violations,
    )
}

fn executable_file_violations(
    allowlist: &BTreeSet<String>,
    git_ls_files_stage: &str,
) -> Vec<String> {
    let mut staged_modes = BTreeMap::new();
    let mut violations = Vec::new();

    for line in git_ls_files_stage.lines() {
        let Some((mode, path)) = parse_git_stage_line(line) else {
            continue;
        };
        let normalized = normalize_slashes(path);
        staged_modes.insert(normalized.clone(), mode);
        if mode == "100755" && !allowlist.contains(&normalized) {
            violations.push(format!(
                "checked-in executable file is not allowlisted: {normalized}\n  preferred: use cargo xtask instead of executable scripts"
            ));
        }
    }

    violations.extend(executable_allowlist_stale_row_violations(
        allowlist,
        &staged_modes,
    ));
    violations
}

fn executable_allowlist_stale_row_violations(
    allowlist: &BTreeSet<String>,
    staged_modes: &BTreeMap<String, &str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for path in allowlist {
        match staged_modes.get(path).copied() {
            Some("100755") => {}
            Some(mode) => violations.push(format!(
                "{path} allowlist entry is stale: mode is {mode}, expected 100755; remove the entry"
            )),
            None => violations.push(format!(
                "{path} allowlist entry is stale: path is not in git ls-files --stage; remove the entry"
            )),
        }
    }
    violations
}

fn read_allowlist_optional(path: &str) -> Result<BTreeSet<String>, String> {
    if Path::new(path).exists() {
        let mut allowed = BTreeSet::new();
        let text = crate::read_text_lossy(Path::new(path))?;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            allowed.insert(normalize_slashes(trimmed));
        }
        Ok(allowed)
    } else {
        Ok(BTreeSet::new())
    }
}

fn git_index_modes() -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["ls-files", "--stage"])
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files --stage failed with {}",
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_git_stage_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let mode = parts.next()?;
    let _object_type = parts.next()?;
    let _hash = parts.next()?;
    let stage_and_path = line.split('\t').nth(1)?;
    Some((mode, stage_and_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executable_allowlist_rejects_stale_row() -> Result<(), String> {
        let allowlist = BTreeSet::from([
            "deleted/script.sh".to_string(),
            "demoted/script.sh".to_string(),
        ]);
        let stage = "\
100644 abcdef0123456789abcdef0123456789abcdef01 0\tdemoted/script.sh
100644 fedcba0123456789abcdef0123456789abcdef01 0\tordinary.rs
";
        let violations = executable_file_violations(&allowlist, stage);
        if !violations.iter().any(|row| {
        row.contains(
            "deleted/script.sh allowlist entry is stale: path is not in git ls-files --stage; remove the entry",
        )
    }) {
        return Err(format!(
            "missing executable allowlist path must fail: {violations:?}"
        ));
    }
        if !violations.iter().any(|row| {
        row.contains(
            "demoted/script.sh allowlist entry is stale: mode is 100644, expected 100755; remove the entry",
        )
    }) {
        return Err(format!(
            "demoted executable allowlist path must fail: {violations:?}"
        ));
    }
        Ok(())
    }

    #[test]
    fn executable_allowlist_accepts_live_100755_row() -> Result<(), String> {
        let path = "packaging/npm/launcher/bin/ripr.cjs";
        let allowlist = BTreeSet::from([path.to_string()]);
        let stage = format!("100755 9cea22d6aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 0\t{path}\n");
        let violations = executable_file_violations(&allowlist, &stage);
        if !violations.is_empty() {
            return Err(format!(
                "a live 100755 allowlist row must still pass: {violations:?}"
            ));
        }
        Ok(())
    }
}
