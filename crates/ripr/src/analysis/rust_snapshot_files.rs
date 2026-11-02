//! Committed `.snap` files as a filesystem input to the Rust grading path
//! (#12189).
//!
//! A file-backed `insta` assertion (`assert_json_snapshot!(value)`) compares
//! the value with a committed `.snap` file. With no such file the assertion
//! fails on its first run (insta writes a `.snap.new` and fails), so it
//! cannot be the discriminator that would notice a wrong value. Snapshot
//! syntax alone does not establish expected bytes.
//!
//! ripr does not derive insta's exact file path (crate name, module path,
//! `with_settings!` overrides). It decides only the case it can be certain
//! of: the assertion is the plain single-assertion shape, the test file does
//! not redirect snapshots, and no committed `.snap` anywhere under the
//! workspace carries the test's snapshot name. Every other shape keeps its
//! existing grade, which is "not established", never "missing".
//!
//! `INSTA_UPDATE`/`INSTA_FORCE_PASS` set outside the Rust file (a
//! `.cargo/config.toml` `[env]` table, `insta.yaml`) are not seen. Without
//! `INSTA_FORCE_PASS` a missing `.snap` still fails the assertion, so the
//! demotion stays right in the common case. A workspace with more than
//! [`LISTING_LIMIT`] `.snap` files reads as unknown and keeps today's grade.
//!
//! The listing is shared with the analysis cache key, so adding, deleting or
//! (un)tracking a `.snap` file moves the key. It follows the TypeScript
//! committed-snapshot precedent in [`super::committed_source`]: a `.snap`
//! file the repository does not track is a local file CI never sees (#7708),
//! and a committed-history overlay that records a differing snapshot input
//! makes the answer unknown.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use super::facts::{OracleFact, RustIndex, TestSummary};

/// Directories the walk never enters. They hold no committed snapshot.
const SKIPPED_DIRS: [&str; 4] = [".git", ".ripr", "target", "node_modules"];

/// More `.snap` files than this, or any directory the walk cannot read, and
/// the listing is unknown rather than partial.
const LISTING_LIMIT: usize = 20_000;

/// Text in the test file that can redirect or rename snapshot files. Any of
/// it makes the expected name unresolvable, so the grade stays as it is.
const NAMING_OVERRIDES: [&str; 3] = ["snapshot_path", "glob!", "INSTA_"];

/// The insta macros whose first argument, when there are two, is the
/// snapshot name. `assert_binary_snapshot!(".ext", bytes)` takes a file
/// extension first and is deliberately absent: any macro not listed here is
/// unresolved, so a first argument that is not a name is never guessed.
const NAMED_FORM_MACROS: [&str; 10] = [
    "assert_snapshot",
    "assert_debug_snapshot",
    "assert_compact_debug_snapshot",
    "assert_display_snapshot",
    "assert_json_snapshot",
    "assert_compact_json_snapshot",
    "assert_yaml_snapshot",
    "assert_ron_snapshot",
    "assert_toml_snapshot",
    "assert_csv_snapshot",
];

/// Test attribute text that fans one function out into several snapshot
/// names (`rstest` cases and similar). The name is not the function's.
const CASE_ATTRIBUTES: [&str; 5] = ["case", "rstest", "template", "parameterized", "dir"];

#[derive(Debug, PartialEq, Eq)]
struct ListedSnapshot {
    /// Root-relative, `/`-separated.
    path: String,
    /// `Some(false)` when the repository lists its tracked snapshots and this
    /// one is not among them; `None` when the repository cannot say.
    tracked: Option<bool>,
}

/// The `.snap` files under one workspace root.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SnapshotListing {
    files: Vec<ListedSnapshot>,
    /// Incomplete walk (size limit or an unreadable directory), or a committed-history overlay that disagrees with the
    /// working tree about a snapshot input: the listing cannot say a file is
    /// absent.
    unknown: bool,
}

impl SnapshotListing {
    pub(crate) fn read(root: &Path) -> Self {
        let mut paths = Vec::new();
        let truncated = collect(root, root, &mut paths);
        paths.sort();
        let tracked = if paths.is_empty() {
            None
        } else {
            super::committed_source::tracked_snapshot_files(root)
        };
        let files = paths
            .into_iter()
            .map(|path| ListedSnapshot {
                tracked: tracked.as_ref().map(|tracked| tracked.contains(&path)),
                path,
            })
            .collect();
        Self {
            files,
            unknown: truncated || super::committed_source::any_snapshot_input_differs(root),
        }
    }

