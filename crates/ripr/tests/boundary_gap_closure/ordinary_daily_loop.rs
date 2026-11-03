//! Ordinary developer loop from Quickstart, without a governed repair attempt.
//! Reuse the existing boundary fixture and process owner; the native test run
//! and static evidence remain separate observations (#10841, parent #10690).
//!
//! With all RIPR_ORDINARY_* inputs absent this is a copied-Cargo-binary
//! regression. The package lane opts into strict installed input with
//! RIPR_ORDINARY_MODE=installed, RIPR_ORDINARY_INSTALLED_BINARY (absolute),
//! RIPR_ORDINARY_EXPECTED_SHA256 (64 hex digits), RIPR_ORDINARY_EXPECTED_VERSION
//! (complete version line), and RIPR_ORDINARY_EVIDENCE_DIR (absolute, fresh).
//! Partial input refuses; a strict run never substitutes CARGO_BIN_EXE_ripr.

use super::assert_nonzero_intended_execution;
use super::boundary_gap::{
    AFTER_TESTS, BEFORE_TESTS, EQUALITY_SELECTOR, FocusedExecution, InstalledCandidate,
    JourneyRoots, LIB_CLOSED, LIB_OPEN, SIBLING_TESTS, boundary_finding, decoy_was_invoked,
    file_digest, journey_roots, ordinary_candidate, parse_json, path_arg, plant_path_decoy,
    require_success, run_focused_cargo_test_recorded, run_installed, run_printed_ripr, stdout_text,
    two_commit_before_repo, write_crate,
};
use super::common::fixture_git::fixture_git_output;
use serde_json::Value;
use std::process::Output;

fn check(
    candidate: &InstalledCandidate,
    roots: &JourneyRoots,
    base: &str,
    format: &str,
) -> Result<Output, String> {
    let output = run_installed(
        candidate,
        &roots.decoy,
        &roots.decoy,
        &[
            "check",
            "--root",
            &path_arg(&roots.fixture),
            "--base",
            base,
            "--worktree",
            "--format",
            format,
        ],
    )?;
    require_success("ordinary check --base --worktree", &output)?;
    Ok(output)
}

fn report(
    candidate: &InstalledCandidate,
    roots: &JourneyRoots,
    base: &str,
) -> Result<Value, String> {
    parse_json(
        "ordinary check",
        &stdout_text("ordinary check", &check(candidate, roots, base, "json")?)?,
    )
}

fn assert_same_subject(before: &Value, after: &Value) -> Result<(), String> {
    let before_gap = boundary_finding(before)?;
    let after_gap = boundary_finding(after)?;
    assert_eq!(after.get("base_commit"), before.get("base_commit"));
    assert_eq!(after.get("head"), before.get("head"));
    assert_eq!(after.get("root"), before.get("root"));
    assert_eq!(after.get("mode"), before.get("mode"));
    assert_eq!(
        after_gap.get("canonical_gap"),
        before_gap.get("canonical_gap")
    );
    assert_eq!(after_gap.get("probe"), before_gap.get("probe"));
    assert_eq!(after_gap["source_currentness"], "candidate_current");
    assert_eq!(after["analysis_outcome"]["analysis_complete"], true);
    assert_eq!(after["summary"]["probes"], 1);
    assert_ne!(
        after.pointer("/analysis_outcome/outcome/identity/input_identity"),
        before.pointer("/analysis_outcome/outcome/identity/input_identity"),
        "same-HEAD test edits must refresh the analysis input"
    );
    Ok(())
}

fn native_test(
    candidate: &InstalledCandidate,
    roots: &JourneyRoots,
    selector: &str,
    record: &str,
) -> Result<FocusedExecution, String> {
    let execution = run_focused_cargo_test_recorded(
        &roots.fixture,
        &roots.cargo_target,
        selector,
        candidate.evidence.as_ref(),
    )?;
    candidate.record(record, &execution.as_json())?;
    Ok(execution)
}

