//! Diff intake disclosures for changes that reach ripr without analyzable
//! text (#15324).
//!
//! A deleted file has no new-side code, so it seeds no probe. Before #15324
//! the run said so only when nothing else changed; a deletion beside any
//! other changed file left an empty result that never mentioned the removed
//! file or the tests it held. [`deleted_files_disclosure`] names the deleted
//! files whenever the result is empty.
//!
//! Git prints `Binary files a/x and b/x differ` instead of hunks for a file
//! it treats as binary. For a source file that is almost always a `-diff` or
//! `binary` attribute in `.gitattributes`, or a NUL byte in the file, and the
//! change is then invisible to ripr. The diff parser records each such
//! section's path through the same header, prefix, quoting and scope rules as
//! every other section; [`binary_rendered_source_paths`] keeps the source
//! files among them so the run can name the files, the cause and the recovery
//! instead of reading as a complete analysis.

use std::path::{Path, PathBuf};

use crate::analysis::language::route;
use crate::analysis::pipeline::bounded_path_listing;
use crate::terminal_text::terminal_safe;

/// The stderr line for `deleted_paths`, or `None` when nothing was deleted.
/// `other_changes` is whether the diff also changed files ripr reads.
pub(crate) fn deleted_files_disclosure(
    deleted_paths: &[PathBuf],
    other_changes: bool,
) -> Option<String> {
    if deleted_paths.is_empty() {
        return None;
    }
    let names = deleted_paths
        .iter()
        .map(|path| terminal_safe(path.to_string_lossy().replace('\\', "/")))
        .collect::<Vec<_>>();
    let listed = bounded_path_listing(&names);
    let count = deleted_paths.len();
    let reason = if other_changes {
        "deleted files produce no probes (deleted behavior has no new-side code to analyze), \
         so this result covers only the other changed files"
    } else {
        "deletion-only diffs produce no probes (deleted behavior has no new-side code to analyze)"
    };
    Some(format!(
        "ripr: diff deleted {count} file(s) ({listed}); {reason}."
    ))
}

/// The binary-rendered section paths (from the diff parser) that route to a
/// language ripr analyzes and that `already_read` does not claim, as
/// portable strings in diff order.
pub(crate) fn binary_rendered_source_paths(
    binary_paths: &[PathBuf],
    already_read: impl Fn(&Path) -> bool,
) -> Vec<String> {
    binary_paths
        .iter()
        .filter(|path| route(path).is_some() && !already_read(path))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect()
}

/// The recovery text for the typed limitation on binary-rendered sources.
pub(crate) const BINARY_SOURCE_RECOVERY: &str = "Git printed these source files as binary, \
     usually because of a `-diff` or `binary` attribute in .gitattributes or a NUL byte in the \
     file. Remove the attribute for source files, or pass a `git diff --text` patch with --diff.";

/// The limitation detail naming the binary-rendered source files.
/// Each path is escaped before the listing is bounded, so the detail stays
/// inside the limitation's length cap whatever the file names hold.
pub(crate) fn binary_source_detail(paths: &[String]) -> String {
    let escaped = paths
        .iter()
        .map(|path| terminal_safe(path.clone()))
        .collect::<Vec<_>>();
    format!(
        "{} changed source file(s) were rendered by git as binary, so their changes were not \
         analyzed: {}",
        paths.len(),
        bounded_path_listing(&escaped)
    )
}

#[cfg(test)]
mod tests {
    use super::{binary_rendered_source_paths, binary_source_detail, deleted_files_disclosure};
    use std::path::{Path, PathBuf};

    #[test]
    fn a_deletion_beside_other_changes_names_the_file_and_the_narrower_scope() -> Result<(), String>
    {
        let message = deleted_files_disclosure(&[PathBuf::from("src/pricing.rs")], true)
            .ok_or_else(|| "a deleted file must produce a disclosure".to_string())?;
        assert_eq!(
            message,
            "ripr: diff deleted 1 file(s) (src/pricing.rs); deleted files produce no probes \
             (deleted behavior has no new-side code to analyze), so this result covers only \
             the other changed files."
        );
        Ok(())
    }

    #[test]
    fn a_deletion_only_diff_keeps_its_wording_and_names_the_files() -> Result<(), String> {
        let paths = ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"].map(PathBuf::from);
        let message = deleted_files_disclosure(&paths, false)
            .ok_or_else(|| "deleted files must produce a disclosure".to_string())?;
        assert_eq!(
            message,
            "ripr: diff deleted 4 file(s) (src/a.rs, src/b.rs, src/c.rs and 1 more); \
             deletion-only diffs produce no probes (deleted behavior has no new-side code to \
             analyze)."
        );
        assert_eq!(deleted_files_disclosure(&[], false), None);
        assert_eq!(deleted_files_disclosure(&[], true), None);
        Ok(())
    }

    #[test]
    fn a_control_character_in_a_deleted_path_is_written_escaped() -> Result<(), String> {
        let message = deleted_files_disclosure(&[PathBuf::from("src/\u{1b}[2Jx.rs")], true)
            .ok_or_else(|| "a deleted file must produce a disclosure".to_string())?;
        assert!(!message.contains('\u{1b}'), "{message}");
        assert!(message.contains("src/\\u{1b}[2Jx.rs"), "{message}");
        Ok(())
    }

    #[test]
    fn only_unread_routed_sources_are_named() {
        let paths = [
            "src/tax.rs",
            "logo.png",
            "app/x and y.py",
            "src/copied.rs",
            "src/café.rs",
        ]
        .map(PathBuf::from);
        let named = binary_rendered_source_paths(&paths, |path| path == Path::new("src/copied.rs"));
        assert_eq!(
            named,
            vec![
                "src/tax.rs".to_string(),
                "app/x and y.py".to_string(),
                "src/café.rs".to_string(),
            ]
        );
        assert!(binary_rendered_source_paths(&[], |_| false).is_empty());
    }

    #[test]
    fn hostile_names_keep_the_detail_inside_the_limitation_cap() {
        let name = format!("src/{}.rs", "\u{1b}".repeat(400));
        let detail = binary_source_detail(&[name.clone(), name.clone(), name]);
        assert!(
            detail.chars().count() <= crate::analysis_outcome::MAX_ANALYSIS_LIMITATION_DETAIL_CHARS,
            "{detail}"
        );
        assert!(!detail.contains('\u{1b}'));
    }

    #[test]
    fn the_binary_source_detail_counts_and_escapes_the_paths() {
        let detail = binary_source_detail(&["src/\u{1b}x.rs".to_string()]);
        assert_eq!(
            detail,
            "1 changed source file(s) were rendered by git as binary, so their changes were not \
             analyzed: src/\\u{1b}x.rs"
        );
    }
}
