//! Built-binary stdout/stderr discriminator for CLI analysis progress (#4810).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The binary with progress forced on, so the tests below that observe the
/// stream keep their meaning now that a machine format on a piped stderr is
/// silent by default (#11598). Tests of that default use [`ripr_default`].
fn ripr() -> Command {
    let mut command = ripr_default();
    command.env("RIPR_PROGRESS", "1");
    command
}

/// The binary exactly as a script runs it: no progress switches set.
fn ripr_default() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command.env_remove("RIPR_PROGRESS").env_remove("RIPR_QUIET");
    command
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample_root() -> String {
    workspace_root().display().to_string()
}

/// The format/progress contract needs real, nonempty diff analysis, not this
/// repository's unrelated source/test inventory. Keep one root for each pair
/// so the loud/quiet byte comparison observes the same analysis identity.
struct DiffFixture(PathBuf);

impl DiffFixture {
    fn new() -> Result<Self, String> {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let tag = format!(
            "diff-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let fixture = Self(repo_fixture_root(&tag)?);
        // Cargo can place temporary files inside the checkout. Stop workspace
        // and configuration ancestor lookup at this fixture's own manifest.
        std::fs::OpenOptions::new()
            .append(true)
            .open(fixture.0.join("Cargo.toml"))
            .and_then(|mut manifest| manifest.write_all(b"\n[workspace]\n"))
            .map_err(|error| format!("bound diff fixture workspace: {error}"))?;
        std::fs::create_dir_all(fixture.0.join("tests"))
            .map_err(|error| format!("create diff fixture tests: {error}"))?;
        std::fs::write(
            fixture.0.join("tests/threshold.rs"),
            format!(
                "use ripr_cli_progress_{}::over_threshold;\n\n#[test]\nfn either_side_of_threshold() {{\n    assert!(over_threshold(11, 10));\n    assert!(!over_threshold(9, 10));\n}}\n",
                tag.replace('-', "_")
            ),
        )
        .map_err(|error| format!("write diff fixture tests: {error}"))?;
        std::fs::write(
            fixture.0.join("change.diff"),
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn over_threshold(amount: i32, threshold: i32) -> bool {\n-    amount > threshold\n+    amount >= threshold\n }\n",
        )
        .map_err(|error| format!("write diff fixture input: {error}"))?;
        Ok(fixture)
    }
}

impl Drop for DiffFixture {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}

fn run_check(fixture: &DiffFixture, extra: &[&str]) -> Result<Output, String> {
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("change.diff").display().to_string();
    let mut args = vec!["check", "--root", root.as_str(), "--diff", diff.as_str()];
    args.extend_from_slice(extra);
    ripr()
        .args(&args)
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

fn assert_fixture_json_findings(parsed: &serde_json::Value) -> Result<(), String> {
    // #5996: the location renders through the shared workspace-relative
    // owner, so the file names the fixture's one changed source from the
    // analyzed root (`./src/lib.rs`) on every surface, including this
    // absolute-`--root` run.
    let expected_file = "./src/lib.rs";
    assert_eq!(
        parsed["summary"]["changed_rust_files"], 1,
        "fixture must analyze its one changed Rust file"
    );
    let findings = parsed["findings"]
        .as_array()
        .ok_or_else(|| "diff fixture must produce a findings array".to_string())?;
    assert!(
        findings.iter().any(|finding| {
            finding
                .pointer("/probe/file")
                .and_then(serde_json::Value::as_str)
                == Some(expected_file)
                && finding
                    .pointer("/probe/expression")
                    .and_then(serde_json::Value::as_str)
                    == Some("amount >= threshold")
        }),
        "real analyzer must report the fixture's changed predicate: {parsed}"
    );
    Ok(())
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout_has_progress_record(stdout: &str) -> bool {
    stdout
        .lines()
        .any(|line| line.trim_start().starts_with("ripr progress:"))
}

fn sample_crate_root() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/sample")
        .display()
        .to_string()
}

#[test]
fn check_json_stdout_parses_while_progress_stays_on_stderr() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let output = run_check(&fixture, &["--format", "json"])?;
    assert!(
        output.status.success(),
        "check json failed: {}",
        stderr_text(&output)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("stdout is not JSON: {error}"))?;
    assert_eq!(parsed["schema_version"], "0.2");
    assert_fixture_json_findings(&parsed)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = stderr_text(&output);
    assert!(
        !stdout.contains("ripr progress:"),
        "progress leaked onto stdout"
    );
    assert!(
        stderr.contains("ripr progress: loading_input [diff]"),
        "missing loading_input on stderr: {stderr}"
    );
    assert!(
        stderr.contains("ripr progress: analyzing [diff]"),
        "missing analyzing on stderr: {stderr}"
    );
    assert!(
        stderr.contains("ripr progress: completed [diff]"),
        "missing completed on stderr: {stderr}"
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "non-TTY stderr has ANSI: {stderr}"
    );
    assert!(!stderr.contains('\r'), "non-TTY stderr has CR: {stderr}");
    assert!(
        !stderr.contains('%'),
        "unknown totals leaked a percent: {stderr}"
    );
    assert!(
        !stderr.to_ascii_lowercase().contains("eta"),
        "unknown totals leaked an ETA: {stderr}"
    );
    Ok(())
}

#[test]
fn check_quiet_keeps_json_stdout_byte_identical_and_drops_progress() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let loud = run_check(&fixture, &["--format", "json"])?;
    let quiet = run_check(&fixture, &["--format", "json", "--quiet"])?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    assert_eq!(
        loud.stdout, quiet.stdout,
        "quiet must not change machine stdout"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&loud.stdout)
        .map_err(|error| format!("stdout is not JSON: {error}"))?;
    assert_fixture_json_findings(&parsed)?;
    let quiet_err = stderr_text(&quiet);
    assert!(
        !quiet_err.contains("ripr progress:"),
        "--quiet must suppress progress: {quiet_err}"
    );
    assert!(
        stderr_text(&loud).contains("ripr progress:"),
        "control run must have emitted progress"
    );
    Ok(())
}

