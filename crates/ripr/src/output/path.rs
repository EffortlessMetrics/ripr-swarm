use std::path::{Component, Path, PathBuf};

#[cfg(test)]
pub(crate) use ripr_contract::path_identity::path_identity_key_with;

pub(crate) use ripr_contract::path_identity::{
    host_paths_case_insensitive, path_identity_key, path_identity_key_on,
};

/// The location text a human report shows for a finding file (#15473).
///
/// [`crate::analysis::finding_location_text`] spells a root-relative file
/// `./<path>`, which JSON, MCP, LSP and selectors keep as the identity.
/// A reader who ran `ripr` from a directory below the analysis root (the
/// implicit root walk moved up from `py/` to the repository root) cannot paste
/// that `./` form: it names nothing under the invocation directory. When the
/// invocation directory is strictly inside the root, the human spelling is
/// relative to the invocation directory (`../crates/core/src/lib.rs`). At the
/// root, with a relative root, or for a file outside the root, the text is the
/// shared location text byte for byte.
pub(crate) fn human_location_text(root: &Path, file: &Path) -> String {
    let shared = crate::analysis::finding_location_text(root, file);
    match invocation_depth_below(root) {
        Some(depth) => relative_to_invocation_dir(&shared, depth),
        None => shared,
    }
}

/// Pure core of [`human_location_text`]: `depth` is how many directories the
/// invocation directory sits below the root.
fn relative_to_invocation_dir(shared: &str, depth: usize) -> String {
    match shared.strip_prefix("./") {
        Some(rest) if depth > 0 => format!("{}{rest}", "../".repeat(depth)),
        _ => shared.to_string(),
    }
}

/// Components between the root and the process working directory, or `None`
/// unless the working directory is strictly below an absolute root.
fn invocation_depth_below(root: &Path) -> Option<usize> {
    if !root.is_absolute() {
        return None;
    }
    let cwd = std::fs::canonicalize(std::env::current_dir().ok()?).ok()?;
    let root = std::fs::canonicalize(root).ok()?;
    let below = cwd.strip_prefix(root).ok()?;
    let depth = below.components().count();
    (depth > 0).then_some(depth)
}

/// Render a path with stable slash separators for JSON and Markdown output.
pub(crate) fn display_path(path: &Path) -> String {
    display_path_for_host(&crate::analysis::stable_path_text(path), cfg!(windows))
}

/// Render a related-test or other source-file path without collapsing a literal
/// Unix backslash into a directory separator (#7838).
pub(crate) fn display_source_path(path: &Path) -> String {
    display_path_for_host(
        &crate::analysis::stable_source_path_text(path),
        cfg!(windows),
    )
}

/// [`display_path`] with the host explicit. A canonical Windows root is
/// verbatim (`\\?\<drive>:\…`); slash-unifying it alone printed
/// `//?/<drive>:/…`, a spelling no shell or editor takes and one other surfaces
/// never printed (`artifact.repository.root` already read `<drive>:/…`). The reducible verbatim
/// forms fold to their plain spelling, as `human_path_text` does (#9660).
fn display_path_for_host(text: &str, windows: bool) -> String {
    fold_verbatim_for_host(display_path_text(text), windows)
}

/// Fold an already slash-unified Windows verbatim path to its plain spelling
/// on Windows; elsewhere the text is returned unchanged. Shared by every
/// slash-unifying path renderer so a producer and a consumer that rebuilds
/// the same root never disagree on `//?/` (#9660).
pub(crate) fn fold_verbatim_on_host(unified: String) -> String {
    fold_verbatim_for_host(unified, cfg!(windows))
}

fn fold_verbatim_for_host(unified: String, windows: bool) -> String {
    if windows && let Some(plain) = strip_windows_verbatim_prefix(&unified) {
        return plain;
    }
    unified
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

/// Where the analyzed root sits under its Git checkout's top level, for the
/// surfaces whose consumers resolve a finding's file from that top level
/// rather than from the invoking directory: GitHub workflow annotations
/// (`file=` resolves against `GITHUB_WORKSPACE`) and SARIF uploads (code
/// scanning resolves `artifactLocation.uri` against the repository root).
///
/// Without an anchor those surfaces print the file relative to `--root`
/// (absolute root) or as the invoking spelling (relative root), so a run
/// whose root is a workspace member below the top level (`--root
/// "$PWD/crates/app"`, or `--root .` from inside `crates/app`) annotated
/// `src/lib.rs`, a file that does not exist at the top level.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CheckoutAnchor {
    /// The root's path below the top level (empty when the root is the top
    /// level). `None` when no Git top level was established, for example a
    /// `--diff` run outside any work tree: renderers then keep the
    /// root-relative spelling of [`repository_display_path`].
    root_prefix: Option<PathBuf>,
}

