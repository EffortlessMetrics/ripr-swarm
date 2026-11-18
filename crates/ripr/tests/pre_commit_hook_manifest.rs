//! #14348: every hook in the repository-root `.pre-commit-hooks.yaml` runs its
//! `entry` through the real `ripr` binary, the way pre-commit does (from the
//! repository root, `pass_filenames: false`). A renamed or removed flag would
//! otherwise ship a hook that fails in every consumer's commit or push while
//! `check-file-policy` still passes, because that gate only sees the file.
//!
//! Each entry must exit 0 on a project with a gap (the hooks are advisory),
//! report the change it is meant to see, and keep progress lines out of the
//! hook log. The no-`--quiet` control shows the progress assertion can fail.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug)]
struct Hook {
    id: String,
    entry: String,
    stages: String,
    pass_filenames: String,
}

fn manifest() -> Result<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.pre-commit-hooks.yaml");
    std::fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))
}

/// Reads the `id`, `entry` and `stages` scalars of each top-level list item.
/// The manifest is a flat list of flat mappings, so a line reader is enough
/// and keeps the test free of a YAML dependency; an item missing either
/// `id` or `entry` is an error rather than a silent skip.
fn hooks(text: &str) -> Result<Vec<Hook>, String> {
    type Item = (Option<String>, Option<String>, String, String);
    fn finish(item: Option<Item>, hooks: &mut Vec<Hook>) -> Result<(), String> {
        if let Some((id, entry, stages, pass_filenames)) = item {
            let id = id.ok_or("hook without id")?;
            let entry = entry.ok_or_else(|| format!("hook {id} without entry"))?;
            hooks.push(Hook {
                id,
                entry,
                stages,
                pass_filenames,
            });
        }
        Ok(())
    }
    let mut hooks = Vec::new();
    let mut current: Option<Item> = None;
    for line in text.lines() {
        if line.trim_start().starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let (body, starts_item) = match line.strip_prefix("- ") {
            Some(rest) => (rest, true),
            None => (line.trim_start(), false),
        };
        if starts_item {
            finish(current.take(), &mut hooks)?;
            current = Some((None, None, String::new(), String::new()));
        }
        let item = current
            .as_mut()
            .ok_or_else(|| format!("key outside a list item: {line}"))?;
        if let Some(value) = body.strip_prefix("id: ") {
            item.0 = Some(value.trim().to_string());
        } else if let Some(value) = body.strip_prefix("entry: ") {
            item.1 = Some(value.trim().to_string());
        } else if let Some(value) = body.strip_prefix("stages: ") {
            item.2 = value.trim().to_string();
        } else if let Some(value) = body.strip_prefix("pass_filenames: ") {
            item.3 = value.trim().to_string();
        }
    }
    finish(current.take(), &mut hooks)?;
    Ok(hooks)
}

/// Repository selectors a Git hook or wrapper exports. A test run launched
/// from a hook must not let them point the fixture's git (or ripr) at the
/// caller's repository or index.
const GIT_SELECTORS: [&str; 4] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
];

fn without_git_selectors(command: &mut Command) -> &mut Command {
    for key in GIT_SELECTORS {
        command.env_remove(key);
    }
    command
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let status = without_git_selectors(Command::new("git").current_dir(root).args(args))
        .status()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("git {args:?} failed"))
}

const BASE_SOURCE: &str = "pub fn gate(x: i32) -> bool {\n    x > 10\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert!(super::gate(20));\n    }\n}\n";

