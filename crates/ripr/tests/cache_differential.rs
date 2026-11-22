//! Cache and incremental correctness as a differential: whatever a run
//! reuses, its answer must equal a cold run's.
//!
//! `ripr check` keeps per-file facts, module-tree scans, a corpus fingerprint
//! and classified seams under `RIPR_CACHE_DIR`. A warm run may skip any of
//! that work; it may never print a different document. Each case here walks a
//! seeded sequence of edits over a workspace and, after every step, runs the
//! built binary twice on the same bytes:
//!
//! - **warm**: one cache directory carried through the whole sequence, so it
//!   holds entries from every earlier state;
//! - **cold**: a cache directory created for that single run.
//!
//! The two runs must agree on the exit code and on the whole JSON document
//! for both the diff route (`--format json`) and the repository route
//! (`--format repo-exposure-json`). Further cases cover what a sequential
//! sequence cannot: several runs sharing one cache at once, and a file that is
//! rewritten and restored while a run is reading it.
//!
//! The edits are the ones that have historically separated a cache key from
//! its inputs: a same-length rewrite with the modification time restored, an
//! identical-bytes rewrite, a `touch`, a revert, a commit that moves the
//! base, a staged change, a new, renamed or deleted test file, a manifest
//! edit and a `ripr.toml` mode change.
//!
//! Only two members are excluded from the comparison, both by name:
//! `generated_at` is a wall-clock stamp and `content_sha256` is the digest of
//! the document that contains that stamp. Everything else is compared.
//!
//! Controls show the suite can fail and is not vacuous: every sequence must
//! see findings and seams, the warm run must report reuse, and two different
//! workspace states must compare unequal under the same normalization.
//!
//! The generated workspace and the three example projects shipped under
//! `examples/` run by default. Point the ignored case at any checkout to run
//! the same sequence on a real repository:
//!
//! ```text
//! RIPR_CACHE_DIFFERENTIAL_SUBJECTS=/path/to/repo \
//!   cargo test -p ripr --test cache_differential -- --ignored
//! ```
//!
//! `RIPR_CACHE_DIFFERENTIAL_STEPS` and `RIPR_CACHE_DIFFERENTIAL_SEED` widen or
//! replay a sequence; a failure prints the seed and the edit log. These are
//! static classes throughout; nothing here claims a runtime mutation outcome.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[path = "common/fixture_git.rs"]
mod fixture_git;

const DEFAULT_STEPS: usize = 6;
const DEFAULT_SEED: u64 = 0x5eed_cafe;
/// The two routes with distinct cache layers: per-file facts on the diff
/// route, classified seams and the corpus fingerprint on the repository one.
const FORMATS: [&str; 2] = ["json", "repo-exposure-json"];
/// Wall-clock stamp, and the digest of the document that carries it.
const VOLATILE_MEMBERS: [&str; 2] = ["generated_at", "content_sha256"];
const SOURCE_EXTENSIONS: [&str; 5] = ["rs", "py", "ts", "tsx", "js"];
const SKIPPED_DIRS: [&str; 4] = [".git", "target", "node_modules", ".ripr"];

/// Operator pairs a text-level edit swaps. Each side is a discriminator the
/// analysis probes, so a swap usually moves a finding.
const OPERATOR_SWAPS: [(&str, &str); 10] = [
    (" < ", " <= "),
    (" <= ", " < "),
    (" > ", " >= "),
    (" >= ", " > "),
    (" == ", " != "),
    (" != ", " == "),
    (" + 1", " + 2"),
    (" - 1", " - 2"),
    (" && ", " || "),
    (" || ", " && "),
];

/// SplitMix64: a fixed, dependency-free sequence so a seed replays exactly.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        usize::try_from(self.next() % (bound as u64)).unwrap_or(0)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let index = self.below(items.len());
        items.get(index)
    }
}