fn fixture_subject(roots: &JourneyRoots, base: &str) -> Result<Value, String> {
    Ok(serde_json::json!({
        "base": base,
        "base_tree": fixture_git_output(&roots.fixture, &["rev-parse", &format!("{base}^{{tree}}")])?.trim(),
        "head": fixture_git_output(&roots.fixture, &["rev-parse", "HEAD"])?.trim(),
        "head_tree": fixture_git_output(&roots.fixture, &["rev-parse", "HEAD^{tree}"])?.trim(),
        "production_sha256": file_digest(&roots.fixture.join("src/lib.rs"))?,
        "manifest_sha256": file_digest(&roots.fixture.join("Cargo.toml"))?,
        "lockfile_sha256": file_digest(&roots.fixture.join("Cargo.lock"))?,
    }))
}

fn assert_one_native_failure(execution: &FocusedExecution) {
    assert_eq!(execution.selected, 1);
    assert_eq!(execution.executed, 1);
    assert_eq!(execution.failed, 1);
    assert_eq!(execution.passed, 0);
    assert_eq!(execution.ignored, 0);
    assert!(execution.exit_code.is_some_and(|code| code != 0));
}

#[test]
fn ordinary_loop_preserves_production_subject_and_native_outcome() -> Result<(), String> {
    let roots = journey_roots("ordinary-daily-loop")?;
    let marker = plant_path_decoy(&roots.decoy)?;
    let candidate = ordinary_candidate(&roots)?;
    let base = two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;
    let subject = fixture_subject(&roots, &base)?;
    candidate.record("subject-before.json", &subject)?;
    candidate.record_source("production.rs", LIB_CLOSED.as_bytes())?;
    candidate.record_source("tests-before.rs", BEFORE_TESTS.as_bytes())?;
    for name in ["Cargo.toml", "Cargo.lock"] {
        candidate.record_source(
            name,
            &std::fs::read(roots.fixture.join(name)).map_err(|error| error.to_string())?,
        )?;
    }
    let before = report(&candidate, &roots, &base)?;
    candidate.record("before.json", &before)?;
    let gap = boundary_finding(&before)?;
    assert_eq!(before["base_commit"], base);
    assert_eq!(before["head"]["source"], "working_tree");
    assert_eq!(before["mode"], "draft");
    assert_eq!(before["summary"]["probes"], 1);
    assert_eq!(before["analysis_outcome"]["analysis_complete"], true);
    assert_eq!(gap["classification"], "weakly_exposed");

    let warm = report(&candidate, &roots, &base)?;
    candidate.record("warm.json", &warm)?;
    assert_eq!(
        warm, before,
        "an unchanged warm refresh must retain the same subject and evidence"
    );

    let human = stdout_text(
        "ordinary human check",
        &check(&candidate, &roots, &base, "human")?,
    )?;
    for command in ["explain", "context"] {
        let prefix = format!("ripr {command} ");
        let printed = human
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with(&prefix))
            .ok_or_else(|| format!("ordinary check did not print {command}: {human}"))?;
        let output = run_printed_ripr(&candidate, &roots.decoy, &roots.decoy, printed)?;
        require_success(printed, &output)?;
        let text = stdout_text(printed, &output)?;
        if command == "context" {
            let context = parse_json("printed context", &text)?;
            assert_eq!(context["canonical_gap_id"], gap["canonical_gap_id"]);
            assert_eq!(context["source_currentness"], "candidate_current");
            assert_eq!(context["classification"], "weakly_exposed");
            assert_eq!(context["witness"]["before"], "amount > discount_threshold");
            assert_eq!(context["witness"]["after"], "amount >= discount_threshold");
        } else {
            assert!(text.contains("amount > discount_threshold"));
            assert!(text.contains("amount >= discount_threshold"));
        }
    }

    // A passing test at the wrong input does not close the original gap.
    let test_file = roots.fixture.join("tests/pricing.rs");
    std::fs::write(&test_file, SIBLING_TESTS).map_err(|error| error.to_string())?;
    candidate.record_source("tests-wrong-input.rs", SIBLING_TESTS.as_bytes())?;
    let sibling_run = native_test(
        &candidate,
        &roots,
        EQUALITY_SELECTOR,
        "native-wrong-input.json",
    )?;
    assert_nonzero_intended_execution(&sibling_run)?;
    let sibling = report(&candidate, &roots, &base)?;
    candidate.record("wrong-input.json", &sibling)?;
    assert_same_subject(&before, &sibling)?;
    assert_eq!(
        boundary_finding(&sibling)?["classification"],
        "weakly_exposed"
    );

    // Switching to HEAD hides the committed production change. A quiet
    // test-only comparison is the wrong subject, not a successful improvement.
    let test_only = report(&candidate, &roots, "HEAD")?;
    candidate.record("wrong-base.json", &test_only)?;
    assert_ne!(test_only["base_commit"], before["base_commit"]);
    assert_eq!(test_only["summary"]["probes"], 0);
    assert_eq!(test_only["findings"], serde_json::json!([]));
    assert_eq!(
        test_only["analysis_outcome"]["outcome"]["kind"],
        "no_behavioral_candidates"
    );

    // Keep the intended selector while removing its test, so this control
    // discriminates zero execution independently of wrong-selector refusal.
    std::fs::write(&test_file, BEFORE_TESTS).map_err(|error| error.to_string())?;
    candidate.record_source("tests-zero-selection.rs", BEFORE_TESTS.as_bytes())?;
    let empty = native_test(
        &candidate,
        &roots,
        EQUALITY_SELECTOR,
        "native-zero-selection.json",
    )?;
    assert_eq!(empty.exit_code, Some(0));
    assert_eq!(empty.selected, 0);
    assert_eq!(empty.executed, 0);
    assert!(assert_nonzero_intended_execution(&empty).is_err());
    std::fs::write(&test_file, AFTER_TESTS).map_err(|error| error.to_string())?;
    candidate.record_source("tests-after.rs", AFTER_TESTS.as_bytes())?;
    let execution = native_test(&candidate, &roots, EQUALITY_SELECTOR, "native-after.json")?;
    assert_nonzero_intended_execution(&execution)?;
    assert_eq!(execution.executed, 1);
    assert_eq!(execution.ignored, 0);
    let after = report(&candidate, &roots, &base)?;
    candidate.record("after.json", &after)?;
    assert_same_subject(&before, &after)?;
    assert_eq!(boundary_finding(&after)?["classification"], "exposed");
    assert_eq!(
        boundary_finding(&after)?["missing_discriminators"],
        serde_json::json!([])
    );

    // The wrong expected value statically contradicts the owner's folded
    // behavior, so the whole-test contradiction control withholds the
    // diff-mode credit exactly as the repo surface does (#7007, #10980):
    // a test that fails at baseline cannot claim the change is noticed.
    // The native equality test still fails, and the report subject itself
    // is unchanged.
    let wrong_expectation = AFTER_TESTS.replace(
        "discounted_total(100, 100), 90",
        "discounted_total(100, 100), 91",
    );
    std::fs::write(&test_file, &wrong_expectation).map_err(|error| error.to_string())?;
    candidate.record_source("tests-wrong-expected.rs", wrong_expectation.as_bytes())?;
    let failed = native_test(
        &candidate,
        &roots,
        EQUALITY_SELECTOR,
        "native-wrong-expected.json",
    )?;
    assert_one_native_failure(&failed);
    let failed_static = report(&candidate, &roots, &base)?;
    candidate.record("wrong-expected.json", &failed_static)?;
    assert_same_subject(&after, &failed_static)?;
    assert_eq!(
        boundary_finding(&failed_static)?["classification"],
        "weakly_exposed"
    );
    assert_eq!(
        std::fs::read_to_string(roots.fixture.join("src/lib.rs"))
            .map_err(|error| error.to_string())?,
        LIB_CLOSED
    );

    std::fs::write(&test_file, AFTER_TESTS).map_err(|error| error.to_string())?;
    let restored_execution = native_test(
        &candidate,
        &roots,
        EQUALITY_SELECTOR,
        "native-restored.json",
    )?;
    assert_nonzero_intended_execution(&restored_execution)?;
    let restored = report(&candidate, &roots, &base)?;
    candidate.record("restored.json", &restored)?;
    assert_eq!(
        restored, after,
        "restoring exact test bytes must restore the positive report"
    );

    // A separate native-only fixture establishes the assertion's discriminator:
    // the unchanged equality assertion fails with the original `>` behavior.
    // It never changes the production file or comparison subject above.
    let original = journey_roots("ordinary-original-predicate-control")?;
    write_crate(&original.fixture, LIB_OPEN, AFTER_TESTS)?;
    candidate.record_source("original-predicate-control.rs", LIB_OPEN.as_bytes())?;
    let original_failure = native_test(
        &candidate,
        &original,
        EQUALITY_SELECTOR,
        "native-original-predicate.json",
    )?;
    assert_one_native_failure(&original_failure);
    original.cleanup()?;

    let after_subject = fixture_subject(&roots, &base)?;
    candidate.record("subject-after.json", &after_subject)?;
    assert_eq!(after_subject, subject);
    let changed = fixture_git_output(&roots.fixture, &["diff", "--name-only", "HEAD"])?;
    assert_eq!(changed.trim(), "tests/pricing.rs");
    let delta = fixture_git_output(
        &roots.fixture,
        &["diff", "--no-ext-diff", "HEAD", "--", "tests/pricing.rs"],
    )?;
    candidate.record_source("test-change.diff", delta.as_bytes())?;
    assert_eq!(
        std::fs::read_to_string(&test_file).map_err(|error| error.to_string())?,
        AFTER_TESTS
    );
    assert!(!decoy_was_invoked(&marker));
    candidate.verify_final_identity()?;
    roots.cleanup()?;
    candidate.record("completed.json", &serde_json::json!({
        "state": "completed", "input_kind": "admitted_installed_executable",
        "subject": subject, "positive_native": execution.as_json(),
        "before_classification": "weakly_exposed", "after_classification": "exposed",
        "disposable_roots_removed": true,
        "claim": "controlled ordinary fixture; no governed repair, adoption, or release-readiness claim",
    }))?;
    Ok(())
}

