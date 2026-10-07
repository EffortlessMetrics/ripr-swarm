#![cfg(feature = "lang-rust")]

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok as run_git, fixture_git_output};
use ripr::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const STREAM_CAP: u64 = 256 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const HIDDEN_BASE: &str = "pub fn hidden(n: u32) -> bool {\n    n > 10\n}\n";
const HIDDEN_CHANGE: &str = "pub fn hidden(n: u32) -> bool {\n    n >= 10\n}\n";
const PRESENT_BASE: &str = "pub fn present(n: u32) -> bool {\n    n > 20\n}\n";
const PRESENT_CHANGE: &str = "pub fn present(n: u32) -> bool {\n    n >= 20\n}\n";

struct Fixture {
    root: PathBuf,
    scratch: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.scratch) {}
    }
}

fn fixture(label: &str, mixed: bool) -> Result<Fixture, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let scratch = std::env::temp_dir().join(format!(
        "ripr-{label}-{stamp}-{}-{counter}",
        std::process::id()
    ));
    let fixture = Fixture {
        root: scratch.join("repo"),
        scratch,
    };
    std::fs::create_dir_all(fixture.root.join("src")).map_err(|error| error.to_string())?;
    for (path, source) in [
        (
            "Cargo.toml",
            "[package]\nname = \"coverage_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        ),
        (
            "src/lib.rs",
            "pub mod y_present;\npub mod z_hidden;\npub fn first(n: u32) -> bool { n > 100 }\npub fn second(n: u32) -> bool { n > 200 }\n",
        ),
        ("src/y_present.rs", PRESENT_BASE),
        ("src/z_hidden.rs", HIDDEN_BASE),
    ] {
        std::fs::write(fixture.root.join(path), source).map_err(|error| error.to_string())?;
    }
    run_git(&fixture.root, &["init"])?;
    run_git(&fixture.root, &["config", "user.email", "ripr@example.invalid"])?;
    run_git(&fixture.root, &["config", "user.name", "RIPR Test"])?;
    run_git(&fixture.root, &["add", "."])?;
    run_git(&fixture.root, &["commit", "-m", "base"])?;
    run_git(
        &fixture.root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    )?;
    std::fs::write(fixture.root.join("src/z_hidden.rs"), HIDDEN_CHANGE)
        .map_err(|error| error.to_string())?;
    if mixed {
        std::fs::write(fixture.root.join("src/y_present.rs"), PRESENT_CHANGE)
            .map_err(|error| error.to_string())?;
    }
    run_git(&fixture.root, &["add", "."])?;
    run_git(&fixture.root, &["commit", "-m", "change"])?;
    Ok(fixture)
}

struct Run {
    stdout: String,
    stderr: String,
}

