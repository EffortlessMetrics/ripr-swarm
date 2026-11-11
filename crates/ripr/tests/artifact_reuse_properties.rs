//! Check-artifact reuse properties over generated workspaces (RIPR-SPEC-0140).
//!
//! `ripr check --write-artifact PATH` records a run so that a later
//! `ripr explain --from PATH` or `ripr context --from PATH` can answer from the
//! recorded findings instead of analyzing again. The hand-written tests pin
//! that round trip on the sample fixture and on single findings. This suite
//! generates small Rust workspaces with several changed comparisons and
//! related tests, runs the built binary, and asserts for every finding the
//! check reported:
//!
//! 1. `context --from` prints the same packet as a fresh `context --diff`;
//! 2. `explain --from` prints the same text as a fresh `explain --diff`;
//! 3. the follow-up command each output prints names the source it was given
//!    (`--from` stays `--from`, `--diff` stays `--diff`), and is the only
//!    place the two outputs differ;
//! 4. the `explain` command a reused context packet prints runs as printed
//!    and gives the reused explain output, so the printed next step is a
//!    real one.
//!
//! Every case first asserts its fixture: the check reports exactly one
//! predicate finding per generated change, the artifact file exists, and
//! every reported finding id is selected by both routes. Two controls show
//! the comparison is able to fail: a different finding's output is not
//! accepted as equal, and an artifact reused against a changed diff is
//! refused rather than answered. These are static classes; nothing here
//! claims a runtime mutation outcome.
//!
//! Each case starts the binary several times per finding, so the default case
//! count is small. Raise it locally with `PROPTEST_CASES=100 cargo test -p
//! ripr --test artifact_reuse_properties`.

use proptest::prelude::*;
use proptest::test_runner::Config;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_CASES: u32 = 4;
const CRATE: &str = "artifact_reuse_subject";

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

/// One changed comparison in its own module file.
#[derive(Clone, Debug)]
struct Module {
    threshold: u64,
    /// Whether a test pins the boundary value for this module.
    pinned: bool,
    /// Further tests that call the function away from the boundary.
    extra_tests: u8,
}

/// A generated workspace: one to three module files, each with one changed
/// comparison, and one integration test file.
#[derive(Clone, Debug)]
struct Subject {
    modules: Vec<Module>,
}

impl Subject {
    fn source(index: usize, module: &Module, changed: bool) -> String {
        let op = if changed { ">=" } else { ">" };
        format!(
            "pub fn gate{index}(amount: u64) -> bool {{\n    amount {op} {}\n}}\n",
            module.threshold
        )
    }

    fn diff(&self) -> String {
        let mut diff = String::new();
        for (index, module) in self.modules.iter().enumerate() {
            let path = format!("src/part{index}.rs");
            diff.push_str(&format!(
                "diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n pub fn gate{index}(amount: u64) -> bool {{\n-    amount > {t}\n+    amount >= {t}\n }}\n",
                t = module.threshold
            ));
        }
        diff
    }

    fn files(&self) -> Vec<(String, String)> {
        let mut lib = String::new();
        let mut tests = format!("use {CRATE}::*;\n");
        let mut files = Vec::new();
        for (index, module) in self.modules.iter().enumerate() {
            lib.push_str(&format!("mod part{index};\npub use part{index}::*;\n"));
            let value = if module.pinned {
                module.threshold
            } else {
                module.threshold + 7
            };
            tests.push_str(&format!(
                "\n#[test]\nfn gate{index}_checked() {{\n    assert!(gate{index}({value}));\n}}\n"
            ));
            for extra in 0..module.extra_tests {
                tests.push_str(&format!(
                    "\n#[test]\nfn gate{index}_extra_{extra}() {{\n    assert!(gate{index}({}));\n}}\n",
                    module.threshold + 100 + u64::from(extra)
                ));
            }
            files.push((
                format!("src/part{index}.rs"),
                Self::source(index, module, true),
            ));
        }
        files.push(("src/lib.rs".to_string(), lib));
        files.push(("tests/case.rs".to_string(), tests));
        files.push((
            "Cargo.toml".to_string(),
            "[package]\nname = \"artifact-reuse-subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n"
                .to_string(),
        ));
        files.push(("change.diff".to_string(), self.diff()));
        files
    }
}

fn subject() -> impl Strategy<Value = Subject> {
    prop::collection::vec((2u64..500, any::<bool>(), 0u8..3), 1..4).prop_map(|modules| Subject {
        modules: modules
            .into_iter()
            .map(|(threshold, pinned, extra_tests)| Module {
                threshold,
                pinned,
                extra_tests,
            })
            .collect(),
    })
}