fn env_number(name: &str) -> Option<u64> {
    let text = std::env::var(name).ok()?;
    let text = text.trim();
    match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

fn steps() -> usize {
    env_number("RIPR_CACHE_DIFFERENTIAL_STEPS")
        .and_then(|steps| usize::try_from(steps).ok())
        .unwrap_or(DEFAULT_STEPS)
}

fn seed() -> u64 {
    env_number("RIPR_CACHE_DIFFERENTIAL_SEED").unwrap_or(DEFAULT_SEED)
}

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A scratch directory holding one repository and its cache directories.
struct Scratch {
    base: PathBuf,
    next_cache: u64,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl Scratch {
    fn create(label: &str) -> Result<Self, String> {
        let base = std::env::temp_dir().join(format!(
            "ripr-cache-differential-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&base)
            .map_err(|error| format!("create {} failed: {error}", base.display()))?;
        // Resolve a temp-directory alias once so the root every run names is
        // one spelling.
        let base = std::fs::canonicalize(&base)
            .map_err(|error| format!("canonicalize {} failed: {error}", base.display()))?;
        Ok(Self {
            base,
            next_cache: 0,
        })
    }

    fn root(&self) -> PathBuf {
        self.base.join("repo")
    }

    /// A cache directory no run has used.
    fn fresh_cache(&mut self) -> PathBuf {
        self.next_cache += 1;
        self.base.join(format!("cache-{}", self.next_cache))
    }
}

fn write(root: &Path, relative: &str, content: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
    }
    std::fs::write(&path, content).map_err(|error| format!("write {relative} failed: {error}"))
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to)
        .map_err(|error| format!("create {} failed: {error}", to.display()))?;
    let entries = std::fs::read_dir(from)
        .map_err(|error| format!("read {} failed: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {} failed: {error}", from.display()))?;
        let name = entry.file_name();
        if SKIPPED_DIRS.iter().any(|skipped| name == *skipped) {
            continue;
        }
        let kind = entry
            .file_type()
            .map_err(|error| format!("stat {:?} failed: {error}", entry.path()))?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &to.join(&name))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(&name))
                .map_err(|error| format!("copy {:?} failed: {error}", entry.path()))?;
        }
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    fixture_git::fixture_git_ok(root, args)
}

/// Commit the tree as the base on `main`, with build output ignored.
fn commit_base(root: &Path) -> Result<(), String> {
    git(root, &["init", "-q", "-b", "main"])?;
    git(root, &["config", "user.email", "ripr@example.invalid"])?;
    git(root, &["config", "user.name", "RIPR Test"])?;
    git(root, &["config", "core.autocrlf", "false"])?;
    std::fs::write(root.join(".git/info/exclude"), "/target/\n")
        .map_err(|error| format!("write git exclude failed: {error}"))?;
    git(root, &["add", "-A"])?;
    git(root, &["commit", "-q", "-m", "base"])
}

/// A small crate with several comparisons, some pinned at the boundary by a
/// test and some only exercised far from it, so edits move findings between
/// classes.
fn generated_rust_workspace(root: &Path, modules: usize) -> Result<(), String> {
    let mut lib = String::new();
    let mut tests = String::from("use cache_differential_subject::*;\n");
    for index in 0..modules {
        let threshold = 10 + index * 7;
        lib.push_str(&format!("mod part{index};\npub use part{index}::*;\n"));
        write(
            root,
            &format!("src/part{index}.rs"),
            &format!(
                "pub fn gate{index}(amount: u64) -> bool {{\n    amount > {threshold}\n}}\n\npub fn step{index}(amount: u64) -> u64 {{\n    if amount == {threshold} {{\n        amount + 1\n    }} else {{\n        amount - 1\n    }}\n}}\n"
            ),
        )?;
        let pinned = if index % 2 == 0 {
            threshold
        } else {
            threshold + 50
        };
        tests.push_str(&format!(
            "\n#[test]\nfn gate{index}_checked() {{\n    assert!(gate{index}({}));\n}}\n\n#[test]\nfn step{index}_checked() {{\n    assert_eq!(step{index}({pinned}), {});\n}}\n",
            threshold + 50,
            if pinned == threshold {
                pinned + 1
            } else {
                pinned - 1
            }
        ));
    }
    write(root, "src/lib.rs", &lib)?;
    write(root, "tests/case.rs", &tests)?;
    write(
        root,
        "Cargo.toml",
        "[package]\nname = \"cache-differential-subject\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
}

fn source_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, root: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir)
            .map_err(|error| format!("read {} failed: {error}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("read {} failed: {error}", dir.display()))?;
            let name = entry.file_name();
            if SKIPPED_DIRS.iter().any(|skipped| name == *skipped) {
                continue;
            }
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| format!("stat {} failed: {error}", path.display()))?;
            if kind.is_dir() {
                walk(&path, root, found)?;
            } else if kind.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension))
                && let Ok(relative) = path.strip_prefix(root)
            {
                found.push(relative.to_path_buf());
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    walk(root, root, &mut found)?;
    found.sort();
    Ok(found)
}