#[test]
fn strict_installed_inputs_never_fall_back_on_partial_or_malformed_input() -> Result<(), String> {
    use super::boundary_gap::{ORDINARY_INPUT_KEYS, OrdinaryMode, ordinary_mode};
    use std::collections::BTreeMap;
    use std::ffi::OsString;

    assert_eq!(ordinary_mode(|_| None)?, OrdinaryMode::CopiedCargoBinary);
    let values = [
        "installed".to_string(),
        path_arg(&std::env::temp_dir().join("installed-ripr")),
        "a".repeat(64),
        format!("ripr 0.11.0 ({})", "b".repeat(40)),
        path_arg(&std::env::temp_dir().join("fresh-ordinary-evidence")),
    ];
    let complete: BTreeMap<_, _> = ORDINARY_INPUT_KEYS.into_iter().zip(values).collect();
    let parse = |inputs: &BTreeMap<&str, String>| {
        ordinary_mode(|key| inputs.get(key).map(|value| OsString::from(value.as_str())))
    };
    assert!(matches!(parse(&complete)?, OrdinaryMode::Installed(_)));
    for key in ORDINARY_INPUT_KEYS {
        let mut partial = complete.clone();
        partial.remove(key);
        assert!(parse(&partial).is_err(), "missing {key} must refuse");
    }
    for (key, value) in [
        ("RIPR_ORDINARY_MODE", "copied"),
        ("RIPR_ORDINARY_INSTALLED_BINARY", "relative/ripr"),
        ("RIPR_ORDINARY_EVIDENCE_DIR", "relative/evidence"),
        ("RIPR_ORDINARY_EXPECTED_SHA256", "short"),
        ("RIPR_ORDINARY_EXPECTED_VERSION", "ripr 0.11.0"),
        ("RIPR_ORDINARY_EXPECTED_VERSION", "ripr 0.11.0 (1234567)"),
        ("RIPR_ORDINARY_MODE", ""),
    ] {
        let mut invalid = complete.clone();
        invalid.insert(key, value.to_string());
        assert!(parse(&invalid).is_err(), "{key}={value:?} must refuse");
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let invalid = ordinary_mode(|key| {
            if key == "RIPR_ORDINARY_MODE" {
                Some(OsString::from_vec(vec![0xff]))
            } else {
                complete
                    .get(key)
                    .map(|value| OsString::from(value.as_str()))
            }
        });
        let Err(error) = invalid else {
            return Err("non-UTF-8 installed input was admitted".into());
        };
        assert!(error.contains("RIPR_ORDINARY_MODE"), "{error}");
        assert!(error.contains("UTF-8"), "{error}");
        assert!(error.contains("1 OS bytes supplied"), "{error}");
    }
    Ok(())
}