/// What one run of the binary printed.
struct Ran {
    success: bool,
    stdout: String,
    stderr: String,
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(files: &[(String, String)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-fuzz-artifact-reuse-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self { root };
        for (name, content) in files {
            repo.write(name, content)?;
        }
        Ok(repo)
    }

    fn write(&self, name: &str, content: &str) -> Result<(), String> {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
        }
        std::fs::write(&path, content)
            .map_err(|error| format!("write {} failed: {error}", path.display()))
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).display().to_string()
    }

    /// Runs the built binary. Progress lines are switched off so stdout and
    /// stderr carry only the command's own output.
    fn ripr(&self, args: &[&str]) -> Result<Ran, String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(args)
            .current_dir(&self.root)
            .env("RIPR_PROGRESS", "0")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|error| format!("start ripr {args:?} failed: {error}"))?;
        Ok(Ran {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn ripr_ok(&self, args: &[&str]) -> Result<String, String> {
        let ran = self.ripr(args)?;
        if !ran.success {
            return Err(format!("ripr {args:?} did not succeed: {}", ran.stderr));
        }
        Ok(ran.stdout)
    }

    /// The source flags for a fresh run and for a reused run.
    fn fresh(&self) -> [String; 2] {
        ["--diff".to_string(), self.path("change.diff")]
    }

    fn reused(&self) -> [String; 2] {
        ["--from".to_string(), self.path("check-artifact.json")]
    }

    fn context(&self, source: &[String; 2], id: &str) -> Result<String, String> {
        let root = self.path("");
        self.ripr_ok(&[
            "context", "--root", &root, &source[0], &source[1], "--mode", "ready", "--at", id,
            "--json",
        ])
    }

    fn explain(&self, source: &[String; 2], id: &str) -> Result<String, String> {
        let root = self.path("");
        self.ripr_ok(&[
            "explain", "--root", &root, &source[0], &source[1], "--mode", "ready", id,
        ])
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A checked workspace with a written artifact and the reported finding ids.
struct Checked {
    repo: TempRepo,
    ids: Vec<String>,
}

/// Runs `check --write-artifact` and asserts the fixture: one predicate
/// finding per generated change, and an artifact on disk.
fn checked(subject: &Subject) -> Result<Checked, String> {
    let repo = TempRepo::create(&subject.files())?;
    let root = repo.path("");
    let [flag, diff] = repo.fresh();
    let [_, artifact] = repo.reused();
    let stdout = repo.ripr_ok(&[
        "check",
        "--root",
        &root,
        &flag,
        &diff,
        "--mode",
        "ready",
        "--format",
        "json",
        "--write-artifact",
        &artifact,
    ])?;
    let report: Value = serde_json::from_str(&stdout)
        .map_err(|error| format!("check JSON did not parse: {error}"))?;
    let findings = report
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "check JSON has no findings array".to_string())?;
    let mut ids = Vec::new();
    for finding in findings {
        let id = finding
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("finding has no id: {finding}"))?;
        ids.push(id.to_string());
    }
    for index in 0..subject.modules.len() {
        let wanted = format!("probe:src_part{index}.rs:predicate:");
        let count = ids.iter().filter(|id| id.starts_with(&wanted)).count();
        if count != 1 {
            return Err(format!(
                "fixture produced {count} predicate findings for part{index}: {ids:?}"
            ));
        }
    }
    if !Path::new(&artifact).is_file() {
        return Err(format!("check wrote no artifact at {artifact}"));
    }
    Ok(Checked { repo, ids })
}

/// Rewrites the one place a reused output names its source so it can be
/// compared with the fresh output, and asserts each output names its own
/// source exactly `expected` times and never the other one.
fn normalized(
    repo: &TempRepo,
    fresh: &str,
    reused: &str,
    expected: usize,
) -> Result<(String, String), String> {
    let [diff_flag, diff] = repo.fresh();
    let [from_flag, artifact] = repo.reused();
    let fresh_source = format!("{diff_flag} {diff}");
    let reused_source = format!("{from_flag} {artifact}");
    if fresh.matches(&fresh_source).count() != expected || fresh.contains(&reused_source) {
        return Err(format!(
            "fresh output must name `{fresh_source}` {expected} time(s) and never the artifact:\n{fresh}"
        ));
    }
    if reused.matches(&reused_source).count() != expected || reused.contains(&fresh_source) {
        return Err(format!(
            "reused output must name `{reused_source}` {expected} time(s) and never the diff:\n{reused}"
        ));
    }
    Ok((
        fresh.to_string(),
        reused.replace(&reused_source, &fresh_source),
    ))
}

/// The `explain` command a context packet prints for its finding.
fn printed_explain_command(packet: &str) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_str(packet)
        .map_err(|error| format!("context packet did not parse: {error}"))?;
    let command = value
        .pointer("/witness/explain_command")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("context packet prints no explain command: {packet}"))?;
    let mut words: Vec<String> = command.split_whitespace().map(str::to_string).collect();
    if words.first().map(String::as_str) != Some("ripr") {
        return Err(format!(
            "printed command does not start with ripr: {command}"
        ));
    }
    words.remove(0);
    Ok(words)
}