fn is_test_path(path: &Path) -> bool {
    path.to_string_lossy().to_ascii_lowercase().contains("test")
}

fn relative_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Swap one operator in `text`, chosen by `rng`. With `same_length` only a
/// swap that keeps the byte length is taken.
fn swap_operator(text: &str, rng: &mut Rng, same_length: bool) -> Option<(String, String)> {
    let mut candidates = Vec::new();
    for (from, to) in OPERATOR_SWAPS {
        if same_length && from.len() != to.len() {
            continue;
        }
        candidates.extend(text.match_indices(from).map(|(at, _)| (at, from, to)));
    }
    candidates.sort();
    let (at, from, to) = *rng.pick(&candidates)?;
    let mut swapped = String::with_capacity(text.len() + 1);
    swapped.push_str(text.get(..at)?);
    swapped.push_str(to);
    swapped.push_str(text.get(at + from.len()..)?);
    Some((
        swapped,
        format!("{}->{} at byte {at}", from.trim(), to.trim()),
    ))
}

/// One step of the sequence. Returns the log line, or `None` when the chosen
/// edit had nothing to act on (the caller draws again).
fn apply_edit(root: &Path, rng: &mut Rng, kind: usize) -> Result<Option<String>, String> {
    let sources = source_files(root)?;
    let tests: Vec<PathBuf> = sources
        .iter()
        .filter(|path| is_test_path(path))
        .cloned()
        .collect();
    let production: Vec<PathBuf> = sources
        .iter()
        .filter(|path| !is_test_path(path))
        .cloned()
        .collect();
    let read = |relative: &Path| {
        std::fs::read_to_string(root.join(relative))
            .map_err(|error| format!("read {} failed: {error}", relative.display()))
    };
    let store = |relative: &Path, text: &str| {
        std::fs::write(root.join(relative), text)
            .map_err(|error| format!("write {} failed: {error}", relative.display()))
    };
    match kind {
        // Weighted toward the edit that moves findings.
        0..=2 => {
            let Some(path) = rng.pick(&production) else {
                return Ok(None);
            };
            let Some((swapped, what)) = swap_operator(&read(path)?, rng, false) else {
                return Ok(None);
            };
            store(path, &swapped)?;
            Ok(Some(format!("edit {}: {what}", relative_text(path))))
        }
        3 => {
            let Some(path) = rng.pick(&tests) else {
                return Ok(None);
            };
            let Some((swapped, what)) = swap_operator(&read(path)?, rng, false) else {
                return Ok(None);
            };
            store(path, &swapped)?;
            Ok(Some(format!("edit test {}: {what}", relative_text(path))))
        }
        // Same length, modification time restored: only a content read or a
        // change-time witness can tell this file moved.
        4 => {
            let Some(path) = rng.pick(&sources) else {
                return Ok(None);
            };
            let Some((swapped, what)) = swap_operator(&read(path)?, rng, true) else {
                return Ok(None);
            };
            let absolute = root.join(path);
            let modified = std::fs::metadata(&absolute)
                .and_then(|metadata| metadata.modified())
                .map_err(|error| format!("stat {} failed: {error}", path.display()))?;
            store(path, &swapped)?;
            std::fs::File::options()
                .write(true)
                .open(&absolute)
                .and_then(|file| file.set_modified(modified))
                .map_err(|error| format!("restore mtime of {} failed: {error}", path.display()))?;
            Ok(Some(format!(
                "same-length edit, mtime restored {}: {what}",
                relative_text(path)
            )))
        }
        // Identical bytes, new timestamps.
        5 => {
            let Some(path) = rng.pick(&sources) else {
                return Ok(None);
            };
            let text = read(path)?;
            store(path, &format!("{text}\n"))?;
            store(path, &text)?;
            Ok(Some(format!(
                "rewrite identical bytes {}",
                relative_text(path)
            )))
        }
        6 => {
            let Some(path) = rng.pick(&sources) else {
                return Ok(None);
            };
            std::fs::File::options()
                .write(true)
                .open(root.join(path))
                .and_then(|file| file.set_modified(std::time::SystemTime::now()))
                .map_err(|error| format!("touch {} failed: {error}", path.display()))?;
            Ok(Some(format!("touch {}", relative_text(path))))
        }
        7 => {
            let Some(path) = rng.pick(&sources) else {
                return Ok(None);
            };
            let relative = relative_text(path);
            // Fails for a file the base does not have; that edit is skipped.
            if git(root, &["checkout", "--", &relative]).is_err() {
                return Ok(None);
            }
            Ok(Some(format!("revert {relative}")))
        }
        8 => {
            git(root, &["add", "-A"])?;
            // Nothing staged is a failed commit, not a failed step.
            if git(root, &["commit", "-q", "-m", "step"]).is_err() {
                return Ok(None);
            }
            Ok(Some("commit all".to_string()))
        }
        9 => {
            git(root, &["add", "-A"])?;
            Ok(Some("stage all".to_string()))
        }
        10 => {
            let Some(path) = rng.pick(&tests) else {
                return Ok(None);
            };
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                return Ok(None);
            };
            let copy = path.with_extension(format!("copy{}.{extension}", rng.below(1000)));
            store(&copy, &read(path)?)?;
            git(root, &["add", "-N", &relative_text(&copy)])?;
            Ok(Some(format!("add test file {}", relative_text(&copy))))
        }
        11 => {
            // Keep at least one test file so later edits have a target.
            if tests.len() < 2 {
                return Ok(None);
            }
            let Some(path) = rng.pick(&tests) else {
                return Ok(None);
            };
            if rng.below(2) == 0 {
                std::fs::remove_file(root.join(path))
                    .map_err(|error| format!("delete {} failed: {error}", path.display()))?;
                return Ok(Some(format!("delete test file {}", relative_text(path))));
            }
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                return Ok(None);
            };
            let renamed = path.with_extension(format!("moved{}.{extension}", rng.below(1000)));
            std::fs::rename(root.join(path), root.join(&renamed))
                .map_err(|error| format!("rename {} failed: {error}", path.display()))?;
            Ok(Some(format!(
                "rename test file {} -> {}",
                relative_text(path),
                relative_text(&renamed)
            )))
        }
        12 => {
            let manifest = Path::new("Cargo.toml");
            if !root.join(manifest).is_file() {
                return Ok(None);
            }
            let text = read(manifest)?;
            store(manifest, &format!("{text}\n# step {}\n", rng.below(1000)))?;
            Ok(Some("manifest comment".to_string()))
        }
        _ => {
            let mode = ["instant", "draft", "deep"]
                .get(rng.below(3))
                .copied()
                .unwrap_or("draft");
            write(
                root,
                "ripr.toml",
                &format!("[analysis]\nmode = \"{mode}\"\n"),
            )?;
            Ok(Some(format!("ripr.toml mode = {mode}")))
        }
    }
}