#[test]
fn strict_installed_admission_binds_path_bytes_and_full_version() -> Result<(), String> {
    use super::boundary_gap::{InstalledInputs, admit_installed_candidate, install_candidate};
    use std::io::Write;
    use std::path::PathBuf;

    // This copied executable is an admission-control fixture. It does not
    // establish wheel installation or package provenance.
    let roots = journey_roots("ordinary-admission-controls")?;
    let copied = install_candidate(&roots.install)?;
    let inputs = |name: &str| InstalledInputs {
        binary: copied.binary.clone(),
        digest: copied.digest.clone(),
        version: copied.version.clone(),
        evidence_dir: roots.decoy.join(name),
    };
    let valid = admit_installed_candidate(inputs("valid"))?;
    valid.verify_final_identity()?;
    assert!(roots.decoy.join("valid/admitted.json").is_file());
    assert!(!roots.decoy.join("valid/completed.json").exists());

    let mut absent = inputs("absent");
    absent.binary = roots.install.join("missing-installed-ripr");
    assert!(admit_installed_candidate(absent).is_err());
    assert!(
        !roots.decoy.join("absent/command-000").exists(),
        "no version or workspace fallback may run"
    );

    let mut relative = inputs("relative");
    relative.binary = PathBuf::from("relative/ripr");
    assert!(admit_installed_candidate(relative).is_err());

    let mut workspace = inputs("workspace");
    workspace.binary = PathBuf::from(env!("CARGO_BIN_EXE_ripr"));
    assert!(admit_installed_candidate(workspace).is_err());

    let mut wrong_digest = inputs("wrong-digest");
    wrong_digest.digest = format!("sha256:{}", "0".repeat(64));
    assert!(admit_installed_candidate(wrong_digest).is_err());
    assert!(!roots.decoy.join("wrong-digest/command-000").exists());

    let mut wrong_version = inputs("wrong-version");
    wrong_version.version = copied.version.replacen("ripr ", "ripr different-", 1);
    assert!(admit_installed_candidate(wrong_version).is_err());
    assert!(
        roots
            .decoy
            .join("wrong-version/command-000/stdout")
            .is_file()
    );
    assert!(!roots.decoy.join("wrong-version/admitted.json").exists());

    let occupied = inputs("occupied");
    std::fs::create_dir(&occupied.evidence_dir).map_err(|error| error.to_string())?;
    let canary = occupied.evidence_dir.join("canary");
    std::fs::write(&canary, b"preserve previous evidence").map_err(|error| error.to_string())?;
    assert!(admit_installed_candidate(occupied).is_err());
    assert_eq!(
        std::fs::read(&canary).map_err(|error| error.to_string())?,
        b"preserve previous evidence"
    );

    // Appending bytes retains the executable's version behavior while changing
    // its artifact identity. A version match alone cannot admit those bytes.
    std::fs::OpenOptions::new()
        .append(true)
        .open(&copied.binary)
        .map_err(|error| error.to_string())?
        .write_all(b"ordinary-digest-control")
        .map_err(|error| error.to_string())?;
    let same_version = run_installed(&copied, &roots.decoy, &roots.decoy, &["--version"])?;
    require_success("changed bytes --version control", &same_version)?;
    assert_eq!(
        stdout_text("changed bytes --version", &same_version)?.trim(),
        copied.version
    );
    assert_ne!(file_digest(&copied.binary)?, copied.digest);
    assert!(admit_installed_candidate(inputs("same-version-different-bytes")).is_err());
    assert!(
        valid.verify_final_identity().is_err(),
        "an admitted path cannot silently change later"
    );
    roots.cleanup()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn process_observation_pid() -> Result<u32, String> {
    // A container can expose a procfs mounted in an ancestor PID namespace.
    // Record the identity in that observation namespace, not getpid(), which
    // could name an unrelated process in this procfs after the child is reaped.
    // These values are observation-only; cleanup remains a cooperative file.
    std::fs::read_link("/proc/self")
        .map_err(|error| format!("resolve process observation identity: {error}"))?
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "procfs self identity is not a numeric filename".to_string())?
        .parse()
        .map_err(|error| format!("parse process observation identity: {error}"))
}

