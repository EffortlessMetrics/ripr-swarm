//! Workspace-folder membership and disclosure bounds (#4483, RIPR-SPEC-0139).
//!
//! The transport framing cap bounds one message; these limits bound cumulative
//! retained folder-set state and human disclosure built from that state.

use std::path::{Path, PathBuf};

/// Maximum retained workspace-folder entries. Legitimate editors attach a
/// handful of roots per window; 64 closes unbounded accumulation while
/// tolerating pathological multi-folder workspaces without claiming
/// multi-root analysis.
pub(super) const MAX_WORKSPACE_FOLDER_ENTRIES: usize = 64;

/// Maximum UTF-8 bytes for one folder URI or normalized path identity.
pub(super) const MAX_WORKSPACE_FOLDER_URI_BYTES: usize = 4096;

/// Maximum aggregate UTF-8 bytes across all retained folder URIs and paths.
pub(super) const MAX_WORKSPACE_FOLDER_AGGREGATE_BYTES: usize = 256 * 1024;

/// Maximum bytes when assembling a comma-separated path list for log/showMessage
/// before the global client-visible char bound applies.
pub(super) const MAX_WORKSPACE_FOLDER_DISCLOSURE_BYTES: usize = 512;

/// Maximum paths listed explicitly before reporting an omitted count.
pub(super) const WORKSPACE_FOLDER_DISCLOSURE_MAX_LISTED: usize = 8;

pub(super) fn membership_capacity_detail() -> String {
    format!(
        "workspace folder membership exceeds the reviewed limit (max {MAX_WORKSPACE_FOLDER_ENTRIES} entries, {MAX_WORKSPACE_FOLDER_AGGREGATE_BYTES} aggregate URI/path bytes, {MAX_WORKSPACE_FOLDER_URI_BYTES} bytes per URI)"
    )
}

fn entry_byte_cost(uri: &str, path: &Path) -> usize {
    uri.len().saturating_add(path.as_os_str().len())
}

/// Returns `false` when `entries` exceeds any reviewed membership limit.
pub(super) fn membership_within_capacity<'a, I>(entries: I) -> bool
where
    I: IntoIterator<Item = (&'a str, &'a Path)>,
{
    let entries: Vec<_> = entries.into_iter().collect();
    if entries.len() > MAX_WORKSPACE_FOLDER_ENTRIES {
        return false;
    }
    let mut aggregate = 0_usize;
    for (uri, path) in entries {
        if uri.len() > MAX_WORKSPACE_FOLDER_URI_BYTES
            || path.as_os_str().len() > MAX_WORKSPACE_FOLDER_URI_BYTES
        {
            return false;
        }
        aggregate = aggregate.saturating_add(entry_byte_cost(uri, path));
        if aggregate > MAX_WORKSPACE_FOLDER_AGGREGATE_BYTES {
            return false;
        }
    }
    true
}

pub(super) struct BoundedPathList {
    pub(super) text: String,
    pub(super) omitted_count: usize,
}

/// Builds a comma-separated path summary without joining the full list first.
pub(super) fn format_bounded_path_list(paths: &[PathBuf]) -> BoundedPathList {
    let total = paths.len();
    let mut text = String::new();
    let mut listed = 0_usize;
    for path in paths {
        if listed >= WORKSPACE_FOLDER_DISCLOSURE_MAX_LISTED {
            break;
        }
        let fragment = path.display().to_string();
        let separator = if text.is_empty() { "" } else { ", " };
        let needed = separator.len().saturating_add(fragment.len());
        if text.len().saturating_add(needed) > MAX_WORKSPACE_FOLDER_DISCLOSURE_BYTES {
            break;
        }
        text.push_str(separator);
        text.push_str(&fragment);
        listed += 1;
    }
    BoundedPathList {
        text,
        omitted_count: total.saturating_sub(listed),
    }
}

pub(super) fn bounded_path_list_entries(paths: &[PathBuf]) -> (Vec<String>, usize) {
    let total = paths.len();
    let mut listed = Vec::new();
    let mut bytes = 0_usize;
    for path in paths {
        if listed.len() >= WORKSPACE_FOLDER_DISCLOSURE_MAX_LISTED {
            break;
        }
        let fragment = path.to_string_lossy().into_owned();
        let separator: usize = if listed.is_empty() { 0 } else { 2 };
        let needed = separator.saturating_add(fragment.len());
        if bytes.saturating_add(needed) > MAX_WORKSPACE_FOLDER_DISCLOSURE_BYTES {
            break;
        }
        bytes = bytes.saturating_add(needed);
        listed.push(fragment);
    }
    let listed_len = listed.len();
    (listed, total.saturating_sub(listed_len))
}

pub(super) fn bounded_path_list_phrase(paths: &[PathBuf]) -> String {
    let summary = format_bounded_path_list(paths);
    if summary.omitted_count == 0 {
        summary.text
    } else if summary.text.is_empty() {
        format!(
            "{} folders omitted from this summary",
            summary.omitted_count
        )
    } else {
        format!("{} (+{} more omitted)", summary.text, summary.omitted_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_entry_limit_is_within_capacity() {
        let owned: Vec<(String, PathBuf)> = (0..MAX_WORKSPACE_FOLDER_ENTRIES)
            .map(|index| {
                (
                    format!("file:///w/{index}"),
                    PathBuf::from(format!("/w/{index}")),
                )
            })
            .collect();
        assert!(membership_within_capacity(
            owned
                .iter()
                .map(|(uri, path)| (uri.as_str(), path.as_path()))
        ));
    }

    #[test]
    fn one_over_entry_limit_is_rejected() {
        let owned: Vec<(String, PathBuf)> = (0..=MAX_WORKSPACE_FOLDER_ENTRIES)
            .map(|index| {
                (
                    format!("file:///w/{index}"),
                    PathBuf::from(format!("/w/{index}")),
                )
            })
            .collect();
        assert!(!membership_within_capacity(
            owned
                .iter()
                .map(|(uri, path)| (uri.as_str(), path.as_path()))
        ));
    }

    #[test]
    fn bounded_path_list_reports_omitted_without_full_join() {
        let paths: Vec<PathBuf> = (0..20)
            .map(|index| PathBuf::from(format!("/w/{index}")))
            .collect();
        let summary = format_bounded_path_list(&paths);
        assert!(
            summary.text.len() <= MAX_WORKSPACE_FOLDER_DISCLOSURE_BYTES,
            "{}",
            summary.text.len()
        );
        assert!(summary.omitted_count > 0);
        assert!(!summary.text.contains("/w/19"));
    }
}