fn check_command(root: &Path, cache: &Path, format: &str, verbose: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    if verbose {
        command.arg("-v");
    }
    command
        .arg("check")
        .arg("--root")
        .arg(root)
        .args(["--format", format, "--quiet"])
        .current_dir(root)
        .env("RIPR_CACHE_DIR", cache)
        .env("RIPR_PROGRESS", "0")
        .env_remove("RIPR_VERBOSE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// What one run answered: the exit code and the comparable document.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Answer {
    code: Option<i32>,
    document: String,
}

fn strip_volatile(value: &mut Value) {
    match value {
        Value::Object(members) => {
            for name in VOLATILE_MEMBERS {
                members.remove(name);
            }
            members.values_mut().for_each(strip_volatile);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_volatile),
        _ => {}
    }
}

fn answer(output: &Output) -> Answer {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let document = match serde_json::from_str::<Value>(&stdout) {
        Ok(mut value) => {
            strip_volatile(&mut value);
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| stdout.into_owned())
        }
        // A refusal prints no document; the raw text is still compared.
        Err(_) => stdout.into_owned(),
    };
    Answer {
        code: output.status.code(),
        document,
    }
}

fn run(root: &Path, cache: &Path, format: &str) -> Result<Answer, String> {
    check_command(root, cache, format, false)
        .output()
        .map(|output| answer(&output))
        .map_err(|error| format!("start ripr check --format {format} failed: {error}"))
}