impl CheckoutAnchor {
    /// No established top level: renderers keep their root-relative spelling.
    pub(crate) fn unknown() -> Self {
        Self { root_prefix: None }
    }

    /// The root is `prefix` below the checkout top level (empty for the top
    /// level itself).
    pub(crate) fn below_top_level(prefix: PathBuf) -> Self {
        Self {
            root_prefix: Some(prefix),
        }
    }

    /// Ask Git where `root` sits under its work tree. Any failure (no work
    /// tree, no Git, a root outside the discovered top level, a root Git
    /// ignores or whose ignore status it cannot answer) yields
    /// [`CheckoutAnchor::unknown`], never an invented prefix.
    pub(crate) fn discover(root: &Path) -> Self {
        let Ok(canonical_root) = std::fs::canonicalize(root) else {
            return Self::unknown();
        };
        let Some(top_level) = crate::git::discovered_work_tree_toplevel(
            &canonical_root,
            std::time::Duration::from_secs(5),
        ) else {
            return Self::unknown();
        };
        let Ok(prefix) = canonical_root.strip_prefix(&top_level) else {
            return Self::unknown();
        };
        // A root Git ignores (a scratch fixture under `target/`, a vendored
        // tree) is not part of the checkout those consumers resolve against,
        // so prefixing it would name a file the checkout does not carry.
        // Only Git's negative answer admits the prefix.
        if !prefix.as_os_str().is_empty()
            && crate::git::work_tree_path_ignored(
                &top_level,
                prefix,
                std::time::Duration::from_secs(5),
            ) != Some(false)
        {
            return Self::unknown();
        }
        Self::below_top_level(prefix.to_path_buf())
    }

    /// `file` (a finding path) as a path relative to the checkout top level.
    ///
    /// Producers spell finding files two ways: Rust findings join the root
    /// (`crates/app/src/lib.rs` under `--root crates/app`, `./src/lib.rs`
    /// under `--root .`), while preview-language findings (Python,
    /// TypeScript, JavaScript) are already root-relative (`src/app.py`).
    /// A root-joined spelling strips the root; any other plain relative
    /// spelling is root-relative. When both readings are lexically possible
    /// (a root-relative file whose first directory repeats the root's
    /// spelling) the file system decides: only a file that exists under the
    /// root and not at the invoking-directory spelling is read
    /// root-relative.
    ///
    /// Falls back to [`repository_relative_path`] when no top level is
    /// known or neither reading is a plain path below the root (a `..`
    /// remainder never resolves upward).
    pub(crate) fn relative_path(&self, root: &Path, file: &Path) -> PathBuf {
        if self.root_prefix.is_none() {
            return repository_relative_path(root, file).to_path_buf();
        }
        let joined = file
            .strip_prefix(root)
            .ok()
            .filter(|below_root| is_plain_relative(below_root));
        let root_relative = (!file.has_root() && is_plain_relative(file)).then_some(file);
        let below_root = match (joined, root_relative) {
            (Some(joined), Some(root_relative)) => {
                if root.join(root_relative).exists() && !file.exists() {
                    root_relative
                } else {
                    joined
                }
            }
            (Some(joined), None) => joined,
            (None, Some(root_relative)) => root_relative,
            (None, None) => return repository_relative_path(root, file).to_path_buf(),
        };
        self.root_relative_path(below_root)
    }

    /// `relative` (a path already relative to the root, such as a repo
    /// seam's file) as a path relative to the checkout top level. Unchanged
    /// when no top level is known or `relative` is not a plain relative path.
    pub(crate) fn root_relative_path(&self, relative: &Path) -> PathBuf {
        match &self.root_prefix {
            Some(prefix) if is_plain_relative(relative) => {
                let mut joined = prefix.clone();
                for component in relative.components() {
                    if let Component::Normal(part) = component {
                        joined.push(part);
                    }
                }
                joined
            }
            _ => relative.to_path_buf(),
        }
    }

