//! Leaf-scoped help for subcommand families that document every leaf in one
//! shared body (`reports`, `policy`, `baseline`, `pr-comments`,
//! `assistant-loop`; #14314).
//!
//! The scoping is derived once from the existing `suggest::option_section`
//! mapping, the same one `known_flags` uses to keep typo suggestions inside one
//! leaf: a leaf's help is the family's lead sentence swapped for the leaf's
//! catalog summary, the family `Usage:` lines that name this leaf, the leaf's
//! own `… options:` section, its catalog example, and a pointer to the family
//! help. Both `ripr <family> <leaf> --help` and the `flags` of the leaf's
//! `help --json` row read this one projection, so the human and machine
//! surfaces cannot disagree about which flags the leaf owns.

use crate::cli::help::{catalog_example, catalog_summary, help_text_for};
use crate::cli::suggest::{option_section, usage_line_belongs_to_command};

/// Families whose leaves get scoped help. `gate` also has per-subcommand
/// option sections, but `gate check` documents most of its flags as "As for
/// gate evaluate", so its body stays shared (`shared_help_body`, #14086).
const LEAF_SCOPED_FAMILIES: &[&str] = &[
    "assistant-loop",
    "baseline",
    "policy",
    "pr-comments",
    "reports",
];

/// The scoped help for one leaf path (`reports kani`), or `None` when the path
/// is outside `LEAF_SCOPED_FAMILIES`, has no `option_section`, or the body no
/// longer carries the leaf's usage line or section.
pub(super) fn leaf_help(path: &str) -> Option<String> {
    let (family, _) = path.split_once(' ')?;
    if !LEAF_SCOPED_FAMILIES.contains(&family) {
        return None;
    }
    let section = option_section(path)?;
    let body = help_text_for(path)?;
    let lead = catalog_summary(path).or_else(|| body.lines().next())?;
    let usage = usage_lines(body, path);
    let options = section_block(body, section);
    if usage.is_empty() || options.is_empty() {
        return None;
    }
    let mut help = String::new();
    help.push_str(lead);
    help.push_str("\n\nUsage:\n");
    for line in usage {
        help.push_str("  ");
        help.push_str(line);
        help.push('\n');
    }
    help.push('\n');
    for line in options {
        help.push_str(line);
        help.push('\n');
    }
    for note in leaf_notes(body, path) {
        help.push('\n');
        help.push_str(note);
        help.push('\n');
    }
    if let Some(example) = catalog_example(path)
        && !example.is_empty()
    {
        help.push_str("\nExamples:\n  ");
        help.push_str(example);
        help.push('\n');
    }
    help.push_str(&format!(
        "\nRun `ripr help {family}` for every {family} subcommand.\n"
    ));
    Some(help)
}

/// The family `Usage:` lines that start with `ripr <leaf path>`, trimmed. A
/// family writes them either as a `Usage:` header followed by indented
/// `ripr …` lines or as one `Usage: ripr …` line per leaf.
fn usage_lines<'a>(body: &'a str, path: &str) -> Vec<&'a str> {
    let mut lines = Vec::new();
    let mut in_usage_block = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Usage:") {
            in_usage_block = true;
            let rest = rest.trim();
            if usage_line_belongs_to_command(rest, path) {
                lines.push(rest);
            }
            continue;
        }
        if in_usage_block && trimmed.starts_with("ripr ") {
            if usage_line_belongs_to_command(trimmed, path) {
                lines.push(trimmed);
            }
            continue;
        }
        in_usage_block = false;
    }
    lines
}