    /// Stable text for the analysis cache key: which snapshot files exist and
    /// whether the repository tracks each. File content is not an input,
    /// because only existence moves a grade.
    pub(crate) fn identity(&self) -> String {
        let mut text = format!("unknown={}\n", self.unknown);
        for file in &self.files {
            let tracked = match file.tracked {
                Some(true) => "tracked",
                Some(false) => "untracked",
                None => "unlisted",
            };
            text.push_str(&file.path);
            text.push('\0');
            text.push_str(tracked);
            text.push('\n');
        }
        text
    }

    /// `true` only when the listing is complete and no snapshot file CI would
    /// see carries any of `names`. An unknown listing never says "lacks".
    fn certainly_lacks(&self, names: &[String]) -> bool {
        !self.unknown && !self.holds_any(names)
    }

    /// `true` when a snapshot file CI would see carries one of `names`.
    fn holds_any(&self, names: &[String]) -> bool {
        self.files
            .iter()
            .filter(|file| file.tracked != Some(false))
            .filter_map(|file| file.path.rsplit('/').next())
            .filter_map(|name| name.strip_suffix(".snap"))
            .any(|stem| names.iter().any(|name| stem_names_snapshot(stem, name)))
    }
}

/// Walk `directory` for `*.snap` files; `true` when the listing is
/// incomplete: the limit stopped it, or a directory or entry could not be
/// enumerated (a searchable but unlistable directory still lets insta open its
/// named file, so an unreadable place must never read as "no snapshot").
fn collect(root: &Path, directory: &Path, out: &mut Vec<String>) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return true;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return true;
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            return true;
        };
        if kind.is_dir() {
            if SKIPPED_DIRS.contains(&name) {
                continue;
            }
            if collect(root, &path, out) {
                return true;
            }
        } else if name.ends_with(".snap") {
            // A symlink counts as present: it is a snapshot entry the
            // reading cannot judge, so it must not read as absent.
            if out.len() >= LISTING_LIMIT {
                return true;
            }
            let relative = path.strip_prefix(root).unwrap_or(&path);
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    false
}

/// Whether the snapshot file stem `stem` can belong to the snapshot named
/// `name`. insta writes `<module path>__<name>`, adds `-<n>` for the n-th
/// unnamed assertion of one test and `@<suffix>` for a settings suffix, and
/// leaves the module prefix off when asked to. The match is deliberately
/// lenient on the prefix: a stem that merely might belong keeps the credit.
fn stem_names_snapshot(stem: &str, name: &str) -> bool {
    let core = stem.split('@').next().unwrap_or(stem);
    let counted = core
        .rsplit_once('-')
        .filter(|(_, digits)| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .map(|(rest, _)| rest);
    [Some(core), counted]
        .into_iter()
        .flatten()
        .any(|core| core == name || core.ends_with(&format!("__{name}")))
}

/// One analysis run's listing of the workspace's `.snap` files, read on first
/// use. The index is rebuilt per run and a cloned index starts from the same
/// answer, so the memo never outlives the sources it was read beside.
#[derive(Clone, Debug, Default)]
pub(crate) struct SnapshotListingCell(OnceLock<Arc<SnapshotListing>>);

impl SnapshotListingCell {
    fn get(&self, root: &Path) -> &SnapshotListing {
        self.0
            .get_or_init(|| Arc::new(SnapshotListing::read(root)))
            .as_ref()
    }
}

/// `true` when `oracle` is a plain file-backed insta assertion in `test`
/// whose committed `.snap` file is certainly missing. `false` for every
/// other case, including every one this reading cannot resolve.
pub(crate) fn file_backed_snapshot_missing(
    index: &RustIndex,
    test: &TestSummary,
    oracle: &OracleFact,
) -> bool {
    let Some(authority) = index.workspace_authority.as_ref() else {
        return false;
    };
    let Some(source) = index
        .files()
        .get(&test.file)
        .map(|file| file.source.clone())
    else {
        return false;
    };
    let Some(names) = expected_snapshot_names(
        &test.name,
        &test.attrs,
        test.body.as_str(),
        &oracle.text,
        &source,
    ) else {
        return false;
    };
    let listing = index.snapshot_listing.get(&authority.root);
    listing.certainly_lacks(&names)
}

/// The snapshot names the plain file-backed assertion can write, or `None`
/// when the shape is inline, redirected, repeated, parameterized or otherwise
/// not the single-assertion case this reading resolves.
fn expected_snapshot_names(
    test_name: &str,
    attrs: &[String],
    body: &str,
    oracle_text: &str,
    file_source: &str,
) -> Option<Vec<String>> {
    if NAMING_OVERRIDES
        .iter()
        .any(|text| file_source.contains(text))
        || attrs
            .iter()
            .any(|attr| CASE_ATTRIBUTES.iter().any(|text| attr.contains(text)))
    {
        return None;
    }
    let call = snapshot_call(oracle_text, file_source)?;
    if !NAMED_FORM_MACROS.contains(&call.macro_name.as_str()) {
        return None;
    }
    // The assertion must sit in the test's own body: a helper's assertion is
    // named for the helper, and a second assertion advances insta's counter.
    if body.matches("snapshot!").count() != 1 || !body.contains(call.macro_name.as_str()) {
        return None;
    }
    match call.arguments.as_slice() {
        [_] => {
            let mut names = vec![test_name.to_string()];
            if let Some(stripped) = test_name.strip_prefix("test_") {
                names.push(stripped.to_string());
            }
            Some(names)
        }
        [name, _] => plain_string_literal(name).map(|name| vec![name.to_string()]),
        _ => None,
    }
}

struct SnapshotCall {
    macro_name: String,
    arguments: Vec<String>,
}

/// The one file-backed insta assertion in `text`. `None` for an inline
/// snapshot (`@"..."`), an assertion with the wrong argument shape, a macro
/// path other than `insta::`, or anything not `assert_*snapshot!`.
fn snapshot_call(text: &str, file_source: &str) -> Option<SnapshotCall> {
    let bang = text.find("snapshot!")? + "snapshot!".len() - 1;
    let before = &text[..bang];
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == ':'))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let path = &before[start..];
    let macro_name = path.rsplit("::").next()?;
    if !macro_name.starts_with("assert_") || path.matches("::").count() > 1 {
        return None;
    }
    match path.strip_suffix(macro_name) {
        Some("insta::") => {}
        Some("") if imports_from_insta(file_source, macro_name) => {}
        _ => return None,
    }
    let rest = text[bang + 1..].trim_start();
    let open = rest
        .chars()
        .next()
        .filter(|c| matches!(c, '(' | '[' | '{'))?;
    let close = match open {
        '(' => ')',
        '[' => ']',
        _ => '}',
    };
    let inner = rest.get(1..)?;
    let end = matching_close(inner, open, close)?;
    let arguments = split_top_level(inner.get(..end)?)?;
    if arguments.iter().any(|argument| argument.starts_with('@')) {
        return None;
    }
    Some(SnapshotCall {
        macro_name: macro_name.to_string(),
        arguments,
    })
}