    /// [`CheckoutAnchor::relative_path`] as stable slash text without a
    /// leading `./`, the anchored counterpart of [`repository_display_path`].
    pub(crate) fn display_path(&self, root: &Path, file: &Path) -> String {
        let mut displayed = display_path(&self.relative_path(root, file));
        while let Some(stripped) = displayed.strip_prefix("./") {
            displayed = stripped.to_string();
        }
        displayed
    }
}

/// A nonempty relative path made only of plain names (and `.`), so joining
/// it under a prefix can never climb above that prefix.
fn is_plain_relative(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::Normal(_)))
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// Display-only spelling of a path for LSP messages and editor globs: lossy
/// for non-UTF-8 and `\` shown as `/` only on Windows. This is NOT an
/// identity key; use [`path_identity_key`] to compare or fingerprint paths.
pub(crate) fn path_display_lossy(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

/// Render a finding's file root-relative for machine consumers, tolerating
/// producer spelling drift: a canonicalized `\\?\X:` file still matches a
/// plain `--root`, and drive-letter case drift matches on Windows, so
/// committed-history reads and worktree reads render the same relative path
/// (#5254 item 6). A file that is not under the root — including a
/// `..` remainder that would resolve above it — or a relative root (which
/// keeps its spelling per [`repository_relative_path`]) falls back to the
/// full stable spelling rather than inventing a location.
pub(crate) fn repository_relative_path_text(root: &Path, file: &Path) -> String {
    repository_relative_path_text_with(root, file, cfg!(windows), host_paths_case_insensitive())
}

/// Test-only wrapper over [`repository_relative_path_text_with`] that ties
/// case-insensitivity to the `windows` flag, so both answers stay testable
/// anywhere (#4378).
#[cfg(test)]
pub(crate) fn repository_relative_path_text_on(root: &Path, file: &Path, windows: bool) -> String {
    repository_relative_path_text_with(root, file, windows, windows)
}

/// [`repository_relative_path_text_on`] with case-insensitivity decoupled from
/// the Windows separator/prefix rules, so a case-insensitive non-Windows host
/// (macOS) is testable anywhere.
pub(crate) fn repository_relative_path_text_with(
    root: &Path,
    file: &Path,
    windows: bool,
    case_insensitive: bool,
) -> String {
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
        // `strip_prefix` is lexical: `/repo/../outside.rs` strips to
        // `../outside.rs`, which resolves outside the root. Any parent
        // component escapes, so only a clean remainder strips.
        && !relative
            .components()
            .any(|component| matches!(component, Component::ParentDir))
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
    if !windows
        && case_insensitive
        && let Some(relative) = strip_case_insensitive_root(root, file)
    {
        return display_path(&relative);
    }
    display_path(file)
}

/// Component-wise, ASCII-case-insensitive root strip (ASCII-only by design:
/// it is a display-prefix strip that falls back to the full spelling, so a
/// non-ASCII case or NFC/NFD difference just keeps the absolute spelling) for an absolute
/// non-Windows root. The remainder keeps the file's own spelling and must be
/// non-empty and free of parent components. Rootedness is the POSIX rule, not
/// the native `Path::is_absolute`, which calls `/repo` relative on Windows.
fn strip_case_insensitive_root(root: &Path, file: &Path) -> Option<PathBuf> {
    if !is_absolute_on(&root.to_string_lossy(), false)
        || !is_absolute_on(&file.to_string_lossy(), false)
    {
        return None;
    }
    let mut file_components = file.components();
    for expected in root.components() {
        let actual = file_components.next()?;
        let same = match (expected, actual) {
            (Component::Normal(left), Component::Normal(right)) => left
                .to_string_lossy()
                .eq_ignore_ascii_case(&right.to_string_lossy()),
            (left, right) => left == right,
        };
        if !same {
            return None;
        }
    }
    let relative: PathBuf = file_components.collect();
    (!relative.as_os_str().is_empty()
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_))))
    .then_some(relative)
}

