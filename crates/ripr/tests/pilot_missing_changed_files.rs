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
    seam_limit: Option<&'static str>,
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
        seam_limit: Some("1"),
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
    run_git(&fixture.root, &["config", "core.autocrlf", "false"])?;
    run_git(&fixture.root, &["config", "core.eol", "lf"])?;
    run_git(&fixture.root, &["config", "commit.gpgsign", "false"])?;
    run_git(
        &fixture.root,
        &["config", "user.email", "ripr@example.invalid"],
    )?;
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
        .env("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(limit) = fixture.seam_limit {
        command.env("RIPR_REPO_EXPOSURE_SEAM_LIMIT", limit);
    }
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
    let status = match (outcome, child.terminate_tree()) {
        (Ok(status), Ok(())) => status,
        (Err(primary), Ok(())) => return Err(primary),
        (Err(primary), Err(cleanup)) => {
            return Err(format!("{primary}; process cleanup also failed: {cleanup}"));
        }
        (Ok(status), Err(cleanup)) => {
            let stdout = read_bounded(&stdout_path);
            let stderr = read_bounded(&stderr_path);
            return Err(format!(
                "subject status {status}; cleanup failed: {cleanup}\nstdout: {stdout:?}\nstderr: {stderr:?}"
            ));
        }
    };
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

