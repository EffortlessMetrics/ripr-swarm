//! Grep guard for the closed ripr product exit vocabulary (#12505).

use std::collections::BTreeMap;
use std::path::Path;

use crate::{FixKind, PolicyReportSpec, finish_policy_report, read_text_lossy, tracked_files};

const ALLOWLIST_PATH: &str = "policy/cli_exit_literal_allowlist.toml";
const SCAN_PATH_PREFIXES: &[&str] = &[
    "crates/ripr/src/main.rs",
    "crates/ripr/src/git.rs",
    "crates/ripr/src/atomic_file.rs",
    "crates/ripr/src/cli/",
];

/// Pinned count of one numeric `process::exit` literal in one file. Counts, not
/// line numbers: an unrelated edit above a pinned site must not turn the guard
/// red, while a new, removed or changed literal still does.
#[derive(Clone, Debug, Eq, PartialEq)]
struct AllowEntry {
    path: String,
    literal: i32,
    count: usize,
    reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExitLiteralFinding {
    path: String,
    line: usize,
    literal: i32,
    line_text: String,
}

/// Run by `check-process-policy` (the enforced precommit and hosted gate) and
/// directly as `cargo xtask check-cli-exit-contract`.
pub(crate) fn check_cli_exit_contract_impl() -> Result<(), String> {
    let allowlist = read_allowlist(Path::new(ALLOWLIST_PATH))?;
    let findings = collect_exit_literal_findings()?;
    let violations = exit_literal_violations(&findings, &allowlist);

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
                "policy/cli_exit_literal_allowlist.toml [[allow]] path|literal|count|reason",
            ),
        },
        &violations,
    )
}

/// Compare the observed literal exits with the pinned counts. A missing pin,
/// an over-count, a stale (under-counted) pin and an orphaned pin each fail.
fn exit_literal_violations(
    findings: &[ExitLiteralFinding],
    allowlist: &[AllowEntry],
) -> Vec<String> {
    let mut pinned = BTreeMap::<(&str, i32), usize>::new();
    for entry in allowlist {
        *pinned
            .entry((entry.path.as_str(), entry.literal))
            .or_default() += entry.count;
    }
    let mut observed = BTreeMap::<(&str, i32), Vec<&ExitLiteralFinding>>::new();
    for finding in findings {
        observed
            .entry((finding.path.as_str(), finding.literal))
            .or_default()
            .push(finding);
    }

    let mut violations = Vec::new();
    for ((path, literal), sites) in &observed {
        let allowed = pinned.get(&(*path, *literal)).copied().unwrap_or(0);
        if sites.len() > allowed {
            let lines = sites
                .iter()
                .map(|site| format!("{}: {}", site.line, site.line_text.trim()))
                .collect::<Vec<_>>()
                .join("\n  ");
            violations.push(format!(
                "Path: {path}\nLiteral exit: {literal}\nWhy: {} numeric process::exit literal site(s), {allowed} pinned in {ALLOWLIST_PATH}; route through CommandError or pin count = {} with a reason\nSites:\n  {lines}",
                sites.len(),
                sites.len()
            ));
        } else if sites.len() < allowed {
            violations.push(format!(
                "Path: {path}\nLiteral exit: {literal}\nWhy: stale allowlist entry: pinned count {allowed} but {} site(s) found; tighten the pin",
                sites.len()
            ));
        }
    }
    for entry in allowlist {
        if !observed.contains_key(&(entry.path.as_str(), entry.literal)) && entry.count > 0 {
            violations.push(format!(
                "Path: {}\nLiteral exit: {}\nWhy: stale allowlist entry (no matching process::exit literal in this file); remove it\nReason recorded: {}",
                entry.path, entry.literal, entry.reason
            ));
        }
    }
    violations
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

/// A missing allowlist pins nothing, so every literal exit is reported; it
/// never silently loosens the guard.
fn read_allowlist(path: &Path) -> Result<Vec<AllowEntry>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    parse_allowlist(&read_text_lossy(path)?)
}

