//! Policy implementation cluster shared between the full `xtask` binary and
//! a future slim policy binary (RANK-06 route B prerequisite).
//!
//! Behavior-preserving extraction from `main.rs`: the lint-policy gate with
//! its clippy-debt/CiLedger collectors, workspace-shape, the policy `*_impl`
//! functions behind `policy/all.rs`, the shared count-policy engine, and
//! `tracked_files`. Remaining shared helpers still live at the crate root and
//! are imported below; a slim binary must provide or move those next.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use repo_policy::{
    FixKind, PolicyReportSpec, collect_files, finish_policy_report, normalize_path,
    read_count_allowlist, read_text_lossy,
};

use crate::no_panic::{
    contains_word, parse_string_value, parse_toml_key_value, parse_usize_value,
    strip_toml_value_comment,
};
use crate::policy::parse_workspace_lints_section;
use crate::types::GlobAllow;
use crate::{
    allow_attribute_line_summary, attribute_lint_name, forbidden_static_terms,
    guarded_allow_attribute_lints, guarded_allow_attributes_in_text, is_kebab_case_id,
    is_network_policy_candidate, is_process_policy_candidate, is_snake_case_id, known_command_root,
    known_commands, ledger_name_to_lookup, line_has_static_language_inline_allow,
    load_static_language_allowlist, local_context_findings_for_path, local_context_line_summary,
    matches_any_glob, network_policy_patterns, parse_clippy_debt_ledger, parse_clippy_lints_ledger,
    parse_inline_array, parse_workspace_package_rust_version, process_policy_patterns,
    read_count_policy_allowlist, read_local_context_allowlist, read_pipe_records,
    should_scan_static_language_path, static_language_violation_message,
    validate_local_context_allowlist, workspace_members, write_local_context_json,
};
use crate::{no_panic, policy};

pub(crate) fn check_static_language_impl() -> Result<(), String> {
    let report_spec = PolicyReportSpec {
        report_file: "static-language.md",
        check: "check-static-language",
        why_it_matters: "Static output must preserve the boundary between draft exposure evidence and real mutation results.",
        fix_kind: FixKind::ReviewerDecisionRequired,
        recommended_fixes: &[
            "Rewrite static product output to use the approved exposure vocabulary.",
            "For an innocent word in a comment, append `ripr-allow: static-language: <reason>` to that single line.",
            "If a whole file is explanatory documentation, add a reasoned `[[allow]]` entry to the static-language allowlist.",
        ],
        rerun_command: "cargo xtask check-static-language",
        exception_template: Some(
            ".ripr/static-language-allowlist.toml entry:\n[[allow]]\npath = \"path/to/file.md\"\nowner = \"team\"\nreason = \"why this file may quote prohibited vocabulary\"",
        ),
    };

    let allowed = match load_static_language_allowlist() {
        Ok(entries) => entries,
        Err(violations) => return finish_policy_report(report_spec, &violations),
    };
    let forbidden = forbidden_static_terms();
    let mut violations = Vec::new();

    for path in collect_files(Path::new("."))? {
        let normalized = normalize_path(&path);
        if !should_scan_static_language_path(&allowed, &normalized) {
            continue;
        }
        let text = read_text_lossy(&path)?;
        for (line_number, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            // A reasoned inline allow suppresses this line's forbidden terms.
            // It is finer-grained than a whole-file allowlist entry (smaller
            // bypass surface, reviewable in the diff) and must carry a reason.
            if line_has_static_language_inline_allow(&lower) {
                continue;
            }
            for term in &forbidden {
                if contains_word(&lower, term) {
                    violations.push(static_language_violation_message(
                        &normalized,
                        line_number + 1,
                        term,
                        line,
                    ));
                }
            }
        }
    }

    finish_policy_report(report_spec, &violations)
}

pub(crate) fn check_allow_attributes_impl() -> Result<(), String> {
    let allowlist = read_count_allowlist(".ripr/allow-attributes.txt")?;
    let guarded = guarded_allow_attribute_lints();
    let mut counts = BTreeMap::<(String, String), Vec<usize>>::new();

    for path in tracked_files()? {
        if !path.ends_with(".rs") {
            continue;
        }
        let file_path = Path::new(&path);
        if !file_path.exists() {
            continue;
        }
        let text = read_text_lossy(file_path)?;
        for (line, attribute) in guarded_allow_attributes_in_text(&text, &guarded) {
            counts
                .entry((path.clone(), attribute))
                .or_default()
                .push(line);
        }
    }

    let violations = allow_attribute_budget_violations(&allowlist, &counts, &guarded);

    finish_policy_report(
        PolicyReportSpec {
            report_file: "allow-attributes.md",
            check: "check-allow-attributes",
            why_it_matters: "Lint suppressions should not be used to hide repo guardrails. If a suppression is unavoidable, it needs a narrow reviewed exception with a reason. A count row higher than the current source count is the same unread budget.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Remove the lint suppression and fix the underlying warning.",
                "If the suppression is temporary and intentional, add a narrow allowlist entry with a reason.",
                "Tighten or remove a `.ripr/allow-attributes.txt` row whose max_count is higher than the current source count.",
                "Do not allowlist panic-family, unsafe, or broad warning suppressions unless the PR explicitly owns that exception.",
            ],
            rerun_command: "cargo xtask check-allow-attributes",
            exception_template: Some(
                ".ripr/allow-attributes.txt entry:\npath/to/file.rs|allow(clippy::unwrap_used)|1|reason",
            ),
        },
        &violations,
    )
}

pub(crate) fn allow_attribute_budget_violations(
    allowlist: &BTreeMap<(String, String), usize>,
    counts: &BTreeMap<(String, String), Vec<usize>>,
    guarded: &BTreeSet<&'static str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for ((path, attribute), lines) in counts {
        let allowed = allowlist
            .get(&(path.clone(), attribute.clone()))
            .copied()
            .unwrap_or(0);
        if lines.len() > allowed {
            violations.push(format!(
                "{path}:{} contains `{attribute}` {} time(s), allowed {allowed}\n  preferred: fix the lint or add a narrow allowlist entry with a reason",
                allow_attribute_line_summary(lines),
                lines.len()
            ));
        }
    }

    for ((path, attribute), allowed) in allowlist {
        if !guarded.contains(attribute_lint_name(attribute).unwrap_or(attribute)) {
            violations.push(format!(
                ".ripr/allow-attributes.txt contains unsupported guarded attribute `{attribute}` for {path}; remove stale or out-of-scope exceptions"
            ));
            continue;
        }
        let actual = counts
            .get(&(path.clone(), attribute.clone()))
            .map(Vec::len)
            .unwrap_or(0);
        if actual > *allowed {
            violations.push(format!(
                "{path} contains `{attribute}` {actual} time(s), allowed {allowed}"
            ));
        } else if actual < *allowed {
            // #3923: a row that outlives the suppressions it budgets is unread
            // debt. Exact equality is the steady state, matching check-local-context.
            violations.push(format!(
                "{path} `{attribute}` allowlist count is stale: found {actual}, allowed {allowed}; tighten max_count to {actual} or remove the entry"
            ));
        }
    }
    violations
}