#[test]
fn check_sarif_stdout_is_unchanged_by_progress() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let loud = run_check(&fixture, &["--format", "sarif"])?;
    let quiet = run_check(&fixture, &["--format", "sarif", "--quiet"])?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    assert_eq!(loud.stdout, quiet.stdout);
    let stdout = String::from_utf8_lossy(&loud.stdout);
    assert!(stdout.contains("\"version\": \"2.1.0\"") || stdout.contains("sarif"));
    assert!(!stdout.contains("ripr progress:"));
    assert!(stderr_text(&loud).contains("ripr progress:"));
    let parsed: serde_json::Value = serde_json::from_slice(&loud.stdout)
        .map_err(|error| format!("stdout is not SARIF JSON: {error}"))?;
    assert_eq!(parsed["version"], "2.1.0");
    let results = parsed
        .pointer("/runs/0/results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "SARIF fixture must produce a results array".to_string())?;
    assert!(
        results.iter().any(|result| {
            result
                .pointer("/locations/0/physicalLocation/artifactLocation/uri")
                .and_then(serde_json::Value::as_str)
                == Some("src/lib.rs")
        }),
        "SARIF must contain a located finding for the changed fixture: {parsed}"
    );
    Ok(())
}

#[test]
fn check_progress_failure_emits_failed_not_completed() -> Result<(), String> {
    let root = sample_root();
    let missing = workspace_root().join("target/ripr/absent-progress.diff");
    let missing_arg = missing.display().to_string();
    let output = ripr()
        .args([
            "check",
            "--root",
            root.as_str(),
            "--diff",
            missing_arg.as_str(),
            "--format",
            "json",
        ])
        .output()
        .map_err(|error| format!("run failing check: {error}"))?;
    assert!(
        !output.status.success(),
        "missing diff must fail the command"
    );
    let stderr = stderr_text(&output);
    // A mistyped `--diff` path is input the user can fix: the real error is the
    // message, not an internal-failure progress line (#9922).
    assert!(
        !stderr.contains("ripr progress: failed"),
        "a missing diff file must not project failed: {stderr}"
    );
    assert!(
        !stderr.contains("ripr progress: completed"),
        "failure must not project completed: {stderr}"
    );
    assert!(
        stderr.contains("absent-progress.diff"),
        "the real error must still name the missing file: {stderr}"
    );
    Ok(())
}

/// #7352: `ripr explain` and `ripr context` re-run the full check pipeline
/// on a fresh run; the silent window looked like a hang on a cold large
/// repository. Both must project the same stderr-only stage lines `ripr
/// check` promises, while machine stdout stays free of progress records.
#[test]
fn explain_and_context_fresh_runs_disclose_progress_on_stderr() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let check = run_check(&fixture, &["--format", "json"])?;
    assert!(
        check.status.success(),
        "check json failed: {}",
        stderr_text(&check)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&check.stdout)
        .map_err(|error| format!("stdout is not JSON: {error}"))?;
    let finding_id = parsed["findings"]
        .as_array()
        .and_then(|findings| findings.first())
        .and_then(|finding| finding["id"].as_str())
        .ok_or_else(|| format!("diff fixture must report a finding id: {parsed}"))?
        .to_string();
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("change.diff").display().to_string();
    for (command, extra) in [
        ("explain", vec![finding_id.as_str()]),
        ("context", vec!["--at", finding_id.as_str()]),
    ] {
        let mut args = vec![command, "--root", root.as_str(), "--diff", diff.as_str()];
        args.extend_from_slice(&extra);
        let output = ripr()
            .args(&args)
            .output()
            .map_err(|error| format!("run ripr {args:?}: {error}"))?;
        assert!(
            output.status.success(),
            "{command} failed: {}",
            stderr_text(&output)
        );
        let stderr = stderr_text(&output);
        assert!(
            stderr.contains("ripr progress: loading_input [diff]"),
            "{command} must disclose its pipeline re-run on stderr: {stderr}"
        );
        assert!(
            stderr.contains("ripr progress: analyzing [diff]"),
            "{command} must project the analyzing stage: {stderr}"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("ripr progress:"),
            "{command} leaked progress onto stdout"
        );
    }
    Ok(())
}

