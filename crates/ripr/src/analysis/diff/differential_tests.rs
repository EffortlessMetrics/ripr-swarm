//! Differential and byte-level robustness suite for the unified-diff parser
//! (#12918).
//!
//! `fuzz_panic_freedom` runs the whole binary on hostile bytes and only asks
//! "did it survive"; the two Kani harnesses cover two hunk-number helpers. No
//! test compared the parser's answer against an independent oracle. Every
//! downstream finding is anchored to a changed line the parser reports, so a
//! shifted or invented line number silently moves a finding onto the wrong
//! code.
//!
//! # Differential test
//!
//! A generated *scene* is a set of file edits: a modified file, a renamed and
//! modified file, a deleted file, a binary file and a new file, with LF or
//! CRLF endings, a missing final newline on either side, and `git diff -U<n>`
//! context from 0 to 5. The edit is generated as an event list (`Keep`, `Del`
//! or `Add` per line) over lines whose text is unique, so the correct
//! alignment is unambiguous and the expected changed lines are computed from
//! the generating edit, not from any diff. The scene is committed twice in a
//! temporary repository and the real `git diff` output between the commits is
//! fed to `ripr::analysis::parse_unified_diff`. The parser must report
//! exactly the removed (old-side) and added (new-side) line numbers the edit
//! made, the new-side position of each removal, and the line text.
//!
//! Every case first asserts its fixture: git produced a `diff --git` section
//! per file, a hunk header for each textual edit, and the section kind the
//! case intended (rename, deletion, binary). A diff that git did not produce
//! cannot pass vacuously.
//!
//! # Byte-level robustness test
//!
//! Seeds are the real diffs the differential test generates plus committed
//! fixture diffs under `metrics/rust-judged-behavior-panel/diffs`. Byte
//! edits (flip, delete, insert, duplicate a span, splice a boundary token,
//! truncate) are applied to a seed, and the parser must not panic, must
//! finish before a deadline, and must hold structural invariants on whatever
//! it returns (one-based line numbers, added lines on the new side, nonempty
//! relative paths).
//!
//! # Why proptest and not cargo-fuzz
//!
//! A `cargo fuzz` target needs a nightly toolchain, `libfuzzer-sys` and a
//! separate non-workspace crate. The repository builds on stable (MSRV 1.95),
//! pins its dependency set through `check-dependencies`, and runs its
//! scheduled fuzzing as proptest suites in `fuzz.yml`. A coverage-guided
//! target would add a toolchain and a dependency class the policy gates do
//! not admit, so this suite keeps the byte-level target as a proptest in the
//! existing lane instead of weakening those gates.
//!
//! The default case count is small so the required test run stays cheap. The
//! scheduled and manual `fuzz.yml` workflow raises it with `PROPTEST_CASES`
//! (400). Run longer locally with `PROPTEST_CASES=2000 cargo test -p ripr
//! --lib differential_tests`.

use super::{ChangedFile, parse_unified_diff};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::Config;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

const DEFAULT_CASES: u32 = 16;
const PARSE_DEADLINE: Duration = Duration::from_secs(30);

fn config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CASES),
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..Config::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Keep,
    Del,
    Add,
}

/// Prefixes that make a content line look like diff syntax once git prepends
/// its own `+`, `-` or space marker.
const PREFIXES: &[&str] = &[
    "",
    "  ",
    "+",
    "-",
    "-- ",
    "++ ",
    "@@ -1 +1 @@ ",
    "diff --git a/x b/x ",
    "\\ No newline at end of file ",
    "index 1111111..2222222 ",
    "Binary files a/x and b/x differ ",
];

/// One file's generating edit.
#[derive(Clone, Debug)]
struct Plan {
    events: Vec<Event>,
    prefixes: Vec<usize>,
    crlf: bool,
    old_final_newline: bool,
    new_final_newline: bool,
}

/// One line of the realised edit: its event, text, and both coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    event: Event,
    text: String,
}