/// The first line on which two documents differ, for a readable failure.
fn first_difference(cold: &Answer, other: &Answer) -> String {
    if cold.code != other.code {
        return format!("exit code {:?} (cold) against {:?}", cold.code, other.code);
    }
    let mut line = 1usize;
    let mut left = cold.document.lines();
    let mut right = other.document.lines();
    loop {
        match (left.next(), right.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (None, None) => return "no difference".to_string(),
            (a, b) => {
                return format!(
                    "line {line}: cold `{}` against `{}`",
                    a.unwrap_or("<end>").trim(),
                    b.unwrap_or("<end>").trim()
                );
            }
        }
    }
}

fn require_equal(
    cold: &Answer,
    other: &Answer,
    what: &str,
    format: &str,
    seed: u64,
    log: &[String],
) -> Result<(), String> {
    if cold == other {
        return Ok(());
    }
    Err(format!(
        "{what} run differs from a cold run for --format {format}: {}\nseed {seed:#x}; edits so far:\n  {}",
        first_difference(cold, other),
        log.join("\n  ")
    ))
}

/// What a sequence observed, so a case can refuse to pass on empty evidence.
#[derive(Default)]
struct Observed {
    comparisons: usize,
    most_findings: usize,
    most_seams: usize,
    completed_runs: usize,
}

fn count_array(document: &str, member: &str) -> usize {
    serde_json::from_str::<Value>(document)
        .ok()
        .and_then(|value| value.get(member).and_then(Value::as_array).map(Vec::len))
        .unwrap_or(0)
}

/// Walk `steps` edits from `seed`; after each, a warm run on the carried
/// cache must answer exactly as a cold run does.
fn run_sequence(scratch: &mut Scratch, seed: u64, steps: usize) -> Result<Observed, String> {
    let root = scratch.root();
    let warm = scratch.fresh_cache();
    let mut rng = Rng(seed);
    let mut log = Vec::new();
    let mut observed = Observed::default();
    for step in 0..steps {
        // A draw that had nothing to act on is retried a bounded number of
        // times; a step with no edit still compares warm against cold.
        for _ in 0..8 {
            // The first step always moves a production comparison, so every
            // sequence compares at least one state that has a finding.
            let kind = if step == 0 { 0 } else { rng.below(14) };
            if let Some(line) = apply_edit(&root, &mut rng, kind)? {
                log.push(line);
                break;
            }
        }
        for format in FORMATS {
            let cold_cache = scratch.fresh_cache();
            let cold = run(&root, &cold_cache, format)?;
            let _ = std::fs::remove_dir_all(&cold_cache);
            let warm_answer = run(&root, &warm, format)?;
            require_equal(&cold, &warm_answer, "warm", format, seed, &log)?;
            observed.comparisons += 1;
            if cold.code == Some(0) {
                observed.completed_runs += 1;
            }
            observed.most_findings = observed
                .most_findings
                .max(count_array(&cold.document, "findings"));
            observed.most_seams = observed
                .most_seams
                .max(count_array(&cold.document, "seams"));
        }
    }
    Ok(observed)
}

