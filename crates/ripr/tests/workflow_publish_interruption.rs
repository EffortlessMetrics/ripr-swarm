//! Real process interruption at each gap-bound after-phase publication
//! boundary (#10988, 2026-10-29 22:13 UTC acceptance remainder).
//!
//! Copies the `GapAttempt` / SIGSTOP pattern from
//! `installed_python_typescript_journeys.rs` into this file so that busy
//! journeys file is not edited. Each test waits on filesystem state produced
//! by a live `ripr agent repair --phase after` child, SIGSTOPs, then SIGKILLs.
//! A hang deadline is only a detector; the wait itself is not sleep-timed.

#![cfg(all(feature = "lang-typescript", target_os = "linux"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};

const PRIOR_PROJECTION: &[u8] = b"prior-projection-for-interruption\n";
const PRIOR_VERIFY: &[u8] = b"prior-verify-for-interruption\n";
const PRIOR_OUTCOME: &[u8] = b"prior-outcome-for-interruption\n";
const BACKUP_SUFFIX: &str = ".ripr-preattempt-backup";
const HANG_DEADLINE: Duration = Duration::from_mins(2);

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> Result<TempDirGuard, String> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ripr-interrupt-{label}-{}-{counter}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    Ok(TempDirGuard(dir))
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| format!("create {}: {error}", to.display()))?;
    for entry in
        std::fs::read_dir(from).map_err(|error| format!("read {}: {error}", from.display()))?
    {
        let entry = entry.map_err(|error| format!("entry: {error}"))?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .map_err(|error| format!("copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn materialize(
    fixture: &str,
    head: &str,
    label: &str,
    ripr_toml: Option<&str>,
) -> Result<(TempDirGuard, PathBuf, PathBuf), String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(fixture)
        .join("repository");
    let guard = temp_dir(label)?;
    let launch = guard.0.clone();
    let root = launch.join("sélected root").join("repo");
    copy_tree(&source.join("base"), &root)?;
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
    fixture_git_ok(&root, &["config", "commit.gpgsign", "false"])?;
    fixture_git_ok(&root, &["config", "user.name", "fixture"])?;
    fixture_git_ok(&root, &["config", "user.email", "fixture@example.invalid"])?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "base"])?;
    copy_tree(&source.join(head), &root)?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "head"])?;
    if let Some(text) = ripr_toml {
        std::fs::write(root.join("ripr.toml"), text)
            .map_err(|error| format!("ripr.toml: {error}"))?;
    }
    let decoy = launch.join("decoy");
    std::fs::create_dir_all(&decoy).map_err(|error| format!("decoy: {error}"))?;
    fixture_git_ok(&decoy, &["init", "-q"])?;
    Ok((guard, root, decoy))
}

fn ripr_output(launch: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(launch)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn ripr(launch: &Path, args: &[&str]) -> Result<(bool, String), String> {
    let output = ripr_output(launch, args)?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), text))
}

fn decoy_untouched(decoy: &Path) -> Result<(), String> {
    let status = fixture_git_output(decoy, &["status", "--porcelain", "--ignored"])?;
    if status.trim().is_empty() && !decoy.join("target").exists() {
        return Ok(());
    }
    Err(format!("decoy launch root was written: {status:?}"))
}

fn expect_exit(ok: bool, want_success: bool, label: &str) -> Result<(), String> {
    if ok == want_success {
        return Ok(());
    }
    Err(format!(
        "{label}: expected success={want_success}, got success={ok}"
    ))
}

fn need<'a>(text: &'a str, needle: &str, label: &str) -> Result<&'a str, String> {
    if text.contains(needle) {
        return Ok(text);
    }
    Err(format!("{label}: expected `{needle}` in:\n{text}"))
}

