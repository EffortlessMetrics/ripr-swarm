//! Lint-policy and CI-ledger cluster shared between the full `xtask`
//! binary and a future slim policy binary (RANK-06 route B prerequisite,
//! Part A).
//!
//! Behavior-preserving extraction from `main.rs`: the lint-policy gate
//! (`check_lint_policy`) with its clippy-debt collectors and the CiLedger
//! document model, parser and validators backing the CI-lane ledger.
//! `check_workspace_shape`, the remaining policy `*_impl` functions and
//! `tracked_files` stay in `main.rs` for the Part B extraction. Remaining
//! shared helpers still live at the crate root and are imported below; a
//! slim binary must provide or move those next.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use repo_policy::{FixKind, PolicyReportSpec, finish_policy_report};

use crate::no_panic::{
    parse_string_value, parse_toml_key_value, parse_usize_value, strip_toml_value_comment,
};
use crate::policy::parse_workspace_lints_section;
use crate::{
    is_kebab_case_id, is_snake_case_id, ledger_name_to_lookup, parse_clippy_debt_ledger,
    parse_clippy_lints_ledger, parse_inline_array, parse_workspace_package_rust_version,
};
use crate::{no_panic, policy};

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
    let (lints, ledger_violations) = parse_clippy_lints_ledger(lints_text);
    if !ledger_violations.is_empty() {
        violations.extend(ledger_violations);
        return violations;
    }

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
    if !violations.is_empty() {
        return violations;
    }
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

    for (group, cargo_lints) in [("clippy", &cargo_clippy), ("rust", &cargo_rust)] {
        for (name, level) in cargo_lints {
            if !active_lookup.contains(&(group.to_string(), name.clone())) {
                violations.push(format!(
                    "Cargo.toml `[workspace.lints.{group}]` configures `{name}` at level `{level}` but policy/clippy-lints.toml has no matching `[[active.<group>]]` entry. Add the lint to the ledger or remove it from Cargo.toml."
                ));
            }
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