fn require_evidence(observed: &Observed, label: &str, need_seams: bool) -> Result<(), String> {
    if observed.comparisons == 0 || observed.completed_runs == 0 {
        return Err(format!(
            "{label}: no completed run was compared ({} comparisons)",
            observed.comparisons
        ));
    }
    if observed.most_findings == 0 {
        return Err(format!(
            "{label}: no step reported a finding, so the diff route was compared on empty documents"
        ));
    }
    if need_seams && observed.most_seams == 0 {
        return Err(format!(
            "{label}: no step reported a seam, so the repository route was compared on empty documents"
        ));
    }
    Ok(())
}

fn generated_subject(label: &str) -> Result<Scratch, String> {
    let scratch = Scratch::create(label)?;
    generated_rust_workspace(&scratch.root(), 4)?;
    commit_base(&scratch.root())?;
    Ok(scratch)
}

fn example_subject(example: &str) -> Result<Scratch, String> {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(example)
        .join("project");
    if !project.is_dir() {
        return Err(format!("example project {} is missing", project.display()));
    }
    let scratch = Scratch::create(example)?;
    copy_tree(&project, &scratch.root())?;
    commit_base(&scratch.root())?;
    Ok(scratch)
}

#[test]
fn warm_runs_match_cold_runs_over_generated_edit_sequences() -> Result<(), String> {
    for offset in 0..3u64 {
        let seed = seed().wrapping_add(offset);
        let mut scratch = generated_subject("generated")?;
        let observed = run_sequence(&mut scratch, seed, steps())?;
        require_evidence(
            &observed,
            &format!("generated workspace, seed {seed:#x}"),
            true,
        )?;
    }
    Ok(())
}

#[test]
fn warm_runs_match_cold_runs_on_the_shipped_example_projects() -> Result<(), String> {
    for (example, need_seams) in [
        ("rust-discount-threshold", true),
        ("python-bulk-discount", false),
        ("typescript-discount-threshold", false),
    ] {
        let mut scratch = example_subject(example)?;
        let observed = run_sequence(&mut scratch, seed(), steps())?;
        require_evidence(&observed, example, need_seams)?;
    }
    Ok(())
}

/// Opt-in: the same sequence over checkouts named by the environment. Listed
/// as ignored so an unset variable cannot pass as a run with zero subjects.
#[test]
#[ignore = "set RIPR_CACHE_DIFFERENTIAL_SUBJECTS to one or more checkout paths"]
fn warm_runs_match_cold_runs_on_named_checkouts() -> Result<(), String> {
    let subjects = std::env::var_os("RIPR_CACHE_DIFFERENTIAL_SUBJECTS")
        .ok_or_else(|| "RIPR_CACHE_DIFFERENTIAL_SUBJECTS is not set".to_string())?;
    let subjects: Vec<PathBuf> = std::env::split_paths(&subjects)
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    if subjects.is_empty() {
        return Err("RIPR_CACHE_DIFFERENTIAL_SUBJECTS names no checkout".to_string());
    }
    for (index, subject) in subjects.iter().enumerate() {
        if !subject.is_dir() {
            return Err(format!("{} is not a directory", subject.display()));
        }
        let mut scratch = Scratch::create(&format!("named-{index}"))?;
        copy_tree(subject, &scratch.root())?;
        commit_base(&scratch.root())?;
        let observed = run_sequence(&mut scratch, seed(), steps())?;
        require_evidence(&observed, &subject.display().to_string(), false)?;
    }
    Ok(())
}