impl Plan {
    /// Normalise to the order git emits and fold end-of-file terminator
    /// differences into the edit. Returns the rows in diff order.
    fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = self
            .events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                let prefix = PREFIXES[self.prefixes[index % self.prefixes.len()] % PREFIXES.len()];
                Row {
                    event: *event,
                    text: format!("{prefix}line_{index}"),
                }
            })
            .collect();
        // A change block is always deletions then additions.
        normalise(&mut rows);
        // A kept line that is the last line of one side but not the other, or
        // whose side lacks the final newline the other has, is a different
        // line to git: it is removed and re-added.
        let old_count = rows.iter().filter(|r| r.event != Event::Add).count();
        let new_count = rows.iter().filter(|r| r.event != Event::Del).count();
        let mut old_seen = 0;
        let mut new_seen = 0;
        let mut converted = Vec::with_capacity(rows.len() + 2);
        for row in rows {
            match row.event {
                Event::Del => old_seen += 1,
                Event::Add => new_seen += 1,
                Event::Keep => {
                    old_seen += 1;
                    new_seen += 1;
                }
            }
            if row.event == Event::Keep {
                let old_bare = old_seen == old_count && !self.old_final_newline;
                let new_bare = new_seen == new_count && !self.new_final_newline;
                if old_bare != new_bare {
                    converted.push(Row {
                        event: Event::Del,
                        text: row.text.clone(),
                    });
                    converted.push(Row {
                        event: Event::Add,
                        text: row.text,
                    });
                    continue;
                }
            }
            converted.push(row);
        }
        normalise(&mut converted);
        converted
    }

    fn render(&self, rows: &[Row], old_side: bool) -> Vec<u8> {
        let eol = if self.crlf { "\r\n" } else { "\n" };
        let keep: Vec<&Row> = rows
            .iter()
            .filter(|r| {
                if old_side {
                    r.event != Event::Add
                } else {
                    r.event != Event::Del
                }
            })
            .collect();
        let final_newline = if old_side {
            self.old_final_newline
        } else {
            self.new_final_newline
        };
        let mut out = String::new();
        for (index, row) in keep.iter().enumerate() {
            out.push_str(&row.text);
            if index + 1 < keep.len() || final_newline {
                out.push_str(eol);
            }
        }
        out.into_bytes()
    }
}

/// Reorder every run between kept lines to deletions first, additions second,
/// preserving relative order inside each group.
fn normalise(rows: &mut Vec<Row>) {
    let mut out: Vec<Row> = Vec::with_capacity(rows.len());
    let mut dels: Vec<Row> = Vec::new();
    let mut adds: Vec<Row> = Vec::new();
    for row in rows.drain(..) {
        match row.event {
            Event::Del => dels.push(row),
            Event::Add => adds.push(row),
            Event::Keep => {
                out.append(&mut dels);
                out.append(&mut adds);
                out.push(row);
            }
        }
    }
    out.append(&mut dels);
    out.append(&mut adds);
    *rows = out;
}

/// Expected changed lines for one file: `(old_line, new_side_line, text)` for
/// removals and `(new_line, text)` for additions.
#[derive(Debug, Default, PartialEq, Eq)]
struct Expected {
    removed: Vec<(usize, usize, String)>,
    added: Vec<(usize, String)>,
    /// Old-side numbers of removals whose `new_side_line` is the known
    /// zero-count-hunk divergence (#13098). The default run
    /// does not compare that one coordinate for them; the ignored regression
    /// test does.
    gaps: std::collections::BTreeSet<usize>,
}

/// Git writes the new-side start of a hunk with no new-side lines (`+S,0`) as
/// the line before the gap, while the parser takes it as the next-line
/// counter, so `new_side_line` of a removal in such a hunk is one low (zero
/// when the new file is empty). Tracked as #13098.