/// A Cargo crate on branch `feat` with one committed boundary edit against
/// local `main` (what `ripr-push` sees) and one further staged boundary edit
/// against `HEAD` (what `ripr` sees at commit time).
fn project() -> Result<Scratch, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    // Create the fixture directory exclusively so a leftover from an
    // interrupted run is never reused or removed by this run's cleanup.
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let root = loop {
        let candidate = parent.join(format!(
            "ripr-pre-commit-manifest-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    let scratch = Scratch(root.clone());
    std::fs::create_dir(root.join("src")).map_err(|error| error.to_string())?;
    let excludes = root.join("empty-excludes");
    std::fs::write(&excludes, "").map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join(".gitignore"),
        "/target/\n/empty-excludes\n/no-hooks/\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/lib.rs"), BASE_SOURCE).map_err(|error| error.to_string())?;
    git(&root, &["init", "-q", "-b", "main"])?;
    git(&root, &["config", "user.email", "ripr@example.invalid"])?;
    git(&root, &["config", "user.name", "ripr tests"])?;
    git(&root, &["config", "commit.gpgsign", "false"])?;
    std::fs::create_dir(root.join("no-hooks")).map_err(|error| error.to_string())?;
    git(&root, &["config", "core.hooksPath", "no-hooks"])?;
    git(
        &root,
        &[
            "config",
            "core.excludesFile",
            excludes.to_str().ok_or("non-utf8 excludes path")?,
        ],
    )?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    git(&root, &["checkout", "-q", "-b", "feat"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        BASE_SOURCE.replace("x > 10", "x >= 10"),
    )
    .map_err(|error| error.to_string())?;
    git(&root, &["commit", "-q", "-am", "pushed change"])?;
    std::fs::write(
        root.join("src/lib.rs"),
        BASE_SOURCE.replace("x > 10", "x >= 11"),
    )
    .map_err(|error| error.to_string())?;
    git(&root, &["add", "src/lib.rs"])?;
    Ok(scratch)
}

/// Runs a hook entry as pre-commit would: split on whitespace, the first word
/// resolved from PATH (here the binary under test), from the repository root.
fn run_entry(root: &Path, entry: &str) -> Result<Output, String> {
    let mut words = entry.split_whitespace();
    let program = words.next().ok_or("empty entry")?;
    if program != "ripr" {
        return Err(format!("entry must start with `ripr`: {entry}"));
    }
    without_git_selectors(
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(root)
            .args(words),
    )
    .output()
    .map_err(|error| format!("spawn {entry}: {error}"))
}

fn hook<'a>(hooks: &'a [Hook], id: &str) -> Result<&'a Hook, String> {
    hooks
        .iter()
        .find(|hook| hook.id == id)
        .ok_or_else(|| format!("manifest has no hook `{id}`: {hooks:?}"))
}

#[test]
fn manifest_lists_the_commit_and_push_hooks_on_separate_stages_without_filenames()
-> Result<(), String> {
    let hooks = hooks(&manifest()?)?;
    let ids: Vec<&str> = hooks.iter().map(|hook| hook.id.as_str()).collect();
    assert_eq!(ids, ["ripr", "ripr-push"], "{hooks:?}");
    assert_eq!(
        hook(&hooks, "ripr")?.stages,
        "[pre-commit, pre-merge-commit, manual]"
    );
    assert_eq!(hook(&hooks, "ripr-push")?.stages, "[pre-push]");
    // Otherwise pre-commit appends every selected path to the entry, which
    // changes what `ripr check` is asked to analyze.
    for hook in &hooks {
        assert_eq!(hook.pass_filenames, "false", "{hook:?}");
    }
    Ok(())
}

#[test]
fn commit_hook_entry_runs_quietly_and_sees_the_staged_change() -> Result<(), String> {
    let hooks = hooks(&manifest()?)?;
    let scratch = project()?;
    let out = run_entry(&scratch.0, &hook(&hooks, "ripr")?.entry)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stdout}\n{stderr}");
    assert!(stdout.contains("base: HEAD"), "{stdout}");
    assert!(stdout.contains("x >= 11"), "{stdout}");
    assert!(!stderr.contains("ripr progress"), "{stderr}");
    Ok(())
}

#[test]
fn push_hook_entry_runs_quietly_and_sees_the_committed_change() -> Result<(), String> {
    let hooks = hooks(&manifest()?)?;
    let scratch = project()?;
    let out = run_entry(&scratch.0, &hook(&hooks, "ripr-push")?.entry)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stdout}\n{stderr}");
    assert!(stdout.contains("base: main"), "{stdout}");
    assert!(stdout.contains("x >= 10"), "{stdout}");
    assert!(
        !stdout.contains("x >= 11"),
        "staged edit is not pushed: {stdout}"
    );
    assert!(!stderr.contains("ripr progress"), "{stderr}");
    Ok(())
}

#[test]
fn without_quiet_the_same_entry_writes_progress_lines() -> Result<(), String> {
    let hooks = hooks(&manifest()?)?;
    let entry = hook(&hooks, "ripr")?.entry.replace(" --quiet", "");
    assert!(!entry.contains("--quiet"), "{entry}");
    let scratch = project()?;
    let out = run_entry(&scratch.0, &entry)?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("ripr progress"), "{stderr}");
    Ok(())
}

#[test]
fn reader_rejects_a_hook_without_an_entry() {
    let result = hooks("- id: ripr\n  name: x\n");
    assert!(result.is_err(), "{result:?}");
}