/// Several processes filling and reading one cache directory at once: each
/// must still answer as a cold run does, on a fresh cache and on a warm one.
#[test]
fn concurrent_runs_on_one_cache_match_a_cold_run() -> Result<(), String> {
    const PROCESSES: usize = 3;
    let mut scratch = generated_subject("concurrent")?;
    let root = scratch.root();
    let seed = seed();
    let mut rng = Rng(seed);
    let mut log = Vec::new();
    let shared = scratch.fresh_cache();
    let mut findings = 0;
    for step in 0..steps().min(4) {
        for _ in 0..8 {
            let kind = if step == 0 { 0 } else { rng.below(14) };
            if let Some(line) = apply_edit(&root, &mut rng, kind)? {
                log.push(line);
                break;
            }
        }
        for format in FORMATS {
            let cold_cache = scratch.fresh_cache();
            let cold = run(&root, &cold_cache, format)?;
            findings = findings.max(count_array(&cold.document, "findings"));
            let children = (0..PROCESSES)
                .map(|_| {
                    check_command(&root, &shared, format, false)
                        .spawn()
                        .map_err(|error| format!("start concurrent ripr failed: {error}"))
                })
                .collect::<Result<Vec<_>, String>>()?;
            for (index, child) in children.into_iter().enumerate() {
                let output = child
                    .wait_with_output()
                    .map_err(|error| format!("wait for concurrent ripr failed: {error}"))?;
                require_equal(
                    &cold,
                    &answer(&output),
                    &format!("concurrent ({} of {PROCESSES})", index + 1),
                    format,
                    seed,
                    &log,
                )?;
            }
        }
    }
    if findings == 0 {
        return Err(
            "no step reported a finding; the concurrent runs compared empty documents".into(),
        );
    }
    Ok(())
}

/// A file rewritten and then restored to its exact bytes while a run is in
/// flight. Whatever that run read, nothing it stored may be served for the
/// restored workspace: the next warm run must equal a cold run.
#[test]
fn edits_made_during_a_run_do_not_poison_the_cache() -> Result<(), String> {
    let mut scratch = generated_subject("mid-run")?;
    let root = scratch.root();
    // A dirty tree, so the diff route has findings to get wrong.
    write(
        &root,
        "src/part0.rs",
        &std::fs::read_to_string(root.join("src/part0.rs"))
            .map_err(|error| format!("read src/part0.rs failed: {error}"))?
            .replace(" > ", " >= "),
    )?;
    let seed = seed();
    let mut rng = Rng(seed);
    let warm = scratch.fresh_cache();
    let sources = source_files(&root)?;
    let mut findings = 0;
    for format in FORMATS {
        for round in 0..4u64 {
            let Some(victim) = rng.pick(&sources).cloned() else {
                return Err("fixture: the generated workspace has no source file".to_string());
            };
            let absolute = root.join(&victim);
            let original = std::fs::read_to_string(&absolute)
                .map_err(|error| format!("read {} failed: {error}", victim.display()))?;
            let transient = swap_operator(&original, &mut rng, false)
                .map(|(text, _)| text)
                .unwrap_or_else(|| format!("{original}\n"));
            let delay = Duration::from_millis(5 + 20 * round);
            let child = check_command(&root, &warm, format, false)
                .spawn()
                .map_err(|error| format!("start ripr failed: {error}"))?;
            std::thread::sleep(delay);
            std::fs::write(&absolute, &transient)
                .map_err(|error| format!("write {} failed: {error}", victim.display()))?;
            std::thread::sleep(delay);
            std::fs::write(&absolute, &original)
                .map_err(|error| format!("restore {} failed: {error}", victim.display()))?;
            child
                .wait_with_output()
                .map_err(|error| format!("wait for ripr failed: {error}"))?;
            // The racing run's own answer is not compared: it may have read
            // either version. The workspace is back to `original` now.
            let cold_cache = scratch.fresh_cache();
            let cold = run(&root, &cold_cache, format)?;
            let warm_answer = run(&root, &warm, format)?;
            let log = [format!(
                "round {round}: {} rewritten and restored {delay:?} into a run",
                relative_text(&victim)
            )];
            require_equal(
                &cold,
                &warm_answer,
                "warm (after a mid-run edit)",
                format,
                seed,
                &log,
            )?;
            findings = findings.max(count_array(&cold.document, "findings"));
        }
    }
    if findings == 0 {
        return Err("fixture: the dirty workspace reported no finding".to_string());
    }
    Ok(())
}