/// Strip a Windows absolute root from a file across the spelling drift a
/// plain `strip_prefix` rejects: verbatim `\\?\X:` / `\\?\UNC\` prefixes in
/// either operand, mixed separators, and ASCII-case drift. String-level on
/// purpose: `Path::components` parses Windows prefixes only on Windows, so a
/// component implementation would be untestable in the Linux matrix, and the
/// non-Unix `stable_path_text` branch this feeds is itself lossy. Comparison
/// is segment-wise, so a root matches only whole leading segments and never
/// a longer sibling (`repo` vs `repo2`). `None` when the file is not under
/// the root, the root is not absolute, the remainder is empty, or the
/// remainder holds a `..` that would resolve above the root.
///
/// Case drift is ASCII-only by design (it only decides whether a display
/// prefix is stripped; a miss keeps the full spelling).
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
    let remainder = &file_segments[root_segments.len()..];
    // A parent component in the remainder resolves above the root, so the
    // file is not under it: fall back to the full spelling.
    if remainder.iter().any(|segment| segment == "..") {
        return None;
    }
    Some(remainder.join("/"))
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
pub(crate) fn strip_windows_verbatim_prefix(unified: &str) -> Option<String> {
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

/// Whether the unified text starts with an absolute drive-letter root
/// (a drive letter, a colon, and a slash). A bare `X:` prefix is
/// drive-relative on Windows, not rooted.
fn is_windows_drive_path(unified: &str) -> bool {
    unified.len() >= 3
        && unified.as_bytes()[0].is_ascii_alphabetic()
        && unified.as_bytes()[1] == b':'
        && unified.as_bytes()[2] == b'/'
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

/// Whether writing `output` would replace the file a command reads from
/// `input` (#14312). Filesystem identity decides: both paths must resolve and
/// their canonical paths must match, so an absolute spelling or a symlink to
/// the input is caught, and a `..` after a symlinked directory is not
/// collapsed lexically into a false match. A missing input has nothing to
/// replace (reports that tolerate one still write and warn), and a
/// destination that does not exist yet cannot be an existing input.
pub(crate) fn output_replaces_input(output: &Path, input: &Path) -> bool {
    match (std::fs::canonicalize(output), std::fs::canonicalize(input)) {
        (Ok(output), Ok(input)) => output == input,
        _ => false,
    }
}

fn normalize_output_leaf(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    // Display text uses slash separators even for Unix backslash filenames.
    // Destination identity must instead follow the host's native components.
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    let _ = out.pop();
                } else if !out.has_root() {
                    // A relative parent with no normal component to cancel
                    // still selects a different directory from the cwd.
                    out.push(component);
                }
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
/// On Windows `\` is a separator: every `\` renders as `/` and a verbatim
/// prefix renders as its plain spelling, so a forward-slash argv prefix and a
/// joined suffix never mix separators in one path. Both reducible verbatim
/// forms fold here at the shared owner (#8754): `\\?\` before `<drive>:` and
/// `\\?\UNC\server\share`, whose plain spellings (`C:...` and
/// `\\server\share`) every shell accepts. Other verbatim forms
/// (`\\?\Volume{..}`, unknown device prefixes) have no plain spelling, so
/// they render exactly as given — one disclosed exception owned here, not a
/// per-call-site policy. On Unix `\` is an ordinary filename character, so
/// the text is returned unchanged; rewriting it would name a different file.
pub(crate) fn human_path_text(text: &str, windows: bool) -> String {
    if !windows {
        return text.to_string();
    }
    let unified = text.replace('\\', "/");
    match strip_windows_verbatim_prefix(&unified) {
        Some(plain) => plain,
        // A verbatim form with no plain spelling renders exactly as given.
        None if unified.starts_with("//?/") => text.to_string(),
        None => unified,
    }
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
    use std::path::{Path, PathBuf};

    use super::{
        CheckoutAnchor, display_path, display_path_for_host, display_path_text, human_path_text,
        repository_display_path, repository_relative_path_text_on, same_output_leaf,
    };

    #[test]
    fn display_path_folds_a_canonical_windows_root_and_keeps_unix_text() {
        let drive = "F:";
        let canonical = format!(r"\\?\{drive}\work\repo");
        assert_eq!(
            display_path_for_host(&canonical, true),
            format!("{drive}/work/repo")
        );
        assert_eq!(
            display_path_for_host(r"\\?\UNC\server\share\repo", true),
            "//server/share/repo"
        );
        // No plain spelling: unified but not folded.
        assert_eq!(
            display_path_for_host(r"\\?\Volume{0}\repo", true),
            "//?/Volume{0}/repo"
        );
        // Unix: `\` is a filename byte; text is slash-unified as before and
        // never folded.
        assert_eq!(
            display_path_for_host(&canonical, false),
            format!("//?/{drive}/work/repo")
        );
        assert_eq!(
            display_path_for_host("tests/test_x.py", false),
            "tests/test_x.py"
        );
        assert_eq!(
            display_path_for_host(r"tests\test_x.py", true),
            "tests/test_x.py"
        );
    }

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
        // #8754: a UNC verbatim prefix has the plain spelling `\\server\share`,
        // so it folds like the drive-letter form; a volume GUID has no plain
        // spelling and stays exactly as given.
        assert_eq!(
            human_path_text(r"\\?\UNC\server\share\ripr.exe", true),
            r"//server/share/ripr.exe"
        );
        assert_eq!(
            human_path_text(r"\\?\Volume{1}\x", true),
            r"\\?\Volume{1}\x"
        );
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

    // #7882: human path text is not the stable-path percent codec. These
    // host-explicit controls also run on Linux, without claiming native
    // Windows filesystem or command execution.
    #[test]
    fn repair_path_display_preserves_literal_percent_on_both_hosts() {
        let drive = "F:";
        let verbatim = format!(r"\\?\{drive}\repo 100%\target/ripr\attempt.json");
        assert_eq!(
            human_path_text(&verbatim, true),
            format!("{drive}/repo 100%/target/ripr/attempt.json")
        );
        assert_eq!(human_path_text(&verbatim, false), verbatim);
        let unix = r"/tmp/repo 100%\literal/attempt.json";
        assert_eq!(human_path_text(unix, false), unix);
        // #8754: the UNC verbatim form folds to its plain spelling with the
        // literal `%` intact (this renderer is not the percent codec); a
        // volume GUID has no plain spelling and stays exactly as given.
        assert_eq!(
            human_path_text(r"\\?\UNC\server\share 100%", true),
            "//server/share 100%"
        );
        assert_eq!(
            human_path_text(r"\\?\Volume{1}\100%", true),
            r"\\?\Volume{1}\100%"
        );
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
    fn anchored_display_path_names_the_file_from_the_checkout_top_level() {
        // A member root two levels below the top level, in every spelling a
        // CI step or a developer uses: absolute, `.` from inside the member,
        // a relative path with a `..` detour, and `..` from a sibling.
        let member = CheckoutAnchor::below_top_level(PathBuf::from("crates/app"));
        let absolute = std::env::temp_dir().join("ripr-anchor").join("crates/app");
        let spellings = [
            (absolute.clone(), absolute.join("src/lib.rs")),
            (PathBuf::from("."), PathBuf::from("./src/lib.rs")),
            (
                PathBuf::from("crates/../crates/app"),
                PathBuf::from("crates/../crates/app/src/lib.rs"),
            ),
            (PathBuf::from("../app"), PathBuf::from("../app/src/lib.rs")),
        ];
        for (root, file) in &spellings {
            assert_eq!(
                member.display_path(root, file),
                "crates/app/src/lib.rs",
                "root {} file {}",
                root.display(),
                file.display()
            );
        }
        // Without an anchor the same spellings keep the root-relative
        // rendering (the pre-anchor behavior), so the anchor is what moves
        // them: an absolute member root used to annotate `src/lib.rs`.
        assert_eq!(
            CheckoutAnchor::unknown().display_path(&absolute, &absolute.join("src/lib.rs")),
            "src/lib.rs"
        );
        assert_eq!(
            CheckoutAnchor::unknown().display_path(
                Path::new("crates/../crates/app"),
                Path::new("crates/../crates/app/src/lib.rs")
            ),
            "crates/../crates/app/src/lib.rs"
        );
        // The top level itself anchors with an empty prefix: unchanged text.
        let top = CheckoutAnchor::below_top_level(PathBuf::new());
        assert_eq!(
            top.display_path(Path::new("."), Path::new("./src/lib.rs")),
            "src/lib.rs"
        );
        assert_eq!(
            top.display_path(Path::new("crates/app"), Path::new("crates/app/src/lib.rs")),
            "src/lib.rs"
        );
    }

    #[test]
    fn anchored_paths_never_climb_or_invent_a_location() {
        let member = CheckoutAnchor::below_top_level(PathBuf::from("crates/app"));
        // A plain relative file that does not repeat the root's spelling is
        // root-relative (the preview-language convention): it joins under
        // the prefix whatever the root spelling.
        for root in ["crates/app", ".", "/checkout/crates/app"] {
            assert_eq!(
                member.display_path(Path::new(root), Path::new("src/app.py")),
                "crates/app/src/app.py",
                "root {root}"
            );
        }
        // A `..` remainder keeps the unanchored spelling instead of being
        // joined under the prefix.
        assert_eq!(
            member.display_path(Path::new("."), Path::new("./../escape.rs")),
            repository_display_path(Path::new("."), Path::new("./../escape.rs"))
        );
        // The root itself is not a file under it.
        assert_eq!(
            member.display_path(Path::new("."), Path::new(".")),
            repository_display_path(Path::new("."), Path::new("."))
        );
        // Root-relative seam files join under the prefix; climbing or
        // rooted paths stay as given.
        assert_eq!(
            member.root_relative_path(Path::new("src/lib.rs")),
            PathBuf::from("crates/app/src/lib.rs")
        );
        assert_eq!(
            member.root_relative_path(Path::new("../x.rs")),
            PathBuf::from("../x.rs")
        );
        assert_eq!(
            CheckoutAnchor::unknown().root_relative_path(Path::new("src/lib.rs")),
            PathBuf::from("src/lib.rs")
        );
    }

    #[test]
    fn checkout_anchor_discovers_a_member_root_below_the_git_top_level() -> Result<(), String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|err| format!("clock: {err}"))?
            .as_nanos();
        let top = std::env::temp_dir().join(format!("ripr-anchor-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&top).map_err(|err| format!("mkdir: {err}"))?;
        // A root that does not exist cannot be placed: no invented prefix.
        assert_eq!(
            CheckoutAnchor::discover(&top.join("missing")),
            CheckoutAnchor::unknown()
        );
        crate::git::run_git(&top, &["init", "-q", "repo"])
            .map_err(|err| format!("git init: {err}"))?;
        let repo = top.join("repo");
        let repo_member = repo.join("crates").join("app");
        std::fs::create_dir_all(&repo_member).map_err(|err| format!("mkdir: {err}"))?;
        // A root Git ignores (a scratch fixture under `target/`) is not part
        // of the checkout: it keeps the root-relative spelling.
        std::fs::write(repo.join(".gitignore"), "target/\n")
            .map_err(|err| format!("write .gitignore: {err}"))?;
        let ignored_member = repo.join("target").join("fx");
        std::fs::create_dir_all(&ignored_member).map_err(|err| format!("mkdir: {err}"))?;
        let at_member = CheckoutAnchor::discover(&repo_member);
        let at_top = CheckoutAnchor::discover(&repo);
        let at_ignored = CheckoutAnchor::discover(&ignored_member);
        let _ = std::fs::remove_dir_all(&top);
        assert_eq!(at_ignored, CheckoutAnchor::unknown());
        assert_eq!(
            at_member,
            CheckoutAnchor::below_top_level(PathBuf::from("crates").join("app"))
        );
        assert_eq!(at_top, CheckoutAnchor::below_top_level(PathBuf::new()));
        assert_eq!(
            at_member.display_path(&repo_member, &repo_member.join("src/lib.rs")),
            "crates/app/src/lib.rs"
        );
        Ok(())
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
    fn same_output_leaf_keeps_unresolved_relative_parents() {
        assert!(!same_output_leaf(
            Path::new("../report.json"),
            Path::new("report.json")
        ));
        assert!(!same_output_leaf(
            Path::new("../../report.json"),
            Path::new("../report.json")
        ));
        assert!(same_output_leaf(
            Path::new("reports/../../report.json"),
            Path::new("../report.json")
        ));
        assert!(same_output_leaf(
            Path::new("../reports/../report.json"),
            Path::new("../report.json")
        ));
    }

    #[test]
    fn same_output_leaf_uses_native_separator_identity() {
        assert_eq!(
            same_output_leaf(
                Path::new(r"reports\history.json"),
                Path::new("reports/history.json")
            ),
            cfg!(windows)
        );
    }

    #[cfg(windows)]
    #[test]
    fn same_output_leaf_keeps_drive_relative_parents_and_root_prefixes() {
        let drive = "F:";
        assert!(!same_output_leaf(
            Path::new(&format!("{drive}../report.json")),
            Path::new(&format!("{drive}report.json"))
        ));
        assert!(same_output_leaf(
            Path::new(&format!("{drive}/../report.json")),
            Path::new(&format!("{drive}/report.json"))
        ));
        assert!(!same_output_leaf(
            Path::new(&format!("{drive}report.json")),
            Path::new(&format!("{drive}/report.json"))
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

    #[test]
    fn relative_path_text_rejects_a_parent_escape() {
        // `strip_prefix` is lexical: the remainder can name a path above
        // the root. Such a file is not under the root, so it keeps the
        // full spelling instead of serving a `..` escape on the wire.
        assert_eq!(
            repository_relative_path_text_on(
                Path::new("/repo"),
                Path::new("/repo/../outside.rs"),
                false
            ),
            "/repo/../outside.rs"
        );
        // A non-leading parent still escapes.
        assert_eq!(
            repository_relative_path_text_on(
                Path::new("/repo"),
                Path::new("/repo/a/../../outside.rs"),
                false
            ),
            "/repo/a/../../outside.rs"
        );
        // Same rule through the Windows slow path (assembled parts: no
        // drive-letter literals in the tree).
        let drive = "F:";
        let root_text = format!(r"{drive}\repo");
        let root = Path::new(&root_text);
        let escape_text = format!(r"{drive}\repo\..\outside.rs");
        assert_eq!(
            repository_relative_path_text_on(root, Path::new(&escape_text), true),
            format!(r"{drive}/repo/../outside.rs")
        );
    }

    #[test]
    fn relative_path_text_keeps_drive_relative_roots() {
        // `X:foo` is drive-relative on Windows, not rooted: the root is
        // not absolute, so the file keeps its spelling.
        let drive = "F:";
        let root_text = format!(r"{drive}repo");
        let file_text = format!(r"{drive}repo/file.rs");
        assert_eq!(
            repository_relative_path_text_on(Path::new(&root_text), Path::new(&file_text), true),
            format!(r"{drive}repo/file.rs")
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

    #[test]
    fn case_insensitive_posix_host_strips_root_across_case_drift() {
        use super::repository_relative_path_text_with as with;
        let root = Path::new("/Srv/dev/Repo");
        let file = Path::new("/srv/dev/repo/Src/lib.rs");
        // macOS-style host: case drift strips; remainder keeps file spelling.
        assert_eq!(with(root, file, false, true), "Src/lib.rs");
        // Linux-style host: unchanged, falls back to the full spelling.
        assert_eq!(with(root, file, false, false), "/srv/dev/repo/Src/lib.rs");
        // Exact spelling strips on both.
        let exact = Path::new("/Srv/dev/Repo/src/lib.rs");
        assert_eq!(with(root, exact, false, true), "src/lib.rs");
        assert_eq!(with(root, exact, false, false), "src/lib.rs");
        // Escapes and non-prefixes stay refused even when folding.
        let escape = Path::new("/srv/dev/repo/../outside.rs");
        assert_eq!(
            with(root, escape, false, true),
            "/srv/dev/repo/../outside.rs"
        );
        let sibling = Path::new("/srv/dev/repository/x.rs");
        assert_eq!(with(root, sibling, false, true), "/srv/dev/repository/x.rs");
        assert_eq!(
            with(root, Path::new("/srv/dev/repo"), false, true),
            "/srv/dev/repo"
        );
    }

    #[test]
    fn invocation_directory_below_root_rewrites_only_the_dot_slash_form() {
        use super::relative_to_invocation_dir as rel;
        assert_eq!(
            rel("./crates/core/src/lib.rs", 1),
            "../crates/core/src/lib.rs"
        );
        assert_eq!(rel("./a.rs", 2), "../../a.rs");
        // At the root, and for files outside the root, the text is unchanged.
        assert_eq!(rel("./a.rs", 0), "./a.rs");
        assert_eq!(rel("/elsewhere/a.rs", 1), "/elsewhere/a.rs");
    }
}