fn expected_for(rows: &[Row], context: u8) -> Expected {
    let mut expected = Expected::default();
    let new_count = rows.iter().filter(|r| r.event != Event::Del).count();
    // A removal sits in a zero-new-count hunk when its change block has no
    // additions and there is no context, or the new file is empty.
    let mut block_start = 0;
    let mut gap_rows = vec![false; rows.len()];
    while block_start < rows.len() {
        if rows[block_start].event == Event::Keep {
            block_start += 1;
            continue;
        }
        let mut end = block_start;
        while end < rows.len() && rows[end].event != Event::Keep {
            end += 1;
        }
        let pure = rows[block_start..end].iter().all(|r| r.event == Event::Del);
        for flag in &mut gap_rows[block_start..end] {
            *flag = (pure && context == 0) || new_count == 0;
        }
        block_start = end;
    }
    let mut row_index = 0;
    let mut old_line = 0;
    let mut new_line = 0;
    for row in rows {
        let gap = gap_rows[row_index];
        row_index += 1;
        match row.event {
            Event::Keep => {
                old_line += 1;
                new_line += 1;
            }
            Event::Del => {
                old_line += 1;
                if gap {
                    expected.gaps.insert(old_line);
                }
                expected
                    .removed
                    .push((old_line, new_line + 1, row.text.clone()));
            }
            Event::Add => {
                new_line += 1;
                expected.added.push((new_line, row.text.clone()));
            }
        }
    }
    expected
}

fn plan_strategy(min_keep: usize, max_changes: usize) -> impl Strategy<Value = Plan> {
    (
        prop::collection::vec(
            prop_oneof![
                4 => Just(Event::Keep),
                2 => Just(Event::Del),
                2 => Just(Event::Add),
            ],
            1..24,
        ),
        prop::collection::vec(0..PREFIXES.len(), 1..8),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            move |(mut events, prefixes, crlf, old_final_newline, new_final_newline)| {
                // Make the plan satisfy the shape the case needs: enough kept
                // lines for rename similarity, a bounded number of changes,
                // and at least one change so the diff is nonempty.
                let mut changes = 0;
                for event in &mut events {
                    if *event != Event::Keep {
                        changes += 1;
                        if changes > max_changes {
                            *event = Event::Keep;
                        }
                    }
                }
                while events.iter().filter(|e| **e == Event::Keep).count() < min_keep {
                    events.insert(0, Event::Keep);
                }
                if events.iter().all(|e| *e == Event::Keep) {
                    events.push(Event::Add);
                }
                Plan {
                    events,
                    prefixes,
                    crlf,
                    old_final_newline,
                    new_final_newline,
                }
            },
        )
}