/// #8739: `ripr explain` / `ripr context` accept `--quiet` with the `ripr
/// check` meaning. The paired loud run proves the flag is what silences the
/// stream, and stdout/exit status are identical either way.
#[test]
fn explain_and_context_quiet_suppress_progress_only() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let check = run_check(&fixture, &["--format", "json", "--quiet"])?;
    assert!(check.status.success(), "{}", stderr_text(&check));
    let parsed: serde_json::Value = serde_json::from_slice(&check.stdout)
        .map_err(|error| format!("stdout is not JSON: {error}"))?;
    let finding_id = parsed["findings"]
        .as_array()
        .and_then(|findings| findings.first())
        .and_then(|finding| finding["id"].as_str())
        .ok_or_else(|| format!("diff fixture must report a finding id: {parsed}"))?
        .to_string();
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("change.diff").display().to_string();
    for (command, extra) in [
        ("explain", vec![finding_id.as_str()]),
        ("context", vec!["--at", finding_id.as_str()]),
    ] {
        let mut base = vec![command, "--root", root.as_str(), "--diff", diff.as_str()];
        base.extend_from_slice(&extra);
        let loud = ripr()
            .args(&base)
            .output()
            .map_err(|error| format!("run ripr {base:?}: {error}"))?;
        let mut quiet_args = base.clone();
        quiet_args.push("--quiet");
        let quiet = ripr()
            .args(&quiet_args)
            .output()
            .map_err(|error| format!("run ripr {quiet_args:?}: {error}"))?;
        assert!(loud.status.success(), "{}", stderr_text(&loud));
        assert!(
            quiet.status.success(),
            "{command} --quiet must be accepted: {}",
            stderr_text(&quiet)
        );
        assert!(
            stderr_text(&loud).contains("ripr progress:"),
            "{command} default run must keep projecting progress"
        );
        assert!(
            !stderr_text(&quiet).contains("ripr progress:"),
            "{command} --quiet must suppress progress: {}",
            stderr_text(&quiet)
        );
        assert_eq!(
            loud.stdout, quiet.stdout,
            "{command} stdout must not change"
        );
        assert!(!quiet.stdout.is_empty(), "{command} must still render");
    }
    Ok(())
}

#[test]
fn check_help_does_not_spray_progress() -> Result<(), String> {
    let output = ripr()
        .args(["check", "--help", "--all"])
        .output()
        .map_err(|error| format!("run check help: {error}"))?;
    assert!(output.status.success());
    let stderr = stderr_text(&output);
    assert!(
        !stderr.contains("ripr progress:"),
        "help is a short command: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--quiet"));
    assert!(stdout.contains("percentage or ETA"));
    Ok(())
}

/// The default one-screen route stays a short command too: it writes nothing
/// to stderr and still carries SPEC-0185's `--quiet`, unknown-totals and
/// no-speed-claim disclosure.
#[test]
fn check_default_help_does_not_spray_progress() -> Result<(), String> {
    let output = ripr()
        .args(["check", "--help"])
        .output()
        .map_err(|error| format!("run check help: {error}"))?;
    assert!(output.status.success());
    let stderr = stderr_text(&output);
    assert!(
        stderr.is_empty(),
        "default help is a short command with no stderr: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let flat = stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("--quiet"));
    assert!(flat.contains("percentage or ETA"), "{flat}");
    assert!(flat.contains("no speed claim"), "{flat}");
    assert!(flat.contains("ripr check --help --all"));
    Ok(())
}

#[test]
fn check_github_stdout_is_unchanged_by_progress() -> Result<(), String> {
    machine_format_stdout_is_unchanged("github")
}

fn machine_format_stdout_is_unchanged(format: &str) -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let loud = run_check(&fixture, &["--format", format])?;
    let quiet = run_check(&fixture, &["--format", format, "--quiet"])?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    assert_eq!(loud.stdout, quiet.stdout);
    let stdout = String::from_utf8_lossy(&loud.stdout);
    assert!(!stdout.contains("ripr progress:"));
    assert!(stderr_text(&loud).contains("ripr progress: loading_input [diff]"));
    assert!(
        stdout.lines().any(|line| {
            (line.starts_with("::warning ")
                || line.starts_with("::error ")
                || line.starts_with("::notice "))
                && line.contains("file=src/lib.rs,")
                && line.contains("title=ripr ")
        }),
        "GitHub must emit a finding annotation, not an empty-result notice: {stdout}"
    );
    Ok(())
}