pub(crate) fn check_local_context_impl() -> Result<(), String> {
    let allowlist = read_local_context_allowlist("policy/local_context_allowlist.txt")?;
    let mut violations = validate_local_context_allowlist(&allowlist);
    let mut grouped = BTreeMap::<(String, String), (BTreeSet<String>, Vec<Option<usize>>)>::new();

    for path in tracked_files()? {
        let file_path = Path::new(&path);
        if !file_path.exists() || path == "policy/local_context_allowlist.txt" {
            continue;
        }

        for finding in local_context_findings_for_path(&path)? {
            let entry = grouped
                .entry((finding.path.clone(), finding.pattern.clone()))
                .or_insert_with(|| (BTreeSet::new(), Vec::new()));
            entry.0.insert(finding.problem);
            entry.1.push(finding.line);
        }
    }

    let allowed = allowlist
        .iter()
        .map(|entry| ((entry.path.clone(), entry.pattern.clone()), entry.max_count))
        .collect::<BTreeMap<_, _>>();

    for ((path, pattern), (problems, lines)) in &grouped {
        let actual = lines.len();
        let allowed_count = allowed
            .get(&(path.clone(), pattern.clone()))
            .copied()
            .unwrap_or(0);
        if actual <= allowed_count && actual == 0 {
            continue;
        }
        if actual > allowed_count {
            let line_summary = local_context_line_summary(lines);
            violations.push(format!(
                "Path: {path}\nProblem: {}\nPattern: {pattern}\nCount: {actual}, allowed: {allowed_count}\nLines: {line_summary}\nWhy this matters: Repository docs should contain durable project state, not local runtime/session state from one machine or Codex run.\nRecommended fixes:\n1. Delete runtime/session artifacts instead of committing them.\n2. Move durable learnings to docs/LEARNINGS.md.\n3. Move generated state to target/ripr/reports, target/ripr/receipts, or target/ripr/learning.",
                problems.iter().cloned().collect::<Vec<_>>().join("; ")
            ));
        } else if actual < allowed_count && actual > 0 {
            // Stale bound: the allowlist grants more budget than the code uses
            // (#2413). Flag so the bound is tightened.
            violations.push(format!(
                "Path: {path}\nPattern: {pattern}\nStale allowlist bound: max_count={allowed_count} but actual={actual}; tighten to {actual}"
            ));
        }
    }

    // Flag orphaned allowlist entries whose path/pattern no longer appears in
    // any tracked file (count dropped to zero or the file was removed).
    for entry in &allowlist {
        let key = (entry.path.clone(), entry.pattern.clone());
        if !grouped.contains_key(&key) && entry.max_count > 0 {
            violations.push(format!(
                "Path: {}\nPattern: {}\nOrphaned allowlist entry: max_count={} but pattern not found in any tracked file; remove the entry",
                entry.path, entry.pattern, entry.max_count
            ));
        }
    }

    write_local_context_json(&violations)?;
    finish_policy_report(
        PolicyReportSpec {
            report_file: "local-context.md",
            check: "check-local-context",
            why_it_matters: "Repository state must be durable and portable. Machine paths, Codex memory paths, sandbox references, local transcripts, and session-state documents belong in generated artifacts or local notes, not committed repo knowledge.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Delete committed runtime/session artifacts.",
                "Move durable repo knowledge to docs/LEARNINGS.md or campaign/capability metadata.",
                "Move generated state to target/ripr/reports, target/ripr/receipts, or target/ripr/learning.",
                "Use policy/local_context_allowlist.txt only for narrow generic examples with a reason.",
            ],
            rerun_command: "cargo xtask check-local-context",
            exception_template: Some(
                "policy/local_context_allowlist.txt entry:\npath|pattern|max_count|reason",
            ),
        },
        &violations,
    )
}

pub(crate) fn check_workspace_shape() -> Result<(), String> {
    let records = read_pipe_records("policy/workspace_shape.txt", 3)?;
    let mut allowed_members = BTreeSet::new();
    let mut allowed_manifests = Vec::new();
    let mut violations = Vec::new();

    for record in records {
        match record[0].as_str() {
            "workspace_member" => {
                allowed_members.insert(record[1].clone());
            }
            "cargo_manifest" => allowed_manifests.push(GlobAllow {
                glob: record[1].clone(),
            }),
            other => violations.push(format!(
                "policy/workspace_shape.txt uses unsupported kind `{other}`"
            )),
        }
    }

    for member in workspace_members()? {
        if !allowed_members.contains(&member) {
            violations.push(format!(
                "workspace member is not allowlisted: {member}\n  preferred: keep one published `crates/ripr` package and `xtask` automation unless an ADR approves a new package"
            ));
        }
    }
    for member in &allowed_members {
        if !Path::new(member).exists() {
            violations.push(format!(
                "allowlisted workspace member does not exist: {member}"
            ));
        }
    }
    for file in tracked_files()? {
        if !file.ends_with("Cargo.toml") {
            continue;
        }
        if !matches_any_glob(&allowed_manifests, &file) {
            violations.push(format!(
                "Cargo manifest is not allowlisted by workspace shape policy: {file}"
            ));
        }
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "workspace-shape.md",
            check: "check-workspace-shape",
            why_it_matters: "ripr intentionally stays one published package with internal module seams; new packages need explicit review.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Keep product code inside crates/ripr unless an ADR approves a new package.",
                "Keep repo automation inside xtask.",
                "If a new Cargo manifest is truly needed, add a workspace-shape policy entry with owner and reason.",
            ],
            rerun_command: "cargo xtask check-workspace-shape",
            exception_template: Some("kind|path|reason"),
        },
        &violations,
    )
}

pub(crate) fn collect_clippy_debt_violations(
    debt_text: &str,
    cargo_text: &str,
    lints_text: &str,
    today: &str,
) -> Vec<String> {
    let (entries, mut violations) = parse_clippy_debt_ledger(debt_text);
    let cargo_clippy = match parse_workspace_lints_section(cargo_text, "clippy") {
        Ok(lints) => lints,
        Err(err) => {
            violations.push(err);
            return violations;
        }
    };
    let (lints, _) = parse_clippy_lints_ledger(lints_text);

    let mut seen_ids = BTreeSet::new();
    let mut seen_lints = BTreeSet::new();
    let active: BTreeSet<String> = lints
        .iter()
        .filter(|entry| !entry.is_planned)
        .map(|entry| entry.name.clone())
        .collect();
    let planned: BTreeSet<String> = lints
        .iter()
        .filter(|entry| entry.is_planned)
        .map(|entry| entry.name.clone())
        .collect();

    for entry in &entries {
        if !seen_ids.insert(entry.id.clone()) {
            violations.push(format!(
                "policy/clippy-debt.toml:{} duplicate debt id `{}`.",
                entry.block_line, entry.id
            ));
        }
        if !seen_lints.insert(entry.lint.clone()) {
            violations.push(format!(
                "policy/clippy-debt.toml:{} `{}` duplicates lint `{}`.",
                entry.block_line, entry.id, entry.lint
            ));
        }
        if !no_panic::is_valid_iso_date(&entry.target) {
            violations.push(format!(
                "policy/clippy-debt.toml:{} `{}` has malformed target `{}`; expected YYYY-MM-DD.",
                entry.block_line, entry.id, entry.target
            ));
        } else if entry.target.as_str() < today {
            violations.push(format!(
                "policy/clippy-debt.toml:{} `{}` has target `{}` before today `{today}`. Renew, remove, or pay the debt.",
                entry.block_line, entry.id, entry.target
            ));
        }
        let Some(bare) = entry
            .lint
            .strip_prefix("clippy::")
            .filter(|bare| !bare.is_empty())
        else {
            violations.push(format!(
                "policy/clippy-debt.toml:{} `{}` has lint `{}`; expected a `clippy::` lint name.",
                entry.block_line, entry.id, entry.lint
            ));
            continue;
        };
        if active.contains(&entry.lint) {
            violations.push(format!(
                "policy/clippy-debt.toml:{} `{}` records `{}` which is already `[[active]]` in policy/clippy-lints.toml. Remove the debt row or demote the active entry.",
                entry.block_line, entry.id, entry.lint
            ));
        }
        if planned.contains(&entry.lint) {
            violations.push(format!(
                "policy/clippy-debt.toml:{} `{}` records `{}` which is already `[[planned]]` in policy/clippy-lints.toml. Planned lints stay there; do not duplicate them into debt.",
                entry.block_line, entry.id, entry.lint
            ));
        }
        if let Some(level) = cargo_clippy.get(bare) {
            violations.push(format!(
                "policy/clippy-debt.toml:{} `{}` records `{}` which Cargo.toml `[workspace.lints.clippy]` already activates at level `{level}`. Pay the debt or remove the Cargo.toml line.",
                entry.block_line, entry.id, entry.lint
            ));
        }
    }

    violations
}

/// First basic TOML string on a value (`"..."` or `'...'`). Table-form and
/// multiline strings are intentionally out of scope for these ledgers.
/// Double-quoted strings decode basic escapes so `\n` is whitespace, not the
/// token `n`. Single-quoted strings stay literal, matching TOML.
pub(crate) fn unquote_toml_basic_string(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('"') {
        return rest
            .split_once('"')
            .map(|(token, _)| decode_toml_basic_escapes(token));
    }
    if let Some(rest) = value.strip_prefix('\'') {
        return rest.split_once('\'').map(|(token, _)| token.to_string());
    }
    None
}