/// Keeps nothing from the old file: an addition-only plan (new file) or a
/// deletion-only plan.
fn whole_file_lines(tag: &'static str) -> impl Strategy<Value = (Vec<String>, bool, bool)> {
    (
        prop::collection::vec(0..PREFIXES.len(), 1..12),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(move |(prefixes, crlf, final_newline)| {
            let lines = prefixes
                .iter()
                .enumerate()
                .map(|(index, prefix)| format!("{}{tag}_{index}", PREFIXES[*prefix]))
                .collect();
            (lines, crlf, final_newline)
        })
}

#[derive(Clone, Debug)]
struct Scene {
    edit: Plan,
    rename: Option<Plan>,
    pure_rename: bool,
    delete: Option<(Vec<String>, bool, bool)>,
    binary: bool,
    new_file: Option<(Vec<String>, bool, bool)>,
    context: u8,
}

fn scene_strategy() -> impl Strategy<Value = Scene> {
    (
        plan_strategy(0, 24),
        proptest::option::of(plan_strategy(8, 2)),
        any::<bool>(),
        // Distinct line text per file so a deletion and a new file are never
        // paired into a rename by similarity.
        proptest::option::of(whole_file_lines("deleted_content")),
        any::<bool>(),
        proptest::option::of(whole_file_lines("fresh_content")),
        0u8..=5,
    )
        .prop_map(
            |(edit, rename, pure_rename, delete, binary, new_file, context)| Scene {
                edit,
                rename,
                pure_rename,
                delete,
                binary,
                new_file,
                context,
            },
        )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-fuzz-diff-parser-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create {} failed: {error}", root.display()))?;
        let repo = Self { root };
        repo.git(&["init", "-q"])?;
        for (key, value) in [
            ("user.email", "differential@example.invalid"),
            ("user.name", "differential"),
            ("core.autocrlf", "false"),
            ("core.safecrlf", "false"),
            ("core.quotepath", "false"),
            ("commit.gpgsign", "false"),
            ("diff.renames", "true"),
        ] {
            repo.git(&["config", key, value])?;
        }
        Ok(repo)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("LC_ALL", "C")
            .output()
            .map_err(|error| format!("git {args:?} failed to start: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&path, bytes).map_err(|error| format!("write {name} failed: {error}"))
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        std::fs::remove_file(self.root.join(name)).map_err(|error| error.to_string())
    }

    fn commit(&self, message: &str) -> Result<(), String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--allow-empty", "-m", message])?;
        Ok(())
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn whole_file_bytes(lines: &[String], crlf: bool, final_newline: bool) -> Vec<u8> {
    let eol = if crlf { "\r\n" } else { "\n" };
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        out.push_str(line);
        if index + 1 < lines.len() || final_newline {
            out.push_str(eol);
        }
    }
    out.into_bytes()
}

/// Build the scene, run real `git diff`, and return the diff plus the
/// expected per-path changed lines.
fn realise(scene: &Scene) -> Result<(String, BTreeMap<String, Expected>), String> {
    let repo = TempRepo::create()?;
    let mut expected: BTreeMap<String, Expected> = BTreeMap::new();
    let edit_rows = scene.edit.rows();
    let rename_rows = scene.rename.as_ref().map(Plan::rows);

    repo.write("src/edit.rs", &scene.edit.render(&edit_rows, true))?;
    if let (Some(plan), Some(rows)) = (&scene.rename, &rename_rows) {
        repo.write("src/before name.rs", &plan.render(rows, true))?;
    }
    if let Some((lines, crlf, nl)) = &scene.delete {
        repo.write("src/gone.rs", &whole_file_bytes(lines, *crlf, *nl))?;
    }
    if scene.binary {
        repo.write("data/blob.bin", b"\0\x01old\0binary\xff")?;
    }
    repo.commit("old")?;

    repo.write("src/edit.rs", &scene.edit.render(&edit_rows, false))?;
    expected.insert(
        "src/edit.rs".into(),
        expected_for(&edit_rows, scene.context),
    );
    if let (Some(plan), Some(rows)) = (&scene.rename, &rename_rows) {
        repo.remove("src/before name.rs")?;
        // A pure rename keeps the old bytes; otherwise apply the edit.
        if scene.pure_rename {
            repo.write("src/after name.rs", &plan.render(rows, true))?;
            expected.insert("src/after name.rs".into(), Expected::default());
        } else {
            repo.write("src/after name.rs", &plan.render(rows, false))?;
            expected.insert(
                "src/after name.rs".into(),
                expected_for(rows, scene.context),
            );
        }
    }
    if scene.delete.is_some() {
        repo.remove("src/gone.rs")?;
    }
    if scene.binary {
        repo.write("data/blob.bin", b"\0\x02new\0binary\xfe\xfe")?;
    }
    if let Some((lines, crlf, nl)) = &scene.new_file {
        repo.write("src/fresh.rs", &whole_file_bytes(lines, *crlf, *nl))?;
        expected.insert(
            "src/fresh.rs".into(),
            Expected {
                removed: Vec::new(),
                gaps: Default::default(),
                added: lines
                    .iter()
                    .enumerate()
                    .map(|(index, line)| (index + 1, line.clone()))
                    .collect(),
            },
        );
    }
    repo.commit("new")?;

    let context = format!("-U{}", scene.context);
    let diff = repo.git(&[
        "diff",
        "--no-color",
        "--no-ext-diff",
        "-M10%",
        &context,
        "HEAD~1",
        "HEAD",
    ])?;
    Ok((diff, expected))
}

fn parse_with_deadline(input: String) -> Result<Vec<ChangedFile>, String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(parse_unified_diff(&input));
    });
    receiver
        .recv_timeout(PARSE_DEADLINE)
        .map_err(|error| format!("parser did not return within the deadline: {error}"))
}

