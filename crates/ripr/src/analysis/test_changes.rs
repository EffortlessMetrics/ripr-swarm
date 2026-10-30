//! Test-edit disclosure for a two-sided diff. This is not test admission,
//! evidence strength, recommendation selection, or current-line placement.

use super::diff::{ChangedFile, reconstruct_old_source};
use super::facts::{RustIndex, normalize_reconstructed_source_roles};
use super::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use super::workspace::{SourceRole, SourceRoleContext, classify_with};
use crate::config::RiprConfig;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiffTestChanges {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) unresolved: bool,
}

/// Read changed Rust files within the remaining admission budget, at the
/// requested head. Reconstruct
/// their old source from the actual diff so removed coordinates never resolve
/// against a different function in the current source. Keep uncertainty out of
/// the affirmative `unchanged_tests` claim without changing analysis status.
pub(crate) fn diff_test_changes(
    root: &Path,
    head: &str,
    diff_text: &str,
    config: &RiprConfig,
    mut remaining_files: usize,
    mut remaining_bytes: u64,
) -> DiffTestChanges {
    let parsed = super::diff::parse_unified_diff_with_metadata(diff_text);
    // Whole-file deletions have no new-side ChangedFile. The shared parser
    // retains their count, not test-role attribution: do not infer unchanged
    // tests or invent a confirmed test path from that missing evidence.
    let mut changes = DiffTestChanges {
        unresolved: parsed.deleted_file_count > 0,
        ..DiffTestChanges::default()
    };
    let layout = SourceRoleContext::empty();
    for file in parsed.changed_files {
        if super::cancellation::checkpoint().is_err() {
            changes.unresolved = true;
            break;
        }
        if file.added_lines.is_empty() && file.removed_lines.is_empty() {
            continue;
        }
        if classify_with(&file.path, &layout) == SourceRole::TestEvidence {
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
        let Some(path) = file.path.to_str() else {
            changes.unresolved = true;
            continue;
        };
        // Git paths are already slash-separated. A literal Unix backslash
        // belongs to the object name and must not be rewritten for display.
        let object = format!("{head}:{path}");
        if remaining_files == 0 {
            changes.unresolved = true;
            continue;
        }
        remaining_files -= 1;
        let Some(size) = blob_size(root, &object) else {
            changes.unresolved = true;
            continue;
        };
        let Ok(output_limit) = usize::try_from(size.max(1)) else {
            changes.unresolved = true;
            continue;
        };
        if size > remaining_bytes {
            changes.unresolved = true;
            continue;
        }
        remaining_bytes -= size;
        let source = crate::git::run_git_output_with_deadline_and_limit(
            root,
            &["show", "--no-textconv", &object],
            Duration::from_mins(1),
            output_limit,
        )
        .ok()
        .filter(|output| output.status.success() && output.stdout.len() as u64 == size)
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

fn blob_size(root: &Path, object: &str) -> Option<u64> {
    let kind = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["cat-file", "-t", object],
        Duration::from_mins(1),
        128,
    )
    .ok()?;
    if !kind.status.success() || kind.stdout != b"blob\n" {
        return None;
    }
    let size = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["cat-file", "-s", object],
        Duration::from_mins(1),
        128,
    )
    .ok()?;
    if !size.status.success() {
        return None;
    }
    std::str::from_utf8(&size.stdout).ok()?.trim().parse().ok()
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
