//! Grep guard for the closed ripr product exit vocabulary (#12505).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::{FixKind, PolicyReportSpec, finish_policy_report, read_text_lossy, tracked_files};

const ALLOWLIST_PATH: &str = "policy/cli_exit_literal_allowlist.toml";
const SCAN_PATH_PREFIXES: &[&str] = &[
    "crates/ripr/src/main.rs",
    "crates/ripr/src/git.rs",
    "crates/ripr/src/atomic_file.rs",
    "crates/ripr/src/cli/",
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct AllowEntry {
    path: String,
    line: usize,
    literal: i32,
    reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExitLiteralFinding {
    path: String,
    line: usize,
    literal: i32,
    line_text: String,
}

pub(crate) fn check_cli_exit_contract_impl() -> Result<(), String> {
    let allowlist = parse_allowlist(Path::new(ALLOWLIST_PATH))?;
    let findings = collect_exit_literal_findings()?;
    let allowed_keys = allowlist
        .iter()
        .map(|entry| (entry.path.as_str(), entry.line, entry.literal))
        .collect::<BTreeSet<_>>();

    let mut violations = Vec::new();
    for finding in &findings {
        let key = (finding.path.as_str(), finding.line, finding.literal);
        if allowed_keys.contains(&key) {
            continue;
        }
        violations.push(format!(
            "Path: {}\nLine: {}\nLiteral exit: {}\nWhy: numeric process::exit literal is not pinned in {ALLOWLIST_PATH}; route through CommandError or add an explicit allow entry\nSnippet: {}",
            finding.path,
            finding.line,
            finding.literal,
            finding.line_text.trim()
        ));
    }

    for entry in &allowlist {
        if findings.iter().any(|finding| {
            finding.path == entry.path
                && finding.line == entry.line
                && finding.literal == entry.literal
        }) {
            continue;
        }
        violations.push(format!(
            "Path: {}\nLine: {}\nLiteral exit: {}\nWhy: stale allowlist entry (no matching process::exit literal at this site)\nReason recorded: {}",
            entry.path, entry.line, entry.literal, entry.reason
        ));
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "cli-exit-contract.md",
            check: "check-cli-exit-contract",
            why_it_matters: "ripr's CLI contract exposes only documented exit codes; undocumented numeric process exits reintroduce silent harness failures.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Route failures through CommandError instead of process::exit.",
                "If the literal is fixture analyzed source, add a pinned allowlist entry with reason.",
                "Keep main.rs panic-boundary codes on the closed {0,2,3,130} set.",
            ],
            rerun_command: "cargo xtask check-cli-exit-contract",
            exception_template: Some(
                "policy/cli_exit_literal_allowlist.toml [[allow]] path|line|literal|reason",
            ),
        },
        &violations,
    )
}

fn collect_exit_literal_findings() -> Result<Vec<ExitLiteralFinding>, String> {
    let mut findings = Vec::new();
    for path in tracked_files()? {
        if !path.ends_with(".rs") || !should_scan_path(&path) {
            continue;
        }
        let text = read_text_lossy(Path::new(&path))?;
        for (index, line) in text.lines().enumerate() {
            for literal in exit_literals_on_line(line) {
                findings.push(ExitLiteralFinding {
                    path: path.clone(),
                    line: index + 1,
                    literal,
                    line_text: line.to_string(),
                });
            }
        }
    }
    findings.sort_by(|left, right| {
        (&left.path, left.line, left.literal).cmp(&(&right.path, right.line, right.literal))
    });
    Ok(findings)
}

fn should_scan_path(path: &str) -> bool {
    SCAN_PATH_PREFIXES
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(prefix))
}

fn exit_literals_on_line(line: &str) -> Vec<i32> {
    let mut literals = Vec::new();
    let mut search = line;
    while let Some(index) = search.find("process::exit(") {
        let line_index = line.len() - search.len() + index;
        if is_process_exit_inside_string(&line[..line_index]) {
            search = &search[index + 1..];
            continue;
        }
        let tail = &search[index + "process::exit(".len()..];
        if let Some(literal) = parse_exit_literal_prefix(tail) {
            literals.push(literal);
        }
        search = &search[index + 1..];
    }
    literals
}

fn is_process_exit_inside_string(prefix: &str) -> bool {
    let mut in_string = false;
    for ch in prefix.chars() {
        if ch == '"' {
            in_string = !in_string;
        }
    }
    in_string
}

fn parse_exit_literal_prefix(tail: &str) -> Option<i32> {
    let trimmed = tail.trim_start();
    let digits = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

fn parse_allowlist(path: &Path) -> Result<Vec<AllowEntry>, String> {
    let text = read_text_lossy(path)?;
    let mut entries = Vec::new();
    let mut current: BTreeMap<String, String> = BTreeMap::new();
    let mut in_allow = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[allow]]" {
            if let Some(entry) = flush_allow_entry(&current)? {
                entries.push(entry);
            }
            current.clear();
            in_allow = true;
            continue;
        }
        if !in_allow {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        current.insert(
            key.trim().to_string(),
            value.trim().trim_matches('"').to_string(),
        );
    }
    if let Some(entry) = flush_allow_entry(&current)? {
        entries.push(entry);
    }
    Ok(entries)
}

fn flush_allow_entry(fields: &BTreeMap<String, String>) -> Result<Option<AllowEntry>, String> {
    if fields.is_empty() {
        return Ok(None);
    }
    let path = fields
        .get("path")
        .ok_or_else(|| "allowlist entry missing path".to_string())?
        .clone();
    let line = fields
        .get("line")
        .ok_or_else(|| format!("allowlist entry for {path} missing line"))?
        .parse::<usize>()
        .map_err(|err| format!("allowlist entry for {path} has invalid line: {err}"))?;
    let literal = fields
        .get("literal")
        .ok_or_else(|| format!("allowlist entry for {path}:{line} missing literal"))?
        .parse::<i32>()
        .map_err(|err| format!("allowlist entry for {path}:{line} has invalid literal: {err}"))?;
    let reason = fields
        .get("reason")
        .cloned()
        .unwrap_or_else(|| "unspecified".to_string());
    Ok(Some(AllowEntry {
        path,
        line,
        literal,
        reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::{exit_literals_on_line, parse_allowlist};
    use std::path::Path;

    #[test]
    fn exit_literals_on_line_finds_numeric_process_exit() {
        let literals = exit_literals_on_line("Err(x) => std::process::exit(1),");
        assert_eq!(literals, vec![1]);
    }

    #[test]
    fn check_cli_exit_contract_rejects_new_numeric_exit_literal() -> Result<(), String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-cli-exit-contract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let sample = dir.join("sample.rs");
        std::fs::write(&sample, "fn demo() { std::process::exit(9); }\n")
            .map_err(|err| err.to_string())?;
        let line = std::fs::read_to_string(&sample).map_err(|err| err.to_string())?;
        assert_eq!(super::exit_literals_on_line(&line), vec![9]);
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn parse_allowlist_reads_entries() -> Result<(), String> {
        let entries = parse_allowlist(Path::new(super::ALLOWLIST_PATH))?;
        assert!(
            entries
                .iter()
                .any(|entry| entry.path.ends_with("main.rs") && entry.literal == 2),
            "expected main.rs panic hook allow entry"
        );
        Ok(())
    }
}