/// Compare the parser's answer for one path with the generating edit.
fn compare(
    path: &str,
    files: &[ChangedFile],
    expected: &Expected,
    strict: bool,
) -> Result<(), String> {
    let found: Vec<&ChangedFile> = files
        .iter()
        .filter(|file| file.path == Path::new(path))
        .collect();
    if expected.added.is_empty() && expected.removed.is_empty() {
        // Nothing textual changed (pure rename): the parser may omit the file
        // or report it empty, but must not invent lines.
        for file in found {
            if !file.added_lines.is_empty() || !file.removed_lines.is_empty() {
                return Err(format!("{path}: invented changed lines {file:?}"));
            }
        }
        return Ok(());
    }
    let [file] = found.as_slice() else {
        return Err(format!(
            "{path}: expected exactly one parsed file, found {}",
            found.len()
        ));
    };
    let added: Vec<(usize, String)> = file
        .added_lines
        .iter()
        .map(|line| (line.line, line.text.clone()))
        .collect();
    let removed: Vec<(usize, usize, String)> = file
        .removed_lines
        .iter()
        .map(|line| (line.line, line.new_side_line, line.text.clone()))
        .collect();
    let actual = Expected {
        gaps: Default::default(),
        removed: removed
            .into_iter()
            .map(|(old, new, text)| (old, new, normalise_text(&text)))
            .collect(),
        added: added
            .into_iter()
            .map(|(line, text)| (line, normalise_text(&text)))
            .collect(),
    };
    let want = Expected {
        gaps: Default::default(),
        removed: expected
            .removed
            .iter()
            .map(|(old, new, text)| {
                let mut new = *new;
                if !strict && expected.gaps.contains(old) {
                    // Known divergence: take the parser's coordinate.
                    if let Some((_, parsed, _)) = actual.removed.iter().find(|r| r.0 == *old) {
                        new = *parsed;
                    }
                }
                (*old, new, normalise_text(text))
            })
            .collect(),
        added: expected
            .added
            .iter()
            .map(|(line, text)| (*line, normalise_text(text)))
            .collect(),
    };
    if actual == want {
        Ok(())
    } else {
        Err(format!(
            "{path}: parser disagrees with the generating edit\n  expected {want:?}\n  parsed   {actual:?}"
        ))
    }
}

fn normalise_text(text: &str) -> String {
    text.trim_end_matches('\r').to_string()
}