/// Decode TOML basic-string escapes. Invalid sequences are left intact so
/// they still count as leftover tokens in the MSRV filter.
pub(crate) fn decode_toml_basic_escapes(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let bytes = token.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            let Some(ch) = token[index..].chars().next() else {
                break;
            };
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        let Some(esc) = bytes.get(index + 1).copied() else {
            out.push('\\');
            break;
        };
        let simple = match esc {
            b'b' => Some('\u{0008}'),
            b't' => Some('\t'),
            b'n' => Some('\n'),
            b'f' => Some('\u{000c}'),
            b'r' => Some('\r'),
            b'"' => Some('"'),
            b'\\' => Some('\\'),
            _ => None,
        };
        if let Some(ch) = simple {
            out.push(ch);
            index += 2;
            continue;
        }
        let width = match esc {
            b'u' => Some(4),
            b'U' => Some(8),
            _ => None,
        };
        if let Some(width) = width {
            let start = index + 2;
            let end = start + width;
            if end <= bytes.len()
                && let Ok(hex) = std::str::from_utf8(&bytes[start..end])
                && let Ok(code) = u32::from_str_radix(hex, 16)
                && let Some(ch) = char::from_u32(code)
            {
                out.push(ch);
                index = end;
                continue;
            }
        }
        out.push('\\');
        index += 1;
    }
    out
}

