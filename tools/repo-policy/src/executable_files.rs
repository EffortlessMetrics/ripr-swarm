//! Git-index executable policy, shared by the lightweight and xtask front doors.
use crate::{FixKind, PolicyReportSpec, finish_policy_report};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn check() -> Result<(), String> {
    let allowlist = read_allowlist_optional("policy/executable_allowlist.txt")?;
    let output = git_index_modes()?;
    // Unsupported or malformed index input is also a policy failure: refresh
    // the report so an earlier passing report cannot survive this invocation.
    let violations =
        executable_file_violations(&allowlist, &output).unwrap_or_else(|error| vec![error]);

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
    git_ls_files_stage: &[u8],
) -> Result<Vec<String>, String> {
    let mut staged_modes = BTreeMap::new();
    let mut violations = Vec::new();

    if !git_ls_files_stage.is_empty() && !git_ls_files_stage.ends_with(&[0]) {
        return Err("malformed git ls-files --stage -z output: missing final NUL".into());
    }
    for record in git_ls_files_stage.split_inclusive(|byte| *byte == 0) {
        let record = &record[..record.len() - 1];
        let (mode, path) = parse_git_stage_record(record)?;
        staged_modes.insert(path.to_owned(), mode);
        if mode == "100755" && !allowlist.contains(path) {
            let path = report_path(path);
            violations.push(format!(
                "checked-in executable file is not allowlisted: {path}\n  preferred: use cargo xtask instead of executable scripts"
            ));
        }
    }

    violations.extend(executable_allowlist_stale_row_violations(
        allowlist,
        &staged_modes,
    ));
    Ok(violations)
}

fn executable_allowlist_stale_row_violations(
    allowlist: &BTreeSet<String>,
    staged_modes: &BTreeMap<String, &str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for path in allowlist {
        let displayed = report_path(path);
        match staged_modes.get(path).copied() {
            Some("100755") => {}
            Some(mode) => violations.push(format!(
                "{displayed} allowlist entry is stale: mode is {mode}, expected 100755; remove the entry"
            )),
            None => violations.push(format!(
                "{displayed} allowlist entry is stale: path is not in git ls-files --stage; remove the entry"
            )),
        }
    }
    violations
}

fn read_allowlist_optional(path: &str) -> Result<BTreeSet<String>, String> {
    if Path::new(path).exists() {
        let mut allowed = BTreeSet::new();
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read {path} as UTF-8: {error}"))?;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            validate_literal_path(trimmed)
                .map_err(|error| format!("unsupported executable allowlist path: {error}"))?;
            allowed.insert(trimmed.to_owned());
        }
        Ok(allowed)
    } else {
        Ok(BTreeSet::new())
    }
}

fn git_index_modes() -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("git")
        .args(["ls-files", "--stage", "-z"])
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files --stage failed with {}",
            output.status
        ));
    }
    Ok(output.stdout)
}

// The public allowlist remains UTF-8, trimmed non-comment rows. Do not invent
// byte-path or newline escaping, or reinterpret a Unix backslash as a separator.
fn validate_literal_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.trim() != path
        || path.starts_with('#')
        || path.chars().any(|ch| ch.is_control() && ch != '\t')
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        || (cfg!(windows) && path.contains('\\'))
    {
        return Err(format!(
            "{path:?}; expected a literal repository-relative UTF-8 path representable by a non-comment allowlist row"
        ));
    }
    Ok(())
}

fn report_path(path: &str) -> String {
    if path.contains(['\t', '\\', '"', '`']) {
        format!("{path:?}")
    } else {
        path.to_owned()
    }
}

fn parse_git_stage_record(record: &[u8]) -> Result<(&str, &str), String> {
    let separator = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or("malformed git ls-files --stage -z record: missing header separator")?;
    let header = std::str::from_utf8(&record[..separator])
        .map_err(|_| "malformed Git index header: non-UTF-8")?;
    let fields = header.split(' ').collect::<Vec<_>>();
    if fields.len() != 3
        || !matches!(fields[0], "100644" | "100755" | "120000" | "160000")
        || !matches!(fields[1].len(), 40 | 64)
        || !fields[1].bytes().all(|byte| byte.is_ascii_hexdigit())
        || !matches!(fields[2], "0" | "1" | "2" | "3")
    {
        return Err(format!("malformed Git index header: {header:?}"));
    }
    let path = std::str::from_utf8(&record[separator + 1..]).map_err(
        |_| "unsupported Git index path: non-UTF-8 bytes; the executable allowlist requires UTF-8",
    )?;
    validate_literal_path(path).map_err(|error| format!("unsupported Git index path: {error}"))?;
    if fields[2] != "0" {
        return Err(format!(
            "unmerged Git index path: {}; executable policy requires stage 0",
            report_path(path)
        ));
    }
    Ok((fields[0], path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_malformed_boundaries_and_unmerged_stages() -> Result<(), String> {
        let empty = BTreeSet::new();
        assert!(executable_file_violations(&empty, b"")?.is_empty());
        let row = "100755 abcdef0123456789abcdef0123456789abcdef01 0\tfile";
        assert!(executable_file_violations(&empty, row.as_bytes()).is_err());
        assert!(executable_file_violations(&empty, b"\0").is_err());
        assert!(executable_file_violations(&empty, format!("{row}\0\0").as_bytes()).is_err());
        for stage in ["1", "2", "3"] {
            let record = row.replace(" 0\t", &format!(" {stage}\t"));
            assert!(
                parse_git_stage_record(record.as_bytes())
                    .err()
                    .is_some_and(|error| error.contains("requires stage 0"))
            );
        }
        for path in [
            "../outside",
            "/absolute",
            "dir/../file",
            "dir//file",
            "dir/./file",
        ] {
            assert!(parse_git_stage_record(row.replace("file", path).as_bytes()).is_err());
        }
        Ok(())
    }

    #[test]
    fn executable_allowlist_rejects_stale_row() -> Result<(), String> {
        let allowlist = BTreeSet::from([
            "deleted/script.sh".to_string(),
            "demoted/script.sh".to_string(),
        ]);
        let stage = concat!(
            "100644 abcdef0123456789abcdef0123456789abcdef01 0\tdemoted/script.sh\0",
            "100644 fedcba0123456789abcdef0123456789abcdef01 0\tordinary.rs\0",
        );
        let violations = executable_file_violations(&allowlist, stage.as_bytes())?;
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
        let stage = format!("100755 9cea22d6aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 0\t{path}\0");
        let violations = executable_file_violations(&allowlist, stage.as_bytes())?;
        if !violations.is_empty() {
            return Err(format!(
                "a live 100755 allowlist row must still pass: {violations:?}"
            ));
        }
        Ok(())
    }
}
