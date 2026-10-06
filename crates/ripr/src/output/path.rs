use std::path::{Component, Path, PathBuf};

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

/// Render a finding's file root-relative for machine consumers, tolerating
/// producer spelling drift: a canonicalized `\\?\X:` file still matches a
/// plain `--root`, and drive-letter case drift matches on Windows, so
/// committed-history reads and worktree reads render the same relative path
/// (#5254 item 6). A file that is not under the root (or a relative root,
/// which keeps its spelling per [`repository_relative_path`]) falls back to
/// the full stable spelling rather than inventing a location.
pub(crate) fn repository_relative_path_text(root: &Path, file: &Path) -> String {
    repository_relative_path_text_on(root, file, cfg!(windows))
}

/// [`repository_relative_path_text`] with the host made explicit so both
/// answers are testable anywhere (#4378).
pub(crate) fn repository_relative_path_text_on(root: &Path, file: &Path, windows: bool) -> String {
    // Fast path: host-native exact prefix. A relative root keeps its spelling
    // (it already names a path under the invoking directory), and an empty
    // remainder (file == root) falls through to the full spelling. Rootedness
    // is host-explicit like the rest of this function: `Path::is_absolute`
    // would call `/repo` relative on Windows. On Unix this path is also
    // byte-exact for non-UTF-8 paths, which never reach the lossy Windows
    // slow path below.
    if is_absolute_on(&root.to_string_lossy(), windows)
        && let Ok(relative) = file.strip_prefix(root)
        && !relative.as_os_str().is_empty()
    {
        return display_path(relative);
    }
    if windows
        && let Some(relative) =
            strip_windows_root_prefix_text(&root.to_string_lossy(), &file.to_string_lossy())
    {
        // Re-encode through the shared stable renderer so `%` escaping
        // matches every other path rendering.
        return display_path(&PathBuf::from(relative));
    }
    display_path(file)
}

/// Strip a Windows absolute root from a file across the spelling drift a
/// plain `strip_prefix` rejects: verbatim `\\?\X:` / `\\?\UNC\` prefixes in
/// either operand, mixed separators, and ASCII-case drift. String-level on
/// purpose: `Path::components` parses Windows prefixes only on Windows, so a
/// component implementation would be untestable in the Linux matrix, and the
/// non-Unix `stable_path_text` branch this feeds is itself lossy. Comparison
/// is segment-wise so `C:/repo` never matches `C:/repo2/f`. `None` when the
/// file is not under the root, the root is not absolute, or the remainder is
/// empty.
fn strip_windows_root_prefix_text(root_text: &str, file_text: &str) -> Option<String> {
    // Both sides must be absolute: a relative file whose first segments
    // happen to equal a UNC server/share (or a drive-relative `C:foo`
    // shape) must keep its spelling, not lose real components.
    let root_segments = windows_absolute_segments(root_text)?;
    let file_segments = windows_absolute_segments(file_text)?;
    if root_segments.len() >= file_segments.len() {
        return None;
    }
    for (root_segment, file_segment) in root_segments.iter().zip(file_segments.iter()) {
        if !root_segment.eq_ignore_ascii_case(file_segment) {
            return None;
        }
    }
    Some(file_segments[root_segments.len()..].join("/"))
}

