//! TypeScript / JavaScript repo-mode producer (#5545, #6392, #6395).
//!
//! `TypeScriptAdapter::analyze_repo` selects a bounded production working set
//! ([`select_repo_input`]), then runs the SAME diff-mode core over it: every
//! non-blank line of each selected production file is presented as an added
//! line, so owners, related tests, oracle strength, relations, probes and
//! static limits come from the producers diff mode uses and repo mode never
//! strengthens a fact because the scan is larger. The test index stays
//! workspace-wide (tests contribute evidence only); test files, declaration
//! files, generated/vendor/build trees never seed production findings.
//!
//! Boundaries:
//!
//! - no `RepoSeam` and no Rust seam vocabulary: findings keep their native
//!   `language` identity (`typescript` or `javascript`), preview status, and
//!   `gap:typescript:` canonical gaps;
//! - a capped, unreadable, unparseable or incompletely indexed run discloses
//!   itself through [`RepoSelection::partial_reason`] and so can never back a
//!   full-denominator claim; only a fully discovered, fully read run (or a
//!   workspace without TS/JS production source) is complete;
//! - the working-set cap is the shared `RIPR_MAX_REPO_INDEX_FILES` override
//!   (#2109).

use super::{
    AnalysisOptions, ChangedFile, Finding, LanguageRepoResult, TypeScriptAdapter, is_test_file,
    is_typescript_declaration_file, parse_error_reason, read_workspace_sources_capped,
    ts_file_read_limit, ts_workspace_file_limit, ts_workspace_read_budget,
    visit_workspace_with_handwritten,
};
use crate::analysis::diff::ChangedLine;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Default repo working-set limit; the same bound family as the Rust and
/// Python repo guards (#2109).
pub(super) const TS_REPO_FILE_LIMIT: usize = 800;
/// Shared repo working-set override (#2109): one operator knob for every
/// language producer.
pub(super) const TS_REPO_FILE_LIMIT_ENV: &str = "RIPR_MAX_REPO_INDEX_FILES";

/// Resolve the working-set limit from a shared-env read. Absent means the
/// default; zero, non-integer or non-UTF-8 values fail with a named error.
pub(super) fn resolve_repo_working_set_limit(
    env: Result<String, std::env::VarError>,
) -> Result<usize, String> {
    match env {
        Err(std::env::VarError::NotPresent) => Ok(TS_REPO_FILE_LIMIT),
        Ok(raw) => {
            let parsed = raw.trim().parse::<usize>().map_err(|err| {
                format!("{TS_REPO_FILE_LIMIT_ENV} must be a positive integer, got {raw:?}: {err}")
            })?;
            if parsed == 0 {
                return Err(format!(
                    "{TS_REPO_FILE_LIMIT_ENV} must be a positive integer, got {raw:?}"
                ));
            }
            Ok(parsed)
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{TS_REPO_FILE_LIMIT_ENV} must be valid UTF-8"))
        }
    }
}

/// The bounded repo input: a deterministic, sorted partition of the
/// discovered TS/JS files.
#[derive(Debug, Default)]
pub(super) struct RepoInput {
    /// Production files selected for analysis (sorted, within the limit).
    pub(super) production: Vec<PathBuf>,
    /// Test files discovered (evidence only; counted, never findings).
    pub(super) tests: Vec<PathBuf>,
    /// Type-only declaration files (counted, never probed).
    pub(super) declarations: usize,
    /// Production files beyond the working-set limit. Test files are the
    /// evidence index and are not capped here; the read bounds govern them.
    pub(super) capped: usize,
    /// The discovery walk hit its entry cap, so source may be unseen.
    pub(super) discovery_truncated: bool,
    /// Symlink/junction entries the walk did not follow.
    pub(super) links_skipped: usize,
    /// The workspace root could not be read at all.
    pub(super) root_unreadable: bool,
}