/// Parse `1.95` / `1.95.0` into a comparable triple. Rejects empty or extra parts.
pub(crate) fn parse_msrv_triple(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = match parts.next() {
        Some(part) => part.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// True when `text` is empty or only restates an MSRV/Rust-version delay.
///
/// This is a closed token filter, not NLP: version triples and a small
/// MSRV-vocabulary list are stripped, then any leftover token counts as a
/// remaining (non-MSRV) blocker. `check-lint-policy` applies it to
/// `[[planned]]` `blocked_by` once `activate_when_msrv` is already met.
/// `reason` is narrative and is not this gate.
pub(crate) fn planned_blocker_text_is_msrv_only(text: &str) -> bool {
    const MSRV_VOCABULARY: &[&str] = &[
        "a",
        "activate",
        "activation",
        "after",
        "already",
        "an",
        "and",
        "at",
        "available",
        "be",
        "before",
        "blocked",
        "blocker",
        "bump",
        "by",
        "exceeded",
        "exceeds",
        "for",
        "hit",
        "hits",
        "is",
        "met",
        "msrv",
        "need",
        "needs",
        "of",
        "on",
        "once",
        "or",
        "planned",
        "reached",
        "reaches",
        "required",
        "requires",
        "rust",
        "rustc",
        "rustversion",
        "since",
        "still",
        "the",
        "to",
        "toolchain",
        "until",
        "upgrade",
        "upgraded",
        "version",
        "wait",
        "waiting",
        "when",
    ];
    let leftover = text
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '.')
        .filter_map(|raw| {
            let token = raw.trim_matches('.');
            if token.is_empty() {
                None
            } else {
                Some(token.to_string())
            }
        })
        .filter(|token| parse_msrv_triple(token).is_none())
        .filter(|token| !MSRV_VOCABULARY.contains(&token.as_str()))
        .count();
    leftover == 0
}

pub(crate) fn collect_lint_policy_violations(cargo_text: &str, ledger_text: &str) -> Vec<String> {
    let cargo_clippy = match parse_workspace_lints_section(cargo_text, "clippy") {
        Ok(lints) => lints,
        Err(err) => return vec![err],
    };
    let cargo_rust = match parse_workspace_lints_section(cargo_text, "rust") {
        Ok(lints) => lints,
        Err(err) => return vec![err],
    };
    let (entries, mut violations) = parse_clippy_lints_ledger(ledger_text);
    let workspace_msrv = parse_workspace_package_rust_version(cargo_text);
    let workspace_triple = workspace_msrv.as_deref().and_then(parse_msrv_triple);

    for entry in &entries {
        let (bare, group) = ledger_name_to_lookup(&entry.name);
        let cargo_map = match group {
            "clippy" => &cargo_clippy,
            _ => &cargo_rust,
        };
        let actual = cargo_map.get(bare).cloned();
        if entry.is_planned {
            if let Some(level) = actual {
                violations.push(format!(
                    "policy/clippy-lints.toml:{} declares `{}` as `[[planned]]` but Cargo.toml `[workspace.lints.{}]` already activates it at level `{}`. Promote the ledger entry to `[[active.<group>]]` or remove the Cargo.toml line.",
                    entry.block_line, entry.name, group, level
                ));
            }
            if let Some(msrv) = entry.activate_when_msrv.as_deref() {
                match (parse_msrv_triple(msrv), workspace_triple, workspace_msrv.as_deref()) {
                    (None, _, _) => violations.push(format!(
                        "policy/clippy-lints.toml:{} `{}` has unparsable `activate_when_msrv = {msrv:?}`; expected `MAJOR.MINOR` or `MAJOR.MINOR.PATCH`.",
                        entry.block_line, entry.name
                    )),
                    (_, None, ws) => violations.push(format!(
                        "policy/clippy-lints.toml:{} `{}` records `activate_when_msrv = {msrv:?}` but Cargo.toml `[workspace.package]` has no parsable `rust-version` (found {ws:?}). The gate compares those two values.",
                        entry.block_line, entry.name
                    )),
                    (Some(activate), Some(workspace), Some(ws)) if activate <= workspace => {
                        // #3990: blocked_by is the remaining-blocker field.
                        // reason stays narrative and does not satisfy the gate.
                        let blocked_by = entry.blocked_by.as_deref().map(str::trim).unwrap_or("");
                        if blocked_by.is_empty() {
                            violations.push(format!(
                                "policy/clippy-lints.toml:{} `{}` has `activate_when_msrv = {msrv:?}` already met by workspace rust-version `{ws}`. Record a non-MSRV `blocked_by` explaining why it is still `[[planned]]`, or promote it.",
                                entry.block_line, entry.name
                            ));
                        } else if planned_blocker_text_is_msrv_only(blocked_by) {
                            violations.push(format!(
                                "policy/clippy-lints.toml:{} `{}` has `activate_when_msrv = {msrv:?}` already met by workspace rust-version `{ws}`. `blocked_by` {blocked_by:?} is MSRV-only. Record a remaining non-MSRV blocker, or promote it.",
                                entry.block_line, entry.name
                            ));
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }
        match actual {
            None => violations.push(format!(
                "policy/clippy-lints.toml:{} declares `{}` as active at level `{}` but Cargo.toml `[workspace.lints.{}]` does not configure it. Add the lint to Cargo.toml or move the ledger entry to `[[planned]]`.",
                entry.block_line, entry.name, entry.level, group
            )),
            Some(level) if level != entry.level => violations.push(format!(
                "policy/clippy-lints.toml:{} declares `{}` at level `{}` but Cargo.toml `[workspace.lints.{}]` configures it at level `{}`. Reconcile the two.",
                entry.block_line, entry.name, entry.level, group, level
            )),
            Some(_) => {}
        }
    }

    let active_lookup: BTreeSet<(String, String)> = entries
        .iter()
        .filter(|entry| !entry.is_planned)
        .map(|entry| {
            let (bare, group) = ledger_name_to_lookup(&entry.name);
            (group.to_string(), bare.to_string())
        })
        .collect();

    for (name, level) in cargo_clippy.iter().chain(cargo_rust.iter()) {
        let group = if cargo_clippy.contains_key(name) {
            "clippy"
        } else {
            "rust"
        };
        if !active_lookup.contains(&(group.to_string(), name.clone())) {
            violations.push(format!(
                "Cargo.toml `[workspace.lints.{group}]` configures `{name}` at level `{level}` but policy/clippy-lints.toml has no matching `[[active.<group>]]` entry. Add the lint to the ledger or remove it from Cargo.toml."
            ));
        }
    }

    violations
}

pub(crate) fn check_lint_policy() -> Result<(), String> {
    let cargo_text = fs::read_to_string("Cargo.toml")
        .map_err(|err| format!("failed to read Cargo.toml: {err}"))?;
    let ledger_text = fs::read_to_string("policy/clippy-lints.toml")
        .map_err(|err| format!("failed to read policy/clippy-lints.toml: {err}"))?;
    let debt_text = fs::read_to_string("policy/clippy-debt.toml")
        .map_err(|err| format!("failed to read policy/clippy-debt.toml: {err}"))?;
    let mut violations = collect_lint_policy_violations(&cargo_text, &ledger_text);
    violations.extend(collect_clippy_debt_violations(
        &debt_text,
        &cargo_text,
        &ledger_text,
        &no_panic::today_date_string(),
    ));
    match policy::workspace_lint_inheritance_violations() {
        Ok(member_violations) => violations.extend(member_violations),
        Err(error) => violations.push(error),
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "lint-policy.md",
            check: "check-lint-policy",
            why_it_matters: "`policy/clippy-lints.toml` is the reviewable ledger of the workspace lint stance, including planned 1.94 / 1.95 flips. If Cargo.toml drifts from the ledger, reviewers lose the trajectory and the dual-rail design (clippy + semantic checker) loses its receipt. `activate_when_msrv` is compared to `[workspace.package] rust-version`; an already-met MSRV without a remaining non-MSRV `blocked_by` is overdue. `reason` is narrative and does not satisfy that gate. `policy/clippy-debt.toml` is parsed as TOML: required nonblank fields, unknown fields, duplicate keys, trailing garbage, `target` dates, and dual-rail collisions fail here rather than being trusted as comments.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Make `Cargo.toml` and `policy/clippy-lints.toml` agree: every `[[active.<group>]]` entry must appear in `[workspace.lints.*]` at the same level, and `[[planned]]` entries must not yet appear there.",
                "When a planned lint's `activate_when_msrv` is already met by workspace `rust-version`, record a remaining non-MSRV `blocked_by` (not an MSRV-only delay) or promote the entry. `reason` does not satisfy that gate.",
                "When promoting a planned lint, move the ledger entry from `[[planned]]` to `[[active.<group>]]` and add the matching `Cargo.toml` line in the same PR.",
                "Keep `policy/clippy-debt.toml` rows unique, complete, and not already active, planned, or present in Cargo.toml. A past `target` must be renewed or the debt paid.",
                "Document `[[active.<group>]]` family blocks in `docs/CLIPPY_POLICY.md` so the public surface stays in sync.",
                "Every Cargo workspace member must opt in with `[lints] workspace = true`; remove member-local lint tables that replace or override the shared policy.",
            ],
            rerun_command: "cargo xtask check-lint-policy",
            exception_template: None,
        },
        &violations,
    )
}

#[derive(Clone, Debug)]
pub(crate) struct CiLedgerValue {
    pub(crate) line: usize,
    pub(crate) raw: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CiLedgerTable {
    pub(crate) header: String,
    pub(crate) line: usize,
    pub(crate) values: BTreeMap<String, CiLedgerValue>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CiLedgerDocument {
    pub(crate) top_level: BTreeMap<String, CiLedgerValue>,
    pub(crate) tables: Vec<CiLedgerTable>,
}

pub(crate) fn check_ci_lane_whitelist_impl() -> Result<(), String> {
    let mut violations = Vec::new();

    let budget = read_ci_ledger_document("policy/ci-budget.toml", &mut violations);
    let lanes = read_ci_ledger_document("policy/ci-lane-whitelist.toml", &mut violations);
    let risk_packs = read_ci_ledger_document("policy/ci-risk-packs.toml", &mut violations);
    let exceptions =
        read_ci_ledger_document("policy/ci-whitelist-exceptions.toml", &mut violations);

    if let (Some(budget), Some(lanes), Some(risk_packs), Some(exceptions)) =
        (&budget, &lanes, &risk_packs, &exceptions)
    {
        violations.extend(ci_lane_whitelist_violations(
            budget, lanes, risk_packs, exceptions,
        ));
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "ci-lane-whitelist.md",
            check: "check-ci-lane-whitelist",
            why_it_matters: "The CI lane ledgers are the reviewable contract for future PR planning, advisory evidence, artifact families, and budget actuals. They should stay structurally coherent before any workflow starts consuming them.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Keep `policy/ci-budget.toml`, `policy/ci-lane-whitelist.toml`, `policy/ci-risk-packs.toml`, and `policy/ci-whitelist-exceptions.toml` on schema version 0.1 with advisory, non-enforcing metadata.",
                "Add lane IDs and artifact families to `policy/ci-lane-whitelist.toml` before risk packs or exceptions reference them.",
                "Keep referenced lane IDs, artifact families, budget bands, owners, and reasons explicit before workflow or planner code consumes the ledgers.",
            ],
            rerun_command: "cargo xtask check-ci-lane-whitelist",
            exception_template: None,
        },
        &violations,
    )
}

pub(crate) fn read_ci_ledger_document(
    path: &str,
    violations: &mut Vec<String>,
) -> Option<CiLedgerDocument> {
    let path_ref = Path::new(path);
    if !path_ref.exists() {
        violations.push(format!("{path} is missing"));
        return None;
    }
    let text = match fs::read_to_string(path_ref) {
        Ok(text) => text,
        Err(err) => {
            violations.push(format!("failed to read {path}: {err}"));
            return None;
        }
    };
    let (document, mut parse_violations) = parse_ci_ledger_document(path, &text);
    violations.append(&mut parse_violations);
    Some(document)
}

pub(crate) fn parse_ci_ledger_document(path: &str, text: &str) -> (CiLedgerDocument, Vec<String>) {
    let mut document = CiLedgerDocument::default();
    let mut violations = Vec::new();
    let mut current: Option<CiLedgerTable> = None;

    for (index, raw) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with("[[") && line.ends_with("]]") {
            if let Some(table) = current.take() {
                document.tables.push(table);
            }
            let header = line
                .trim_start_matches("[[")
                .trim_end_matches("]]")
                .trim()
                .to_string();
            if header.is_empty() {
                violations.push(format!("{path}:{line_number} empty table header"));
                continue;
            }
            current = Some(CiLedgerTable {
                header,
                line: line_number,
                values: BTreeMap::new(),
            });
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            if let Some(table) = current.take() {
                document.tables.push(table);
            }
            let header = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            if header.is_empty() {
                violations.push(format!("{path}:{line_number} empty table header"));
                continue;
            }
            current = Some(CiLedgerTable {
                header,
                line: line_number,
                values: BTreeMap::new(),
            });
            continue;
        }

        let Some((key, value)) = parse_toml_key_value(line) else {
            violations.push(format!(
                "{path}:{line_number} invalid TOML line (expected key = value)"
            ));
            continue;
        };
        let value = strip_toml_value_comment(value).trim().to_string();
        if let Some(table) = current.as_mut() {
            insert_ci_ledger_value(
                path,
                &mut table.values,
                key,
                value,
                line_number,
                &mut violations,
            );
        } else {
            insert_ci_ledger_value(
                path,
                &mut document.top_level,
                key,
                value,
                line_number,
                &mut violations,
            );
        }
    }

    if let Some(table) = current {
        document.tables.push(table);
    }

    (document, violations)
}

pub(crate) fn insert_ci_ledger_value(
    path: &str,
    values: &mut BTreeMap<String, CiLedgerValue>,
    key: &str,
    raw: String,
    line: usize,
    violations: &mut Vec<String>,
) {
    if values
        .insert(key.to_string(), CiLedgerValue { line, raw })
        .is_some()
    {
        violations.push(format!("{path}:{line} duplicate key `{key}`"));
    }
}

pub(crate) fn ci_lane_whitelist_violations(
    budget: &CiLedgerDocument,
    lanes: &CiLedgerDocument,
    risk_packs: &CiLedgerDocument,
    exceptions: &CiLedgerDocument,
) -> Vec<String> {
    let mut violations = Vec::new();

    validate_ci_common_metadata("policy/ci-budget.toml", budget, &mut violations);
    validate_ci_common_metadata("policy/ci-lane-whitelist.toml", lanes, &mut violations);
    validate_ci_common_metadata("policy/ci-risk-packs.toml", risk_packs, &mut violations);
    validate_ci_common_metadata(
        "policy/ci-whitelist-exceptions.toml",
        exceptions,
        &mut violations,
    );

    let budget_bands = validate_ci_budget("policy/ci-budget.toml", budget, &mut violations);
    let (lane_ids, _lane_postures, artifact_ids) =
        validate_ci_lanes("policy/ci-lane-whitelist.toml", lanes, &mut violations);
    validate_ci_risk_packs(
        "policy/ci-risk-packs.toml",
        risk_packs,
        &lane_ids,
        &artifact_ids,
        &mut violations,
    );
    validate_ci_exceptions(
        "policy/ci-whitelist-exceptions.toml",
        exceptions,
        &lane_ids,
        &artifact_ids,
        &mut violations,
    );
    validate_ci_budget_references(
        "policy/ci-budget.toml",
        budget,
        &budget_bands,
        &mut violations,
    );

    violations
}

pub(crate) fn validate_ci_common_metadata(
    path: &str,
    document: &CiLedgerDocument,
    violations: &mut Vec<String>,
) {
    ci_expect_top_string(path, document, "schema_version", "0.1", violations);
    ci_expect_top_string(
        path,
        document,
        "policy_state",
        "advisory-ledger",
        violations,
    );
    ci_expect_top_string(path, document, "enforcement", "none", violations);
    ci_required_non_empty_top_string(path, document, "owner", violations);
    ci_required_non_empty_top_string(path, document, "reason", violations);
}

pub(crate) fn validate_ci_budget(
    path: &str,
    document: &CiLedgerDocument,
    violations: &mut Vec<String>,
) -> BTreeSet<String> {
    ci_expect_top_string(path, document, "unit", "lem", violations);

    let mut budget_bands = BTreeSet::new();
    let bands = ci_tables(document, "budget_band");
    if bands.is_empty() {
        violations.push(format!("{path} has no [[budget_band]] entries"));
    }
    for table in bands {
        let Some(id) = ci_required_table_id(path, table, "id", "budget band", violations) else {
            continue;
        };
        if !budget_bands.insert(id.clone()) {
            violations.push(format!(
                "{path}:{} duplicate budget band id `{id}`",
                table.line
            ));
        }
        let min = ci_required_table_usize(path, table, "min_lem", violations);
        let max = ci_optional_table_usize(path, table, "max_lem", violations);
        if let (Some(min), Some(max)) = (min, max)
            && max < min
        {
            violations.push(format!(
                "{path}:{} budget band `{id}` has max_lem below min_lem",
                table.line
            ));
        }
        ci_required_non_empty_table_string(path, table, "posture", violations);
        ci_required_non_empty_table_string(path, table, "description", violations);
    }

    let labels = ci_tables(document, "label");
    if labels.is_empty() {
        violations.push(format!("{path} has no [[label]] entries"));
    }
    let mut label_names = BTreeSet::new();
    for table in labels {
        let Some(name) = ci_required_non_empty_table_string(path, table, "name", violations) else {
            continue;
        };
        if !label_names.insert(name.clone()) {
            violations.push(format!("{path}:{} duplicate label `{name}`", table.line));
        }
        ci_required_non_empty_table_string(path, table, "effect", violations);
        ci_required_non_empty_table_string(path, table, "budget_effect", violations);
        ci_required_non_empty_table_string(path, table, "notes", violations);
    }

    budget_bands
}

pub(crate) fn validate_ci_budget_references(
    path: &str,
    document: &CiLedgerDocument,
    budget_bands: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    let defaults = ci_tables(document, "defaults");
    if defaults.len() != 1 {
        violations.push(format!("{path} should have exactly one [defaults] table"));
    }
    if let Some(table) = defaults.first() {
        for key in ["required_pr_budget", "advisory_pr_budget", "release_budget"] {
            if let Some(value) = ci_required_non_empty_table_string(path, table, key, violations)
                && !budget_bands.contains(&value)
            {
                violations.push(format!(
                    "{path}:{} defaults `{key}` references unknown budget band `{value}`",
                    table.line
                ));
            }
        }
    }

    for table in ci_tables(document, "label") {
        if let Some(effect) =
            ci_required_non_empty_table_string(path, table, "budget_effect", violations)
            && effect != "none"
            && !budget_bands.contains(&effect)
        {
            violations.push(format!(
                "{path}:{} label budget_effect references unknown budget band `{effect}`",
                table.line
            ));
        }
    }
}

pub(crate) fn validate_ci_lanes(
    path: &str,
    document: &CiLedgerDocument,
    violations: &mut Vec<String>,
) -> (BTreeSet<String>, BTreeMap<String, String>, BTreeSet<String>) {
    let mut artifact_ids = BTreeSet::new();
    let artifacts = ci_tables(document, "artifact_family");
    if artifacts.is_empty() {
        violations.push(format!("{path} has no [[artifact_family]] entries"));
    }
    for table in artifacts {
        let Some(id) = ci_required_table_id(path, table, "id", "artifact family", violations)
        else {
            continue;
        };
        if !artifact_ids.insert(id.clone()) {
            violations.push(format!(
                "{path}:{} duplicate artifact family id `{id}`",
                table.line
            ));
        }
        if let Some(paths) = ci_required_table_array(path, table, "paths", violations)
            && paths.is_empty()
        {
            violations.push(format!(
                "{path}:{} artifact family `{id}` has empty paths",
                table.line
            ));
        }
    }

    let mut lane_ids = BTreeSet::new();
    let mut lane_postures = BTreeMap::new();
    let lanes = ci_tables(document, "lane");
    if lanes.is_empty() {
        violations.push(format!("{path} has no [[lane]] entries"));
    }
    for table in lanes {
        let Some(id) = ci_required_table_id(path, table, "id", "lane", violations) else {
            continue;
        };
        if !lane_ids.insert(id.clone()) {
            violations.push(format!("{path}:{} duplicate lane id `{id}`", table.line));
        }
        let posture = ci_required_non_empty_table_string(path, table, "posture", violations);
        if let Some(posture) = posture {
            if !matches!(
                posture.as_str(),
                "required" | "advisory" | "on_demand_release"
            ) {
                violations.push(format!(
                    "{path}:{} lane `{id}` has unsupported posture `{posture}`",
                    table.line
                ));
            }
            lane_postures.insert(id.clone(), posture);
        }
        ci_required_non_empty_table_string(path, table, "workflow", violations);
        ci_required_table_array(path, table, "jobs", violations);
        ci_required_table_array(path, table, "commands", violations);
        ci_required_table_usize(path, table, "estimated_lem", violations);
        if let Some(families) =
            ci_required_table_array(path, table, "artifact_families", violations)
        {
            validate_ci_artifact_references(
                path,
                table.line,
                &format!("lane `{id}`"),
                &families,
                &artifact_ids,
                violations,
            );
        }
        ci_required_non_empty_table_string(path, table, "description", violations);
    }

    (lane_ids, lane_postures, artifact_ids)
}

pub(crate) fn validate_ci_risk_packs(
    path: &str,
    document: &CiLedgerDocument,
    lane_ids: &BTreeSet<String>,
    artifact_ids: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    let risk_packs: Vec<&CiLedgerTable> = document
        .tables
        .iter()
        .filter(|table| table.header.starts_with("risk_pack."))
        .collect();
    if risk_packs.is_empty() {
        violations.push(format!("{path} has no [risk_pack.<id>] entries"));
    }

    let mut pack_ids = BTreeSet::new();
    for table in risk_packs {
        let id = table.header.trim_start_matches("risk_pack.");
        if !is_snake_case_id(id) {
            violations.push(format!(
                "{path}:{} risk pack id `{id}` should be snake_case",
                table.line
            ));
        }
        if !pack_ids.insert(id.to_string()) {
            violations.push(format!(
                "{path}:{} duplicate risk pack id `{id}`",
                table.line
            ));
        }
        if let Some(paths) = ci_required_table_array(path, table, "paths", violations)
            && paths.is_empty()
        {
            violations.push(format!(
                "{path}:{} risk pack `{id}` has empty paths",
                table.line
            ));
        }
        validate_ci_lane_bucket(path, table, id, "required", lane_ids, violations);
        validate_ci_lane_bucket(path, table, id, "advisory", lane_ids, violations);
        validate_ci_lane_bucket(path, table, id, "on_demand", lane_ids, violations);
        if let Some(families) =
            ci_required_table_array(path, table, "artifact_families", violations)
        {
            validate_ci_artifact_references(
                path,
                table.line,
                &format!("risk pack `{id}`"),
                &families,
                artifact_ids,
                violations,
            );
        }
        ci_required_table_usize(path, table, "estimated_lem", violations);
        ci_required_non_empty_table_string(path, table, "owner", violations);
        ci_required_non_empty_table_string(path, table, "reason", violations);
    }
}

pub(crate) fn validate_ci_exceptions(
    path: &str,
    document: &CiLedgerDocument,
    lane_ids: &BTreeSet<String>,
    artifact_ids: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    let exceptions = ci_tables(document, "exception");
    if exceptions.is_empty() {
        violations.push(format!("{path} has no [[exception]] entries"));
    }

    let mut exception_ids = BTreeSet::new();
    for table in exceptions {
        let Some(id) = ci_required_non_empty_table_string(path, table, "id", violations) else {
            continue;
        };
        if !exception_ids.insert(id.clone()) {
            violations.push(format!(
                "{path}:{} duplicate exception id `{id}`",
                table.line
            ));
        }
        ci_required_non_empty_table_string(path, table, "kind", violations);
        if let Some(paths) = ci_required_table_array(path, table, "paths", violations)
            && paths.is_empty()
        {
            violations.push(format!(
                "{path}:{} exception `{id}` has empty paths",
                table.line
            ));
        }
        ci_required_non_empty_table_string(path, table, "current_behavior", violations);
        ci_required_non_empty_table_string(path, table, "target_behavior", violations);
        ci_required_non_empty_table_string(path, table, "review_note", violations);

        if let Some(lanes) = ci_optional_table_array(path, table, "lanes", violations) {
            validate_ci_lane_references(
                path,
                table.line,
                &format!("exception `{id}`"),
                &lanes,
                lane_ids,
                violations,
            );
        }
        if let Some(families) =
            ci_optional_table_array(path, table, "artifact_families", violations)
        {
            validate_ci_artifact_references(
                path,
                table.line,
                &format!("exception `{id}`"),
                &families,
                artifact_ids,
                violations,
            );
        }
    }
}

pub(crate) fn validate_ci_lane_bucket(
    path: &str,
    table: &CiLedgerTable,
    risk_pack_id: &str,
    key: &str,
    lane_ids: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    let Some(lanes) = ci_required_table_array(path, table, key, violations) else {
        return;
    };
    for lane in lanes {
        if !lane_ids.contains(&lane) {
            violations.push(format!(
                "{path}:{} risk pack `{risk_pack_id}` references unknown lane `{lane}` in `{key}`",
                table.line
            ));
        }
    }
}

pub(crate) fn validate_ci_lane_references(
    path: &str,
    line: usize,
    label: &str,
    lanes: &[String],
    lane_ids: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    for lane in lanes {
        if !lane_ids.contains(lane) {
            violations.push(format!(
                "{path}:{line} {label} references unknown lane `{lane}`"
            ));
        }
    }
}

pub(crate) fn validate_ci_artifact_references(
    path: &str,
    line: usize,
    label: &str,
    families: &[String],
    artifact_ids: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    for family in families {
        if !artifact_ids.contains(family) {
            violations.push(format!(
                "{path}:{line} {label} references unknown artifact family `{family}`"
            ));
        }
    }
}

pub(crate) fn ci_expect_top_string(
    path: &str,
    document: &CiLedgerDocument,
    key: &str,
    expected: &str,
    violations: &mut Vec<String>,
) {
    if let Some(actual) = ci_required_non_empty_top_string(path, document, key, violations)
        && actual != expected
    {
        violations.push(format!(
            "{path}: top-level `{key}` should be `{expected}`, got `{actual}`"
        ));
    }
}

pub(crate) fn ci_required_non_empty_top_string(
    path: &str,
    document: &CiLedgerDocument,
    key: &str,
    violations: &mut Vec<String>,
) -> Option<String> {
    let Some(value) = document.top_level.get(key) else {
        violations.push(format!("{path}: missing top-level `{key}`"));
        return None;
    };
    ci_string_value(path, "top-level", key, value, violations).and_then(|parsed| {
        if parsed.trim().is_empty() {
            violations.push(format!("{path}:{} top-level `{key}` is empty", value.line));
            None
        } else {
            Some(parsed)
        }
    })
}

pub(crate) fn ci_required_table_id(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    label: &str,
    violations: &mut Vec<String>,
) -> Option<String> {
    let value = ci_required_non_empty_table_string(path, table, key, violations)?;
    if !is_kebab_case_id(&value) {
        violations.push(format!(
            "{path}:{} {label} id `{value}` should be kebab-case",
            table.line
        ));
    }
    Some(value)
}

pub(crate) fn ci_required_non_empty_table_string(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    violations: &mut Vec<String>,
) -> Option<String> {
    let Some(value) = table.values.get(key) else {
        violations.push(format!(
            "{path}:{} table `[{}]` missing `{key}`",
            table.line, table.header
        ));
        return None;
    };
    ci_string_value(
        path,
        &format!("table `[{}]`", table.header),
        key,
        value,
        violations,
    )
    .and_then(|parsed| {
        if parsed.trim().is_empty() {
            violations.push(format!(
                "{path}:{} table `[{}]` field `{key}` is empty",
                value.line, table.header
            ));
            None
        } else {
            Some(parsed)
        }
    })
}

pub(crate) fn ci_string_value(
    path: &str,
    label: &str,
    key: &str,
    value: &CiLedgerValue,
    violations: &mut Vec<String>,
) -> Option<String> {
    match parse_string_value(&value.raw, path, value.line) {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            violations.push(format!("{path}:{} {label} `{key}`: {err}", value.line));
            None
        }
    }
}

pub(crate) fn ci_required_table_array(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    violations: &mut Vec<String>,
) -> Option<Vec<String>> {
    let Some(value) = table.values.get(key) else {
        violations.push(format!(
            "{path}:{} table `[{}]` missing `{key}`",
            table.line, table.header
        ));
        return None;
    };
    ci_array_value(path, table, key, value, violations)
}

pub(crate) fn ci_optional_table_array(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    violations: &mut Vec<String>,
) -> Option<Vec<String>> {
    let value = table.values.get(key)?;
    ci_array_value(path, table, key, value, violations)
}

pub(crate) fn ci_array_value(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    value: &CiLedgerValue,
    violations: &mut Vec<String>,
) -> Option<Vec<String>> {
    match parse_inline_array(&value.raw) {
        Ok(values) => Some(values),
        Err(err) => {
            violations.push(format!(
                "{path}:{} table `[{}]` field `{key}`: {err}",
                value.line, table.header
            ));
            None
        }
    }
}

pub(crate) fn ci_required_table_usize(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    violations: &mut Vec<String>,
) -> Option<usize> {
    let Some(value) = table.values.get(key) else {
        violations.push(format!(
            "{path}:{} table `[{}]` missing `{key}`",
            table.line, table.header
        ));
        return None;
    };
    ci_usize_value(path, table, key, value, violations)
}

pub(crate) fn ci_optional_table_usize(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    violations: &mut Vec<String>,
) -> Option<usize> {
    let value = table.values.get(key)?;
    ci_usize_value(path, table, key, value, violations)
}

pub(crate) fn ci_usize_value(
    path: &str,
    table: &CiLedgerTable,
    key: &str,
    value: &CiLedgerValue,
    violations: &mut Vec<String>,
) -> Option<usize> {
    match parse_usize_value(&value.raw, path, value.line) {
        Ok(parsed) => Some(parsed),
        Err(err) => {
            violations.push(format!(
                "{path}:{} table `[{}]` field `{key}`: {err}",
                value.line, table.header
            ));
            None
        }
    }
}

pub(crate) fn ci_tables<'a>(
    document: &'a CiLedgerDocument,
    header: &str,
) -> Vec<&'a CiLedgerTable> {
    document
        .tables
        .iter()
        .filter(|table| table.header == header)
        .collect()
}