/// Split a Windows path into comparable segments: separators unified,
/// verbatim drive/UNC prefixes reduced to their plain form. Returns `None`
/// for anything that is not an absolute drive-letter or `//` path, so
/// relative inputs keep the fallback spelling. Other `//?/` forms keep their
/// spelling and can only match byte-identically.
fn windows_absolute_segments(path_text: &str) -> Option<Vec<String>> {
    let unified = path_text.replace('\\', "/");
    let plain = strip_windows_verbatim_prefix(&unified).unwrap_or(unified);
    let absolute = is_windows_drive_path(&plain) || plain.starts_with("//");
    if !absolute {
        return None;
    }
    Some(
        plain
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Reduce a verbatim prefix to its plain form: `//?/X:` becomes `X:`,
/// `//?/UNC/server/share` becomes `//server/share`. Any other `//?/` form
/// (and non-verbatim text) is returned unchanged.
fn strip_windows_verbatim_prefix(unified: &str) -> Option<String> {
    let rest = unified.strip_prefix("//?/")?;
    let bytes = rest.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Some(rest.to_string());
    }
    // Byte-level compare: `rest[..4]` could split a UTF-8 sequence. A
    // successful match means the first four bytes are ASCII, so `rest[4..]`
    // is a char boundary.
    if bytes.len() > 4 && bytes[..4].eq_ignore_ascii_case(b"UNC/") {
        return Some(format!("//{}", &rest[4..]));
    }
    None
}

/// Whether the path text is absolute on the named host. Lossy conversion is
/// safe here: rootedness depends on ASCII separators and drive letters only.
fn is_absolute_on(root_text: &str, windows: bool) -> bool {
    if !windows {
        return root_text.starts_with('/');
    }
    let unified = root_text.replace('\\', "/");
    let plain = strip_windows_verbatim_prefix(&unified).unwrap_or(unified);
    is_windows_drive_path(&plain) || plain.starts_with("//")
}

/// Whether the unified text starts with a drive-letter root (`X:`).
fn is_windows_drive_path(unified: &str) -> bool {
    unified.len() >= 2
        && unified.as_bytes()[0].is_ascii_alphabetic()
        && unified.as_bytes()[1] == b':'
}

/// Render path-like text with stable slash separators for JSON and Markdown output.
pub(crate) fn display_path_text(path: &str) -> String {
    path.replace('\\', "/")
}

/// Compare output destinations after collapsing `.` / `..` so `--out ./a.json`
/// and `--out-jsonl a.json` are treated as the same file.
pub(crate) fn same_output_leaf(left: &Path, right: &Path) -> bool {
    normalize_output_leaf(left) == normalize_output_leaf(right)
}

fn normalize_output_leaf(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in Path::new(&display_path(path)).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Render a path for human CLI display (`ripr init`, `ripr doctor`) with one
/// separator convention on the running host (#4378). See
/// [`human_path_text`].
pub(crate) fn human_path(path: &Path) -> String {
    human_path_text(&path.to_string_lossy(), cfg!(windows))
}

/// Human display rule for path text, pure over the host so both answers are
/// testable anywhere (#4378).
///
/// On Windows `\` is a separator: a drive-letter verbatim prefix (`\\?\`
/// before `<drive>:`) renders as the plain path and every `\` renders as `/`,
/// so a forward-slash argv prefix and a joined suffix never mix separators in
/// one path. Other verbatim forms (`\\?\UNC\..`, `\\?\Volume{..}`) render
/// exactly as given, like `app::verification_execution`'s normalizer. On
/// Unix `\` is an ordinary filename character, so the text is returned
/// unchanged; rewriting it would name a different file.
pub(crate) fn human_path_text(text: &str, windows: bool) -> String {
    if !windows {
        return text.to_string();
    }
    let plain = match text.strip_prefix(r"\\?\") {
        None => text,
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest,
        Some(_) => return text.to_string(),
    };
    plain.replace('\\', "/")
}

/// A valid user-supplied alias can resolve to non-UTF-8 filesystem bytes.
/// Keep a lossless absolute alias in that case, without collapsing `..`:
/// its filesystem traversal still selects the diagnosed physical directory.
pub(crate) fn command_root_display(root: &Path, resolved: &Path) -> Result<String, String> {
    if resolved.to_str().is_some() {
        return Ok(human_path(resolved));
    }
    absolute_command_root_display(root)
}

pub(crate) fn absolute_command_root_display(root: &Path) -> Result<String, String> {
    let path = if root.is_absolute() {
        std::borrow::Cow::Borrowed(root)
    } else {
        let cwd = std::env::current_dir()
            .map_err(|error| format!("cannot bind the selected root to its directory: {error}"))?;
        std::borrow::Cow::Owned(cwd.join(root))
    };
    require_lossless_command_path(&path)?;
    Ok(human_path(&path))
}

fn require_lossless_command_path(path: &Path) -> Result<(), String> {
    path.to_str().map(|_| ()).ok_or_else(|| {
        "selected root cannot be represented losslessly in a command; rerun doctor from a UTF-8 parent using a UTF-8 alias".to_string()
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        display_path, display_path_text, human_path_text, repository_display_path,
        repository_relative_path_text_on, same_output_leaf,
    };

    #[test]
    fn human_path_text_uses_one_separator_and_drops_verbatim_prefix_on_windows() {
        // `check-local-context` forbids drive-letter path literals, so the
        // Windows shapes are assembled from parts.
        let drive = "F:";
        assert_eq!(
            human_path_text(&format!(r"{drive}/Temp/demo\ripr.toml"), true),
            format!("{drive}/Temp/demo/ripr.toml")
        );
        assert_eq!(
            human_path_text(&format!(r"\\?\{drive}\code\target\debug\ripr.exe"), true),
            format!("{drive}/code/target/debug/ripr.exe")
        );
        assert_eq!(human_path_text(r".\Cargo.toml", true), "./Cargo.toml");
        // Verbatim forms other than a drive letter are shown exactly as given.
        for verbatim in [r"\\?\UNC\server\share\ripr.exe", r"\\?\Volume{1}\x"] {
            assert_eq!(human_path_text(verbatim, true), verbatim);
        }
    }

    #[test]
    fn human_path_text_keeps_unix_backslash_filenames() {
        assert_eq!(
            human_path_text(r"/tmp/odd\name/ripr.toml", false),
            r"/tmp/odd\name/ripr.toml"
        );
        let verbatim = format!(r"\\?\{}\x", "F:");
        assert_eq!(human_path_text(&verbatim, false), verbatim);
    }

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

    #[test]
    fn same_output_leaf_collapses_dot_and_parent_components() {
        assert!(same_output_leaf(
            Path::new("./policy-history.json"),
            Path::new("policy-history.json")
        ));
        assert!(same_output_leaf(
            Path::new("reports/../ledger.json"),
            Path::new("ledger.json")
        ));
        assert!(!same_output_leaf(
            Path::new("ledger.json"),
            Path::new("ledger.jsonl")
        ));
    }

    #[test]
    fn relative_path_text_strips_an_exact_unix_prefix() {
        assert_eq!(
            repository_relative_path_text_on(
                Path::new("/repo"),
                Path::new("/repo/src/lib.rs"),
                false
            ),
            "src/lib.rs"
        );
        // A relative file is already root-relative: identity.
        assert_eq!(
            repository_relative_path_text_on(Path::new("/repo"), Path::new("src/lib.rs"), false),
            "src/lib.rs"
        );
    }

    #[test]
    fn relative_path_text_tolerates_verbatim_and_case_drift_on_windows() {
        // `check-local-context` forbids drive-letter path literals, so the
        // Windows shapes are assembled from parts.
        let drive = "F:";
        let root_text = format!(r"{drive}\repo");
        let root = Path::new(&root_text);
        // The observed #5254 shape: canonicalized file, plain root.
        let verbatim_text = format!(r"\\?\{drive}\repo\src\lib.rs");
        let verbatim = Path::new(&verbatim_text);
        assert_eq!(
            repository_relative_path_text_on(root, verbatim, true),
            "src/lib.rs"
        );
        // Drift in either direction, with mixed separators.
        let verbatim_root_text = format!(r"\\?\{drive}\repo");
        let verbatim_root = Path::new(&verbatim_root_text);
        let plain_text = format!(r"{drive}/repo/src/lib.rs");
        let plain = Path::new(&plain_text);
        assert_eq!(
            repository_relative_path_text_on(verbatim_root, plain, true),
            "src/lib.rs"
        );
        // Identical spellings strip on every host (fast path where the
        // host parses prefixes, slow path elsewhere).
        let same_text = format!(r"{drive}\repo\src\lib.rs");
        assert_eq!(
            repository_relative_path_text_on(root, Path::new(&same_text), true),
            "src/lib.rs"
        );
        // Case drift matches, and the remainder keeps the file's spelling.
        let upper_text = format!(r"{drive}\REPO\SRC\lib.rs");
        let upper_file = Path::new(&upper_text);
        assert_eq!(
            repository_relative_path_text_on(root, upper_file, true),
            "SRC/lib.rs"
        );
        // Verbatim UNC matches plain UNC.
        assert_eq!(
            repository_relative_path_text_on(
                Path::new(r"\\server\share\dir"),
                Path::new(r"\\?\UNC\server\share\dir\f.rs"),
                true
            ),
            "f.rs"
        );
        // `%` in the remainder keeps the shared stable escaping.
        let percent_text = format!(r"{drive}\repo\100%.rs");
        assert_eq!(
            repository_relative_path_text_on(root, Path::new(&percent_text), true),
            "100%25.rs"
        );
    }

    #[test]
    fn relative_path_text_falls_back_to_the_full_spelling() {
        let drive = "F:";
        let root_text = format!(r"{drive}\repo");
        let root = Path::new(&root_text);
        // Segment-wise: `repo` never matches `repo2`.
        let sibling_text = format!(r"{drive}\repo2\f.rs");
        assert_eq!(
            repository_relative_path_text_on(root, Path::new(&sibling_text), true),
            format!(r"{drive}/repo2/f.rs")
        );
        // A different drive is not under the root.
        let other = "G:";
        let other_text = format!(r"{other}\repo\f.rs");
        assert_eq!(
            repository_relative_path_text_on(root, Path::new(&other_text), true),
            format!(r"{other}/repo/f.rs")
        );
        // A relative root keeps the file's spelling.
        assert_eq!(
            repository_relative_path_text_on(Path::new("repo"), Path::new("repo/f.rs"), true),
            "repo/f.rs"
        );
        // No empty remainder: file == root keeps the full spelling.
        assert_eq!(
            repository_relative_path_text_on(root, root, true),
            format!(r"{drive}/repo")
        );
        // Case drift is significant off Windows.
        assert_eq!(
            repository_relative_path_text_on(Path::new("/repo"), Path::new("/REPO/f.rs"), false),
            "/REPO/f.rs"
        );
        // A relative file keeps its spelling even when its first segments
        // collide with a UNC server/share: rootedness is verified on both
        // sides, not just the root.
        assert_eq!(
            repository_relative_path_text_on(
                Path::new(r"\\server\share"),
                Path::new("server/share/src/lib.rs"),
                true
            ),
            "server/share/src/lib.rs"
        );
    }

    /// Native verification through the production entry point: on Windows a
    /// verbatim producer path strips against a plain root via the real
    /// host-native `Path` parsing, not just the emulated-text matrix.
    #[cfg(windows)]
    #[test]
    fn relative_path_text_strips_verbatim_producer_paths_natively_on_windows() {
        let drive = "F:";
        let root_text = format!(r"{drive}\repo");
        let verbatim_text = format!(r"\\?\{drive}\repo\src\lib.rs");
        assert_eq!(
            super::repository_relative_path_text(Path::new(&root_text), Path::new(&verbatim_text)),
            "src/lib.rs"
        );
    }

    /// The Unix-native counterpart: a verbatim-looking spelling is an
    /// ordinary filename there, so the production entry point keeps it.
    #[cfg(unix)]
    #[test]
    fn relative_path_text_keeps_verbatim_spellings_natively_on_unix() {
        let drive = "F:";
        let root_text = "/repo".to_string();
        let verbatim_looking = format!(r"\\?\{drive}\repo\src\lib.rs");
        assert_eq!(
            super::repository_relative_path_text(
                Path::new(&root_text),
                Path::new(&verbatim_looking)
            ),
            verbatim_looking.replace('\\', "/")
        );
    }
}