/// Discover and partition the workspace under `limit`.
pub(super) fn select_repo_input(root: &Path, limit: usize, walk_limit: usize) -> RepoInput {
    if !root.is_dir() || std::fs::read_dir(root).is_err() {
        return RepoInput {
            root_unreadable: true,
            ..RepoInput::default()
        };
    }
    let scan = visit_workspace_with_handwritten(root, walk_limit, &[]);
    let mut files: Vec<PathBuf> = scan.files;
    files.sort();
    files.dedup();
    let mut input = RepoInput {
        discovery_truncated: scan.truncated,
        links_skipped: scan.skipped_links,
        ..RepoInput::default()
    };
    for file in files {
        if is_typescript_declaration_file(&file) {
            input.declarations += 1;
        } else if is_test_file(&file) {
            input.tests.push(file);
        } else if input.production.len() < limit {
            input.production.push(file);
        } else {
            input.capped += 1;
        }
    }
    input
}

/// Outcome of one repo-mode run, before projection onto
/// [`LanguageRepoResult`].
#[derive(Debug)]
pub(super) struct RepoSelection {
    pub(super) findings: Vec<Finding>,
    pub(super) production_files: usize,
    pub(super) skipped_files: usize,
    pub(super) partial_reason: Option<String>,
}

/// Run the bounded repo-mode analysis for `options.root` under `limit`.
pub(super) fn analyze_repo_with_limit(
    options: &AnalysisOptions,
    limit: usize,
    walk_limit: usize,
    file_read_limit: u64,
    workspace_read_budget: u64,
) -> Result<LanguageRepoResult, String> {
    let selection = select_and_analyze(
        options,
        limit,
        walk_limit,
        file_read_limit,
        workspace_read_budget,
    )?;
    Ok(LanguageRepoResult {
        findings: selection.findings,
        harness_projections: Vec::new(),
        production_files: selection.production_files,
        skipped_files: selection.skipped_files,
        partial_reason: selection.partial_reason,
        rust_diagnostic_origins: Default::default(),
        rust_consumed_sources: Default::default(),
    })
}

pub(super) fn analyze_repo(options: &AnalysisOptions) -> Result<LanguageRepoResult, String> {
    let limit = resolve_repo_working_set_limit(std::env::var(TS_REPO_FILE_LIMIT_ENV))?;
    analyze_repo_with_limit(
        options,
        limit,
        ts_workspace_file_limit(),
        ts_file_read_limit(),
        ts_workspace_read_budget(),
    )
}

