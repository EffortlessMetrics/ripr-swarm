use std::path::{Path, PathBuf};

use crate::{
    FixKind, PolicyReportSpec, collect_files, finish_policy_report, normalize_path, read_text_lossy,
};

/// Advisory check that validates per-role section requirements for the
/// repo's typed planning docs. See `docs/REPO_TRACKING_MODEL.md` and
/// `docs/agent-context/CONTEXT_SYSTEM.md` for the role doctrine.
///
/// Currently enforced:
///
/// - Proposals (`docs/proposals/RIPR-PROP-*.md`) require sections that
///   match `docs/templates/PROPOSAL_TEMPLATE.md`.
/// - ADRs (`docs/adr/*.md` excluding `README.md`) require sections that
///   match `docs/templates/ADR_TEMPLATE.md`.
///
/// Handoff validation is intentionally deferred to a later slice: the
/// existing handoffs predate `docs/templates/HANDOFF_TEMPLATE.md` and
/// conforming them is a separate job.
pub fn check_doc_roles() -> Result<(), String> {
    let report_spec = PolicyReportSpec {
        report_file: "doc-roles.md",
        check: "check-doc-roles",
        why_it_matters: "Typed planning docs blur when role boundaries drift. Proposals should answer *why*; ADRs should record durable decisions. Required sections per role keep each doc to one job and keep the typed context graph reviewable.",
        fix_kind: FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Add the missing section heading exactly (case-sensitive `## Name`).",
            "If the doc is intentionally minimal, fold it into a related doc rather than skipping the required sections.",
            "Use `docs/templates/PROPOSAL_TEMPLATE.md` and `docs/templates/ADR_TEMPLATE.md` as the canonical structure.",
        ],
        rerun_command: "cargo xtask check-doc-roles",
        exception_template: None,
    };

    let violations = violations_at(Path::new("."), |dir| {
        if dir.exists() {
            collect_files(dir)
        } else {
            Ok(Vec::new())
        }
    })?;
    finish_policy_report(report_spec, &violations)
}

/// Evaluate the same whole-tree obligation in a materialized candidate revision.
pub(crate) fn staged_violations(root: &Path) -> Result<Vec<String>, String> {
    let docs = root.join("docs");
    match crate::regular_metadata(&docs) {
        Ok(_) => violations_at(root, crate::collect_regular_files),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("cannot inspect {}: {error}", docs.display())),
    }
}

fn violations_at(
    root: &Path,
    list_files: impl Fn(&Path) -> Result<Vec<PathBuf>, String>,
) -> Result<Vec<String>, String> {
    let mut violations = Vec::new();
    for dir in ["docs/proposals", "docs/adr"] {
        let dir = root.join(dir);
        let mut paths = list_files(&dir)?;
        paths.sort();
        for path in paths {
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            let Some((kind, headings)) = required_sections(relative) else {
                continue;
            };
            let normalized = normalize_path(relative);
            let text = read_text_lossy(&path)?;
            for heading in headings {
                if !has_heading(&text, heading) {
                    violations.push(format!(
                        "{normalized} is missing required {kind} section `{heading}`"
                    ));
                }
            }
        }
    }
    Ok(violations)
}

pub(crate) fn required_sections(path: &Path) -> Option<(&'static str, &'static [&'static str])> {
    let name = path.file_name()?.to_str()?;
    if !name.ends_with(".md") {
        return None;
    }
    let (relative, role, headings) = if let Ok(relative) = path.strip_prefix("docs/proposals") {
        if !name.starts_with("RIPR-PROP-") {
            return None;
        }
        (relative, "proposal", required_proposal_headings())
    } else {
        let relative = path.strip_prefix("docs/adr").ok()?;
        if name == "README.md" {
            return None;
        }
        (relative, "ADR", required_adr_headings())
    };
    (!crate::should_skip_path(&normalize_path(relative))).then_some((role, headings))
}

fn required_proposal_headings() -> &'static [&'static str] {
    &[
        "## Problem",
        "## Success criteria",
        "## Alternatives considered",
        "## Risks",
        "## Non-goals",
    ]
}

fn required_adr_headings() -> &'static [&'static str] {
    &[
        "## Context",
        "## Decision",
        "## Consequences",
        "## Alternatives Considered",
    ]
}

fn has_heading(text: &str, heading: &str) -> bool {
    text.lines().any(|line| line.trim_end() == heading)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_match_is_exact_and_trim_aware() {
        assert!(has_heading("body\n## Problem\nmore", "## Problem"));
        assert!(has_heading("## Problem  \n", "## Problem"));
        // Wrong heading level
        assert!(!has_heading("# Problem\n", "## Problem"));
        // Different case
        assert!(!has_heading("## problem\n", "## Problem"));
        // Inline mention, not a heading
        assert!(!has_heading("see ## Problem in the body", "## Problem"));
    }

    #[test]
    fn required_proposal_headings_match_template() {
        let headings = required_proposal_headings();
        assert!(headings.contains(&"## Problem"));
        assert!(headings.contains(&"## Success criteria"));
        assert!(headings.contains(&"## Alternatives considered"));
        assert!(headings.contains(&"## Risks"));
        assert!(headings.contains(&"## Non-goals"));
    }

    #[test]
    fn required_adr_headings_match_template() {
        let headings = required_adr_headings();
        assert!(headings.contains(&"## Context"));
        assert!(headings.contains(&"## Decision"));
        assert!(headings.contains(&"## Consequences"));
        assert!(headings.contains(&"## Alternatives Considered"));
    }
}