fn check_scene(scene: &Scene, strict: bool) -> Result<(), String> {
    let (diff, expected) = realise(scene)?;

    // Fixture assertions: git produced the sections the scene intended.
    let section = |path: &str| diff.contains(&format!("+++ b/{path}"));
    if !diff.contains("diff --git a/src/edit.rs b/src/edit.rs") {
        return Err(format!("fixture: no section for the edited file\n{diff}"));
    }
    if !diff.contains("\n@@ ") {
        return Err(format!("fixture: git produced no hunk\n{diff}"));
    }
    if scene.rename.is_some() {
        if !diff.contains("rename from src/before name.rs") {
            return Err(format!("fixture: git did not detect the rename\n{diff}"));
        }
        if !scene.pure_rename && !section("src/after name.rs") {
            return Err(format!("fixture: renamed file has no hunk\n{diff}"));
        }
    }
    if scene.delete.is_some() && !diff.contains("deleted file mode") {
        return Err(format!("fixture: git did not report a deletion\n{diff}"));
    }
    if scene.binary && !diff.contains("Binary files") {
        return Err(format!("fixture: git did not report a binary file\n{diff}"));
    }
    if scene.new_file.is_some() && !diff.contains("new file mode") {
        return Err(format!("fixture: git did not report a new file\n{diff}"));
    }

    let files = parse_with_deadline(diff.clone())?;
    for (path, want) in &expected {
        compare(path, &files, want, strict)
            .map_err(|error| format!("{error}\n--- diff ---\n{diff}"))?;
    }
    // Deleted and binary files carry no new-side lines.
    for file in &files {
        let path = file.path.to_string_lossy();
        if path == "data/blob.bin" && !file.added_lines.is_empty() {
            return Err(format!("binary file reported added lines\n{diff}"));
        }
        if path == "src/gone.rs" && !file.added_lines.is_empty() {
            return Err(format!("deleted file reported added lines\n{diff}"));
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// Real `git diff` output parses to the changed lines the edit made.
    #[test]
    fn git_diff_parses_to_the_generating_edit(scene in scene_strategy()) {
        if let Err(error) = check_scene(&scene, false) {
            return Err(TestCaseError::fail(error));
        }
    }
}

#[test]
fn deterministic_scene_pins_the_harness() {
    // A fixed scene that exercises every file kind, so a harness regression
    // cannot hide behind generator luck.
    let scene = Scene {
        edit: Plan {
            events: vec![
                Event::Keep,
                Event::Del,
                Event::Add,
                Event::Keep,
                Event::Keep,
                Event::Add,
            ],
            prefixes: vec![0, 4],
            crlf: true,
            old_final_newline: false,
            new_final_newline: true,
        },
        rename: Some(Plan {
            events: vec![
                Event::Keep,
                Event::Keep,
                Event::Keep,
                Event::Keep,
                Event::Keep,
                Event::Keep,
                Event::Keep,
                Event::Keep,
                Event::Add,
            ],
            prefixes: vec![0],
            crlf: false,
            old_final_newline: true,
            new_final_newline: true,
        }),
        pure_rename: false,
        delete: Some((vec!["a".into(), "b".into()], false, true)),
        binary: true,
        new_file: Some((vec!["n1".into(), "n2".into()], true, false)),
        context: 1,
    };
    let outcome = check_scene(&scene, false);
    assert!(outcome.is_ok(), "{outcome:?}");
}

// ---------------------------------------------------------------------------
// Byte-level robustness
// ---------------------------------------------------------------------------

fn fixture_seeds() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../metrics/rust-judged-behavior-panel/diffs");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "diff"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter(|text| !text.is_empty() && text.len() <= 64 * 1024)
        .take(32)
        .collect()
}

const SPLICE_TOKENS: &[&str] = &[
    "\n--- a/x\n+++ b/x\n",
    "\n@@ -1,3 +1,3 @@\n",
    "\n@@ -18446744073709551615,2 +1 @@\n",
    "\n@@ -0,0 +0,0 @@\n",
    "\ndiff --git a/x b/x\n",
    "\n\\ No newline at end of file\n",
    "\nBinary files a/x and b/x differ\n",
    "\nrename from ../../etc\nrename to /abs\n",
    "\r\n",
    "\u{feff}",
    "\0",
];

#[derive(Clone, Debug)]
enum Mutation {
    Flip(usize, u8),
    Delete(usize, usize),
    Insert(usize, u8),
    Duplicate(usize, usize),
    Splice(usize, usize),
    Truncate(usize),
}

fn mutation_strategy() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (any::<usize>(), any::<u8>()).prop_map(|(at, byte)| Mutation::Flip(at, byte)),
        (any::<usize>(), 1usize..64).prop_map(|(at, len)| Mutation::Delete(at, len)),
        (any::<usize>(), any::<u8>()).prop_map(|(at, byte)| Mutation::Insert(at, byte)),
        (any::<usize>(), 1usize..256).prop_map(|(at, len)| Mutation::Duplicate(at, len)),
        (any::<usize>(), 0..SPLICE_TOKENS.len())
            .prop_map(|(at, token)| Mutation::Splice(at, token)),
        any::<usize>().prop_map(Mutation::Truncate),
    ]
}

fn apply(mut bytes: Vec<u8>, mutation: &Mutation) -> Vec<u8> {
    let len = bytes.len();
    match *mutation {
        Mutation::Flip(at, byte) if len > 0 => bytes[at % len] = byte,
        Mutation::Delete(at, count) if len > 0 => {
            let start = at % len;
            bytes.drain(start..(start + count).min(len));
        }
        Mutation::Insert(at, byte) => bytes.insert(at % (len + 1), byte),
        Mutation::Duplicate(at, count) if len > 0 => {
            let start = at % len;
            let span = bytes[start..(start + count).min(len)].to_vec();
            let target = (start + span.len()).min(len);
            bytes.splice(target..target, span);
        }
        Mutation::Splice(at, token) => {
            let at = at % (len + 1);
            bytes.splice(at..at, SPLICE_TOKENS[token].bytes());
        }
        Mutation::Truncate(at) if len > 0 => bytes.truncate(at % len),
        _ => {}
    }
    bytes
}