/// Opening words of the family-body prose paragraphs that describe each leaf:
/// its contract, exit status, environment, and side effects. The scoped page
/// keeps them so it drops sibling detail, not this leaf's own rules. A
/// genuinely shared caveat paragraph is listed for each leaf it covers.
fn leaf_note_openings(path: &str) -> &'static [&'static str] {
    const BASELINE_OUTPUT: &str = "Output discipline:";
    const POLICY_LIMITS: &str = "These policy commands do not run analysis";
    match path {
        "reports index" => &["The report-packet index"],
        "reports ci-packet" => &["The CI packet runs"],
        "reports ci-summary" => &["The CI summary prints"],
        "reports gap-ledger" => &["The gap decision ledger"],
        "reports kani" => &["The Kani report imports"],
        "reports ts-limitations" => &["The TypeScript limitation leaderboard"],
        "reports ts-false-actionable" => &["The TypeScript false-actionable audit report"],
        "policy readiness" => &["The policy readiness report", POLICY_LIMITS],
        "policy operations" => &["The policy operations report", POLICY_LIMITS],
        "policy history" => &["The policy history report", POLICY_LIMITS],
        "policy promote" => &["The policy promotion packet", POLICY_LIMITS],
        "policy preview-promote" => &["The preview promotion packet", POLICY_LIMITS],
        "policy waiver-aging" => &["The waiver-aging report", POLICY_LIMITS],
        "policy suppression-health" => &["The suppression-health report", POLICY_LIMITS],
        "baseline create" => &["The baseline create command", BASELINE_OUTPUT],
        "baseline diff" => &["The baseline diff command", BASELINE_OUTPUT],
        "baseline update" => &["The baseline update command", BASELINE_OUTPUT],
        "pr-comments plan" => &["The PR comments plan"],
        "pr-comments existing" => &["`existing` keeps only marked comments"],
        "pr-comments requests" => &["`requests` writes one JSON payload"],
        "pr-comments summary" => &["`summary` renders"],
        "assistant-loop proof" => &["The assistant-loop proof report"],
        "assistant-loop health" => &["The assistant-loop health report"],
        _ => &[],
    }
}

/// The family-body paragraphs named by `leaf_note_openings`, in order. A
/// paragraph is a run of non-blank lines; an opening that no longer matches
/// a paragraph is skipped here and caught by the unit tests.
fn leaf_notes<'a>(body: &'a str, path: &str) -> Vec<&'a str> {
    leaf_note_openings(path)
        .iter()
        .filter_map(|opening| {
            body.split("\n\n")
                .map(|paragraph| paragraph.trim_matches('\n'))
                .find(|paragraph| paragraph.starts_with(opening))
        })
        .collect()
}

