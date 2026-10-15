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

/// Snapshot matcher calls in `text`: `Snapshot` followed, after optional
/// whitespace, by `(`. Each such call advances the runner's per-title
/// counter, so it is counted whether or not it writes to the committed file;
/// comments and strings only over-count, which refuses.
fn snapshot_calls(text: &str) -> usize {
    text.match_indices("Snapshot")
        .filter(|(index, matched)| {
            text.get(index + matched.len()..)
                .is_some_and(|rest| rest.trim_start().starts_with('('))
        })
        .count()
}

/// The hint of the body's one `toMatchSnapshot("<hint>")` call, when it is a
/// single quoted string literal whose source text is its value and that the
/// snapshot file writes unescaped: no `` ` ``, `$`, `\`, line break or
/// matching quote inside, and not empty (Jest drops an empty hint). Any
/// other argument (a template, an identifier, a property matcher, a comment,
/// a second argument) is `None`.
fn literal_hint(body: &str) -> Option<&str> {
    let mut calls = body
        .match_indices("toMatchSnapshot")
        .filter_map(|(index, matched)| {
            body.get(index + matched.len()..)?
                .trim_start()
                .strip_prefix('(')
        });
    let rest = calls.next()?.trim_start();
    if calls.next().is_some() {
        return None;
    }
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
        // it `<title>: <hint>`, and that hint may make this key.
        let sharing = titles
            .iter()
            .filter(|title| {
                *title == key
                    || key
                        .strip_prefix(title.as_str())
                        .is_some_and(|rest| rest.starts_with(": "))
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
struct RunnerConfigs {
    mentions_vitest: bool,
}

/// Text that moves snapshot files away from the default location or makes
/// every run rewrite them, so a committed entry no longer fails a changed
/// value.
const SNAPSHOT_OVERRIDES: [&str; 7] = [
    "snapshotResolver",
    "resolveSnapshotPath",
    "snapshotEnvironment",
    "updateSnapshot",
    "--update",
    " -u",
    "\"-u\"",
];

/// `true` when `text` holds an `update` key (`update: true`, `update : x`,
/// `"update": x`), Vitest's option to rewrite every changed snapshot.
fn mentions_update_key(text: &str) -> bool {
    text.match_indices("update").any(|(index, matched)| {
        text.get(index + matched.len()..).is_some_and(|rest| {
            rest.strip_prefix(['"', '\''])
                .unwrap_or(rest)
                .trim_start()
                .starts_with(':')
        })
    })
}

/// The runner configs and `package.json` files in the test's directory and
/// every ancestor up to `root`, or `None` when one mentions a snapshot
/// override, cannot be read, or (in a committed-history run) differs from
/// `HEAD`.
fn runner_configs(root: &Path, directory: &Path) -> Option<RunnerConfigs> {
    let mut configs = RunnerConfigs {
        mentions_vitest: false,
    };
    let mut current = Some(directory);
    while let Some(relative) = current {
        if crate::analysis::committed_source::snapshot_inputs_differ_in(root, relative) {
            return None;
        }
        for entry in std::fs::read_dir(root.join(relative)).ok()? {
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
                // Vitest reads `update` from its own config only; a
                // `package.json` script may be named `update`.
                || (file_name != "package.json" && mentions_update_key(&text))
            {
                return None;
            }
            configs.mentions_vitest |= file_name.starts_with("vite") || text.contains("vitest");
        }
        current = relative.parent();
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
