//! #15121: a `copy from`/`copy to` section creates a file whose copied lines
//! the patch does not carry. A rename moves code; a copy leaves its source in
//! place, so every line of the destination is new code in the candidate. The
//! diff intake therefore reads each copy destination from the working tree as
//! an added file, the same changed lines the patch would name without copy
//! detection (`git diff --no-renames`).

use super::model::{ChangedFile, ChangedLine};
use super::parse::{DIFF_CHANGED_LINE_LIMIT_ENV, diff_changed_line_limit_from_env_core};
use std::path::{Path, PathBuf};

/// Replace the changed lines of every copy destination with the whole file
/// as added lines. A destination that is not a regular file reached through
/// real directories below `root`, has no content in the analyzed snapshot, or
/// is not UTF-8 text, keeps the lines its section carried: the absent-file disclosure, not this reader, owns a
/// missing file. The parser's changed-line ceiling stays in force over the
/// expanded total.
pub(crate) fn expand_copy_destinations(
    changed_files: &mut [ChangedFile],
    copy_destinations: &[PathBuf],
    root: &Path,
) -> Result<(), String> {
    if copy_destinations.is_empty() {
        return Ok(());
    }
    let limit = diff_changed_line_limit_from_env_core().map_err(String::from)?;
    for file in changed_files.iter_mut() {
        if !copy_destinations.contains(&file.path) {
            continue;
        }
        // The same read the adapters make: committed bytes for a dirty path
        // under a committed-history overlay, the working tree otherwise.
        let Some(source) =
            crate::analysis::workspace::worktree_regular_source_file(root, &file.path)
                .and_then(|on_disk| {
                    let relative = on_disk.strip_prefix(root).ok()?;
                    crate::analysis::committed_source::read_source_bytes(root, relative).ok()?
                })
                .and_then(|bytes| String::from_utf8(bytes).ok())
        else {
            continue;
        };
        file.removed_lines.clear();
        file.added_lines = whole_file_as_added(&source);
    }
    let count = changed_files
        .iter()
        .map(|file| file.added_lines.len() + file.removed_lines.len())
        .sum::<usize>();
    if count > limit {
        return Err(format!(
            "diff_scope_oversized: {count} changed lines, counting every line of the files \
             the diff creates by copy, exceed the {DIFF_CHANGED_LINE_LIMIT_ENV} limit \
             ({limit}); analysis was not run. Repair route: reduce the diff scope, run a \
             narrower diff, or raise the limit via {DIFF_CHANGED_LINE_LIMIT_ENV}=<number>."
        ));
    }
    Ok(())
}