/// The options section headed exactly `section`, from the heading through the
/// last line before the next blank line.
fn section_block<'a>(body: &'a str, section: &str) -> Vec<&'a str> {
    body.lines()
        .skip_while(|line| *line != section)
        .take_while(|line| !line.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_paths() -> Vec<&'static str> {
        crate::cli::help::registered_command_paths()
            .iter()
            .copied()
            .filter(|path| option_section(path).is_some())
            .filter(|path| {
                path.split_once(' ')
                    .is_some_and(|(family, _)| LEAF_SCOPED_FAMILIES.contains(&family))
            })
            .collect()
    }

    /// `gate` subcommands keep the shared body (#14086).
    #[test]
    fn gate_subcommands_keep_the_shared_body() {
        for path in ["gate check", "gate evaluate"] {
            assert!(option_section(path).is_some(), "{path}");
            assert!(leaf_help(path).is_none(), "{path} must stay shared");
        }
    }

    fn section_headings(text: &str) -> Vec<&str> {
        text.lines()
            .filter(|line| line.ends_with(" options:") && !line.starts_with(' '))
            .collect()
    }

    #[test]
    fn every_shared_body_leaf_has_scoped_help() {
        let leaves = leaf_paths();
        assert_eq!(leaves.len(), 23, "{leaves:?}");
        for path in leaves {
            assert!(leaf_help(path).is_some(), "{path} has no scoped help");
        }
    }

    #[test]
    fn scoped_help_has_the_leaf_section_and_no_sibling_section() {
        for path in leaf_paths() {
            let Some(section) = option_section(path) else {
                continue;
            };
            let help = leaf_help(path).unwrap_or_default();
            assert!(!help.is_empty(), "{path} has no scoped help");
            assert_eq!(section_headings(&help), vec![section], "{path}: {help}");
            let family_body = help_text_for(path).unwrap_or_default();
            assert!(
                help.lines().count() < family_body.lines().count(),
                "{path}: scoped help must be shorter than the family body"
            );
            assert!(
                help.lines()
                    .filter(|line| line.starts_with("  ripr "))
                    .all(|line| usage_line_belongs_to_command(line.trim(), path)),
                "{path}: foreign usage or example line: {help}"
            );
            assert!(
                help.contains(&format!("\n  ripr {path}")),
                "{path}: usage line missing: {help}"
            );
            assert!(
                help.contains("\nExamples:\n  ripr "),
                "{path}: catalog example missing: {help}"
            );
            let family = path.split(' ').next().unwrap_or_default();
            assert!(
                help.ends_with(&format!(
                    "\nRun `ripr help {family}` for every {family} subcommand.\n"
                )),
                "{path}: family pointer missing: {help}"
            );
        }
    }

    #[test]
    fn scoped_help_leads_with_the_catalog_summary() {
        for path in leaf_paths() {
            let summary = catalog_summary(path).unwrap_or_default();
            let help = leaf_help(path).unwrap_or_default();
            assert!(!summary.is_empty(), "{path}");
            assert_eq!(help.lines().next(), Some(summary), "{path}");
        }
    }

    #[test]
    fn multi_usage_leaves_keep_every_own_line_and_no_sibling_line() {
        let gap = leaf_help("reports gap-ledger").unwrap_or_default();
        for flag in [
            "--records PATH",
            "--check-output PATH",
            "--repo-exposure PATH",
        ] {
            assert!(
                gap.contains(&format!("ripr reports gap-ledger {flag}")),
                "{gap}"
            );
        }
        assert!(!gap.contains("reports kani"), "{gap}");
        let update = leaf_help("baseline update").unwrap_or_default();
        // Both usage lines, counted before the catalog example.
        let usage = update.split("\nExamples:").next().unwrap_or_default();
        assert_eq!(
            usage.matches("  ripr baseline update ").count(),
            2,
            "{update}"
        );
        let promote = leaf_help("policy promote").unwrap_or_default();
        assert!(!promote.contains("preview-promote"), "{promote}");
    }

    /// The scoped page and typo suggestions must agree on which flags the
    /// leaf owns: `known_flags` over the scoped page equals `known_flags`
    /// over the whole family body (a section that ran past a blank line, or
    /// a usage line that was dropped, would split the two).
    #[test]
    fn scoped_help_lists_exactly_the_flags_suggestions_know() {
        for path in leaf_paths() {
            let family_body = help_text_for(path).unwrap_or_default();
            let help = leaf_help(path).unwrap_or_default();
            let mut from_family = crate::cli::suggest::known_flags(path, family_body);
            let mut from_leaf = crate::cli::suggest::known_flags(path, &help);
            from_family.sort_unstable();
            from_family.dedup();
            from_leaf.sort_unstable();
            from_leaf.dedup();
            assert_eq!(from_leaf, from_family, "{path}: {help}");
        }
    }

    /// Every leaf keeps its own contract prose, and each named opening
    /// matches exactly one paragraph of the family body (#14366 review: the
    /// scoped page had dropped exit status, environment and side effects).
    #[test]
    fn every_leaf_keeps_its_own_contract_paragraphs() {
        for path in leaf_paths() {
            let body = help_text_for(path).unwrap_or_default();
            let openings = leaf_note_openings(path);
            assert!(!openings.is_empty(), "{path}: no contract paragraph named");
            for opening in openings {
                let matches = body
                    .split("\n\n")
                    .filter(|paragraph| paragraph.trim_matches('\n').starts_with(opening))
                    .count();
                assert_eq!(matches, 1, "{path}: opening {opening:?}");
            }
            let help = leaf_help(path).unwrap_or_default();
            for note in leaf_notes(body, path) {
                assert!(help.contains(note), "{path}: missing {note:?}");
            }
        }
        let kani = leaf_help("reports kani").unwrap_or_default();
        assert!(kani.contains("Exit 0 means"), "{kani}");
        assert!(!kani.contains("The CI packet runs"), "{kani}");
        let ci = leaf_help("reports ci-packet").unwrap_or_default();
        assert!(ci.contains("RIPR_GATE_MODE"), "{ci}");
        let requests = leaf_help("pr-comments requests").unwrap_or_default();
        assert!(requests.contains("requests.tsv"), "{requests}");
        assert!(!requests.contains("`summary` renders"), "{requests}");
        assert!(!requests.contains("`existing` keeps"), "{requests}");
        let existing = leaf_help("pr-comments existing").unwrap_or_default();
        assert!(!existing.contains("requests.tsv"), "{existing}");
        let readiness = leaf_help("policy readiness").unwrap_or_default();
        assert!(
            readiness.contains("These policy commands do not"),
            "{readiness}"
        );
        assert!(!readiness.contains("waiver-aging report"), "{readiness}");
        assert!(!readiness.contains("promotion packet"), "{readiness}");
    }

    #[test]
    fn a_path_without_a_shared_body_has_no_scoped_help() {
        assert!(leaf_help("check").is_none());
        assert!(leaf_help("reports").is_none());
    }
}