/// Structural invariants any parse result must satisfy, valid input or not.
fn check_invariants(files: &[ChangedFile]) -> Result<(), String> {
    for file in files {
        if file.path.as_os_str().is_empty() {
            return Err("empty path in a parsed file".into());
        }
        for line in &file.added_lines {
            if line.line == 0 || line.line != line.new_side_line {
                return Err(format!(
                    "{}: added line {} has new_side_line {}",
                    file.path.display(),
                    line.line,
                    line.new_side_line
                ));
            }
        }
        for line in &file.removed_lines {
            if line.line == 0 {
                return Err(format!(
                    "{}: removed line has old-side number zero",
                    file.path.display()
                ));
            }
        }
    }
    Ok(())
}

/// Real diffs from the generator, so the byte edits start from valid shapes.
fn generated_seeds() -> Vec<String> {
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    (0..6)
        .filter_map(|_| {
            let scene = scene_strategy().new_tree(&mut runner).ok()?.current();
            realise(&scene).ok().map(|(diff, _)| diff)
        })
        .collect()
}

fn seeds() -> &'static [String] {
    static SEEDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    SEEDS.get_or_init(|| {
        let mut seeds = fixture_seeds();
        seeds.extend(generated_seeds());
        seeds
    })
}

proptest! {
    #![proptest_config(config())]

    /// Mutated real diffs never panic or hang the parser and keep the shape
    /// invariants.
    #[test]
    fn mutated_diffs_never_panic_or_break_invariants(
        seed_index in any::<usize>(),
        mutations in prop::collection::vec(mutation_strategy(), 1..6),
    ) {
        let seeds = seeds();
        prop_assert!(!seeds.is_empty(), "no seed diffs available");
        let mut bytes = seeds[seed_index % seeds.len()].clone().into_bytes();
        for mutation in &mutations {
            bytes = apply(bytes, mutation);
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let files = parse_with_deadline(text).map_err(TestCaseError::fail)?;
        check_invariants(&files).map_err(TestCaseError::fail)?;
    }
}

#[test]
fn seed_corpus_is_nonempty_and_parses() {
    let fixtures = fixture_seeds();
    assert!(
        !fixtures.is_empty(),
        "no fixture diffs found under metrics/rust-judged-behavior-panel/diffs"
    );
    let generated = generated_seeds();
    assert!(!generated.is_empty(), "generator produced no real diffs");
    for seed in fixtures.iter().chain(generated.iter()) {
        let files = parse_unified_diff(seed);
        assert!(check_invariants(&files).is_ok());
    }
    let with_lines = fixtures
        .iter()
        .filter(|seed| {
            parse_unified_diff(seed)
                .iter()
                .any(|file| !file.added_lines.is_empty() || !file.removed_lines.is_empty())
        })
        .count();
    assert!(with_lines > 0, "no fixture seed parsed to changed lines");
}

/// Shrunk repro for the zero-count-hunk divergence (#13098):
/// `-U0` diff output has `@@ -2 +1,0 @@` for deleting line 2, and the parser
/// reports the removal's `new_side_line` as 1 instead of 2.
#[test]
#[ignore = "#13098: new_side_line of a removal in a +S,0 hunk is one low"]
fn zero_count_hunk_removal_new_side_line_regression() {
    let scene = Scene {
        edit: Plan {
            events: vec![Event::Keep, Event::Del, Event::Keep, Event::Keep],
            prefixes: vec![0],
            crlf: false,
            old_final_newline: false,
            new_final_newline: false,
        },
        rename: None,
        pure_rename: false,
        delete: None,
        binary: false,
        new_file: None,
        context: 0,
    };
    let outcome = check_scene(&scene, true);
    assert!(outcome.is_ok(), "{outcome:?}");
}