pub(crate) const PROOF_PACK_MANIFEST_PATH: &str = "policy/proof-packs.toml";
pub(crate) const PROOF_PACK_LANE_WHITELIST_PATH: &str = "policy/ci-lane-whitelist.toml";
const PROOF_PACK_RELEASE_PACK_ID: &str = "release-package";
const PROOF_PACK_OUTPUT_CONTRACTS_ID: &str = "output-contracts";

/// Release surfaces the `release-package` pack MUST cover so that any change to
/// a version file, the changelog, or a release workflow trips
/// `release_proof_required`. Asserted by `check-proof-packs` (the release-proof
/// protection slice of docs/PROOF_ROUTING.md) so a future routing slice cannot
/// weaken release detection by dropping a surface from the pack.
const PROOF_PACK_RELEASE_REQUIRED_PATHS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "crates/ripr/Cargo.toml",
    "CHANGELOG.md",
    "editors/vscode/package.json",
    "editors/vscode/package-lock.json",
    ".github/workflows/publish-extension.yml",
    ".github/workflows/release-server-binaries.yml",
    ".github/workflows/package-family-qualification.yml",
];

/// The full release proof the `release-package` pack MUST require: workspace
/// tests, clippy, check-pr, package contents, publish dry-run, and the
/// release-readiness gate (release-notes and known-limits docs validation).
/// Asserted by `check-proof-packs` so a future routing slice cannot weaken
/// release validation by dropping a gate from the pack.
const PROOF_PACK_RELEASE_REQUIRED_COMMANDS: &[&str] = &[
    "cargo test --workspace",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo xtask check-pr",
    "cargo package -p ripr --list",
    "cargo xtask publish-dry-run",
    "cargo xtask release-readiness",
];