/// Every line of `source` as an added line, numbered and spelled the way the
/// parser records a new file's hunk: no line ending, no line-1 byte-order mark.
fn whole_file_as_added(source: &str) -> Vec<ChangedLine> {
    source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .lines()
        .enumerate()
        .map(|(index, text)| ChangedLine {
            line: index + 1,
            text: text.to_string(),
            new_side_line: index + 1,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{expand_copy_destinations, whole_file_as_added};
    use crate::analysis::diff::{ChangedFile, parse_unified_diff_with_metadata};
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> Result<PathBuf, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("clock failed: {error}"))?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ripr-copy-section-test-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("src"))
            .map_err(|error| format!("mkdir {}: {error}", dir.display()))?;
        Ok(dir)
    }

    fn write(path: PathBuf, text: &str) -> Result<(), String> {
        std::fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
    }

    /// One file's path, added lines and removed-line count.
    type FileShape = (PathBuf, Vec<(usize, usize, String)>, usize);

    fn shape(files: &[ChangedFile]) -> Vec<FileShape> {
        files
            .iter()
            .map(|file| {
                (
                    file.path.clone(),
                    file.added_lines
                        .iter()
                        .map(|line| (line.line, line.new_side_line, line.text.clone()))
                        .collect(),
                    file.removed_lines.len(),
                )
            })
            .collect()
    }

    const SOURCE: &str = "pub fn fee(amount: i64) -> i64 {\n    amount + 1\n}\n";

    fn added_file_patch(path: &str, source: &str) -> String {
        let body = source
            .lines()
            .map(|line| format!("+{line}\n"))
            .collect::<String>();
        format!(
            "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n{body}",
            source.lines().count()
        )
    }

    #[test]
    fn a_copy_destination_reads_as_the_added_file_the_plain_patch_names() -> Result<(), String> {
        let root = unique_temp_dir("expand")?;
        // CRLF on disk and a byte-order mark: the patch spelling has neither.
        let on_disk = format!("\u{feff}{}", SOURCE.replace('\n', "\r\n"));
        write(root.join("src/fee.rs"), &on_disk)?;

        let pure = "diff --git a/src/price.rs b/src/fee.rs\nsimilarity index 100%\ncopy from src/price.rs\ncopy to src/fee.rs\n";
        let edited = "diff --git a/src/price.rs b/src/fee.rs\nsimilarity index 80%\ncopy from src/price.rs\ncopy to src/fee.rs\n--- a/src/price.rs\n+++ b/src/fee.rs\n@@ -2 +2 @@\n-    amount - 1\n+    amount + 1\n";
        let plain = parse_unified_diff_with_metadata(&added_file_patch("src/fee.rs", SOURCE));
        for patch in [pure, edited] {
            let mut parsed = parse_unified_diff_with_metadata(patch);
            assert_eq!(
                parsed.copy_destination_paths,
                vec![PathBuf::from("src/fee.rs")],
                "{patch}"
            );
            assert_eq!(parsed.renamed_file_count, 0, "{patch}");
            expand_copy_destinations(
                &mut parsed.changed_files,
                &parsed.copy_destination_paths,
                &root,
            )?;
            assert_eq!(
                shape(&parsed.changed_files),
                shape(&plain.changed_files),
                "{patch}"
            );
        }
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn only_a_verified_copy_destination_on_disk_is_expanded() -> Result<(), String> {
        let root = unique_temp_dir("verified")?;
        write(root.join("src/fee.rs"), SOURCE)?;
        write(root.join("src/new.rs"), SOURCE)?;

        // A rename with an edit keeps its hunk lines: its other lines moved.
        let rename = "diff --git a/src/old.rs b/src/new.rs\nsimilarity index 80%\nrename from src/old.rs\nrename to src/new.rs\n--- a/src/old.rs\n+++ b/src/new.rs\n@@ -2 +2 @@\n-    amount - 1\n+    amount + 1\n";
        // `copy to` naming a path the header does not, no `similarity index`,
        // and `copy from` naming another source: none is a copy destination.
        let foreign = "diff --git a/src/price.rs b/src/other.rs\nsimilarity index 100%\ncopy from src/price.rs\ncopy to src/fee.rs\n";
        let bare =
            "diff --git a/src/price.rs b/src/fee.rs\ncopy from src/price.rs\ncopy to src/fee.rs\n";
        let other_source = "diff --git a/src/price.rs b/src/fee.rs\nsimilarity index 100%\ncopy from src/lib.rs\ncopy to src/fee.rs\n";
        for patch in [rename, foreign, bare, other_source] {
            let mut parsed = parse_unified_diff_with_metadata(patch);
            assert!(parsed.copy_destination_paths.is_empty(), "{patch}");
            let before = shape(&parsed.changed_files);
            expand_copy_destinations(
                &mut parsed.changed_files,
                &parsed.copy_destination_paths,
                &root,
            )?;
            assert_eq!(shape(&parsed.changed_files), before, "{patch}");
        }

        // A verified destination that is absent, or reached through a
        // symlink, stays registered with the lines its section carried.
        let absent = "diff --git a/src/price.rs b/src/gone.rs\nsimilarity index 100%\ncopy from src/price.rs\ncopy to src/gone.rs\n";
        let mut parsed = parse_unified_diff_with_metadata(absent);
        assert_eq!(
            parsed.copy_destination_paths,
            vec![PathBuf::from("src/gone.rs")]
        );
        expand_copy_destinations(
            &mut parsed.changed_files,
            &parsed.copy_destination_paths,
            &root,
        )?;
        assert_eq!(
            shape(&parsed.changed_files),
            vec![(PathBuf::from("src/gone.rs"), Vec::new(), 0)]
        );
        #[cfg(unix)]
        {
            let outside = unique_temp_dir("outside")?;
            write(outside.join("secret.rs"), SOURCE)?;
            std::os::unix::fs::symlink(outside.join("secret.rs"), root.join("src/gone.rs"))
                .map_err(|error| error.to_string())?;
            let mut parsed = parse_unified_diff_with_metadata(absent);
            expand_copy_destinations(
                &mut parsed.changed_files,
                &parsed.copy_destination_paths,
                &root,
            )?;
            assert!(
                parsed
                    .changed_files
                    .iter()
                    .all(|file| file.added_lines.is_empty())
            );
        }
        Ok(())
    }

    #[test]
    fn whole_file_lines_are_numbered_from_one_without_endings() {
        let lines = whole_file_as_added("a\r\n\r\nb");
        let seen = lines
            .iter()
            .map(|line| (line.line, line.new_side_line, line.text.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(seen, vec![(1, 1, "a"), (2, 2, ""), (3, 3, "b")]);
        assert!(whole_file_as_added("").is_empty());
    }
}