#[test]
fn check_worktree_projects_worktree_scope_on_stderr() -> Result<(), String> {
    // Analyze the sample crate rather than this repository. A worktree scan of
    // ripr itself is slow, and its JSON findings can mention `ripr progress:`
    // as source text.
    let root = sample_crate_root();
    let output = ripr()
        .args([
            "check",
            "--root",
            root.as_str(),
            "--worktree",
            "--format",
            "json",
        ])
        .output()
        .map_err(|error| format!("run worktree check: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = stderr_text(&output);
    assert!(
        !stdout_has_progress_record(&stdout),
        "progress leaked onto stdout: {stdout}"
    );
    assert!(
        stderr.contains("ripr progress: loading_input [worktree]"),
        "missing worktree scope on stderr: {stderr}"
    );
    assert!(
        !stderr.contains("[diff]"),
        "worktree run must not project the diff scope: {stderr}"
    );
    if output.status.success() {
        assert!(
            stderr.contains("ripr progress: completed [worktree]"),
            "successful worktree check must commit completed: {stderr}"
        );
        let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("stdout is not JSON: {error}"))?;
        assert_eq!(parsed["schema_version"], "0.2");
    } else {
        assert!(
            stderr.contains("ripr progress: failed [worktree]"),
            "failed worktree check must project failed: {stderr}"
        );
        assert!(
            !stderr.contains("ripr progress: completed"),
            "worktree failure must not project completed: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn check_quiet_failure_keeps_errors_and_drops_progress() -> Result<(), String> {
    let root = sample_root();
    let missing = workspace_root().join("target/ripr/absent-progress.diff");
    let missing_arg = missing.display().to_string();
    let output = ripr()
        .args([
            "check",
            "--root",
            root.as_str(),
            "--diff",
            missing_arg.as_str(),
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .map_err(|error| format!("run quiet failing check: {error}"))?;
    assert!(
        !output.status.success(),
        "missing diff must fail under --quiet"
    );
    let stderr = stderr_text(&output);
    assert!(
        !stderr.contains("ripr progress:"),
        "--quiet must suppress progress on failure: {stderr}"
    );
    assert!(
        !stderr.trim().is_empty(),
        "--quiet must still report the command error"
    );
    Ok(())
}

#[test]
fn check_unwritable_artifact_projects_failed_not_completed() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let dir = fixture.0.join("artifact-dir");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create artifact dir: {error}"))?;
    let dir_arg = dir.display().to_string();
    let output = run_check(
        &fixture,
        &["--format", "json", "--write-artifact", dir_arg.as_str()],
    )?;
    assert!(
        !output.status.success(),
        "writing an artifact onto a directory must fail the command: {}",
        stderr_text(&output)
    );
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("ripr progress: building_output [diff]"),
        "artifact failure must follow real analysis: {stderr}"
    );
    assert!(
        stderr.contains("ripr progress: failed [diff]"),
        "post-analysis artifact failure must project failed: {stderr}"
    );
    assert!(
        !stderr.contains("ripr progress: completed"),
        "post-analysis artifact failure must not project completed: {stderr}"
    );
    Ok(())
}

/// #9244 item 9: a `context --at` selector miss is a user-fixable refusal. The
/// real error and exit code stay, but no `ripr progress: failed` line precedes
/// it; the artifact-write control above still projects `failed`.
#[test]
fn context_selector_miss_prints_error_without_a_failed_progress_line() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("change.diff").display().to_string();
    let output = ripr()
        .args([
            "context",
            "--root",
            root.as_str(),
            "--diff",
            diff.as_str(),
            "--at",
            "nocolon",
        ])
        .output()
        .map_err(|error| format!("run ripr context: {error}"))?;
    assert_eq!(output.status.code(), Some(2), "selector miss keeps exit 2");
    assert!(output.stdout.is_empty(), "selector miss keeps empty stdout");
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("no finding matched \"nocolon\""),
        "the real selector error must still print: {stderr}"
    );
    assert!(
        stderr.contains("ripr progress: building_output [diff]"),
        "analysis progress still discloses the real work: {stderr}"
    );
    assert!(
        !stderr.contains("ripr progress: failed"),
        "a user-fixable selector miss must not project failed: {stderr}"
    );
    assert!(
        !stderr.contains("ripr progress: completed"),
        "a refused context run must not project completed: {stderr}"
    );
    Ok(())
}

fn run_check_with_env(
    fixture: &DiffFixture,
    extra: &[&str],
    quiet_env: Option<&str>,
) -> Result<Output, String> {
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("change.diff").display().to_string();
    let mut args = vec!["check", "--root", root.as_str(), "--diff", diff.as_str()];
    args.extend_from_slice(extra);
    let mut command = ripr();
    command.args(&args).env_remove("RIPR_QUIET");
    if let Some(value) = quiet_env {
        command.env("RIPR_QUIET", value);
    }
    command
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

/// #11580: `RIPR_QUIET` is an environment twin of `--quiet`: it drops the
/// stderr progress stream and changes nothing on stdout.
#[test]
fn ripr_quiet_env_drops_progress_without_changing_stdout() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let loud = run_check_with_env(&fixture, &["--format", "json"], None)?;
    let quiet = run_check_with_env(&fixture, &["--format", "json"], Some("1"))?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    assert_eq!(
        loud.stdout, quiet.stdout,
        "env quiet must not change stdout"
    );
    assert!(
        stderr_text(&loud).contains("ripr progress:"),
        "control run must have emitted progress"
    );
    let quiet_err = stderr_text(&quiet);
    assert!(
        !quiet_err.contains("ripr progress:"),
        "RIPR_QUIET=1 must suppress progress: {quiet_err}"
    );
    Ok(())
}

/// Negative: the explicit off spellings keep progress, so a harness that
/// exports `RIPR_QUIET=0` is not silenced by accident.
#[test]
fn ripr_quiet_env_off_spellings_keep_progress() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    for off in ["0", "false", ""] {
        let run = run_check_with_env(&fixture, &["--format", "json"], Some(off))?;
        assert!(run.status.success(), "{}", stderr_text(&run));
        assert!(
            stderr_text(&run).contains("ripr progress:"),
            "RIPR_QUIET={off:?} must keep progress"
        );
    }
    Ok(())
}