fn read_bounded(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > STREAM_CAP {
        return Err(format!(
            "captured stream exceeds 256 KiB: {}",
            path.display()
        ));
    }
    let mut bounded = file.take(STREAM_CAP);
    let mut bytes = Vec::new();
    bounded
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bounded
        .get_ref()
        .metadata()
        .map_err(|error| error.to_string())?
        .len()
        > STREAM_CAP
    {
        return Err(format!(
            "captured stream grew past 256 KiB: {}",
            path.display()
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn run_subject(fixture: &Fixture, cache: &Path, args: &[&str]) -> Result<Run, String> {
    let stdout_path = fixture.scratch.join("subject.stdout");
    let stderr_path = fixture.scratch.join("subject.stderr");
    let stdout = File::create(&stdout_path).map_err(|error| error.to_string())?;
    let stderr = File::create(&stderr_path).map_err(|error| error.to_string())?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.current_dir(&fixture.root).env_clear();
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "COMSPEC",
        "TEMP",
        "TMP",
        "TMPDIR",
        "HOME",
        "USERPROFILE",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "LLVM_PROFILE_FILE",
        "CARGO_LLVM_COV",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("RIPR_CACHE_DIR", cache)
        .env("RIPR_REPO_EXPOSURE_SEAM_LIMIT", "1")
        .env("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut child = OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let outcome = (|| {
        loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                return Ok(status);
            }
            for path in [&stdout_path, &stderr_path] {
                if std::fs::metadata(path)
                    .map_err(|error| error.to_string())?
                    .len()
                    > STREAM_CAP
                {
                    return Err(format!(
                        "subject exceeded captured stream limit: {}",
                        path.display()
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Err(format!("subject exceeded 10-second deadline: {args:?}"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    child.terminate_tree()?;
    let status = outcome?;
    let stdout = read_bounded(&stdout_path)?;
    let stderr = read_bounded(&stderr_path)?;
    if !status.success() {
        return Err(format!(
            "subject {args:?} failed ({status})\nstdout:\n{stdout}\nstderr:\n{stderr}"
        ));
    }
    Ok(Run { stdout, stderr })
}

fn read_json(path: &Path) -> Result<Value, String> {
    let source = read_bounded(path)?;
    serde_json::from_str(&source).map_err(|error| error.to_string())
}

fn limited_repo(fixture: &Fixture, cache: &Path) -> Result<Value, String> {
    let root_arg = fixture.root.display().to_string();
    let output = run_subject(
        fixture,
        cache,
        &["check", "--root", &root_arg, "--format", "repo-exposure-json"],
    )?;
    let report: Value =
        serde_json::from_str(&output.stdout).map_err(|error| error.to_string())?;
    assert_eq!(report["run_status"], "seam_limit_applied", "{report:#}");
    assert_eq!(report["limitations"][0]["seams_analyzed"], 1, "{report:#}");
    assert!(
        report["limitations"][0]["seams_total"]
            .as_u64()
            .is_some_and(|count| count > 1),
        "{report:#}"
    );
    let seams = report["seams"].as_array().ok_or("missing analyzed subjects")?;
    assert_eq!(seams.len(), 1, "{report:#}");
    assert_eq!(
        seams[0]["file"], "src/lib.rs",
        "fixture cap must cut the changed files: {report:#}"
    );
    Ok(json!({
        "run_status": report["run_status"],
        "limitations": report["limitations"],
        "metrics": report["metrics"],
        "seams": report["seams"],
    }))
}

struct Pilot {
    stdout: String,
    stderr: String,
    md: String,
    summary: Value,
}

fn pilot(fixture: &Fixture, cache: &Path, files: usize) -> Result<Pilot, String> {
    let root_arg = fixture.root.display().to_string();
    let out_dir = fixture.scratch.join("out");
    let out_arg = out_dir.display().to_string();
    let output = run_subject(
        fixture,
        cache,
        &[
            "pilot",
            "--root",
            &root_arg,
            "--out",
            &out_arg,
            "--timeout-ms",
            "5000",
        ],
    )?;
    assert!(
        output.stderr.contains(&format!(
            "current change's {files} Rust file(s) on their own"
        )),
        "supplement branch not observed: {}",
        output.stderr
    );
    assert!(
        !output
            .stderr
            .contains("could not classify the current change"),
        "supplement failed: {}",
        output.stderr
    );
    let summary = read_json(&out_dir.join("pilot-summary.json"))?;
    assert_eq!(summary["current_change"]["state"], "changed", "{summary:#}");
    assert_eq!(summary["current_change"]["base"], "origin/main", "{summary:#}");
    let md = read_bounded(&out_dir.join("pilot-summary.md"))?;
    assert!(
        md.contains("- Seam limit reached: ranked "),
        "outer limit not observed: {md}"
    );
    Ok(Pilot {
        stdout: output.stdout,
        stderr: output.stderr,
        md,
        summary,
    })
}

fn committed_identity(root: &Path) -> Result<(String, String), String> {
    assert!(
        fixture_git_output(root, &["status", "--porcelain"])?
            .trim()
            .is_empty(),
        "fixture must have no tracked or untracked work"
    );
    Ok((
        fixture_git_output(root, &["rev-parse", "HEAD"])?,
        fixture_git_output(root, &["diff", "--name-only", "origin/main...HEAD"])?,
    ))
}

fn semantic_summary(pilot: &Pilot) -> Value {
    json!({
        "current_change": pilot.summary["current_change"],
        "top_actionable_seams": pilot.summary["top_actionable_seams"],
        "actionable_seams_total": pilot.summary["actionable_seams_total"],
        "withheld_static_limitations_total": pilot.summary["withheld_static_limitations_total"],
    })
}

fn assert_absent(pilot: &Pilot) {
    assert_eq!(
        pilot.summary["current_change"]["absent_changed_files"],
        json!(["src/z_hidden.rs"]),
        "{:#}",
        pilot.summary
    );
    for text in [&pilot.stdout, &pilot.md] {
        assert!(
            text.lines().any(|line| {
                line.contains("changed_file_absent_from_worktree")
                    && line.contains("src/z_hidden.rs")
            }),
            "a named missing-file disclosure is required on both user surfaces: {text}"
        );
    }
}

fn assert_restored(pilot: &Pilot) -> Result<(), String> {
    assert_eq!(
        pilot.summary["current_change"]["absent_changed_files"],
        json!([]),
        "{:#}",
        pilot.summary
    );
    assert_eq!(
        pilot.summary["current_change"]["top_recommendation_in_change"], true,
        "{:#}",
        pilot.summary
    );
    let top = pilot.summary["top_actionable_seams"]
        .as_array()
        .ok_or("missing ranked subjects")?;
    assert!(
        top.iter()
            .any(|seam| seam["file"] == "src/z_hidden.rs" && seam["line"] == 2),
        "the exact restored changed predicate must be ranked: {:#}",
        pilot.summary
    );
    for text in [&pilot.stdout, &pilot.md] {
        assert!(
            !text.contains("changed_file_absent_from_worktree"),
            "stale absence disclosure after restore: {text}"
        );
        assert!(
            !text.contains("may have seams pilot did not see"),
            "successful full change recovery must clear change uncertainty: {text}"
        );
    }
    Ok(())
}

fn assert_cache_hit(pilot: &Pilot) {
    assert!(
        pilot
            .stderr
            .contains("ripr_repo_exposure_latency phase=cache_load status=hit "),
        "same-cache run did not establish a classified-cache hit: {}",
        pilot.stderr
    );
}

fn exercise_missing_coverage(label: &str, mixed: bool) -> Result<(), String> {
    let fixture = fixture(label, mixed)?;
    let expected_files = if mixed {
        "src/y_present.rs\nsrc/z_hidden.rs\n"
    } else {
        "src/z_hidden.rs\n"
    };
    let before_sparse = committed_identity(&fixture.root)?;
    assert_eq!(before_sparse.1.replace("\r\n", "\n"), expected_files);
    run_git(&fixture.root, &["sparse-checkout", "init", "--no-cone"])?;
    run_git(
        &fixture.root,
        &["sparse-checkout", "set", "--no-cone", "/*", "!/src/z_hidden.rs"],
    )?;
    assert!(
        !fixture.root.join("src/z_hidden.rs").exists(),
        "sparse setup did not omit the changed file"
    );
    assert!(fixture.root.join("Cargo.toml").is_file());
    assert!(fixture.root.join("src/lib.rs").is_file());
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("src/y_present.rs"))
            .map_err(|error| error.to_string())?,
        if mixed { PRESENT_CHANGE } else { PRESENT_BASE }
    );
    assert_eq!(
        committed_identity(&fixture.root)?,
        before_sparse,
        "sparse checkout must keep the exact committed diff"
    );
    let cache = fixture.scratch.join("cache");
    let files = if mixed { 2 } else { 1 };
    let repo_before = limited_repo(&fixture, &cache)?;
    let sparse = pilot(&fixture, &cache, files)?;
    assert_absent(&sparse);
    if mixed {
        assert_eq!(
            sparse.summary["current_change"]["top_recommendation_in_change"], true,
            "{:#}",
            sparse.summary
        );
        assert_eq!(
            sparse.summary["top_actionable_seams"][0]["file"], "src/y_present.rs",
            "{:#}",
            sparse.summary
        );
        assert_eq!(
            sparse.summary["top_actionable_seams"][0]["line"], 2,
            "{:#}",
            sparse.summary
        );
    } else {
        assert_eq!(
            sparse.summary["current_change"]["actionable_seams_in_change"], 0,
            "{:#}",
            sparse.summary
        );
        assert_eq!(
            sparse.summary["current_change"]["top_recommendation_in_change"], false,
            "{:#}",
            sparse.summary
        );
    }
    let warm_sparse = pilot(&fixture, &cache, files)?;
    assert_absent(&warm_sparse);
    assert_cache_hit(&warm_sparse);
    assert_eq!(
        semantic_summary(&warm_sparse),
        semantic_summary(&sparse),
        "same-cache fresh-process result drifted"
    );
    let cold_sparse = pilot(&fixture, &fixture.scratch.join("cold-sparse-cache"), files)?;
    assert_absent(&cold_sparse);
    assert_eq!(
        semantic_summary(&cold_sparse),
        semantic_summary(&sparse),
        "cold and same-cache absence results differ"
    );
    assert_eq!(
        limited_repo(&fixture, &cache)?,
        repo_before,
        "pilot's diff supplement polluted the diff-independent classified cache"
    );
    run_git(&fixture.root, &["sparse-checkout", "disable"])?;
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("src/z_hidden.rs"))
            .map_err(|error| error.to_string())?,
        HIDDEN_CHANGE
    );
    assert_eq!(
        committed_identity(&fixture.root)?,
        before_sparse,
        "restore changed HEAD or committed diff"
    );
    let repo_restored_before = limited_repo(&fixture, &cache)?;
    let restored = pilot(&fixture, &cache, files)?;
    assert_restored(&restored)?;
    let restored_warm = pilot(&fixture, &cache, files)?;
    assert_restored(&restored_warm)?;
    assert_cache_hit(&restored_warm);
    assert_eq!(
        semantic_summary(&restored_warm),
        semantic_summary(&restored)
    );
    let restored_cold = pilot(
        &fixture,
        &fixture.scratch.join("cold-restored-cache"),
        files,
    )?;
    assert_restored(&restored_cold)?;
    assert_eq!(
        semantic_summary(&restored_cold),
        semantic_summary(&restored)
    );
    assert_eq!(
        limited_repo(&fixture, &cache)?,
        repo_restored_before,
        "restored supplement polluted classified cache"
    );
    Ok(())
}

#[test]
fn pilot_discloses_sparse_missing_changed_file_and_recovers_same_head() -> Result<(), String> {
    exercise_missing_coverage("pilot-sparse-changed-coverage", false)
}

#[test]
fn pilot_discloses_mixed_missing_changed_file_even_with_changed_recommendation() -> Result<(), String> {
    exercise_missing_coverage("pilot-mixed-changed-coverage", true)
}