#[cfg(target_os = "linux")]
fn process_stopped(pid: u32) -> Result<bool, String> {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => {
            let state = stat
                .rsplit_once(')')
                .and_then(|(_, fields)| fields.split_whitespace().next())
                .ok_or_else(|| format!("malformed process state for {pid}"))?;
            Ok(matches!(state, "Z" | "X"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(format!("read process {pid}: {error}")),
    }
}

#[cfg(target_os = "linux")]
fn wait_stopped(pid: u32) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !process_stopped(pid)? {
        if std::time::Instant::now() >= deadline {
            return Err(format!("owned fixture process {pid} is still running"));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Ok(())
}

/// An owned file is the negative control's cooperative cleanup signal. It
/// never signals a recycled PID or a process group after its leader was reaped.
#[cfg(target_os = "linux")]
struct ProcessFixture(std::path::PathBuf);

#[cfg(target_os = "linux")]
impl ProcessFixture {
    fn pid(&self, name: &str) -> Result<u32, String> {
        std::fs::read_to_string(self.0.join(name))
            .map_err(|error| error.to_string())?
            .parse()
            .map_err(|error| format!("parse {name}: {error}"))
    }

    fn release(&self) -> Result<(), String> {
        std::fs::write(self.0.join("release"), b"stop").map_err(|error| error.to_string())?;
        wait_stopped(self.pid("descendant.proc-pid")?)
    }
}

#[cfg(target_os = "linux")]
impl Drop for ProcessFixture {
    fn drop(&mut self) {
        // Leave the release file in place if setup failed before its PID was
        // recorded, so a late-starting descendant will immediately stop.
        if self.release().is_ok() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(target_os = "linux")]
fn process_fixture_child(role: &str, root: &std::path::Path) -> Result<(), String> {
    use super::boundary_gap::prepared_command;
    use ripr::process_owner::OwnedProcess;
    use std::ffi::OsStr;
    use std::io::Write;
    use std::time::{Duration, Instant};

    const SELECTOR: &str =
        "ordinary_daily_loop::isolated_commands_reap_primary_and_stop_descendants";
    if role == "descendant" {
        std::fs::write(
            root.join("descendant.proc-pid"),
            process_observation_pid()?.to_string(),
        )
        .map_err(|error| error.to_string())?;
        let rescue = Instant::now() + Duration::from_mins(1);
        while !root.join("release").exists() && Instant::now() < rescue {
            std::thread::sleep(Duration::from_millis(10));
        }
        return Ok(());
    }
    std::fs::write(
        root.join("primary.proc-pid"),
        process_observation_pid()?.to_string(),
    )
    .map_err(|error| error.to_string())?;
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = prepared_command(
        &exe,
        Some(root),
        &[SELECTOR, "--exact", "--nocapture"],
        &[
            (
                OsStr::new("RIPR_ORDINARY_PROCESS_ROLE"),
                OsStr::new("descendant"),
            ),
            (OsStr::new("RIPR_ORDINARY_PROCESS_ROOT"), root.as_os_str()),
        ],
    );
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // The descendant must inherit its parent's group. Default spawn retains
    // that existing contract; the outer isolated owner has custody of it.
    let descendant = OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("descendant.proc-pid").exists() {
        if Instant::now() >= deadline {
            return Err("descendant fixture did not start".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // This fixture deliberately leaves the live descendant to the outer
    // process owner (or the negative control's cooperative release). Dropping
    // its inner guard here would clean it up before containment is exercised.
    let _descendant = std::mem::ManuallyDrop::new(descendant);
    if role == "primary-exits" {
        return Ok(());
    }
    if role == "output-limit" {
        // Each stream remains below four MiB; their combined size exceeds
        // the shared budget. A per-stream admission would wrongly succeed.
        for _ in 0..48 {
            std::io::stdout()
                .write_all(&[b'x'; 64 * 1024])
                .map_err(|error| error.to_string())?;
            std::io::stderr()
                .write_all(&[b'y'; 64 * 1024])
                .map_err(|error| error.to_string())?;
        }
        return Ok(());
    }
    let rescue = Instant::now() + Duration::from_mins(1);
    while !root.join("release").exists() && Instant::now() < rescue {
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn isolated_commands_reap_primary_and_stop_descendants() -> Result<(), String> {
    use super::boundary_gap::{
        CommandEvidence, prepared_command, run_program_recorded, unique_dir,
    };
    use ripr::process_owner::OwnedProcess;
    use std::ffi::OsStr;
    use std::time::{Duration, Instant};

    const SELECTOR: &str =
        "ordinary_daily_loop::isolated_commands_reap_primary_and_stop_descendants";
    if let Some(role) = std::env::var_os("RIPR_ORDINARY_PROCESS_ROLE") {
        let root = std::env::var_os("RIPR_ORDINARY_PROCESS_ROOT")
            .ok_or_else(|| "process fixture root is missing".to_string())?;
        return process_fixture_child(&role.to_string_lossy(), std::path::Path::new(&root));
    }
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let control = ProcessFixture(unique_dir("ordinary-uncontained-control")?);
    let mut command = prepared_command(
        &exe,
        Some(&control.0),
        &[SELECTOR, "--exact", "--nocapture"],
        &[
            (
                OsStr::new("RIPR_ORDINARY_PROCESS_ROLE"),
                OsStr::new("primary-exits"),
            ),
            (
                OsStr::new("RIPR_ORDINARY_PROCESS_ROOT"),
                control.0.as_os_str(),
            ),
        ],
    );
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut uncontained = OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = uncontained.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            return Err("uncontained control primary did not exit".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    let unrelated_pid = control.pid("descendant.proc-pid")?;
    assert!(
        !process_stopped(unrelated_pid)?,
        "default-spawn control must actually leave its descendant alive"
    );

    for role in ["primary-exits", "deadline", "output-limit"] {
        let fixture = ProcessFixture(unique_dir(&format!("ordinary-contained-{role}"))?);
        let evidence = CommandEvidence::create(&fixture.0.join("evidence"))?;
        let output = run_program_recorded(
            &exe,
            Some(&fixture.0),
            &[SELECTOR, "--exact", "--nocapture"],
            &[
                (OsStr::new("RIPR_ORDINARY_PROCESS_ROLE"), OsStr::new(role)),
                (
                    OsStr::new("RIPR_ORDINARY_PROCESS_ROOT"),
                    fixture.0.as_os_str(),
                ),
            ],
            Some(&evidence),
            Duration::from_secs(3),
        );
        let terminal: Value = serde_json::from_slice(
            &std::fs::read(fixture.0.join("evidence/command-000/result.json"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        assert!(terminal["cleanup_error"].is_null(), "{terminal}");
        if role == "primary-exits" {
            require_success("contained primary exit", &output?)?;
            assert_eq!(terminal["native_exit_code"], 0);
            assert!(terminal["harness_stop"].is_null());
        } else {
            let Err(error) = output else {
                return Err(format!("contained {role} unexpectedly succeeded"));
            };
            assert!(
                error.contains(if role == "deadline" {
                    "harness deadline"
                } else {
                    "output"
                }),
                "{error}"
            );
            assert_eq!(
                terminal["harness_stop"],
                if role == "deadline" {
                    "deadline"
                } else {
                    "output_limit"
                }
            );
            if role == "deadline" {
                assert!(
                    terminal["native_exit_code"].is_null(),
                    "outer deadline is not a native test result"
                );
            } else {
                let stdout = std::fs::metadata(fixture.0.join("evidence/command-000/stdout"))
                    .map_err(|error| error.to_string())?
                    .len();
                let stderr = std::fs::metadata(fixture.0.join("evidence/command-000/stderr"))
                    .map_err(|error| error.to_string())?
                    .len();
                assert!(stdout < 4 * 1024 * 1024 && stderr < 4 * 1024 * 1024);
                assert!(stdout + stderr > 4 * 1024 * 1024);
            }
        }
        let primary = fixture.pid("primary.proc-pid")?;
        assert!(
            !std::path::Path::new(&format!("/proc/{primary}")).exists(),
            "primary must be reaped"
        );
        wait_stopped(fixture.pid("descendant.proc-pid")?)?;
        assert!(
            !process_stopped(unrelated_pid)?,
            "isolated cleanup must leave an unrelated same-image process alive"
        );
    }
    control.release()?;
    uncontained.terminate_tree()?;
    Ok(())
}