/// `true` when `source` imports `macro_name` from `insta` (by name or glob)
/// and defines no macro of that name, so a bare `assert_snapshot!` is insta's
/// and not a local assertion that checks an explicit value.
fn imports_from_insta(source: &str, macro_name: &str) -> bool {
    if source.contains(&format!("macro_rules! {macro_name}")) {
        return false;
    }
    source.split(';').any(|statement| {
        let statement = statement.trim_start();
        let statement = statement.strip_prefix("pub ").unwrap_or(statement);
        statement
            .strip_prefix("use insta")
            .filter(|rest| rest.starts_with("::") || rest.starts_with(' '))
            .is_some_and(|rest| rest.contains(macro_name) || rest.contains('*'))
    })
}

/// Byte index of the delimiter closing a group already opened, or `None`.
fn matching_close(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 1usize;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        match c {
            '"' => skip_string(&mut chars)?,
            '\'' => skip_char_literal(text, index, &mut chars),
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn skip_string(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> Option<()> {
    while let Some((_, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '"' => return Some(()),
            _ => {}
        }
    }
    None
}

/// A `'x'` or `'\n'` literal; a lifetime (`'a`) has no closing quote and is
/// left alone.
fn skip_char_literal(
    text: &str,
    index: usize,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) {
    let rest = &text[index + 1..];
    let length = if rest.starts_with('\\') {
        rest.find('\'').map(|end| end + 1)
    } else {
        rest.chars()
            .next()
            .filter(|_| rest.chars().nth(1) == Some('\''))
            .map(|c| c.len_utf8() + 1)
    };
    if let Some(length) = length {
        let target = index + 1 + length;
        while chars.peek().is_some_and(|(next, _)| *next < target) {
            chars.next();
        }
    }
}

/// The comma-separated arguments of a macro call, split only at nesting
/// depth zero outside strings; a trailing comma adds no argument.
fn split_top_level(inner: &str) -> Option<Vec<String>> {
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut chars = inner.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        match c {
            '"' => skip_string(&mut chars)?,
            '\'' => skip_char_literal(inner, index, &mut chars),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                arguments.push(inner[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        arguments.push(last.to_string());
    }
    (!arguments.is_empty() && arguments.iter().all(|argument| !argument.is_empty()))
        .then_some(arguments)
}

/// `"name"` with no escapes or interpolation.
fn plain_string_literal(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.is_empty() && !inner.contains(['"', '\\', '{', '/'])).then_some(inner)
}

#[cfg(test)]
mod tests;
