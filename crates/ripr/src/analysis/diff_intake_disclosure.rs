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
//! change is then invisible to ripr. [`binary_rendered_source_paths`] finds
//! those sections so the run can name the files, the cause and the recovery
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

/// The new-side paths of file sections that git rendered as binary although
/// the path routes to a language ripr analyzes. A deleted file
/// (`and /dev/null differ`) is left to the deletion disclosure. A sentinel
/// whose sides cannot be split unambiguously is skipped rather than guessed.
pub(crate) fn binary_rendered_source_paths(diff_text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in diff_text.lines() {
        let Some(path) = binary_sentinel_new_path(line) else {
            continue;
        };
        if route(Path::new(&path)).is_some() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
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

/// The new side of `Binary files <old> and <new> differ`. Each side is
/// `/dev/null` or a path with a one-component prefix (`a/`, `b/`, or a
/// mnemonic prefix); the split is the ` and ` where both sides name the same
/// path, or where the old side is `/dev/null` (an added file); a sentinel
/// with a single ` and ` also reads as a rename to its new side.
fn binary_sentinel_new_path(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("Binary files ")?
        .strip_suffix(" differ")?;
    let splits = rest
        .match_indices(" and ")
        .map(|(at, separator)| (unquote(&rest[..at]), unquote(&rest[at + separator.len()..])))
        .collect::<Vec<_>>();
    let paired = splits.iter().find_map(|(old, new)| {
        let new_path = strip_side_prefix(new).filter(|_| *new != "/dev/null")?;
        (*old == "/dev/null" || strip_side_prefix(old) == Some(new_path)).then_some(new_path)
    });
    // A renamed binary file names two different paths; that is only
    // unambiguous when the sentinel has a single ` and `.
    let renamed = || match splits.as_slice() {
        [(old, new)] if *old != "/dev/null" && *new != "/dev/null" => {
            strip_side_prefix(old).and(strip_side_prefix(new))
        }
        _ => None,
    };
    paired.or_else(renamed).map(str::to_string)
}

fn unquote(side: &str) -> &str {
    side.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(side)
}

fn strip_side_prefix(side: &str) -> Option<&str> {
    let (_, path) = side.split_once('/')?;
    (!path.is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::{binary_rendered_source_paths, binary_source_detail, deleted_files_disclosure};
    use std::path::PathBuf;

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
    fn binary_sentinels_on_analyzed_sources_are_named_and_others_are_not() {
        let diff = "\
diff --git a/src/pricing.rs b/src/pricing.rs
index 059b527..c63ac94 100644
Binary files a/src/pricing.rs and b/src/pricing.rs differ
diff --git a/src/new.rs b/src/new.rs
new file mode 100644
Binary files /dev/null and b/src/new.rs differ
diff --git a/src/gone.rs b/src/gone.rs
deleted file mode 100644
Binary files a/src/gone.rs and /dev/null differ
diff --git a/logo.png b/logo.png
Binary files a/logo.png and b/logo.png differ
diff --git a/app/x and y.py b/app/x and y.py
Binary files a/app/x and y.py and b/app/x and y.py differ
Binary files nonsense differ
Binary files a/src/one.rs and b/src/two.rs differ
";
        assert_eq!(
            binary_rendered_source_paths(diff),
            vec![
                "src/pricing.rs".to_string(),
                "src/new.rs".to_string(),
                "app/x and y.py".to_string(),
                "src/two.rs".to_string(),
            ]
        );
        assert!(
            binary_rendered_source_paths("diff --git a/x b/x\n@@ -1 +1 @@\n-a\n+b\n").is_empty()
        );
    }

    #[test]
    fn mnemonic_and_quoted_sides_pair_by_path() {
        assert_eq!(
            binary_rendered_source_paths(
                "Binary files \"i/src/lib.rs\" and \"w/src/lib.rs\" differ\n"
            ),
            vec!["src/lib.rs".to_string()]
        );
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