/// Non-xtask repo commands a proof pack may reference. `cargo xtask <command>`
/// entries are validated against the known xtask command roots instead.
const PROOF_PACK_KNOWN_REPO_COMMANDS: &[&str] = &[
    "cargo fmt --check",
    "cargo check --workspace --all-targets",
    "cargo test --workspace",
    "cargo test -p ripr",
    "cargo test -p ripr-contract",
    "cargo test -p xtask",
    "cargo clippy --workspace --all-targets -- -D warnings",
    "cargo doc --workspace --no-deps",
    "cargo package -p ripr --list",
    "cargo xtask publish-dry-run",
];

pub(crate) fn check_proof_packs_impl() -> Result<(), String> {
    let mut violations = Vec::new();

    let manifest = read_ci_ledger_document(PROOF_PACK_MANIFEST_PATH, &mut violations);
    let lanes = read_ci_ledger_document(PROOF_PACK_LANE_WHITELIST_PATH, &mut violations);

    if let (Some(manifest), Some(lanes)) = (&manifest, &lanes) {
        let lane_ids = proof_pack_lane_ids(lanes);
        let lane_commands = proof_pack_lane_commands(lanes);
        violations.extend(proof_pack_violations(manifest, &lane_ids));
        violations.extend(proof_pack_output_contracts_lane_violations(
            manifest,
            &lane_commands,
        ));
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "proof-packs.md",
            check: "check-proof-packs",
            why_it_matters: "policy/proof-packs.toml is the routing unit for proof-aware validation (docs/PROOF_ROUTING.md). While state is manifest-only, nothing routes on it yet, but the manifest must stay parseable, name only real repo commands and CI lanes, and keep the release-package pack pinned to full proof before any routing behavior consumes it.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Keep `policy/proof-packs.toml` on version 0.1 with `state = \"manifest-only\"` and `unknown_surface_policy = \"full-proof\"`.",
                "Give every pack a unique kebab-case id, at least one path, and at least one required command.",
                "Reference only known `cargo xtask` commands or known repo commands, and a `ci_lane` declared in `policy/ci-lane-whitelist.toml`.",
                "Keep `never_routed = true` on the release-package pack; release proof is never routed away.",
                "Keep the release-package pack covering every release surface (version files, changelog, release workflows) and requiring the full release proof (workspace tests, clippy, check-pr, package, publish dry-run, release-readiness).",
            ],
            rerun_command: "cargo xtask check-proof-packs",
            exception_template: None,
        },
        &violations,
    )
}