fn select_and_analyze(
    options: &AnalysisOptions,
    limit: usize,
    walk_limit: usize,
    file_read_limit: u64,
    workspace_read_budget: u64,
) -> Result<RepoSelection, String> {
    let input = select_repo_input(&options.root, limit, walk_limit);
    let mut reasons: Vec<String> = Vec::new();
    if input.root_unreadable {
        reasons.push("typescript_repo_root_unreadable: workspace root could not be read, so TypeScript/JavaScript source absence is unestablished".to_string());
    }
    if input.capped > 0 {
        reasons.push(format!(
            "typescript_repo_capped: {} TypeScript/JavaScript file(s) beyond the working-set limit of {limit}; findings cover the selected files only. Recovery: narrow the scope with a diff-based run (--base/--diff), or raise the limit via {TS_REPO_FILE_LIMIT_ENV}=<number>",
            input.capped
        ));
    }
    if input.discovery_truncated {
        reasons.push(
            "typescript_repo_discovery_truncated: workspace discovery hit its entry cap, so TypeScript/JavaScript source may have been missed (raise RIPR_TS_MAX_WORKSPACE_FILES)".to_string(),
        );
    }
    if input.production.is_empty() {
        if input.links_skipped > 0 {
            reasons.push(format!(
                "typescript_repo_links_skipped: {} symlink/junction entr(ies) were not followed, so TypeScript/JavaScript source behind them may have been missed",
                input.links_skipped
            ));
        }
        return Ok(RepoSelection {
            findings: Vec::new(),
            production_files: 0,
            skipped_files: input.capped,
            partial_reason: join_reasons(&reasons),
        });
    }

    // One bounded read per selected production file builds the "every line
    // is added" view; refused or unreadable files leave the denominator.
    let read = read_workspace_sources_capped(
        &options.root,
        &input.production,
        file_read_limit,
        workspace_read_budget,
    );
    let unread = read.limits.len() + read.io_failures.len();
    if unread > 0 {
        reasons.push(format!(
            "typescript_repo_unread: {unread} selected TypeScript/JavaScript file(s) could not be read within the read bounds and left the analyzed denominator"
        ));
    }
    // A test file that fails to parse drops its tests from the evidence
    // index and can flip owners to a false `no_static_path`; name it.
    let test_read = read_workspace_sources_capped(
        &options.root,
        &input.tests,
        file_read_limit,
        workspace_read_budget,
    );
    let unread_tests = test_read.limits.len() + test_read.io_failures.len();
    if unread_tests > 0 {
        reasons.push(format!(
            "typescript_repo_unread_tests: {unread_tests} test file(s) could not be read and left the evidence index, so related-test evidence may be missing"
        ));
    }
    let unparseable_tests = input
        .tests
        .iter()
        .filter_map(|path| test_read.sources.get(path).map(|source| (path, source)))
        .filter(|(path, source)| parse_error_reason(path, source).is_some())
        .count();
    if unparseable_tests > 0 {
        reasons.push(format!(
            "typescript_repo_test_parse_failure: {unparseable_tests} test file(s) failed to parse and left the evidence index, so related-test evidence may be missing"
        ));
    }
    let changed: Vec<ChangedFile> = input
        .production
        .iter()
        .filter_map(|path| {
            let source = read.sources.get(path)?;
            Some(ChangedFile {
                path: path.clone(),
                added_lines: source
                    .lines()
                    .enumerate()
                    .filter(|(_, text)| !text.trim().is_empty())
                    .map(|(index, text)| ChangedLine {
                        line: index + 1,
                        text: text.to_string(),
                        new_side_line: index + 1,
                    })
                    .collect(),
                removed_lines: Vec::new(),
            })
        })
        .collect();

    let diff = TypeScriptAdapter::analyze_diff_with_read_limits_and_handwritten(
        options,
        &changed,
        file_read_limit,
        workspace_read_budget,
        &[],
    )?;
    if !diff.limitations.is_empty() {
        let mut details: Vec<String> = diff
            .limitations
            .iter()
            .filter_map(|limitation| limitation.bounded_detail.clone())
            .collect();
        details.sort();
        details.dedup();
        details.truncate(3);
        reasons.push(format!(
            "typescript_repo_limitations: {} typed limitation(s) (parse failures, unindexed owner shapes or partial test extraction) bound the evidence{}",
            diff.limitations.len(),
            if details.is_empty() {
                String::new()
            } else {
                format!(": {}", details.join(" / "))
            }
        ));
    }

    Ok(RepoSelection {
        findings: representative_findings(diff.findings),
        production_files: changed.len(),
        skipped_files: input.capped + diff.skipped_files,
        partial_reason: join_reasons(&reasons),
    })
}

/// Whole-file presentation probes every behavior line; keep one finding per
/// (file, owner, probe family, delta, exposure class) so repo evidence is one
/// row per distinct behavior rather than one per line, while a weaker class
/// is never hidden behind a stronger row. Order stays the diff-mode
/// order (production-file order, ascending line).
fn representative_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen: BTreeSet<(String, Option<String>, String, String, String)> = BTreeSet::new();
    findings
        .into_iter()
        .filter(|finding| {
            seen.insert((
                finding.probe.location.file.to_string_lossy().into_owned(),
                finding
                    .probe
                    .owner
                    .as_ref()
                    .map(|owner| format!("{owner:?}")),
                format!("{:?}", finding.probe.family),
                format!("{:?}", finding.probe.delta),
                format!("{:?}", finding.class),
            ))
        })
        .collect()
}

fn join_reasons(reasons: &[String]) -> Option<String> {
    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join(" | "))
    }
}