fn fail(message: String) -> TestCaseError {
    TestCaseError::fail(message)
}

proptest! {
    #![proptest_config(config())]

    /// Relations 1 and 3 for `context`: every reported finding reads back
    /// from the artifact as the same packet a fresh run prints.
    #[test]
    fn context_from_the_artifact_matches_a_fresh_run(subject in subject()) {
        let run = checked(&subject).map_err(fail)?;
        for id in &run.ids {
            let fresh = run.repo.context(&run.repo.fresh(), id).map_err(fail)?;
            let reused = run.repo.context(&run.repo.reused(), id).map_err(fail)?;
            let (fresh, reused) = normalized(&run.repo, &fresh, &reused, 1).map_err(fail)?;
            prop_assert_eq!(fresh, reused, "context for {} differs for {:?}", id, subject);
        }
    }

    /// Relations 2 and 3 for `explain`.
    #[test]
    fn explain_from_the_artifact_matches_a_fresh_run(subject in subject()) {
        let run = checked(&subject).map_err(fail)?;
        for id in &run.ids {
            let fresh = run.repo.explain(&run.repo.fresh(), id).map_err(fail)?;
            let reused = run.repo.explain(&run.repo.reused(), id).map_err(fail)?;
            let (fresh, reused) = normalized(&run.repo, &fresh, &reused, 1).map_err(fail)?;
            prop_assert_eq!(fresh, reused, "explain for {} differs for {:?}", id, subject);
        }
    }

    /// Relation 4: the explain command a reused context packet prints runs
    /// as printed and gives the reused explain output.
    #[test]
    fn the_printed_explain_command_runs_as_printed(subject in subject()) {
        let run = checked(&subject).map_err(fail)?;
        for id in &run.ids {
            let packet = run.repo.context(&run.repo.reused(), id).map_err(fail)?;
            let words = printed_explain_command(&packet).map_err(fail)?;
            let args: Vec<&str> = words.iter().map(String::as_str).collect();
            let printed = run.repo.ripr_ok(&args).map_err(fail)?;
            let direct = run.repo.explain(&run.repo.reused(), id).map_err(fail)?;
            prop_assert_eq!(printed, direct, "printed command for {} differs for {:?}", id, subject);
        }
    }
}

fn control_subject() -> Subject {
    Subject {
        modules: vec![
            Module {
                threshold: 18,
                pinned: true,
                extra_tests: 1,
            },
            Module {
                threshold: 40,
                pinned: false,
                extra_tests: 0,
            },
        ],
    }
}

/// Control: the comparison is able to fail. Two different findings do not
/// compare equal through the same normalization the relations use.
#[test]
fn a_different_finding_does_not_compare_equal() -> Result<(), String> {
    let run = checked(&control_subject())?;
    let (first, second) = match run.ids.as_slice() {
        [first, second, ..] => (first, second),
        _ => return Err(format!("control needs two findings: {:?}", run.ids)),
    };
    let fresh = run.repo.context(&run.repo.fresh(), first)?;
    let reused = run.repo.context(&run.repo.reused(), second)?;
    let (fresh, reused) = normalized(&run.repo, &fresh, &reused, 1)?;
    if fresh == reused {
        return Err("two different findings compared equal".to_string());
    }
    Ok(())
}

/// Control: reuse is bound to the recorded diff. After the diff file changes,
/// `--from` refuses instead of answering from the stale artifact.
#[test]
fn a_changed_diff_is_refused_not_answered() -> Result<(), String> {
    let subject = control_subject();
    let run = checked(&subject)?;
    let id = run
        .ids
        .first()
        .ok_or_else(|| "control produced no finding".to_string())?;
    let mut diff = subject.diff();
    diff.push_str("diff --git a/notes.md b/notes.md\nindex 3333333..4444444 100644\n--- a/notes.md\n+++ b/notes.md\n@@ -1,1 +1,2 @@\n first line\n+second line\n");
    run.repo.write("change.diff", &diff)?;
    let root = run.repo.path("");
    let [flag, artifact] = run.repo.reused();
    let ran = run.repo.ripr(&[
        "context", "--root", &root, &flag, &artifact, "--mode", "ready", "--at", id, "--json",
    ])?;
    if ran.success {
        return Err(format!(
            "reuse answered from an artifact whose diff changed: {}",
            ran.stdout
        ));
    }
    Ok(())
}