fn parse_allowlist(text: &str) -> Result<Vec<AllowEntry>, String> {
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
    let literal = fields
        .get("literal")
        .ok_or_else(|| format!("allowlist entry for {path} missing literal"))?
        .parse::<i32>()
        .map_err(|err| format!("allowlist entry for {path} has invalid literal: {err}"))?;
    let count = fields
        .get("count")
        .ok_or_else(|| format!("allowlist entry for {path} literal {literal} missing count"))?
        .parse::<usize>()
        .map_err(|err| format!("allowlist entry for {path} has invalid count: {err}"))?;
    let reason = fields
        .get("reason")
        .cloned()
        .unwrap_or_else(|| "unspecified".to_string());
    Ok(Some(AllowEntry {
        path,
        literal,
        count,
        reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        AllowEntry, ExitLiteralFinding, exit_literal_violations, exit_literals_on_line,
        parse_allowlist,
    };

    fn finding(path: &str, line: usize, literal: i32) -> ExitLiteralFinding {
        ExitLiteralFinding {
            path: path.to_string(),
            line,
            literal,
            line_text: format!("std::process::exit({literal});"),
        }
    }

    fn pin(path: &str, literal: i32, count: usize) -> AllowEntry {
        AllowEntry {
            path: path.to_string(),
            literal,
            count,
            reason: "test".to_string(),
        }
    }

    #[test]
    fn exit_literals_on_line_finds_numeric_process_exit() {
        assert_eq!(
            exit_literals_on_line("Err(x) => std::process::exit(1),"),
            vec![1]
        );
        assert!(exit_literals_on_line("std::process::exit(code)").is_empty());
        assert!(exit_literals_on_line("let s = \"process::exit(7)\";").is_empty());
    }

    #[test]
    fn pinned_literal_passes_wherever_its_line_moves() {
        let pins = [pin("a.rs", 2, 1)];
        assert!(exit_literal_violations(&[finding("a.rs", 10, 2)], &pins).is_empty());
        assert!(exit_literal_violations(&[finding("a.rs", 400, 2)], &pins).is_empty());
    }

    #[test]
    fn unpinned_new_or_extra_literal_exit_is_rejected() {
        let pins = [pin("a.rs", 2, 1)];
        let new_literal = exit_literal_violations(&[finding("a.rs", 3, 9)], &pins);
        assert!(
            new_literal.iter().any(|v| v.contains("Literal exit: 9")),
            "{new_literal:?}"
        );
        let extra = exit_literal_violations(&[finding("a.rs", 3, 2), finding("a.rs", 8, 2)], &pins);
        assert!(
            extra.iter().any(|v| v.contains("2 numeric process::exit")),
            "{extra:?}"
        );
        let other_file = exit_literal_violations(&[finding("b.rs", 3, 2)], &pins);
        assert!(
            other_file.iter().any(|v| v.contains("Path: b.rs")),
            "{other_file:?}"
        );
    }

    #[test]
    fn stale_or_orphaned_pin_is_rejected() {
        let pins = [pin("a.rs", 2, 2)];
        let stale = exit_literal_violations(&[finding("a.rs", 3, 2)], &pins);
        assert!(stale.iter().any(|v| v.contains("stale")), "{stale:?}");
        let orphan = exit_literal_violations(&[], &pins);
        assert!(orphan.iter().any(|v| v.contains("stale")), "{orphan:?}");
    }

    #[test]
    fn parse_allowlist_reads_counted_entries() -> Result<(), String> {
        let entries = parse_allowlist(
            "schema_version = \"1\"\n[[allow]]\npath = \"a.rs\"\nliteral = 2\ncount = 3\nreason = \"r\"\n",
        )?;
        assert_eq!(entries, vec![pin_with_reason("a.rs", 2, 3, "r")]);
        assert!(parse_allowlist("[[allow]]\npath = \"a.rs\"\nliteral = 2\n").is_err());
        Ok(())
    }

    fn pin_with_reason(path: &str, literal: i32, count: usize, reason: &str) -> AllowEntry {
        AllowEntry {
            reason: reason.to_string(),
            ..pin(path, literal, count)
        }
    }

    #[test]
    fn repository_allowlist_parses_and_pins_the_main_panic_hook() -> Result<(), String> {
        let text = std::fs::read_to_string(super::ALLOWLIST_PATH)
            .or_else(|_| std::fs::read_to_string(format!("../{}", super::ALLOWLIST_PATH)))
            .map_err(|err| err.to_string())?;
        let entries = parse_allowlist(&text)?;
        assert!(
            entries
                .iter()
                .any(|entry| entry.path.ends_with("main.rs") && entry.literal == 2),
            "expected main.rs panic hook allow entry"
        );
        Ok(())
    }
}