pub(crate) fn proof_pack_lane_commands(
    lanes: &CiLedgerDocument,
) -> BTreeMap<String, BTreeSet<String>> {
    let path = PROOF_PACK_LANE_WHITELIST_PATH;
    let mut lane_commands = BTreeMap::new();
    let mut ignored = Vec::new();
    for table in ci_tables(lanes, "lane") {
        let Some(id_value) = table.values.get("id") else {
            continue;
        };
        let Some(id) = ci_string_value(path, "table `[[lane]]`", "id", id_value, &mut ignored)
        else {
            continue;
        };
        let mut parse_violations = Vec::new();
        let Some(commands) =
            ci_required_table_array(path, table, "commands", &mut parse_violations)
        else {
            continue;
        };
        lane_commands.insert(id, commands.into_iter().collect());
    }
    lane_commands
}

pub(crate) fn proof_pack_output_contracts_lane_violations(
    manifest: &CiLedgerDocument,
    lane_commands: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<String> {
    let path = PROOF_PACK_MANIFEST_PATH;
    let mut violations = Vec::new();
    let mut ignored = Vec::new();
    let packs = policy::proof_packs::parse_proof_packs(manifest, &mut ignored);
    let Some(pack) = packs
        .iter()
        .find(|pack| pack.id == PROOF_PACK_OUTPUT_CONTRACTS_ID)
    else {
        violations.push(format!(
            "{path} must define an `{PROOF_PACK_OUTPUT_CONTRACTS_ID}` pack (P18 output-contracts routing slice)"
        ));
        return violations;
    };
    let Some(ci_lane) = &pack.ci_lane else {
        violations.push(format!(
            "{path}:{} proof pack `{PROOF_PACK_OUTPUT_CONTRACTS_ID}` must declare `ci_lane`",
            pack.line
        ));
        return violations;
    };
    let Some(lane_commands) = lane_commands.get(ci_lane) else {
        violations.push(format!(
            "{path}:{} proof pack `{PROOF_PACK_OUTPUT_CONTRACTS_ID}` references unknown ci_lane `{ci_lane}`",
            pack.line
        ));
        return violations;
    };
    for required in &pack.required_commands {
        if !lane_commands.contains(required) {
            violations.push(format!(
                "{path}:{} proof pack `{PROOF_PACK_OUTPUT_CONTRACTS_ID}` requires `{required}` but ci_lane `{ci_lane}` at {PROOF_PACK_LANE_WHITELIST_PATH} does not list it\n  rule: P18 Correction #3 — the lane must run every required command the pack names (docs/PROOF_ROUTING.md slice 8)",
                pack.line
            ));
        }
    }
    violations
}

pub(crate) fn proof_pack_lane_ids(lanes: &CiLedgerDocument) -> BTreeSet<String> {
    let mut ignored = Vec::new();
    ci_tables(lanes, "lane")
        .into_iter()
        .filter_map(|table| {
            let value = table.values.get("id")?;
            ci_string_value(
                PROOF_PACK_LANE_WHITELIST_PATH,
                "table `[[lane]]`",
                "id",
                value,
                &mut ignored,
            )
        })
        .collect()
}

pub(crate) fn proof_pack_violations(
    manifest: &CiLedgerDocument,
    lane_ids: &BTreeSet<String>,
) -> Vec<String> {
    let path = PROOF_PACK_MANIFEST_PATH;
    let mut violations = Vec::new();

    ci_expect_top_string(path, manifest, "version", "0.1", &mut violations);
    ci_expect_top_string(path, manifest, "state", "manifest-only", &mut violations);
    ci_expect_top_string(
        path,
        manifest,
        "unknown_surface_policy",
        "full-proof",
        &mut violations,
    );

    if ci_tables(manifest, "pack").is_empty() {
        violations.push(format!("{path} has no [[pack]] entries"));
    }

    let packs = policy::proof_packs::parse_proof_packs(manifest, &mut violations);

    let mut seen_ids = BTreeSet::new();
    let mut release_pack_seen = false;
    for pack in &packs {
        if !seen_ids.insert(pack.id.clone()) {
            violations.push(format!(
                "{path}:{} duplicate proof pack id `{}`",
                pack.line, pack.id
            ));
        }

        for command in &pack.required_commands {
            proof_pack_command_violation(
                path,
                pack.line,
                &pack.id,
                "required_commands",
                command,
                &mut violations,
            );
        }
        for command in &pack.advisory_commands {
            proof_pack_command_violation(
                path,
                pack.line,
                &pack.id,
                "advisory_commands",
                command,
                &mut violations,
            );
        }

        if let Some(ci_lane) = &pack.ci_lane
            && !lane_ids.contains(ci_lane)
        {
            violations.push(format!(
                "{path}:{} proof pack `{}` references unknown ci_lane `{ci_lane}`\n  rule: ci_lane must be a lane id declared in {PROOF_PACK_LANE_WHITELIST_PATH}",
                pack.line, pack.id
            ));
        }

        if pack.id == PROOF_PACK_RELEASE_PACK_ID {
            release_pack_seen = true;
            if !pack.never_routed {
                violations.push(format!(
                    "{path}:{} proof pack `{PROOF_PACK_RELEASE_PACK_ID}` must set `never_routed = true`\n  rule: release proof is never routed away (docs/PROOF_ROUTING.md)",
                    pack.line
                ));
            }
            for surface in PROOF_PACK_RELEASE_REQUIRED_PATHS {
                if !pack.paths.iter().any(|covered| covered == surface) {
                    violations.push(format!(
                        "{path}:{} proof pack `{PROOF_PACK_RELEASE_PACK_ID}` must cover release surface `{surface}`\n  rule: every release surface (version files, changelog, release workflows) must trip release_proof_required (docs/PROOF_ROUTING.md, release-proof protection slice)",
                        pack.line
                    ));
                }
            }
            for required in PROOF_PACK_RELEASE_REQUIRED_COMMANDS {
                if !pack.required_commands.iter().any(|cmd| cmd == required) {
                    violations.push(format!(
                        "{path}:{} proof pack `{PROOF_PACK_RELEASE_PACK_ID}` must require `{required}`\n  rule: the release-package pack must name the full release proof (docs/PROOF_ROUTING.md, release-proof protection slice)",
                        pack.line
                    ));
                }
            }
        }
    }

    if !release_pack_seen {
        violations.push(format!(
            "{path} must define a `{PROOF_PACK_RELEASE_PACK_ID}` pack with `never_routed = true`"
        ));
    }

    violations
}

pub(crate) fn proof_pack_command_violation(
    path: &str,
    line: usize,
    pack_id: &str,
    field: &str,
    command: &str,
    violations: &mut Vec<String>,
) {
    if proof_pack_command_is_known(command) {
        return;
    }
    violations.push(format!(
        "{path}:{line} proof pack `{pack_id}` field `{field}` names unknown repo command `{command}`\n  rule: use a `cargo xtask <command>` from `cargo xtask help` or a known repo proof command"
    ));
}

pub(crate) fn proof_pack_command_is_known(command: &str) -> bool {
    let trimmed = command.trim();
    if let Some(rest) = trimmed.strip_prefix("cargo xtask ") {
        let root = known_command_root(rest.trim());
        return !root.is_empty()
            && known_commands()
                .into_iter()
                .any(|known| known_command_root(known) == root);
    }
    PROOF_PACK_KNOWN_REPO_COMMANDS.contains(&trimmed)
}

pub(crate) fn check_process_policy_impl() -> Result<(), String> {
    check_count_policy(
        "process policy",
        "policy/process_allowlist.txt",
        &process_policy_patterns(),
        is_process_policy_candidate,
    )
}

pub(crate) fn check_network_policy_impl() -> Result<(), String> {
    check_count_policy(
        "network policy",
        "policy/network_allowlist.txt",
        &network_policy_patterns(),
        is_network_policy_candidate,
    )
}

pub(crate) fn check_count_policy(
    label: &str,
    allowlist_path: &str,
    patterns: &[String],
    is_candidate: fn(&str) -> bool,
) -> Result<(), String> {
    let allowlist = read_count_policy_allowlist(allowlist_path)?;
    let fragment_dir = normalize_path(&Path::new(allowlist_path).with_extension("d"));
    let mut counts = BTreeMap::<(String, String), usize>::new();

    for normalized in tracked_files()? {
        if !is_candidate(&normalized) {
            continue;
        }
        let text = read_text_lossy(Path::new(&normalized))?;
        for pattern in patterns {
            let count = text.matches(pattern).count();
            if count > 0 {
                counts.insert((normalized.clone(), pattern.clone()), count);
            }
        }
    }

    let mut violations = Vec::new();
    for ((path, pattern), count) in &counts {
        let allowed = allowlist
            .get(&(path.clone(), pattern.clone()))
            .copied()
            .unwrap_or(0);
        if *count > allowed {
            violations.push(format!(
                "{path} contains `{pattern}` {count} time(s), allowed {allowed}\n  to allowlist, add a file under {fragment_dir}/ (named for this PR, e.g. <issue>-<slug>.txt) holding:\n  {path}|{pattern}|{count}|owner|reason"
            ));
        } else if *count < allowed {
            // Stale bound: the allowlist grants more budget than the code
            // uses. This is the reverse-direction check that prevents the
            // bound from silently rotting (#2413). A stale bound lets new
            // occurrences slip through without an allowlist update, so flag
            // it so the bound is tightened to the actual count.
            violations.push(format!(
                "{path} allowlist entry for `{pattern}` is stale: max_count={allowed} but actual={count}; tighten to {count}"
            ));
        }
    }

    // Also flag allowlist entries for paths that no longer contain the pattern
    // at all (count dropped to zero, or the file was removed). These are fully
    // orphaned entries whose bound provides invisible slack.
    for ((path, pattern), allowed) in &allowlist {
        if !counts.contains_key(&(path.clone(), pattern.clone())) && *allowed > 0 {
            violations.push(format!(
                "{path} allowlist entry for `{pattern}` is orphaned: max_count={allowed} but pattern not found in any tracked file; remove the entry"
            ));
        }
    }

    let report_file = format!("{}.md", label.replace(' ', "-"));
    let why = format!(
        "{label} entries are explicit because hidden side effects make automation and analyzer behavior harder to review."
    );
    let template = format!("{allowlist_path} entry:\npath|pattern|max_count|owner|reason");
    let check = format!("check-{}", label.replace(' ', "-"));
    let rerun_command = format!("cargo xtask {check}");
    finish_policy_report(
        PolicyReportSpec {
            report_file: &report_file,
            check: &check,
            why_it_matters: &why,
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Move the behavior to the approved adapter or automation surface.",
                "Reduce the process or network usage when it is not required.",
                "Add an allowlist entry only when the behavior is intentional and owned.",
            ],
            rerun_command: &rerun_command,
            exception_template: Some(&template),
        },
        &violations,
    )
}