fn repo_inventory(fixture: &Fixture, cache: &Path) -> Result<Value, String> {
    let root_arg = fixture.root.display().to_string();
    let output = run_subject(
        fixture,
        cache,
        &[
            "check",
            "--root",
            &root_arg,
            "--format",
            "repo-exposure-json",
        ],
    )?;
    let report: Value = serde_json::from_str(&output.stdout).map_err(|error| error.to_string())?;
    let seams = report["seams"]
        .as_array()
        .ok_or("missing analyzed subjects")?;
    if fixture.seam_limit.is_some() {
        assert_eq!(report["run_status"], "seam_limit_applied", "{report:#}");
        assert_eq!(report["limitations"][0]["seams_analyzed"], 1, "{report:#}");
        assert!(
            report["limitations"][0]["seams_total"]
                .as_u64()
                .is_some_and(|count| count > 1),
            "{report:#}"
        );
        assert_eq!(seams.len(), 1, "{report:#}");
        assert_eq!(
            seams[0]["file"], "src/lib.rs",
            "fixture cap must cut the changed files: {report:#}"
        );
    } else {
        assert_eq!(report["run_status"], "complete", "{report:#}");
        assert!(
            seams.len() >= 3,
            "uncapped fixture has nonempty subjects: {report:#}"
        );
    }
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
    assert_eq!(
        output.stderr.contains(&format!(
            "current change's {files} Rust file(s) on their own"
        )),
        fixture.seam_limit.is_some(),
        "unexpected supplement branch: {}",
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
    assert_eq!(
        summary["current_change"]["base"], "origin/main",
        "{summary:#}"
    );
    let md = read_bounded(&out_dir.join("pilot-summary.md"))?;
    assert_eq!(
        md.contains("- Seam limit reached: ranked "),
        fixture.seam_limit.is_some(),
        "unexpected outer limit: {md}"
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

fn exercise_missing_coverage(
    label: &str,
    mixed: bool,
    seam_limit: Option<&'static str>,
) -> Result<(), String> {
    let mut fixture = fixture(label, mixed)?;
    fixture.seam_limit = seam_limit;
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
        &[
            "sparse-checkout",
            "set",
            "--no-cone",
            "/*",
            "!/src/z_hidden.rs",
        ],
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
    let repo_before = repo_inventory(&fixture, &cache)?;
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
        repo_inventory(&fixture, &cache)?,
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
    let repo_restored_before = repo_inventory(&fixture, &cache)?;
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
        repo_inventory(&fixture, &cache)?,
        repo_restored_before,
        "restored supplement polluted classified cache"
    );
    Ok(())
}

#[test]
fn pilot_discloses_sparse_missing_changed_file_and_recovers_same_head() -> Result<(), String> {
    exercise_missing_coverage("pilot-sparse-changed-coverage", false, Some("1"))
}

#[test]
fn pilot_discloses_mixed_missing_changed_file_even_with_changed_recommendation()
-> Result<(), String> {
    exercise_missing_coverage("pilot-mixed-changed-coverage", true, Some("1"))
}

#[test]
fn pilot_discloses_sparse_missing_changed_file_without_inventory_limit() -> Result<(), String> {
    exercise_missing_coverage("pilot-uncapped-sparse-coverage", false, None)
}

#[test]
fn pilot_discloses_mixed_missing_changed_file_without_inventory_limit() -> Result<(), String> {
    exercise_missing_coverage("pilot-uncapped-mixed-coverage", true, None)
}

#[test]
fn pilot_uncapped_absence_honors_generated_policy_and_handwritten_opt_in() -> Result<(), String> {
    let mut fixture = fixture("pilot-uncapped-generated-admission", true)?;
    fixture.seam_limit = None;
    std::fs::write(fixture.root.join("src/generated.rs"), HIDDEN_CHANGE)
        .map_err(|error| error.to_string())?;
    run_git(&fixture.root, &["add", "src/generated.rs"])?;
    run_git(&fixture.root, &["commit", "-m", "generated source"])?;
    let identity = committed_identity(&fixture.root)?;
    assert_eq!(
        identity.1.replace("\r\n", "\n"),
        "src/generated.rs\nsrc/y_present.rs\nsrc/z_hidden.rs\n"
    );
    run_git(&fixture.root, &["sparse-checkout", "init", "--no-cone"])?;
    run_git(
        &fixture.root,
        &[
            "sparse-checkout",
            "set",
            "--no-cone",
            "/*",
            "!/src/generated.rs",
            "!/src/z_hidden.rs",
        ],
    )?;
    assert!(!fixture.root.join("src/generated.rs").exists());
    assert!(!fixture.root.join("src/z_hidden.rs").exists());
    assert_eq!(committed_identity(&fixture.root)?, identity);
    let cache = fixture.scratch.join("cache");
    let default = pilot(&fixture, &cache, 3)?;
    assert_absent(&default);
    for text in [&default.stdout, &default.md] {
        assert!(
            !text
                .lines()
                .any(|line| line.contains("changed_file_absent_from_worktree")
                    && line.contains("src/generated.rs")),
            "{text}"
        );
    }
    std::fs::write(
        fixture.root.join("ripr.toml"),
        "[languages.rust]\nhandwritten_files = [\"src/generated.rs\"]\n",
    )
    .map_err(|error| error.to_string())?;
    let opted_in = pilot(&fixture, &cache, 3)?;
    assert_eq!(
        opted_in.summary["current_change"]["absent_changed_files"],
        json!(["src/generated.rs", "src/z_hidden.rs"])
    );
    std::fs::write(fixture.root.join("ripr.toml"), "[languages.rust]\nhandwritten_files = [\"src/generated.rs\", \"src/z_hidden.rs\"]\ngenerated_file_patterns = [\"src/z_hidden.rs\"]\n")
        .map_err(|error| error.to_string())?;
    let explicit = pilot(&fixture, &cache, 3)?;
    assert_eq!(
        explicit.summary["current_change"]["absent_changed_files"],
        json!(["src/generated.rs"])
    );
    Ok(())
}

#[test]
fn pilot_uncapped_absence_retains_changed_missing_vendor_marker_context() -> Result<(), String> {
    let mut fixture = fixture("pilot-uncapped-absent-vendor-marker", true)?;
    fixture.seam_limit = None;
    std::fs::create_dir_all(fixture.root.join("src/vendor")).map_err(|error| error.to_string())?;
    std::fs::write(fixture.root.join("src/vendor/ordinary.rs"), HIDDEN_CHANGE)
        .map_err(|error| error.to_string())?;
    std::fs::write(fixture.root.join("src/vendor/.cargo-checksum.json"), "{}\n")
        .map_err(|error| error.to_string())?;
    run_git(&fixture.root, &["add", "src/vendor"])?;
    run_git(
        &fixture.root,
        &["commit", "-m", "vendored source and checksum"],
    )?;
    let identity = committed_identity(&fixture.root)?;
    assert!(identity.1.contains("src/vendor/.cargo-checksum.json"));
    run_git(&fixture.root, &["sparse-checkout", "init", "--no-cone"])?;
    run_git(
        &fixture.root,
        &[
            "sparse-checkout",
            "set",
            "--no-cone",
            "/*",
            "!/src/vendor/",
            "!/src/z_hidden.rs",
        ],
    )?;
    assert!(
        !fixture
            .root
            .join("src/vendor/.cargo-checksum.json")
            .exists()
    );
    assert!(!fixture.root.join("src/vendor/ordinary.rs").exists());
    assert_eq!(committed_identity(&fixture.root)?, identity);
    std::fs::write(
        fixture.root.join("ripr.toml"),
        "[languages.rust]\nhandwritten_files = [\"src/vendor/ordinary.rs\"]\n",
    )
    .map_err(|error| error.to_string())?;
    let result = pilot(&fixture, &fixture.scratch.join("cache"), 3)?;
    assert_absent(&result);
    for text in [&result.stdout, &result.md] {
        assert!(
            !text
                .lines()
                .any(|line| line.contains("changed_file_absent_from_worktree")
                    && line.contains("src/vendor/ordinary.rs")),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn pilot_does_not_name_intentionally_deleted_file_as_absent() -> Result<(), String> {
    let mut fixture = fixture("pilot-deleted-changed-source", true)?;
    fixture.seam_limit = None;
    run_git(&fixture.root, &["rm", "src/z_hidden.rs"])?;
    run_git(&fixture.root, &["commit", "-m", "delete hidden source"])?;
    let result = pilot(&fixture, &fixture.scratch.join("cache"), 1)?;
    let empty = json!([]);
    let absent = result.summary["current_change"].get("absent_changed_files");
    assert!(
        absent.is_none() || absent == Some(&empty),
        "{:#}",
        result.summary
    );
    assert_eq!(
        result.summary["current_change"]["top_recommendation_in_change"],
        true
    );
    for text in [&result.stdout, &result.md] {
        assert!(
            !text.contains("changed_file_absent_from_worktree"),
            "{text}"
        );
    }
    Ok(())
}

fn generated_policy_fixture() -> Result<Fixture, String> {
    let fixture = fixture("pilot-capped-generated-policy", true)?;
    // Reuse the bounded Git/process harness, then establish the exact nine-file
    // baseline independently of the simpler missing-file fixtures above.
    std::fs::remove_dir_all(fixture.root.join("src")).map_err(|error| error.to_string())?;
    let paths = [
        "present.rs",
        "absent.rs",
        "generated.rs",
        "gen/ordinary.rs",
        "explicit.rs",
        "third_party/dependency.rs",
        "vendorish.rs",
        "percent%name.rs",
        "header.rs",
    ];
    let mut lib = String::from(
        "pub fn first(n: u32) -> bool { n > 100 }\npub fn second(n: u32) -> bool { n > 200 }\n",
    );
    for (index, relative) in paths.iter().enumerate() {
        let path = fixture.root.join("src").join(relative);
        let parent = path.parent().ok_or("source path has no parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let header = if *relative == "header.rs" {
            "// @generated\n"
        } else {
            ""
        };
        std::fs::write(&path, format!("{header}{HIDDEN_BASE}"))
            .map_err(|error| error.to_string())?;
        lib.push_str(&format!("#[path = \"{relative}\"] pub mod item_{index};\n"));
    }
    std::fs::write(fixture.root.join("src/lib.rs"), lib).map_err(|error| error.to_string())?;
    std::fs::write(
        fixture.root.join("ripr.toml"),
        "[languages.rust]\ngenerated_file_patterns = [\"src/explicit.rs\"]\n",
    )
    .map_err(|error| error.to_string())?;
    run_git(&fixture.root, &["add", "-A"])?;
    run_git(
        &fixture.root,
        &["commit", "-m", "generated-policy baseline"],
    )?;
    run_git(
        &fixture.root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    )?;
    for relative in paths {
        let header = if relative == "header.rs" {
            "// @generated\n"
        } else {
            ""
        };
        std::fs::write(
            fixture.root.join("src").join(relative),
            format!("{header}{HIDDEN_CHANGE}"),
        )
        .map_err(|error| error.to_string())?;
    }
    std::fs::write(
        fixture.root.join("src/third_party/.cargo-checksum.json"),
        "{}\n",
    )
    .map_err(|error| error.to_string())?;
    run_git(&fixture.root, &["add", "."])?;
    run_git(
        &fixture.root,
        &["commit", "-m", "nine predicates and vendor marker"],
    )?;
    Ok(fixture)
}

fn assert_absent_paths(pilot: &Pilot, expected: &[&str]) {
    assert_eq!(
        pilot.summary["current_change"]["absent_changed_files"],
        json!(expected),
        "{:#}",
        pilot.summary
    );
    for text in [&pilot.stdout, &pilot.md] {
        let warnings: Vec<_> = text
            .lines()
            .filter(|line| line.contains("changed_file_absent_from_worktree"))
            .collect();
        let mut paths: Vec<_> = warnings
            .iter()
            .filter_map(|line| {
                line.split_once("changed_file_absent_from_worktree: ")
                    .and_then(|(_, tail)| tail.split_once(';'))
                    .map(|(path, _)| path.trim_matches('`'))
            })
            .collect();
        assert_eq!(paths.len(), warnings.len(), "unparsed warning: {text}");
        paths.sort_unstable();
        assert_eq!(paths, expected, "wrong human absence population: {text}");
    }
}

#[test]
fn pilot_capped_absence_matches_uncapped_generated_policy_and_recovers() -> Result<(), String> {
    let mut fixture = generated_policy_fixture()?;
    let identity = committed_identity(&fixture.root)?;
    assert_eq!(
        identity.1.replace("\r\n", "\n"),
        "src/absent.rs\nsrc/explicit.rs\nsrc/gen/ordinary.rs\nsrc/generated.rs\nsrc/header.rs\nsrc/percent%name.rs\nsrc/present.rs\nsrc/third_party/.cargo-checksum.json\nsrc/third_party/dependency.rs\nsrc/vendorish.rs\n"
    );
    run_git(&fixture.root, &["sparse-checkout", "init", "--no-cone"])?;
    run_git(
        &fixture.root,
        &[
            "sparse-checkout",
            "set",
            "--no-cone",
            "/*",
            "!/src/absent.rs",
            "!/src/explicit.rs",
            "!/src/gen/ordinary.rs",
            "!/src/generated.rs",
            "!/src/header.rs",
            "!/src/percent%name.rs",
            "!/src/third_party/dependency.rs",
            "!/src/third_party/.cargo-checksum.json",
            "!/src/vendorish.rs",
        ],
    )?;
    assert!(fixture.root.join("src/present.rs").is_file());
    for path in identity.1.lines().filter(|path| *path != "src/present.rs") {
        assert!(!fixture.root.join(path).exists(), "not sparse: {path}");
    }
    assert_eq!(committed_identity(&fixture.root)?, identity);
    let four = [
        "src/absent.rs",
        "src/header.rs",
        "src/percent%name.rs",
        "src/vendorish.rs",
    ];
    fixture.seam_limit = None;
    let uncapped = pilot(&fixture, &fixture.scratch.join("uncapped-cache"), 9)?;
    assert_absent_paths(&uncapped, &four);
    fixture.seam_limit = Some("1");
    let cache = fixture.scratch.join("capped-cache");
    let ordinary = repo_inventory(&fixture, &fixture.scratch.join("ordinary-cache"))?;
    let capped = pilot(&fixture, &cache, 9)?;
    assert_absent_paths(&capped, &four);
    assert_eq!(
        capped.summary["current_change"]["top_recommendation_in_change"],
        true
    );
    let warm = pilot(&fixture, &cache, 9)?;
    assert_absent_paths(&warm, &four);
    assert_cache_hit(&warm);
    assert_eq!(semantic_summary(&warm), semantic_summary(&capped));
    let cold = pilot(&fixture, &fixture.scratch.join("cold-cache"), 9)?;
    assert_absent_paths(&cold, &four);
    assert_eq!(semantic_summary(&cold), semantic_summary(&capped));
    assert_eq!(repo_inventory(&fixture, &cache)?, ordinary);
    std::fs::write(fixture.root.join("ripr.toml"), "[languages.rust]\ngenerated_file_patterns = [\"src/explicit.rs\"]\nhandwritten_files = [\"src/generated.rs\", \"src/gen/ordinary.rs\", \"src/explicit.rs\", \"src/third_party/dependency.rs\"]\n")
        .map_err(|error| error.to_string())?;
    let opted_in = pilot(&fixture, &cache, 9)?;
    let six = [
        "src/absent.rs",
        "src/gen/ordinary.rs",
        "src/generated.rs",
        "src/header.rs",
        "src/percent%name.rs",
        "src/vendorish.rs",
    ];
    assert_absent_paths(&opted_in, &six);
    assert!(
        opted_in.stderr.contains("phase=cache_load status=miss "),
        "{}",
        opted_in.stderr
    );
    let opted_warm = pilot(&fixture, &cache, 9)?;
    assert_absent_paths(&opted_warm, &six);
    assert_cache_hit(&opted_warm);
    std::fs::write(
        fixture.root.join("ripr.toml"),
        "[languages.rust]\ngenerated_file_patterns = [\"src/explicit.rs\"]\n",
    )
    .map_err(|error| error.to_string())?;
    run_git(&fixture.root, &["sparse-checkout", "disable"])?;
    assert_eq!(committed_identity(&fixture.root)?, identity);
    let ordinary_restored = repo_inventory(&fixture, &fixture.scratch.join("restored-ordinary"))?;
    let restored = pilot(&fixture, &cache, 9)?;
    assert_absent_paths(&restored, &[]);
    assert!(
        restored.stderr.contains("phase=cache_load status=miss "),
        "{}",
        restored.stderr
    );
    for seam in restored.summary["top_actionable_seams"]
        .as_array()
        .ok_or("missing restored ranking")?
    {
        assert!(
            ![
                "src/generated.rs",
                "src/gen/ordinary.rs",
                "src/explicit.rs",
                "src/third_party/dependency.rs",
                "src/header.rs"
            ]
            .iter()
            .any(|path| seam["file"] == *path),
            "{seam:#}"
        );
    }
    let restored_warm = pilot(&fixture, &cache, 9)?;
    assert_absent_paths(&restored_warm, &[]);
    assert_cache_hit(&restored_warm);
    assert_eq!(
        semantic_summary(&restored_warm),
        semantic_summary(&restored)
    );
    let restored_cold = pilot(&fixture, &fixture.scratch.join("restored-cold"), 9)?;
    assert_absent_paths(&restored_cold, &[]);
    assert_eq!(
        semantic_summary(&restored_cold),
        semantic_summary(&restored)
    );
    assert_eq!(repo_inventory(&fixture, &cache)?, ordinary_restored);
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn pilot_does_not_probe_absent_rust_source_when_rust_is_disabled() -> Result<(), String> {
    let mut fixture = fixture("pilot-absent-rust-disabled", false)?;
    fixture.seam_limit = None;
    run_git(&fixture.root, &["sparse-checkout", "init", "--no-cone"])?;
    run_git(
        &fixture.root,
        &[
            "sparse-checkout",
            "set",
            "--no-cone",
            "/*",
            "!/src/z_hidden.rs",
        ],
    )?;
    assert!(!fixture.root.join("src/z_hidden.rs").exists());
    std::fs::write(
        fixture.root.join("ripr.toml"),
        "[languages]\nenabled = [\"python\"]\n",
    )
    .map_err(|error| error.to_string())?;
    let result = pilot(&fixture, &fixture.scratch.join("cache"), 1)?;
    assert!(
        result.summary["current_change"]
            .get("absent_changed_files")
            .is_none(),
        "{:#}",
        result.summary
    );
    assert!(
        result
            .stdout
            .contains("Excluded from pilot's Rust seam scan:"),
        "{}",
        result.stdout
    );
    for text in [&result.stdout, &result.md] {
        assert!(
            !text.contains("changed_file_absent_from_worktree"),
            "{text}"
        );
    }
    Ok(())
}