/// #11580: the built binary prints the compact agent report on stdout, keeps
/// progress off stdout, and is a small fraction of the full human report.
#[test]
fn check_format_agent_prints_the_compact_report() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let agent = run_check_with_env(&fixture, &["--format", "agent"], Some("1"))?;
    assert!(agent.status.success(), "{}", stderr_text(&agent));
    assert_eq!(
        stderr_text(&agent),
        "",
        "RIPR_QUIET leaves stderr empty on a clean run"
    );
    let text = String::from_utf8_lossy(&agent.stdout).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("#ripr agent static-advisory base="),
        "{text}"
    );
    let gap = lines
        .iter()
        .find(|line| line.starts_with("weakly_exposed|"))
        .ok_or_else(|| format!("fixture must list its boundary gap: {text}"))?;
    assert!(
        gap.contains("|src/lib.rs:2|amount >= threshold|missing=amount == threshold|test=t1|"),
        "{gap}"
    );
    assert!(gap.contains("|id=probe:"), "{gap}");
    let next = lines
        .iter()
        .find(|line| line.starts_with("next=ripr explain "))
        .ok_or_else(|| format!("top gap must carry a drill-in command: {text}"))?;
    assert!(next.contains("--diff "), "scope-bound command: {next}");
    assert!(!text.contains("ripr progress:"), "{text}");
    let full = run_check_with_env(&fixture, &["--format", "human-full", "--quiet"], None)?;
    assert!(
        text.len() * 2 < full.stdout.len(),
        "agent {} bytes vs human-full {} bytes",
        text.len(),
        full.stdout.len()
    );
    // Deterministic: a second run is byte-identical.
    let again = run_check_with_env(&fixture, &["--format", "agent", "--quiet"], None)?;
    assert_eq!(agent.stdout, again.stdout);
    Ok(())
}

// --- #4945: repo-scoped audit-path formats disclose progress and cost ---

/// Small single-crate fixture workspace for repo-format runs. Repo formats
/// analyze the live tree and may write the seam-facts cache under the root,
/// so these tests never point the binary at this repository or at a fixture
/// inside the checkout.
fn repo_fixture_root(tag: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("ripr-cli-progress-{tag}-{stamp}"));
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("create src: {error}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname=\"ripr-cli-progress-{tag}\"\nversion=\"0.1.0\"\nedition=\"2024\"\n"
        ),
    )
    .map_err(|error| format!("write Cargo.toml: {error}"))?;
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn over_threshold(amount: i32, threshold: i32) -> bool {\n    amount >= threshold\n}\n",
    )
    .map_err(|error| format!("write src/lib.rs: {error}"))?;
    Ok(root)
}

