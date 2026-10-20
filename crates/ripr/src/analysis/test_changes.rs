//! Test-edit disclosure for a two-sided diff. This is not test admission,
//! evidence strength, recommendation selection, or current-line placement.

use super::diff::{ChangedFile, reconstruct_old_source};
use super::facts::{RustIndex, normalize_reconstructed_source_roles};
use super::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::config::RiprConfig;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiffTestChanges {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) unresolved: bool,
}

/// Read only admitted changed Rust files, at the requested head. Reconstruct
/// their old source from the actual diff so removed coordinates never resolve
/// against a different function in the current source. Keep uncertainty out of
/// the affirmative `unchanged_tests` claim without changing analysis status.
pub(crate) fn diff_test_changes(
    root: &Path,
    head: &str,
    diff_text: &str,
    config: &RiprConfig,
) -> DiffTestChanges {
    let mut changes = DiffTestChanges::default();
    for file in super::diff::parse_unified_diff(diff_text) {
        if super::cancellation::checkpoint().is_err() {
            changes.unresolved = true;
            break;
        }
        if file.added_lines.is_empty() && file.removed_lines.is_empty() {
            continue;
        }
        if test_like_path(&file.path) {
            changes.files.push(file.path);
            continue;
        }
        if file
            .path
            .extension()
            .is_none_or(|extension| extension != "rs")
        {
            continue;
        }
        if !file
            .path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            changes.unresolved = true;
            continue;
        }
        let object = format!("{head}:{}", file.path.to_string_lossy().replace('\\', "/"));
        let source = crate::git::run_git_output_with_deadline_and_limit(
            root,
            &["show", "--no-textconv", &object],
            Duration::from_secs(60),
            256 * 1024 * 1024,
        )
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok());
        let Some(source) = source else {
            changes.unresolved = true;
            continue;
        };
        let added = touches_test(root, &file, &source, false, config);
        let removed = if file.removed_lines.is_empty() {
            Some(false)
        } else {
            reconstruct_old_source(&source, &file)
                .and_then(|old| touches_test(root, &file, &old, true, config))
        };
        if added == Some(true) || removed == Some(true) {
            changes.files.push(file.path);
        } else if added.is_none() || removed.is_none() {
            changes.unresolved = true;
        }
    }
    changes.files.sort();
    changes.files.dedup();
    changes
}

fn touches_test(
    root: &Path,
    changed: &ChangedFile,
    source: &str,
    old: bool,
    config: &RiprConfig,
) -> Option<bool> {
    let lines = if old {
        &changed.removed_lines
    } else {
        &changed.added_lines
    };
    if lines.is_empty() {
        return Some(false);
    }
    let facts = RaRustSyntaxAdapter
        .summarize_file(&changed.path, source)
        .ok()?;
    if facts.used_lexical_fallback {
        return None;
    }
    let mut index = RustIndex::default();
    index.insert_file(changed.path.clone(), facts, true);
    normalize_reconstructed_source_roles(&mut index, root, config.analysis().test_harnesses())
        .ok()?;
    Some(lines.iter().any(|line| {
        index.functions().iter().any(|function| {
            function.source_role.is_evidence_role()
                && function.start_line <= line.line
                && line.line <= function.end_line
        })
    }))
}

pub(crate) fn test_like_path(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_start_matches("./");
    text.starts_with("tests/")
        || text.contains("/tests/")
        || text.ends_with("_test.rs")
        || text.ends_with("_tests.rs")
}