/// Control: the comparison is able to fail. Two workspace states that differ
/// by one boundary edit do not compare equal under the same normalization.
#[test]
fn the_comparison_separates_two_workspace_states() -> Result<(), String> {
    let mut scratch = generated_subject("control-states")?;
    let root = scratch.root();
    let path = root.join("src/part0.rs");
    let base = std::fs::read_to_string(&path)
        .map_err(|error| format!("read src/part0.rs failed: {error}"))?;
    if !base.contains(" > ") || !base.contains(" == ") {
        return Err(format!("fixture: part0 lost its comparisons: {base}"));
    }
    std::fs::write(&path, base.replace(" > ", " >= "))
        .map_err(|error| format!("write src/part0.rs failed: {error}"))?;
    for format in FORMATS {
        let first_cache = scratch.fresh_cache();
        let first = run(&root, &first_cache, format)?;
        if first.code != Some(0) {
            return Err(format!(
                "fixture: --format {format} exited {:?}",
                first.code
            ));
        }
        std::fs::write(&path, base.replace(" == ", " != "))
            .map_err(|error| format!("write src/part0.rs failed: {error}"))?;
        let second_cache = scratch.fresh_cache();
        let second = run(&root, &second_cache, format)?;
        if first == second {
            return Err(format!(
                "--format {format}: two different workspace states compared equal, so the comparison cannot detect a stale answer"
            ));
        }
        std::fs::write(&path, base.replace(" > ", " >= "))
            .map_err(|error| format!("write src/part0.rs failed: {error}"))?;
        // And the same state compares equal to itself across two cold runs:
        // the excluded members are the only run-to-run noise.
        let again_cache = scratch.fresh_cache();
        let again = run(&root, &again_cache, format)?;
        if first != again {
            return Err(format!(
                "--format {format}: two cold runs of one state differ: {}",
                first_difference(&first, &again)
            ));
        }
    }
    Ok(())
}

/// Control: the warm run really reuses the cache. Without this a build that
/// never read its cache would pass every comparison above.
#[test]
fn the_warm_run_reports_reuse() -> Result<(), String> {
    let mut scratch = generated_subject("control-reuse")?;
    let root = scratch.root();
    let path = root.join("src/part0.rs");
    let base = std::fs::read_to_string(&path)
        .map_err(|error| format!("read src/part0.rs failed: {error}"))?;
    std::fs::write(&path, base.replace(" > ", " >= "))
        .map_err(|error| format!("write src/part0.rs failed: {error}"))?;
    let cache = scratch.fresh_cache();
    let stderr_of = |format: &str| -> Result<String, String> {
        let output = check_command(&root, &cache, format, true)
            .output()
            .map_err(|error| format!("start ripr failed: {error}"))?;
        if output.status.code() != Some(0) {
            return Err(format!(
                "fixture: -v check --format {format} exited {:?}",
                output.status.code()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stderr).into_owned())
    };
    let reused_file_facts = |stderr: &str| -> Option<u64> {
        let line = stderr
            .lines()
            .find(|line| line.contains("rust file facts:"))?;
        let count = line.split("rust file facts:").nth(1)?.trim();
        count.split_whitespace().next()?.parse().ok()
    };

    let first = stderr_of("json")?;
    if reused_file_facts(&first) != Some(0) {
        return Err(format!(
            "a run on an empty cache must reuse no file facts:\n{first}"
        ));
    }
    let second = stderr_of("json")?;
    match reused_file_facts(&second) {
        Some(reused) if reused > 0 => {}
        other => {
            return Err(format!(
                "the second run on one cache must reuse file facts, reported {other:?}:\n{second}"
            ));
        }
    }

    const NO_ENTRY: &str = "no usable cache entry for this workspace state";
    let first = stderr_of("repo-exposure-json")?;
    if !first.contains(NO_ENTRY) {
        return Err(format!(
            "the first repository run must report a classified-seam miss:\n{first}"
        ));
    }
    let second = stderr_of("repo-exposure-json")?;
    if second.contains(NO_ENTRY) {
        return Err(format!(
            "the second repository run must reuse the classified seams:\n{second}"
        ));
    }
    Ok(())
}
