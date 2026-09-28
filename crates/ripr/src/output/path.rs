use std::path::Path;

/// Render a path with stable slash separators for JSON and Markdown output.
pub(crate) fn display_path(path: &Path) -> String {
    display_path_text(&crate::analysis::stable_path_text(path))
}

/// The part of a finding's file that CI consumers resolve from the checkout:
/// an absolute `--root` prefix is dropped, while a relative root keeps its
/// spelling (it already names a path under the invoking directory).
/// Renderers apply their own text encoding to the result.
pub(crate) fn repository_relative_path<'a>(root: &Path, file: &'a Path) -> &'a Path {
    if root.is_absolute() {
        file.strip_prefix(root).unwrap_or(file)
    } else {
        file
    }
}

/// Render [`repository_relative_path`] as stable text without a leading `./`,
/// so `--root .`, `--root ./` and `--root "$PWD"` render the same path.
pub(crate) fn repository_display_path(root: &Path, file: &Path) -> String {
    let mut displayed = display_path(repository_relative_path(root, file));
    while let Some(stripped) = displayed.strip_prefix("./") {
        displayed = stripped.to_string();
    }
    displayed
}

/// Render path-like text with stable slash separators for JSON and Markdown output.
pub(crate) fn display_path_text(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{display_path, display_path_text, repository_display_path};

    #[test]
    fn display_path_normalizes_backslashes() {
        assert_eq!(display_path(Path::new("a\\b\\c")), "a/b/c");
    }

    #[test]
    fn display_path_text_normalizes_backslashes() {
        assert_eq!(display_path_text("a\\b\\c"), "a/b/c");
    }

    #[test]
    fn display_path_preserves_forward_slashes() {
        assert_eq!(display_path(Path::new("a/b/c")), "a/b/c");
    }

    #[test]
    fn repository_display_path_ignores_root_spelling() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/work"));
        let spellings = [
            (
                Path::new(".").to_path_buf(),
                Path::new("./src/lib.rs").to_path_buf(),
            ),
            (
                Path::new("./").to_path_buf(),
                Path::new("./src/lib.rs").to_path_buf(),
            ),
            (cwd.clone(), cwd.join("src/lib.rs")),
        ];
        for (root, file) in spellings {
            assert_eq!(
                repository_display_path(&root, &file),
                "src/lib.rs",
                "root {} file {}",
                root.display(),
                file.display()
            );
        }
    }

    #[test]
    fn repository_display_path_keeps_relative_root_prefix() {
        // A relative root names a subdirectory of the invoking directory, so
        // its prefix is part of the checkout-relative path.
        assert_eq!(
            repository_display_path(Path::new("crates/app"), Path::new("crates/app/src/lib.rs")),
            "crates/app/src/lib.rs"
        );
    }
}
