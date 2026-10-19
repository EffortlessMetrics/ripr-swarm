//! RIPR-SPEC-0258: a committed snapshot entry pins the value.
//!
//! `expect(x).toMatchSnapshot()` compares the serialized value with the entry
//! stored under `<test title> <counter>` in `__snapshots__/<test file>.snap`;
//! Jest's `toMatchSnapshot("hint")` stores it under `<test title>: <hint>
//! <counter>`, counting per hinted name.
//! When that entry is committed, the runner fails on any changed
//! serialization (under `--ci` a missing entry fails too), so the assertion
//! pins the value the way a literal inline snapshot does (RIPR-SPEC-0247
//! rule 3). The reading is promoted only when the entry this assertion uses
//! can be named without running the suite; every other shape keeps
//! `snapshot` / medium.

use super::*;

/// Text after skipping whitespace, `/* */` comments and `//` line comments.
fn skip_trivia(mut text: &str) -> &str {
    loop {
        text = text.trim_start();
        if let Some(inner) = text.strip_prefix("//") {
            // A line comment ends at the line break (#8895).
            text = inner.split_once('\n').map_or("", |(_, after)| after);
        } else if let Some((_, after)) = text
            .strip_prefix("/*")
            .and_then(|inner| inner.split_once("*/"))
        {
            text = after;
        } else {
            return text;
        }
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// Snapshot matcher calls in `text`: `Snapshot` followed, after optional
/// whitespace, block comments and line comments, by `(`, `?.`, `!`, a
/// type-argument list (`<`), a bracket-access close (`"]`, `']`, `` `] ``,
/// with optional whitespace or comments before the `]`) or by `.call` /
/// `.apply` / `.bind` (#8031). Each such call advances the runner's per-title
/// counter, so it is counted whether or not it writes to the committed file.
/// A matcher name that is used as a value instead of called in place (the
/// parenthesised `(a.toMatchSnapshot)()`, a cast `as` / `satisfies`, an
/// alias `const f = a.toMatchSnapshot`, an argument or destructuring) may be
/// called any number of times later, so it counts as two: it is never
/// credited as the one call and refuses (#8895). Only a word that merely
/// begins with `Snapshot`, or a standalone `Snapshot` that is not a matcher
/// name, is not counted. Comments and strings only over-count, which refuses.
pub(super) fn snapshot_calls(text: &str) -> usize {
    text.match_indices("Snapshot")
        .map(|(index, matched)| {
            let Some(raw) = text.get(index + matched.len()..) else {
                return 0;
            };
            if raw.starts_with(is_identifier_char) {
                return 0;
            }
            let rest = skip_trivia(raw);
            let called = rest.starts_with(['(', '<', '!'])
                || [".call", ".apply", ".bind", "?."]
                    .iter()
                    .any(|tail| rest.starts_with(tail))
                || rest
                    .strip_prefix(['"', '\'', '`'])
                    .is_some_and(|after| skip_trivia(after).starts_with(']'));
            let matcher_name = text
                .get(..index)
                .is_some_and(|before| before.ends_with(is_identifier_char));
            if called {
                1
            } else if matcher_name {
                2
            } else {
                0
            }
        })
        .sum()
}

/// The hint of the body's one `toMatchSnapshot("<hint>")` call, when it is a
/// single quoted string literal whose source text is its value and that the
/// snapshot file writes unescaped: no `` ` ``, `$`, `\`, line break or
/// matching quote inside, and not empty (Jest drops an empty hint). Any
/// other argument (a template, an identifier, a property matcher, a comment,
/// a second argument) is `None`, and so is a body that names
/// `toMatchSnapshot` more than once, even in a string or comment, since the
/// hint read must come from the call the assertion credits.
fn literal_hint(body: &str) -> Option<&str> {
    let mut mentions = body.match_indices("toMatchSnapshot");
    let (index, matched) = mentions.next()?;
    if mentions.next().is_some() {
        return None;
    }
    let rest = body
        .get(index + matched.len()..)?
        .trim_start()
        .strip_prefix('(')?
        .trim_start();
    let quote = rest
        .chars()
        .next()
        .filter(|quote| matches!(quote, '"' | '\''))?;
    let rest = rest.get(1..)?;
    let end = rest.find(quote)?;
    let hint = rest.get(..end)?;
    let closed = rest.get(end + 1..)?.trim_start().starts_with(')');
    (closed && !hint.is_empty() && !hint.contains(['`', '\\', '$', '\n', '\r'])).then_some(hint)
}

/// Promote each test's `toMatchSnapshot()` assertion, or under Jest its
/// `toMatchSnapshot("<hint>")` assertion, to `snapshot` / strong when the
/// committed entry it compares with exists (rule 1).
pub(crate) fn promote_committed_snapshots(
    root: &Path,
    test_file: &Path,
    file_source: &str,
    tests: &mut [TypeScriptTest],
) {
    // A snapshot call outside every test body (a hook or a helper) advances
    // a test's counter where this reader cannot see it. Concurrent tests may
    // record a snapshot under another test's title.
    let in_bodies: usize = tests
        .iter()
        .map(|test| snapshot_calls(&test.body_text))
        .sum();
    if in_bodies == 0
        || snapshot_calls(file_source) != in_bodies
        || file_source.contains(".concurrent")
    {
        return;
    }
    let Some(snapshots) = committed_snapshot_file(root, test_file) else {
        return;
    };
    let titles: Vec<String> = tests
        .iter()
        .map(|test| joined_title(test, &snapshots.separator))
        .collect();
    let keys: Vec<Option<(String, usize)>> = tests
        .iter()
        .map(|test| snapshot_key(test, &snapshots))
        .collect();
    for (test, key) in tests.iter_mut().zip(&keys) {
        let Some((key, arguments)) = key else {
            continue;
        };
        // Tests whose titles join to the same key share its counter, and so
        // does a test titled `<title>` whose call carries a hint: Jest keys
        // it `<title>: <hint>` and Vitest `<title> > <hint>`, and that hint
        // may make this key.
        let hint_join = match snapshots.runner {
            SnapshotRunner::Jest => ": ",
            SnapshotRunner::Vitest => " > ",
        };
        let sharing = titles
            .iter()
            .filter(|title| {
                *title == key
                    || key
                        .strip_prefix(title.as_str())
                        .is_some_and(|rest| rest.starts_with(hint_join))
            })
            .count();
        if sharing != 1 {
            continue;
        }
        if !snapshots.pins_only_entry(key) {
            continue;
        }
        // The body holds one snapshot call, so at most one assertion matches.
        if let Some(assertion) = test.assertions.iter_mut().find(|assertion| {
            assertion.matcher == "toMatchSnapshot"
                && assertion.argument_count == *arguments
                && assertion.oracle_kind == OracleKind::Snapshot
                && assertion.oracle_strength == OracleStrength::Medium
        }) {
            assertion.oracle_strength = OracleStrength::Strong;
        }
    }
}

/// The committed snapshot file for `test_file` at the runner's default
/// location, with the title separator its header names.
struct CommittedSnapshots {
    text: String,
    separator: String,
    runner: SnapshotRunner,
}

impl CommittedSnapshots {
    /// `true` when the file stores `<key> 1` exactly once and no `<key> 2`:
    /// the assertion ran once when the file was written, and its one entry
    /// is the one it compares with. The runner writes each entry at the start
    /// of a line, so text elsewhere (a comment) is not an entry.
    fn pins_only_entry(&self, key: &str) -> bool {
        let entry = |counter: usize| format!("\nexports[`{key} {counter}`] = ");
        self.text.matches(&entry(1)).count() == 1 && !self.text.contains(&entry(2))
    }
}

fn committed_snapshot_file(root: &Path, test_file: &Path) -> Option<CommittedSnapshots> {
    let directory = test_file.parent()?;
    let name = test_file.file_name()?.to_str()?;
    let snapshots = directory.join("__snapshots__");
    // A committed-history run reads the test file at `HEAD`; a `.snap` file
    // edited, deleted, untracked or ignored there is not the entry `HEAD`
    // holds.
    if crate::analysis::committed_source::snapshot_inputs_differ_in(root, &snapshots) {
        return None;
    }
    // `read_config_capped` refuses a symlinked `.snap` leaf only; a linked
    // `__snapshots__` directory could point at a tree no review judged.
    if std::fs::symlink_metadata(root.join(&snapshots))
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }
    let path = root.join(&snapshots).join(format!("{name}.snap"));
    let text = read_config_capped(&path).ok()?;
    let header = text.lines().next()?;
    let (separator, runner) = if header.starts_with("// Jest Snapshot v1") {
        (" ", SnapshotRunner::Jest)
    } else if header.starts_with("// Vitest Snapshot v1") {
        (" > ", SnapshotRunner::Vitest)
    } else {
        return None;
    };
    let configs = runner_configs(root, directory)?;
    // The header names the runner that wrote the file, not the one that runs
    // it: Vitest reads a file Jest left but keys titles with ` > `, and the
    // reverse holds for Jest. Only a header that agrees with the configs
    // names the entry the assertion compares with.
    if (runner == SnapshotRunner::Vitest) != configs.mentions_vitest {
        return None;
    }
    Some(CommittedSnapshots {
        text,
        separator: separator.to_string(),
        runner,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SnapshotRunner {
    Jest,
    Vitest,
}

/// What the runner configs above the test file say.
pub(super) struct RunnerConfigs {
    pub(super) mentions_vitest: bool,
}

/// Text that moves snapshot files away from the default location, changes
/// how they are serialized, or makes every run rewrite them, so a committed
/// entry no longer fails a changed value. The short `-u` flag is matched
/// separately so any whitespace may precede it, not only a space.
const SNAPSHOT_OVERRIDES: [&str; 7] = [
    "snapshotResolver",
    "resolveSnapshotPath",
    "snapshotEnvironment",
    "updateSnapshot",
    "--update",
    "\"-u\"",
    "snapshotFormat",
];

/// `true` when `text` holds `key` as an object key (`key:`, `key :`,
/// `"key":`, `'key':`, or JS shorthand `{ key }` / `{ key, ... }`). A
/// longer identifier (`rootDir`, `roots`) does not match `root`.
fn mentions_named_key(text: &str, key: &str) -> bool {
    text.match_indices(key).any(|(index, matched)| {
        let preceded_by_ident = text
            .get(..index)
            .and_then(|before| before.chars().next_back())
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'));
        !preceded_by_ident
            && text.get(index + matched.len()..).is_some_and(|rest| {
                let rest = rest.strip_prefix(['"', '\'']).unwrap_or(rest).trim_start();
                rest.starts_with(':') || rest.starts_with([',', '}'])
            })
    })
}

/// `true` when `text` holds an `update` key (`update: true`, `update : x`,
/// `"update": x`), Vitest's option to rewrite every changed snapshot.
fn mentions_update_key(text: &str) -> bool {
    mentions_named_key(text, "update")
}

/// `true` when a runner config names another config this walk does not read:
/// Jest `projects`, Vitest `workspace`, or a Vitest/Vite `root` key. Jest's
/// `rootDir` and a `<rootDir>` mapper token do not match `root`.
fn delegates_to_other_config(text: &str) -> bool {
    ["projects", "workspace", "root"]
        .into_iter()
        .any(|key| mentions_named_key(text, key))
}

/// `true` when `text` holds the short update flag after any whitespace
/// (`jest -u`, `jest\t-u`). `"-u"` is in [`SNAPSHOT_OVERRIDES`].
fn mentions_short_update_flag(text: &str) -> bool {
    text.match_indices("-u").any(|(index, _)| {
        text.get(..index)
            .and_then(|before| before.chars().next_back())
            .is_some_and(char::is_whitespace)
    })
}

/// Flags that point a runner at a config, project list or root the
/// directory walk does not read.
const CONFIG_FLAGS: [&str; 7] = [
    "--config",
    "--projects",
    "--rootDir",
    "--root",
    "--dir",
    "--workspace",
    "-r",
];

/// `true` when a `package.json` passes a runner config, project list or root
/// by path: a script flag (`--config x`, `--config=x`, `-c x`, `-cx`,
/// `--rootDir`, Vitest `--root` / `-r` / `--dir` / `--workspace`) or a
/// `"projects"` / `"rootDir"` key under the `jest` key. Either way the
/// config the runner uses may be one the directory walk does not read.
/// Over-matching (a `node -r` preload) only refuses.
fn passes_config_by_path(text: &str) -> bool {
    text.contains("\"projects\"")
        || text.contains("\"rootDir\"")
        || text
            .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '[' | ']'))
            .any(|token| {
                let flag = token.split('=').next().unwrap_or(token);
                CONFIG_FLAGS.contains(&flag)
                    || token.starts_with("--config")
                    || (token.starts_with("-c") && !token.starts_with("--"))
            })
}

/// Read every runner config and `package.json` directly in `directory` into
/// `configs`, or `None` when one mentions a snapshot override, passes a
/// config by path, delegates to another config this walk does not read,
/// cannot be read, or sits in a symlinked directory.
/// `allow_symlink` is `true` only for the analysis root itself, whose path is
/// the user's own checkout path and may be a link.
fn scan_config_directory(
    directory: &Path,
    allow_symlink: bool,
    configs: &mut RunnerConfigs,
) -> Option<()> {
    // `read_dir` follows a symlinked directory, so a link could point at a
    // tree this reader never judged; `read_config_capped` already refuses a
    // symlinked file.
    if !allow_symlink
        && std::fs::symlink_metadata(directory)
            .ok()?
            .file_type()
            .is_symlink()
    {
        return None;
    }
    for entry in std::fs::read_dir(directory).ok()? {
        let entry = entry.ok()?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let is_config = !file_name.ends_with(".snap")
            && crate::analysis::language::snapshot_input(Path::new(file_name));
        if !is_config {
            continue;
        }
        let text = read_config_capped(&entry.path()).ok()?;
        if SNAPSHOT_OVERRIDES
            .iter()
            .any(|override_text| text.contains(override_text))
            || mentions_short_update_flag(&text)
            // Vitest reads `update` from its own config only; a
            // `package.json` script may be named `update`.
            || (file_name != "package.json" && mentions_update_key(&text))
            || (file_name == "package.json" && passes_config_by_path(&text))
            // A Jest/Vitest config that names other configs is not walked.
            || (file_name != "package.json" && delegates_to_other_config(&text))
        {
            return None;
        }
        configs.mentions_vitest |= file_name.starts_with("vite") || text.contains("vitest");
    }
    Some(())
}

/// How many directories above `root` the walk reads while it looks for the
/// repository boundary.
const ABOVE_ROOT_LIMIT: usize = 16;

/// The directories above `root` to read, or `None` when the repository
/// boundary cannot be established.
///
/// The root is canonicalized first so a symlinked analysis root climbs its
/// real parents. The walk stops at (and includes) the first directory holding
/// a repository marker, looking at most `limit` levels up. Empty when `root`
/// is itself the repository top. When no marker is found and the walk reaches
/// the filesystem root within `limit`, every ancestor is returned: nothing
/// above is a repository, and the caller still reads any config there. When
/// the limit is hit before a marker or the filesystem root, the boundary is
/// unknown (a deep nest, or an export without `.git` inside a larger tree), so
/// `None` makes the caller refuse rather than guess.
pub(super) fn directories_above_root(root: &Path, limit: usize) -> Option<Vec<PathBuf>> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if root.join(".git").exists() {
        return Some(Vec::new());
    }
    let all: Vec<&Path> = root.ancestors().skip(1).collect();
    let above: Vec<&Path> = all.iter().copied().take(limit).collect();
    match above.iter().position(|dir| dir.join(".git").exists()) {
        Some(boundary) => Some(
            above
                .into_iter()
                .take(boundary + 1)
                .map(Path::to_path_buf)
                .collect(),
        ),
        None if all.len() <= limit => Some(above.into_iter().map(Path::to_path_buf).collect()),
        None => None,
    }
}

/// The runner configs and `package.json` files in the test's directory and
/// every ancestor up to `root`, then above `root` up to the repository
/// boundary (the first directory holding `.git`, or the filesystem root), or
/// `None` when one mentions a snapshot override, passes a config by path,
/// delegates to another config this walk does not read, cannot be read, (in
/// a committed-history run) differs from `HEAD`, or the boundary is not
/// found within reach.
fn runner_configs(root: &Path, directory: &Path) -> Option<RunnerConfigs> {
    runner_configs_within(root, directory, ABOVE_ROOT_LIMIT)
}

/// [`runner_configs`] with an explicit look-up limit above the root.
pub(super) fn runner_configs_within(
    root: &Path,
    directory: &Path,
    above_root_limit: usize,
) -> Option<RunnerConfigs> {
    let mut configs = RunnerConfigs {
        mentions_vitest: false,
    };
    let mut current = Some(directory);
    while let Some(relative) = current {
        if crate::analysis::committed_source::snapshot_inputs_differ_in(root, relative) {
            return None;
        }
        scan_config_directory(
            &root.join(relative),
            relative.as_os_str().is_empty(),
            &mut configs,
        )?;
        current = relative.parent();
    }
    for dir in directories_above_root(root, above_root_limit)? {
        scan_config_directory(&dir, false, &mut configs)?;
    }
    Some(configs)
}

/// The describe titles and the test title joined the way the runner keys
/// snapshots.
fn joined_title(test: &TypeScriptTest, separator: &str) -> String {
    title_parts(test).join(separator)
}

fn title_parts(test: &TypeScriptTest) -> Vec<&str> {
    test.describe_names
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(test.local_name.as_str()))
        .collect()
}

/// The name the runner keys this test's snapshot under, with the argument
/// count of the call that uses it, or `None` when it cannot be named: more
/// than one snapshot call in the body, or a title with characters the
/// snapshot file escapes. A body whose call carries a literal hint keys
/// `<title>: <hint>` under Jest; Vitest's hinted key is not read. The caller
/// refuses a key another test's title also joins to, since the two share
/// its counter.
fn snapshot_key(test: &TypeScriptTest, snapshots: &CommittedSnapshots) -> Option<(String, usize)> {
    if snapshot_calls(&test.body_text) != 1 {
        return None;
    }
    let parts = title_parts(test);
    if parts
        .iter()
        .any(|part| part.is_empty() || part.contains(['`', '\\', '$', '\n', '\r']))
    {
        return None;
    }
    let title = parts.join(&snapshots.separator);
    match literal_hint(&test.body_text) {
        Some(hint) if snapshots.runner == SnapshotRunner::Jest => {
            Some((format!("{title}: {hint}"), 1))
        }
        Some(_) => None,
        None => Some((title, 0)),
    }
}