fn attempt_id_of(manifest: &serde_json::Value) -> Result<String, String> {
    manifest["repair_attempt_id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "manifest has no repair_attempt_id".to_string())
}

struct GapAttempt {
    _guard: TempDirGuard,
    root: PathBuf,
    root_arg: String,
    decoy: PathBuf,
    gap: String,
    attempt_id: String,
}

impl GapAttempt {
    fn start(label: &str) -> Result<Self, String> {
        let (guard, root, decoy) = materialize(
            "blind_journey_installed_typescript",
            "head",
            label,
            Some("[languages]\nenabled = [\"typescript\"]\n"),
        )?;
        let root_arg = root.to_string_lossy().into_owned();
        let (ok, check) = ripr(&decoy, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
        expect_exit(ok, true, "check")?;
        let gap = check
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("ripr agent repair --gap-id "))
            .and_then(|line| line.split_whitespace().nth(4))
            .ok_or_else(|| format!("check printed no gap-bound start:\n{check}"))?
            .to_string();
        let (ok, started) = ripr(
            &decoy,
            &[
                "agent", "repair", "--gap-id", &gap, "--base", "HEAD~1", "--root", &root_arg,
                "--phase", "before",
            ],
        )?;
        expect_exit(ok, true, &format!("gap-bound before phase: {started}"))?;
        let attempt = Self {
            _guard: guard,
            root,
            root_arg,
            decoy,
            gap,
            attempt_id: String::new(),
        };
        let manifest = attempt.manifest_for_only_attempt()?;
        Ok(Self {
            attempt_id: attempt_id_of(&manifest)?,
            ..attempt
        })
    }

    fn manifest_for_only_attempt(&self) -> Result<serde_json::Value, String> {
        let attempts = self.root.join("target/ripr/repair-attempts");
        let mut found = Vec::new();
        for entry in
            std::fs::read_dir(&attempts).map_err(|error| format!("attempts dir: {error}"))?
        {
            let path = entry.map_err(|error| format!("entry: {error}"))?.path();
            if path.join("attempt.json").is_file() {
                found.push(path.join("attempt.json"));
            }
        }
        if found.len() != 1 {
            return Err(format!("expected one attempt, got {found:?}"));
        }
        let text =
            std::fs::read_to_string(&found[0]).map_err(|error| format!("manifest: {error}"))?;
        serde_json::from_str(&text).map_err(|error| format!("manifest json: {error}"))
    }

    fn commit_closing_test(&self) -> Result<(), String> {
        self.commit_test_edit(
            "  it(\"discounts exactly at the threshold\", () => {\n    expect(discountedTotal(10000)).toBe(9500);\n  });\n\n  it(\"discounts an above-threshold subtotal\"",
            "pin the threshold",
        )
    }

    fn commit_test_edit(&self, insertion: &str, message: &str) -> Result<(), String> {
        self.write_test_edit(insertion)?;
        fixture_git_ok(&self.root, &["add", "test/pricing.test.ts"])?;
        fixture_git_ok(&self.root, &["commit", "-q", "-m", message])
    }

    fn write_test_edit(&self, insertion: &str) -> Result<(), String> {
        let test_file = self.root.join("test/pricing.test.ts");
        let original =
            std::fs::read_to_string(&test_file).map_err(|error| format!("read test: {error}"))?;
        let edited = original.replacen(
            "  it(\"discounts an above-threshold subtotal\"",
            insertion,
            1,
        );
        if edited == original {
            return Err("test edit did not apply".to_string());
        }
        std::fs::write(&test_file, edited).map_err(|error| format!("write test: {error}"))
    }
}

struct ChildGuard(std::process::Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Clone, Copy, Debug)]
enum Boundary {
    Backup,
    ProjectionRewrite,
    DurableFinish,
    Receipt,
    Retention,
}