/// Tracked-file inventory via `git ls-files -z`, decoded by the local
/// NUL-record parser below instead of `ripr::analysis::parse_git_*`, so the
/// policy cluster does not depend on the product crate for path decoding.
pub(crate) fn tracked_files() -> Result<Vec<String>, String> {
    let output = crate::run_output_bytes("git", &["ls-files", "-z"])?;
    parse_git_path_records(&output)
}

/// Local copy of the `--name-only -z` decoder shape from
/// `crates/ripr/src/analysis/diff/records.rs` (#4006): NUL-separated records,
/// strict UTF-8, empty records fail, one trailing NUL tolerated.
pub(crate) fn parse_git_path_records(output: &[u8]) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for (record, field) in nul_fields(output).into_iter().enumerate() {
        if field.is_empty() {
            return Err(format!("path record {record} is empty"));
        }
        paths.push(decode_record_str(field, record)?.to_string());
    }
    Ok(paths)
}

/// Local copy of the `--name-status -z` decoder shape from
/// `crates/ripr/src/analysis/diff/records.rs` (#4006), kept beside the path
/// decoder for the slim policy binary; rename/copy records consume their
/// paired source path.
pub(crate) fn parse_git_status_records(
    output: &[u8],
) -> Result<Vec<(String, String, Option<String>)>, String> {
    let mut fields = nul_fields(output).into_iter();
    let mut records = Vec::new();
    let mut record = 0usize;
    while let Some(status_field) = fields.next() {
        if status_field.is_empty() {
            return Err(format!("path record {record} is empty"));
        }
        let status = decode_record_str(status_field, record)?.to_string();
        let kind = status_kind(&status)
            .ok_or_else(|| format!("path record {record} has invalid status token {status:?}"))?;
        if matches!(kind, 'R' | 'C') {
            let origin = next_record_str(&mut fields, record, &status)?.to_string();
            let target = next_record_str(&mut fields, record, &status)?.to_string();
            records.push((status, target, Some(origin)));
        } else {
            let path = next_record_str(&mut fields, record, &status)?.to_string();
            records.push((status, path, None));
        }
        record += 1;
    }
    Ok(records)
}

pub(crate) fn next_record_str<'output>(
    fields: &mut impl Iterator<Item = &'output [u8]>,
    record: usize,
    status: &str,
) -> Result<&'output str, String> {
    let field = fields.next().ok_or_else(|| {
        format!("path record {record} with status {status:?} is missing its paired path")
    })?;
    if field.is_empty() {
        return Err(format!("path record {record} is empty"));
    }
    decode_record_str(field, record)
}

pub(crate) fn nul_fields(output: &[u8]) -> Vec<&[u8]> {
    if output.is_empty() {
        return Vec::new();
    }
    let mut fields: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    if output.last() == Some(&0) {
        fields.pop();
    }
    fields
}

pub(crate) fn status_kind(status: &str) -> Option<char> {
    let mut chars = status.chars();
    let kind = chars.next()?;
    let valid = matches!(kind, 'A' | 'C' | 'D' | 'M' | 'R' | 'T' | 'U' | 'X' | 'B')
        && chars.all(|c| c.is_ascii_digit());
    valid.then_some(kind)
}

pub(crate) fn decode_record_str(field: &[u8], record: usize) -> Result<&str, String> {
    std::str::from_utf8(field).map_err(|err| {
        format!(
            "path record {record} is not valid UTF-8 (first invalid byte at offset {})",
            err.valid_up_to()
        )
    })
}