fn ignore_remove_dir_all(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

fn run_repo_format(root: &Path, format: &str, extra: &[&str]) -> Result<Output, String> {
    let root_arg = root.display().to_string();
    let mut args = vec!["check", "--root", root_arg.as_str(), "--format", format];
    args.extend_from_slice(extra);
    ripr()
        .args(&args)
        .output()
        .map_err(|error| format!("run ripr check --format {format}: {error}"))
}

#[test]
fn repo_format_audit_path_progress_reaches_stderr_and_stdout_stays_clean() -> Result<(), String> {
    let root = repo_fixture_root("seams")?;
    let output = run_repo_format(&root, "repo-seams-json", &[])?;
    assert!(
        output.status.success(),
        "repo-seams-json must succeed on the fixture: {}",
        stderr_text(&output)
    );
    let stderr = stderr_text(&output);
    // #4945: invocation-time cost disclosure naming the audit-path class.
    assert!(
        stderr.contains("full-repo audit path"),
        "missing audit-path cost disclosure: {stderr}"
    );
    assert!(
        stderr.contains("repo-seams-json"),
        "disclosure must name the invoked format: {stderr}"
    );
    // The repo walk now projects the same producer stages as the diff path.
    assert!(
        stderr.contains("ripr progress: analyzing [repo]"),
        "missing repo-scope analyzing stage: {stderr}"
    );
    assert!(
        stderr.contains("ripr progress: completed [repo]"),
        "missing repo-scope completed stage: {stderr}"
    );
    assert!(
        !stderr.contains("[diff]"),
        "repo run must not project the diff scope: {stderr}"
    );
    // Stdout cleanliness pin: the artifact is the ONLY stdout content.
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("stdout is not pure JSON: {error}"))?;
    assert_eq!(parsed["scope"], "repo");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("ripr progress:"),
        "progress leaked onto stdout: {stdout}"
    );
    assert!(
        !stdout.contains("audit path"),
        "disclosure leaked onto stdout: {stdout}"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn repo_format_quiet_keeps_stdout_byte_identical_and_drops_progress_stages() -> Result<(), String> {
    let root = repo_fixture_root("seams-quiet")?;
    let loud = run_repo_format(&root, "repo-seams-json", &[])?;
    let quiet = run_repo_format(&root, "repo-seams-json", &["--quiet"])?;
    assert!(loud.status.success(), "{}", stderr_text(&loud));
    assert!(quiet.status.success(), "{}", stderr_text(&quiet));
    // Byte-shape pin: the disclosure/progress work must not move stdout.
    // #7369 added a wall-clock `generated_at` disclosure stamp (and the
    // content commitment covers those bytes), so two runs differ by
    // milliseconds; mask the volatile identity lines and compare everything
    // else byte-for-byte.
    let without_volatile_stamp = |bytes: &[u8]| -> String {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter(|line| {
                !line.contains("\"generated_at\"") && !line.contains("\"content_sha256\"")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        without_volatile_stamp(&loud.stdout),
        without_volatile_stamp(&quiet.stdout),
        "--quiet must not change machine stdout"
    );
    let quiet_err = stderr_text(&quiet);
    assert!(
        !quiet_err.contains("ripr progress:"),
        "--quiet must suppress progress stages: {quiet_err}"
    );
    // The cost disclosure is an invocation-time advisory like the repo-scope
    // --base/--diff warning, not part of the progress stream, so --quiet
    // keeps it.
    assert!(
        quiet_err.contains("full-repo audit path"),
        "--quiet must keep the cost disclosure: {quiet_err}"
    );
    assert!(
        stderr_text(&loud).contains("ripr progress: analyzing [repo]"),
        "loud control must project repo stages"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn repo_format_disclosure_is_absent_outside_the_audit_path_group() -> Result<(), String> {
    let root = repo_fixture_root("controls")?;
    // Diff-scoped machine format: no audit-path claim. The scope is an
    // explicit --diff so the run completes regardless of git availability;
    // either way the disclosure must never fire for this format.
    let diff = root.join("control.diff");
    std::fs::write(
        &diff,
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub fn over_threshold(amount: i32, threshold: i32) -> bool {\n+pub fn over_threshold(amount: i32, threshold: i32, margin: i32) -> bool {\n",
    )
    .map_err(|error| format!("write control diff: {error}"))?;
    let diff_arg = diff.display().to_string();
    let diff_json = run_repo_format(&root, "json", &["--diff", diff_arg.as_str()])?;
    assert!(
        !stderr_text(&diff_json).contains("audit path"),
        "diff-scoped json must not claim audit-path cost: {}",
        stderr_text(&diff_json)
    );
    // Repo badge surface: since #5261 its canonical_actionable_gap count
    // derives from the same full classified walk repo-exposure renders, so
    // it joined the audit-path group and must disclose the cost class it
    // now charges. The diff-scoped control above stays outside the group.
    let badge = run_repo_format(&root, "repo-badge-json", &[])?;
    assert!(badge.status.success(), "{}", stderr_text(&badge));
    let badge_err = stderr_text(&badge);
    assert!(
        badge_err.contains("full-repo audit path"),
        "repo-badge-json must disclose audit-path cost now that it walks the          classified inventory: {badge_err}"
    );
    assert!(
        badge_err.contains("warm reruns reuse the seam-facts cache"),
        "cache-backed badge disclosure must claim warm reruns: {badge_err}"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

// --- #5019: pilot projects the same producer-owned repo-scope progress ---

fn run_pilot(root: &Path, out: &Path, extra: &[&str]) -> Result<Output, String> {
    let root_arg = root.display().to_string();
    let out_arg = out.display().to_string();
    let mut args = vec![
        "pilot",
        "--root",
        root_arg.as_str(),
        "--out",
        out_arg.as_str(),
    ];
    args.extend_from_slice(extra);
    ripr()
        .args(&args)
        .output()
        .map_err(|error| format!("run ripr pilot: {error}"))
}

#[test]
fn pilot_projects_repo_stages_on_stderr_and_keeps_packet_bytes_unchanged() -> Result<(), String> {
    let root = repo_fixture_root("pilot-progress")?;
    let out = root.join("pilot-out");
    let loud = run_pilot(&root, &out, &[])?;
    assert!(
        loud.status.success(),
        "pilot must succeed on the fixture: {}",
        stderr_text(&loud)
    );
    let loud_err = stderr_text(&loud);
    // Pilot analyzes the whole repo inventory, so the progress scope is
    // repo, exactly as on the check repo-format audit path.
    assert!(
        loud_err.contains("ripr progress: loading_input [repo]"),
        "missing repo-scope loading_input on stderr: {loud_err}"
    );
    assert!(
        loud_err.contains("ripr progress: analyzing [repo]"),
        "missing repo-scope analyzing on stderr: {loud_err}"
    );
    assert!(
        loud_err.contains("ripr progress: completed [repo]"),
        "missing repo-scope completed on stderr: {loud_err}"
    );
    assert!(
        !loud_err.contains("[diff]"),
        "pilot must not project the diff scope: {loud_err}"
    );
    assert!(
        !loud_err.contains('\u{1b}'),
        "non-TTY pilot stderr has ANSI: {loud_err}"
    );
    assert!(
        !loud_err.contains('\r'),
        "non-TTY pilot stderr has CR: {loud_err}"
    );
    let loud_stdout = String::from_utf8_lossy(&loud.stdout);
    assert!(
        !loud_stdout.contains("ripr progress:"),
        "progress leaked onto pilot stdout: {loud_stdout}"
    );

    // Machine contract pin: the progress stream must not move the packet.
    let summary_path = out.join("pilot-summary.json");
    let loud_summary = std::fs::read(&summary_path)
        .map_err(|error| format!("read loud pilot-summary.json: {error}"))?;
    let parsed: serde_json::Value = serde_json::from_slice(&loud_summary)
        .map_err(|error| format!("pilot-summary.json is not JSON: {error}"))?;
    assert_eq!(parsed["schema_version"], "0.4");

    // Removal experiment (#2608 closure rule): --quiet drops every progress
    // line while the emitted packet stays byte-identical.
    let quiet = run_pilot(&root, &out, &["--quiet"])?;
    assert!(
        quiet.status.success(),
        "quiet pilot must succeed on the fixture: {}",
        stderr_text(&quiet)
    );
    let quiet_err = stderr_text(&quiet);
    assert!(
        !quiet_err.contains("ripr progress:"),
        "--quiet must suppress pilot progress: {quiet_err}"
    );
    assert_eq!(
        loud.stdout, quiet.stdout,
        "--quiet must not change pilot terminal stdout"
    );
    let quiet_summary = std::fs::read(&summary_path)
        .map_err(|error| format!("read quiet pilot-summary.json: {error}"))?;
    assert_eq!(
        loud_summary, quiet_summary,
        "progress wiring must not change pilot-summary.json bytes"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

// --- #10727: verbose pilot prints a per-stage timing table on stderr only ---

#[test]
fn verbose_pilot_prints_a_stage_table_and_leaves_stdout_and_packet_bytes_unchanged()
-> Result<(), String> {
    let root = repo_fixture_root("pilot-progress")?;
    let out = root.join("pilot-out");
    let plain = run_pilot(&root, &out, &[])?;
    assert!(plain.status.success(), "{}", stderr_text(&plain));
    let summary_path = out.join("pilot-summary.json");
    let plain_summary = std::fs::read(&summary_path)
        .map_err(|error| format!("read plain pilot-summary.json: {error}"))?;
    assert!(
        !stderr_text(&plain).contains("stage timings"),
        "the table is verbose-only: {}",
        stderr_text(&plain)
    );

    let verbose = run_pilot(&root, &out, &["-v"])?;
    assert!(verbose.status.success(), "{}", stderr_text(&verbose));
    let verbose_err = stderr_text(&verbose);
    assert!(
        verbose_err.contains("ripr pilot: stage timings ("),
        "missing stage table: {verbose_err}"
    );
    let table: Vec<&str> = verbose_err
        .lines()
        .skip_while(|line| !line.starts_with("ripr pilot: stage timings"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .collect();
    let all_stages: Vec<&str> = table
        .iter()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    // The inventory's own phases ride beneath it; which of them ran depends
    // on whether the plain run above left a warm cache.
    assert!(
        all_stages
            .iter()
            .any(|stage| stage.starts_with("inventory.")),
        "no inventory phase rows: {verbose_err}"
    );
    let stages: Vec<&str> = all_stages
        .iter()
        .copied()
        .filter(|stage| !stage.starts_with("inventory."))
        .collect();
    assert_eq!(
        stages,
        [
            "load_change",
            "inventory",
            "change_classification",
            "diff_only_files",
            "seam_budget",
            "language_scan",
            "write_artifacts",
            "total",
        ],
        "{verbose_err}"
    );
    assert!(
        table.iter().all(|line| line.trim_end().contains(" ms")),
        "every row carries a millisecond count: {verbose_err}"
    );
    assert!(
        table
            .iter()
            .any(|line| line.contains("inventory") && line.contains("items=")),
        "the inventory row names its seam count: {verbose_err}"
    );

    assert_eq!(
        plain.stdout, verbose.stdout,
        "-v must not change pilot terminal stdout"
    );
    let verbose_summary = std::fs::read(&summary_path)
        .map_err(|error| format!("read verbose pilot-summary.json: {error}"))?;
    assert_eq!(
        plain_summary, verbose_summary,
        "timings must not enter pilot-summary.json"
    );
    assert!(
        !String::from_utf8_lossy(&verbose_summary).contains("elapsed"),
        "no wall-clock value in the packet"
    );

    // The sidecar carries the same stages as the table; a later run without
    // `-v` must not leave it standing as that run's timings.
    let sidecar_path = out.join("pilot-timings.json");
    let sidecar: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&sidecar_path)
            .map_err(|error| format!("read pilot-timings.json: {error}"))?,
    )
    .map_err(|error| format!("pilot-timings.json is not JSON: {error}"))?;
    let sidecar_stages: Vec<&str> = sidecar["stages"]
        .as_array()
        .ok_or("stages is not an array")?
        .iter()
        .filter_map(|row| row["stage"].as_str())
        .collect();
    let table_stages: Vec<&str> = all_stages
        .iter()
        .copied()
        .filter(|stage| *stage != "total")
        .collect();
    assert_eq!(sidecar_stages, table_stages, "{sidecar}");
    assert!(sidecar["total_ms"].is_u64(), "{sidecar}");
    let again = run_pilot(&root, &out, &[])?;
    assert!(again.status.success(), "{}", stderr_text(&again));
    assert!(
        !sidecar_path.exists(),
        "a run without -v removes the prior sidecar"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn verbose_pilot_fails_rather_than_leave_an_unwritable_sidecar_as_current() -> Result<(), String> {
    let root = repo_fixture_root("pilot-progress")?;
    let out = root.join("pilot-out");
    // A directory at the sidecar path cannot be replaced or removed as a
    // file, so the prior-sidecar guard must refuse instead of passing.
    std::fs::create_dir_all(out.join("pilot-timings.json"))
        .map_err(|error| format!("create blocking directory: {error}"))?;
    let verbose = run_pilot(&root, &out, &["-v"])?;
    assert!(
        !verbose.status.success(),
        "an unreplaceable sidecar must not read as current: {}",
        stderr_text(&verbose)
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

fn verbose_table_stages(output: &std::process::Output) -> Vec<String> {
    stderr_text(output)
        .lines()
        .skip_while(|line| !line.starts_with("ripr pilot: stage timings"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

#[test]
fn verbose_pilot_names_the_cold_inventory_phases_and_then_the_cache_load() -> Result<(), String> {
    let root = repo_fixture_root("pilot-phases")?;
    let out = root.join("pilot-out");
    // A fresh fixture root has no cache, so the first run computes.
    let cold = run_pilot(&root, &out, &["-v"])?;
    assert!(cold.status.success(), "{}", stderr_text(&cold));
    let cold_stages = verbose_table_stages(&cold);
    for phase in [
        "inventory.seams",
        "inventory.evidence",
        "inventory.classify",
    ] {
        assert!(
            cold_stages.iter().any(|stage| stage == phase),
            "a cold run times {phase}: {cold_stages:?}"
        );
    }
    let warm = run_pilot(&root, &out, &["-v"])?;
    assert!(warm.status.success(), "{}", stderr_text(&warm));
    let warm_stages = verbose_table_stages(&warm);
    assert!(
        warm_stages
            .iter()
            .any(|stage| stage == "inventory.cache_load"),
        "a warm run shows its cache load: {warm_stages:?}"
    );
    assert!(
        !warm_stages
            .iter()
            .any(|stage| stage == "inventory.classify"),
        "a warm run does not recompute: {warm_stages:?}"
    );
    ignore_remove_dir_all(&root);
    Ok(())
}

fn run_check_as(
    mut command: Command,
    fixture: &DiffFixture,
    extra: &[&str],
) -> Result<Output, String> {
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("change.diff").display().to_string();
    let mut args = vec!["check", "--root", root.as_str(), "--diff", diff.as_str()];
    args.extend_from_slice(extra);
    command
        .args(&args)
        .output()
        .map_err(|error| format!("run ripr {args:?}: {error}"))
}

/// #11598: a machine format with a stderr that is a pipe (as it is here) is
/// silent by default, byte-identical on stdout to the run that forces the
/// stream with `RIPR_PROGRESS`, and `--quiet` / `RIPR_QUIET` still win.
#[test]
fn machine_format_on_a_pipe_is_silent_by_default() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let default = run_check_as(ripr_default(), &fixture, &["--format", "json"])?;
    assert!(default.status.success(), "{}", stderr_text(&default));
    let default_err = stderr_text(&default);
    assert!(
        !default_err.contains("ripr progress:"),
        "json on a non-terminal stderr must be silent by default: {default_err}"
    );
    let forced = run_check_as(ripr(), &fixture, &["--format", "json"])?;
    assert!(
        stderr_text(&forced).contains("ripr progress: completed [diff]"),
        "RIPR_PROGRESS=1 must restore the stream (control): {}",
        stderr_text(&forced)
    );
    assert_eq!(
        default.stdout, forced.stdout,
        "the default must change stderr only"
    );
    // `RIPR_QUIET` wins over `RIPR_PROGRESS`.
    let mut both = ripr();
    both.env("RIPR_QUIET", "1");
    let quiet_wins = run_check_as(both, &fixture, &["--format", "json"])?;
    assert!(
        !stderr_text(&quiet_wins).contains("ripr progress:"),
        "RIPR_QUIET must beat RIPR_PROGRESS: {}",
        stderr_text(&quiet_wins)
    );
    Ok(())
}

/// #11598 negative: the default flips only for machine formats. A human
/// format on the same piped stderr keeps its progress.
#[test]
fn human_format_on_a_pipe_keeps_progress_by_default() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let human = run_check_as(ripr_default(), &fixture, &[])?;
    assert!(human.status.success(), "{}", stderr_text(&human));
    assert!(
        stderr_text(&human).contains("ripr progress: completed [diff]"),
        "the human format keeps progress on a pipe: {}",
        stderr_text(&human)
    );
    Ok(())
}

/// #11598: `ripr context` is JSON only, so its piped stderr is silent by
/// default too, and `RIPR_PROGRESS=1` brings the stream back.
#[test]
fn context_on_a_pipe_is_silent_by_default() -> Result<(), String> {
    let fixture = DiffFixture::new()?;
    let check = run_check_as(ripr_default(), &fixture, &["--format", "json"])?;
    let parsed: serde_json::Value = serde_json::from_slice(&check.stdout)
        .map_err(|error| format!("stdout is not JSON: {error}"))?;
    let finding_id = parsed["findings"]
        .as_array()
        .and_then(|findings| findings.first())
        .and_then(|finding| finding["id"].as_str())
        .ok_or_else(|| format!("diff fixture must report a finding id: {parsed}"))?
        .to_string();
    let root = fixture.0.display().to_string();
    let diff = fixture.0.join("change.diff").display().to_string();
    let args = [
        "context",
        "--root",
        root.as_str(),
        "--diff",
        diff.as_str(),
        "--at",
        finding_id.as_str(),
    ];
    let default = ripr_default()
        .args(args)
        .output()
        .map_err(|error| format!("run ripr context: {error}"))?;
    assert!(default.status.success(), "{}", stderr_text(&default));
    assert!(
        !stderr_text(&default).contains("ripr progress:"),
        "context on a pipe is silent by default: {}",
        stderr_text(&default)
    );
    let forced = ripr()
        .args(args)
        .output()
        .map_err(|error| format!("run ripr context: {error}"))?;
    assert!(
        stderr_text(&forced).contains("ripr progress: loading_input [diff]"),
        "RIPR_PROGRESS=1 restores context progress (control): {}",
        stderr_text(&forced)
    );
    assert_eq!(default.stdout, forced.stdout);
    Ok(())
}