impl Boundary {
    fn label(self) -> &'static str {
        match self {
            Self::Backup => "backup present",
            Self::ProjectionRewrite => "projection rewritten",
            Self::DurableFinish => "durable finish record",
            Self::Receipt => "receipt publication",
            Self::Retention => "terminal retention",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Sighting {
    Before,
    At,
    Past,
}

fn projection_path(root: &Path) -> PathBuf {
    root.join("target/ripr/workflow/after.check-findings.json")
}

fn backup_path(root: &Path) -> PathBuf {
    root.join("target/ripr/workflow/after.check-findings.json.ripr-preattempt-backup")
}

fn receipt_path(root: &Path) -> PathBuf {
    root.join("target/ripr/reports/agent-receipt.json")
}

fn plant_prior_projections(root: &Path) -> Result<(), String> {
    let workflow = root.join("target/ripr/workflow");
    std::fs::create_dir_all(&workflow).map_err(|error| format!("workflow dir: {error}"))?;
    std::fs::write(projection_path(root), PRIOR_PROJECTION)
        .map_err(|error| format!("plant projection: {error}"))?;
    std::fs::write(workflow.join("agent-verify.json"), PRIOR_VERIFY)
        .map_err(|error| format!("plant verify: {error}"))?;
    std::fs::write(workflow.join("analysis-outcome.json"), PRIOR_OUTCOME)
        .map_err(|error| format!("plant outcome: {error}"))
}

fn manifest_state(manifest: &serde_json::Value) -> &str {
    manifest["state"].as_str().unwrap_or_default()
}

fn has_after_verdict(manifest: &serde_json::Value) -> bool {
    manifest.get("after").is_some_and(|value| value.is_object())
}

fn terminal_roles(manifest: &serde_json::Value) -> Vec<&str> {
    manifest["terminal_artifacts"]
        .as_array()
        .map(|artifacts| {
            artifacts
                .iter()
                .filter_map(|artifact| artifact["role"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

fn receipt_bound_to(root: &Path, attempt_id: &str) -> bool {
    let Ok(bytes) = std::fs::read(receipt_path(root)) else {
        return false;
    };
    let Ok(receipt) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    receipt["repair_attempt"]["attempt_id"].as_str() == Some(attempt_id)
}

fn sighting(root: &Path, attempt_id: &str, boundary: Boundary) -> Result<Sighting, String> {
    let backup = std::fs::read(backup_path(root)).ok();
    let projection = std::fs::read(projection_path(root)).ok();
    let manifest = match std::fs::read_to_string(
        root.join("target/ripr/repair-attempts")
            .join(attempt_id)
            .join("attempt.json"),
    ) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|error| format!("manifest json: {error}"))?,
        Err(_) => return Ok(Sighting::Before),
    };
    let finished = manifest_state(&manifest) == "ready_to_finish" || has_after_verdict(&manifest);
    let receipt = receipt_bound_to(root, attempt_id);
    let retained = !terminal_roles(&manifest).is_empty();
    Ok(match boundary {
        Boundary::Backup => {
            // Exclusive create then write: a short prefix can be visible
            // before the copy is complete. Wait for the exact planted bytes.
            if backup.as_deref() == Some(PRIOR_PROJECTION) {
                Sighting::At
            } else {
                Sighting::Before
            }
        }
        Boundary::ProjectionRewrite => {
            if projection.as_deref() != Some(PRIOR_PROJECTION) && projection.is_some() && !finished
            {
                Sighting::At
            } else if finished {
                Sighting::Past
            } else {
                Sighting::Before
            }
        }
        Boundary::DurableFinish => {
            if finished && !receipt {
                Sighting::At
            } else if receipt {
                Sighting::Past
            } else {
                Sighting::Before
            }
        }
        Boundary::Receipt => {
            if receipt && !retained {
                Sighting::At
            } else if retained {
                Sighting::Past
            } else {
                Sighting::Before
            }
        }
        Boundary::Retention => {
            let retained_receipt = root
                .join("target/ripr/repair-attempts")
                .join(attempt_id)
                .join("artifacts")
                .join("agent-receipt.json")
                .is_file();
            if retained_receipt && !retained {
                Sighting::At
            } else if retained {
                Sighting::Past
            } else {
                Sighting::Before
            }
        }
    })
}

fn spawn_after(attempt: &GapAttempt) -> Result<ChildGuard, String> {
    let child = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(&attempt.decoy)
        .env("LLVM_PROFILE_FILE", "/dev/null")
        .args([
            "agent",
            "repair",
            "--root",
            &attempt.root_arg,
            "--phase",
            "after",
            "--attempt",
            &attempt.attempt_id,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(ChildGuard(child))
}

fn stop_then_kill_at(
    attempt: &GapAttempt,
    boundary: Boundary,
    child: &mut ChildGuard,
) -> Result<Vec<u8>, String> {
    use nix::sys::signal::{Signal, kill};
    use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
    use nix::unistd::Pid;

    let pid = Pid::from_raw(i32::try_from(child.0.id()).map_err(|error| error.to_string())?);
    let deadline = Instant::now() + HANG_DEADLINE;
    loop {
        if let Some(status) = child.0.try_wait().map_err(|error| error.to_string())? {
            return Err(format!(
                "after process exited before {}: {status}",
                boundary.label()
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "after process did not reach {} before the hang detector",
                boundary.label()
            ));
        }
        match sighting(&attempt.root, &attempt.attempt_id, boundary)? {
            Sighting::At => break,
            Sighting::Past => {
                return Err(format!(
                    "boundary {} cannot be observed from the filesystem without a fault-injection hook in crates/ripr/src/cli/commands/agent.rs",
                    boundary.label()
                ));
            }
            Sighting::Before => std::thread::yield_now(),
        }
    }
    kill(pid, Signal::SIGSTOP).map_err(|error| error.to_string())?;
    if !matches!(
        waitpid(pid, Some(WaitPidFlag::WUNTRACED)).map_err(|error| error.to_string())?,
        WaitStatus::Stopped(_, Signal::SIGSTOP)
    ) {
        return Err("after process did not stop".into());
    }
    match sighting(&attempt.root, &attempt.attempt_id, boundary)? {
        Sighting::At => {}
        Sighting::Past => {
            return Err(format!(
                "boundary {} advanced past the exclusive window after SIGSTOP; not isolatable without a fault-injection hook",
                boundary.label()
            ));
        }
        Sighting::Before => {
            return Err(format!(
                "boundary {} was not visible after SIGSTOP",
                boundary.label()
            ));
        }
    }
    let saved_backup = std::fs::read(backup_path(&attempt.root))
        .map_err(|error| format!("backup missing at {}: {error}", boundary.label()))?;
    child.0.kill().map_err(|error| error.to_string())?;
    child.0.wait().map_err(|error| error.to_string())?;
    Ok(saved_backup)
}

fn status_row(attempt: &GapAttempt) -> Result<serde_json::Value, String> {
    let (ok, text) = ripr(
        &attempt.decoy,
        &["agent", "status", "--root", &attempt.root_arg, "--json"],
    )?;
    expect_exit(ok, true, "agent status")?;
    let document: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("status json: {error}: {text}"))?;
    document["repair_attempts"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["attempt_id"].as_str() == Some(attempt.attempt_id.as_str()))
        })
        .cloned()
        .ok_or_else(|| format!("attempt missing from status: {text}"))
}

fn assert_terminated_not_current(attempt: &GapAttempt) -> Result<serde_json::Value, String> {
    let row = status_row(attempt)?;
    if row["disposition"] == "finished" && row["head_current"] == true {
        return Err(format!(
            "FAIL-OPEN: terminated attempt reads current: {row}"
        ));
    }
    if row["disposition"] == "finished" {
        return Err(format!(
            "terminated attempt disposition is finished without a live producer: {row}"
        ));
    }
    Ok(row)
}

fn continue_terminated(attempt: &GapAttempt) -> Result<String, String> {
    let output = ripr_output(
        &attempt.decoy,
        &[
            "continue",
            "--root",
            &attempt.root_arg,
            "--attempt",
            &attempt.attempt_id,
        ],
    )?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        return Err(format!(
            "continue of a terminated attempt succeeded:\n{text}"
        ));
    }
    Ok(text)
}

fn relocate_unknown_backups(root: &Path, expected: &[u8]) -> Result<(), String> {
    let workflow = root.join("target/ripr/workflow");
    if !workflow.is_dir() {
        return Ok(());
    }
    let mut found = false;
    for entry in std::fs::read_dir(&workflow).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if !name.contains(BACKUP_SUFFIX) || name.ends_with(".inspected") {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        if path.file_name() == backup_path(root).file_name() && bytes.as_slice() != expected {
            return Err("unknown backup was mutated before inspection".into());
        }
        let inspected = path.with_file_name(format!("{name}.inspected"));
        std::fs::rename(&path, &inspected).map_err(|error| error.to_string())?;
        found = true;
    }
    if !found {
        return Err("expected an unknown backup to preserve".into());
    }
    Ok(())
}

fn recover_with_fresh_attempt(terminated: &GapAttempt, base: &str) -> Result<String, String> {
    let test_path = terminated.root.join("test/pricing.test.ts");
    let repaired = std::fs::read_to_string(&test_path).map_err(|error| error.to_string())?;
    let needle = "  it(\"discounts exactly at the threshold\", () => {\n    expect(discountedTotal(10000)).toBe(9500);\n  });\n\n";
    if !repaired.contains(needle) {
        return Err("closing test missing before recovery".into());
    }
    std::fs::write(&test_path, repaired.replacen(needle, "", 1))
        .map_err(|error| error.to_string())?;
    fixture_git_ok(&terminated.root, &["add", "test/pricing.test.ts"])?;
    fixture_git_ok(&terminated.root, &["commit", "-q", "-m", "reopen gap"])?;
    let (ok, before) = ripr(
        &terminated.decoy,
        &[
            "agent",
            "repair",
            "--gap-id",
            &terminated.gap,
            "--base",
            base,
            "--root",
            &terminated.root_arg,
            "--phase",
            "before",
        ],
    )?;
    expect_exit(ok, true, &format!("fresh before: {before}"))?;
    let attempts = terminated.root.join("target/ripr/repair-attempts");
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(&attempts).map_err(|error| error.to_string())? {
        let name = entry
            .map_err(|error| error.to_string())?
            .file_name()
            .into_string()
            .map_err(|_name| "non-utf8 attempt dir".to_string())?;
        if name.starts_with("repair-attempt-") && name != terminated.attempt_id {
            ids.push(name);
        }
    }
    if ids.len() != 1 {
        return Err(format!("expected one fresh attempt, got {ids:?}"));
    }
    let fresh = ids.remove(0);
    std::fs::write(&test_path, repaired).map_err(|error| error.to_string())?;
    fixture_git_ok(&terminated.root, &["add", "test/pricing.test.ts"])?;
    fixture_git_ok(&terminated.root, &["commit", "-q", "-m", "reclose gap"])?;
    let output = ripr_output(
        &terminated.decoy,
        &[
            "agent",
            "repair",
            "--root",
            &terminated.root_arg,
            "--phase",
            "after",
            "--attempt",
            &fresh,
        ],
    )?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    expect_exit(
        output.status.success(),
        true,
        &format!("fresh after: {text}"),
    )?;
    Ok(fresh)
}

fn assert_fresh_current(terminated: &GapAttempt, fresh: &str) -> Result<(), String> {
    let (ok, text) = ripr(
        &terminated.decoy,
        &["agent", "status", "--root", &terminated.root_arg, "--json"],
    )?;
    expect_exit(ok, true, "status after recovery")?;
    let document: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("status json: {error}: {text}"))?;
    let rows = document["repair_attempts"]
        .as_array()
        .ok_or("missing attempts")?;
    let terminated_row = rows
        .iter()
        .find(|row| row["attempt_id"].as_str() == Some(terminated.attempt_id.as_str()))
        .ok_or("terminated attempt missing after recovery")?;
    if terminated_row["disposition"] == "finished" && terminated_row["head_current"] == true {
        return Err(format!(
            "FAIL-OPEN: terminated attempt became current after recovery: {terminated_row}"
        ));
    }
    let fresh_row = rows
        .iter()
        .find(|row| row["attempt_id"].as_str() == Some(fresh))
        .ok_or("fresh attempt missing")?;
    if fresh_row["disposition"] != "finished" || fresh_row["head_current"] != true {
        return Err(format!(
            "fresh attempt did not recover as current: {fresh_row}"
        ));
    }
    Ok(())
}

fn interrupt_at(boundary: Boundary, label: &str) -> Result<(), String> {
    let attempt = GapAttempt::start(label)?;
    let base = attempt.manifest_for_only_attempt()?["gap_binding"]["base_commit"]
        .as_str()
        .ok_or("missing immutable base")?
        .to_string();
    plant_prior_projections(&attempt.root)?;
    attempt.commit_closing_test()?;
    let mut child = spawn_after(&attempt)?;
    let saved_backup = stop_then_kill_at(&attempt, boundary, &mut child)?;
    if std::fs::read(backup_path(&attempt.root)).map_err(|error| error.to_string())? != saved_backup
    {
        return Err("SIGKILL mutated the unknown backup".into());
    }
    let row = assert_terminated_not_current(&attempt)?;
    let continued = continue_terminated(&attempt)?;
    if continued.contains("already complete") {
        return Err(format!(
            "FAIL-OPEN: continue treated the terminated attempt as complete:\n{continued}"
        ));
    }
    if row["disposition"] == "resumable" {
        need(
            &continued,
            "unresolved workflow backup",
            "continue after live backup kill",
        )?;
    }
    if std::fs::read(backup_path(&attempt.root)).map_err(|error| error.to_string())? != saved_backup
    {
        return Err("status/continue mutated the unknown backup".into());
    }
    relocate_unknown_backups(&attempt.root, &saved_backup)?;
    let fresh = recover_with_fresh_attempt(&attempt, &base)?;
    assert_fresh_current(&attempt, &fresh)?;
    decoy_untouched(&attempt.decoy)
}

#[test]
fn after_terminated_while_backup_present_stays_unknown() -> Result<(), String> {
    interrupt_at(Boundary::Backup, "interrupt-backup")
}

#[test]
fn after_terminated_when_projection_rewritten_stays_unknown() -> Result<(), String> {
    interrupt_at(Boundary::ProjectionRewrite, "interrupt-projection")
}

#[test]
fn after_terminated_at_durable_finish_stays_unknown() -> Result<(), String> {
    interrupt_at(Boundary::DurableFinish, "interrupt-finish")
}

#[test]
fn after_terminated_at_receipt_publication_stays_unknown() -> Result<(), String> {
    interrupt_at(Boundary::Receipt, "interrupt-receipt")
}

#[test]
fn after_terminated_at_terminal_retention_stays_unknown() -> Result<(), String> {
    interrupt_at(Boundary::Retention, "interrupt-retention")
}
