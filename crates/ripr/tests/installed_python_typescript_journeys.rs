//! Literal installed-binary walks of the Python and TypeScript fixture
//! journeys (#4518, #4519, parent #4508).
//!
//! The scripted `installed_*` corpus rows never execute `ripr`. These tests
//! materialize the committed `blind_journey_installed_{python,typescript}`
//! snapshots as real git repositories under a root containing a space and a
//! non-ASCII character, then run the built binary from a foreign working
//! directory holding a decoy repository, following only product-emitted
//! routes. They pin what first use actually does today, including the
//! points where the delegated repair loop ends in a named terminal reason, so a
//! change in either direction is a visible decision.
//!
//! Project verification (pytest, vitest) is not executed here: it needs the
//! user's interpreter or `npm install`, and static movement is a separate
//! axis from project verification.

#![cfg(any(feature = "lang-python", feature = "lang-typescript"))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};

/// Owns a launch directory and removes the whole tree on success and on every
/// early error.
struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> Result<TempDirGuard, String> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ripr-installed-{label}-{}-{counter}",
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

/// Builds `<tmp>/<label> ünï/repo` with a base commit then a head commit from
/// the committed fixture snapshots, plus an optional enabling `ripr.toml`.
fn materialize(
    fixture: &str,
    head: &str,
    label: &str,
    ripr_toml: Option<&str>,
) -> Result<(TempDirGuard, PathBuf, PathBuf), String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = manifest_dir
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

/// The canonical TypeScript gap id of the top finding `ripr check` selected,
/// derived from the finding id its drill-in commands print
/// (`probe:<path>:<family>:<fp8>` -> `gap:typescript:<family>:<fp8>`).
#[cfg(feature = "lang-typescript")]
fn selected_typescript_gap(check_output: &str) -> Result<String, String> {
    let id = check_output
        .split_whitespace()
        .find(|token| token.starts_with("probe:") && token.contains(":typescript_preview:"))
        .ok_or_else(|| format!("check printed no finding id:\n{check_output}"))?;
    let parts: Vec<&str> = id.splitn(4, ':').collect();
    if parts.len() != 4 {
        return Err(format!("unexpected finding id `{id}`"));
    }
    Ok(format!("gap:typescript:{}:{}", parts[2], parts[3]))
}

/// Published repair-attempt manifests under the root. The before-phase lock
/// directory exists after any attempt to start; a manifest only after a start.
#[cfg(feature = "lang-typescript")]
fn attempt_manifests(root: &Path) -> Result<usize, String> {
    let attempts = root.join("target").join("ripr").join("repair-attempts");
    if !attempts.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in std::fs::read_dir(&attempts).map_err(|error| format!("attempts dir: {error}"))? {
        let path = entry
            .map_err(|error| format!("attempt entry: {error}"))?
            .path();
        if path.join("attempt.json").is_file() {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(feature = "lang-typescript")]
fn attempt_id_of(manifest: &serde_json::Value) -> Result<String, String> {
    manifest["repair_attempt_id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "manifest has no repair_attempt_id".to_string())
}

#[cfg(feature = "lang-python")]
#[test]
fn python_flat_layout_journey_stays_in_selected_root_and_ends_in_named_terminal_reason()
-> Result<(), String> {
    let (_guard, root, decoy) = materialize("blind_journey_installed_python", "head", "py", None)?;
    let root_arg = root.to_string_lossy().into_owned();

    let (ok, check) = ripr(&decoy, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
    if !ok {
        return Err(format!("check failed:\n{check}"));
    }
    need(
        &check,
        "units == 20",
        "missing discriminator names the equality input",
    )?;
    need(
        &check,
        "Test node: tests/test_pricing.py::test_above_boundary_gets_discount",
        "test node",
    )?;
    // The printed verification route is the module form, never the bare console script.
    need(
        &check,
        "Verify command: python -m pytest tests/test_pricing.py::",
        "module-form verify",
    )?;
    // #9102: the seam inventory behind `repair` is Rust-only, so a Python
    // gap has no seam to bind. The product states that precise terminal
    // reason (not the generic "no seam binds") beside the complete manual
    // route (gap, test node, verify command) printed above.
    need(
        &check,
        "no repair start offered: the seam inventory behind `ripr repair`",
        "check names the terminal reason",
    )?;
    need(
        &check,
        "covers Rust files only, so this Python finding in `pricing/core.py` has no seam",
        "terminal reason names the language and file",
    )?;
    if check.contains("no seam in the finding's file binds") {
        return Err(format!("check kept the generic refusal:\n{check}"));
    }
    // No runnable repair start is offered for the unbindable finding.
    if check.contains("ripr repair probe:") && check.contains("ripr agent repair") {
        return Err(format!("check offered a repair start:\n{check}"));
    }

    // Feed `repair` the exact finding id that `check` printed, not a literal.
    let probe = check
        .split_whitespace()
        .find(|token| token.starts_with("probe:"))
        .ok_or_else(|| format!("check printed no probe id:\n{check}"))?;
    let (repair_ok, repair) = ripr(&decoy, &["repair", probe, "--root", &root_arg])?;
    expect_exit(repair_ok, false, "repair of an unbindable finding id")?;
    need(
        &repair,
        "covers Rust files only, so this Python gap has no seam",
        "repair states the terminal reason for the Python finding id",
    )?;
    if repair.contains("is stale") || repair.contains("stale or maps to no current seam") {
        return Err(format!("repair called the Python id stale:\n{repair}"));
    }

    // The canonical gap id `check` prints is refused the same way by both
    // repair entry points, with the language-specific terminal reason.
    let gap = check
        .split_whitespace()
        .find(|token| token.starts_with("gap:python:"))
        .ok_or_else(|| format!("check printed no python gap id:\n{check}"))?;
    let (gap_ok, gap_repair) = ripr(&decoy, &["repair", gap, "--root", &root_arg])?;
    expect_exit(gap_ok, false, "repair of the python gap id")?;
    need(
        &gap_repair,
        "covers Rust files only",
        "repair of the gap id states the terminal reason",
    )?;
    let (agent_ok, agent_repair) = ripr(
        &decoy,
        &[
            "agent",
            "repair",
            "--seam-id",
            gap,
            "--root",
            &root_arg,
            "--phase",
            "before",
        ],
    )?;
    expect_exit(
        agent_ok,
        false,
        "agent repair --phase before of the python gap id",
    )?;
    need(
        &agent_repair,
        "covers Rust files only",
        "agent repair states the terminal reason",
    )?;

    // Alternate: an id that is not a Python gap or preview finding keeps the
    // generic stale-id guidance and never claims the Python terminal reason.
    let (stale_ok, stale) = ripr(
        &decoy,
        &[
            "repair",
            "probe:src_lib.rs:12:predicate",
            "--root",
            &root_arg,
        ],
    )?;
    expect_exit(stale_ok, false, "repair of an unrelated stale id")?;
    need(
        &stale,
        "stale or maps to no current seam",
        "unrelated id keeps stale-id guidance",
    )?;
    if stale.contains("covers Rust files only") {
        return Err(format!("unrelated id got the Python reason:\n{stale}"));
    }

    let (pilot_ok, pilot) = ripr(&decoy, &["pilot", "--root", &root_arg])?;
    expect_exit(pilot_ok, true, "pilot")?;
    need(
        &pilot,
        "no_python_findings",
        "pilot reports no Python findings for a committed-diff gap",
    )?;

    let (status_ok, status) = ripr(&decoy, &["agent", "status", "--root", &root_arg])?;
    expect_exit(status_ok, true, "agent status")?;
    need(
        &status,
        "Status selects no next command",
        "status terminates without a command",
    )?;
    need(
        &status,
        "pilot_routed_to_check_no_repair_target",
        "status names the terminal reason",
    )?;

    decoy_untouched(&decoy)
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_literal_threshold_packet_is_complete_and_controls_fail_closed() -> Result<(), String>
{
    let toml = "[languages]\nenabled = [\"typescript\"]\n";
    let (_guard, root, decoy) = materialize(
        "blind_journey_installed_typescript",
        "head",
        "ts",
        Some(toml),
    )?;
    let root_arg = root.to_string_lossy().into_owned();

    let (ok, check) = ripr(&decoy, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
    if !ok {
        return Err(format!("check failed:\n{check}"));
    }
    need(
        &check,
        "the repair packet is complete",
        "literal const resolves",
    )?;
    need(&check, "subtotal == DISCOUNT_THRESHOLD", "discriminator")?;
    need(
        &check,
        "expect(discountedTotal(10000)).toBe(expected)",
        "equality input resolved to the literal",
    )?;
    need(
        &check,
        "npx --no-install vitest run test/pricing.test.ts",
        "printed runner command",
    )?;
    // #9103 (RIPR-SPEC-0311): the complete packet now offers a gap-bound
    // repair start, and the manual-only terminal reason is gone for it.
    if check.contains("no repair start offered") {
        return Err(format!("a complete packet still offers no start:\n{check}"));
    }
    let start_line = check
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("ripr agent repair --gap-id "))
        .ok_or_else(|| format!("check printed no gap-bound repair start:\n{check}"))?;
    let gap = start_line
        .split_whitespace()
        .nth(4)
        .ok_or_else(|| format!("start line names no gap: {start_line}"))?
        .to_string();
    if !gap.starts_with("gap:typescript:typescript_preview:") {
        return Err(format!(
            "start line names a non-TypeScript gap: {start_line}"
        ));
    }
    if !start_line.contains("--base ") || !start_line.contains("HEAD~1") {
        return Err(format!("start line base: {start_line}"));
    }
    if !start_line.ends_with(" --phase before") || start_line.contains("--seam-id") {
        return Err(format!(
            "start line is not a gap-bound before start: {start_line}"
        ));
    }
    let (status_ok, status) = ripr(&decoy, &["agent", "status", "--root", &root_arg])?;
    expect_exit(status_ok, true, "agent status")?;
    // Before pilot, status routes to pilot; pilot routes to check; status then
    // terminates with the manual route instead of cycling.
    need(&status, "ripr pilot", "status routes to pilot first")?;
    let (pilot_ok, pilot) = ripr(&decoy, &["pilot", "--root", &root_arg])?;
    expect_exit(pilot_ok, true, "pilot")?;
    need(
        &pilot,
        "analyze the TypeScript diff: ripr check",
        "pilot routes to check",
    )?;
    let (status_ok, status) = ripr(&decoy, &["agent", "status", "--root", &root_arg])?;
    expect_exit(status_ok, true, "agent status")?;
    need(
        &status,
        "Status selects no next command",
        "status terminates after pilot",
    )?;
    decoy_untouched(&decoy)?;

    // An unknown gap id is refused and leaves no attempt or gap artifact.
    let (unknown_ok, unknown) = ripr(
        &decoy,
        &[
            "agent",
            "repair",
            "--gap-id",
            "gap:typescript:typescript_preview:00000000",
            "--base",
            "HEAD~1",
            "--root",
            &root_arg,
            "--phase",
            "before",
        ],
    )?;
    expect_exit(unknown_ok, false, "unknown gap start")?;
    need(&unknown, "names no current TypeScript gap", "unknown gap")?;
    let workflow_dir = root.join("target").join("ripr").join("workflow");
    if attempt_manifests(&root)? != 0
        || workflow_dir.join("gap-repair-packet.json").exists()
        || workflow_dir.join("before.check-findings.json").exists()
    {
        return Err(format!("unknown gap start left artifacts:\n{unknown}"));
    }

    let gap_start_args = [
        "agent", "repair", "--gap-id", &gap, "--base", "HEAD~1", "--root", &root_arg, "--phase",
        "before",
    ];
    let no_leftovers = |what: &str, out: &str| -> Result<(), String> {
        if attempt_manifests(&root)? != 0
            || workflow_dir.join("gap-repair-packet.json").exists()
            || workflow_dir.join("before.check-findings.json").exists()
        {
            return Err(format!("{what} left artifacts:\n{out}"));
        }
        Ok(())
    };

    // A tracked non-test file that already differs from HEAD fails closed
    // with the gap-specific restart and leaves nothing behind.
    let production = root.join("src/pricing.ts");
    let original = std::fs::read_to_string(&production)
        .map_err(|error| format!("read production: {error}"))?;
    std::fs::write(&production, format!("{original}\n// dirty\n"))
        .map_err(|error| format!("dirty production: {error}"))?;
    let (dirty_ok, dirty) = ripr(&decoy, &gap_start_args)?;
    std::fs::write(&production, &original)
        .map_err(|error| format!("restore production: {error}"))?;
    expect_exit(dirty_ok, false, "dirty production start")?;
    need(
        &dirty,
        "outside the allowed test surface already differ from HEAD",
        "dirty premise",
    )?;
    need(&dirty, "--gap-id", "gap-specific restart")?;
    if dirty.contains("--seam-id") {
        return Err(format!("dirty refusal names --seam-id:\n{dirty}"));
    }
    no_leftovers("dirty production start", &dirty)?;

    // A ready gap whose publication fails (the edit-cage baseline path is a
    // directory) removes the snapshots it wrote.
    let baseline_path = workflow_dir.join("attempt-baseline.json");
    std::fs::create_dir_all(&baseline_path).map_err(|error| format!("block baseline: {error}"))?;
    let (blocked_ok, blocked) = ripr(&decoy, &gap_start_args)?;
    std::fs::remove_dir_all(&baseline_path)
        .map_err(|error| format!("unblock baseline: {error}"))?;
    expect_exit(blocked_ok, false, "blocked publication")?;
    no_leftovers("failed publication", &blocked)?;

    // Pre-existing shared workflow artifacts (an earlier gap projection, a
    // seam workflow baseline) survive every refusal byte-identical: a refused
    // start removes or rewrites only what it wrote itself.
    let sentinels = [
        ("before.check-findings.json", "sentinel-snapshot"),
        ("gap-repair-packet.json", "sentinel-packet"),
        ("attempt-baseline.json", "sentinel-baseline"),
    ];
    let plant = || -> Result<(), String> {
        for (name, bytes) in sentinels {
            std::fs::write(workflow_dir.join(name), bytes)
                .map_err(|error| format!("plant {name}: {error}"))?;
        }
        Ok(())
    };
    let sentinels_intact = |what: &str, only: &[&str]| -> Result<(), String> {
        for (name, bytes) in sentinels {
            if !only.contains(&name) {
                continue;
            }
            let found = std::fs::read_to_string(workflow_dir.join(name))
                .map_err(|error| format!("{what}: sentinel {name} lost: {error}"))?;
            if found != bytes {
                return Err(format!("{what}: sentinel {name} rewritten to `{found}`"));
            }
        }
        Ok(())
    };
    let all_names: Vec<&str> = sentinels.iter().map(|(name, _)| *name).collect();
    plant()?;
    let (stale_ok, stale) = ripr(
        &decoy,
        &[
            "agent",
            "repair",
            "--gap-id",
            "gap:typescript:typescript_preview:00000000",
            "--base",
            "HEAD~1",
            "--root",
            &root_arg,
            "--phase",
            "before",
        ],
    )?;
    expect_exit(stale_ok, false, "stale gap with sentinels")?;
    need(&stale, "names no current TypeScript gap", "stale gap")?;
    sentinels_intact("stale gap refusal", &all_names)?;
    std::fs::write(&production, format!("{original}\n// dirty\n"))
        .map_err(|error| format!("dirty production: {error}"))?;
    let (dirty_ok, dirty) = ripr(&decoy, &gap_start_args)?;
    std::fs::write(&production, &original)
        .map_err(|error| format!("restore production: {error}"))?;
    expect_exit(dirty_ok, false, "dirty premise with sentinels")?;
    need(
        &dirty,
        "outside the allowed test surface already differ from HEAD",
        "dirty premise",
    )?;
    sentinels_intact("dirty premise refusal", &all_names)?;
    // Accepted gap whose publication fails after the snapshot and packet were
    // rewritten: their prior bytes return.
    std::fs::remove_file(&baseline_path).map_err(|error| format!("clear baseline: {error}"))?;
    std::fs::create_dir_all(&baseline_path).map_err(|error| format!("block baseline: {error}"))?;
    let (failed_ok, failed) = ripr(&decoy, &gap_start_args)?;
    std::fs::remove_dir_all(&baseline_path)
        .map_err(|error| format!("unblock baseline: {error}"))?;
    expect_exit(failed_ok, false, "blocked publication with sentinels")?;
    sentinels_intact(
        "partial publication failure",
        &["before.check-findings.json", "gap-repair-packet.json"],
    )?;
    if attempt_manifests(&root)? != 0 {
        return Err(format!("failed publication left an attempt:\n{failed}"));
    }
    for (name, _) in sentinels {
        let _ = std::fs::remove_file(workflow_dir.join(name));
    }

    // The printed start is accepted by `agent repair --phase before`: the
    // flags are replayed exactly as the start line names them.
    let (repair_ok, repair) = ripr(
        &decoy,
        &[
            "agent", "repair", "--gap-id", &gap, "--base", "HEAD~1", "--root", &root_arg,
            "--phase", "before",
        ],
    )?;
    if !repair_ok {
        return Err(format!("gap-bound before phase was refused:\n{repair}"));
    }
    need(
        &repair,
        "awaiting the focused test edit",
        "attempt published",
    )?;
    need(&repair, "test/pricing.test.ts", "edit target is the test")?;
    if repair.contains("--seam-id") {
        return Err(format!("gap-bound output leaks --seam-id:\n{repair}"));
    }
    let attempts = root.join("target").join("ripr").join("repair-attempts");
    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(&attempts).map_err(|error| format!("attempts dir: {error}"))? {
        let path = entry
            .map_err(|error| format!("attempt entry: {error}"))?
            .path();
        if path.join("attempt.json").is_file() {
            manifests.push(path.join("attempt.json"));
        }
    }
    if manifests.len() != 1 {
        return Err(format!("expected one attempt manifest, got {manifests:?}"));
    }
    let manifest_text =
        std::fs::read_to_string(&manifests[0]).map_err(|error| format!("manifest: {error}"))?;
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).map_err(|error| format!("manifest json: {error}"))?;
    if manifest["seam_id"] != gap.as_str() || manifest["gap_binding"]["base"] != "HEAD~1" {
        return Err(format!(
            "manifest does not record the gap binding:\n{manifest_text}"
        ));
    }
    let persisted_next = manifest["next_command"].as_str().unwrap_or_default();
    if persisted_next.contains("--phase after") || !persisted_next.contains("--gap-id") {
        return Err(format!(
            "persisted next step is not the manual by-gap route: {persisted_next}"
        ));
    }
    let roles: Vec<&str> = manifest["artifacts"]
        .as_array()
        .ok_or_else(|| "manifest artifacts".to_string())?
        .iter()
        .filter_map(|artifact| artifact["role"].as_str())
        .collect();
    if !roles.contains(&"before_check_findings") || !roles.contains(&"edit_cage_baseline") {
        return Err(format!("unexpected artifact roles {roles:?}"));
    }
    // The retained before snapshot is the check-findings producer's artifact.
    let snapshot = std::fs::read_to_string(
        root.join("target")
            .join("ripr")
            .join("workflow")
            .join("before.check-findings.json"),
    )
    .map_err(|error| format!("before snapshot: {error}"))?;
    need(&snapshot, "\"kind\":\"check_findings\"", "snapshot kind")?;
    need(&snapshot, &gap, "snapshot carries the gap")?;
    // A later refused start (unknown gap) leaves the accepted start's shared
    // snapshot and baseline byte-identical.
    let accepted_snapshot_path = workflow_dir.join("before.check-findings.json");
    let accepted_baseline_path = workflow_dir.join("attempt-baseline.json");
    let accepted_baseline = std::fs::read(&accepted_baseline_path)
        .map_err(|error| format!("accepted baseline: {error}"))?;
    let (late_ok, _) = ripr(
        &decoy,
        &[
            "agent",
            "repair",
            "--gap-id",
            "gap:typescript:typescript_preview:00000000",
            "--base",
            "HEAD~1",
            "--root",
            &root_arg,
            "--phase",
            "before",
        ],
    )?;
    expect_exit(late_ok, false, "unknown gap after accepted start")?;
    if std::fs::read_to_string(&accepted_snapshot_path)
        .ok()
        .as_deref()
        != Some(snapshot.as_str())
        || std::fs::read(&accepted_baseline_path).ok().as_deref()
            != Some(accepted_baseline.as_slice())
    {
        return Err("a refused start changed the accepted start's shared artifacts".to_string());
    }
    // Status over a gap-bound attempt never names a seam-bound start.
    let (status_ok, status) = ripr(&decoy, &["agent", "status", "--root", &root_arg])?;
    expect_exit(status_ok, true, "agent status over a gap-bound attempt")?;
    if status.contains("--seam-id") {
        return Err(format!(
            "status leaks --seam-id for a gap attempt:\n{status}"
        ));
    }
    // The gap-bound after phase is a later slice: it refuses with the manual
    // by-gap route instead of scoring against the wrong snapshot kind.
    let (after_ok, after) = ripr(
        &decoy,
        &[
            "agent",
            "repair",
            "--root",
            &root_arg,
            "--phase",
            "after",
            "--attempt",
            &attempt_id_of(&manifest)?,
        ],
    )?;
    expect_exit(after_ok, false, "gap-bound after phase")?;
    need(&after, "gap-bound", "after refusal names the binding")?;
    need(
        &after,
        "agent verify",
        "after refusal names the manual route",
    )?;
    decoy_untouched(&decoy)?;

    // Variants that must stay unresolved, paired with whether the product
    // fails closed. The object-property and enum rows were fail-open
    // defects until #9107; every row now fails closed.
    let variants = [
        ("variant-let-binding", true),
        ("variant-reassignment", true),
        ("variant-shadowing", true),
        ("variant-imported-constant", true),
        ("variant-object-namespace-write", true),
        ("variant-enum-computed-initializer", true),
        ("variant-nonliteral-arithmetic", true),
        ("variant-reach-barrier", true),
    ];
    for (variant, fails_closed) in variants {
        let (_vguard, vroot, vdecoy) = materialize(
            "blind_journey_installed_typescript",
            variant,
            "tsv",
            Some(toml),
        )?;
        let vroot_arg = vroot.to_string_lossy().into_owned();
        let (out_ok, out) = ripr(
            &vdecoy,
            &["check", "--root", &vroot_arg, "--base", "HEAD~1"],
        )?;
        expect_exit(out_ok, true, variant)?;
        decoy_untouched(&vdecoy)?;
        let projected = out.contains("the repair packet is complete");
        if fails_closed {
            need(&out, "repair packet is not ready", variant)?;
            if projected {
                return Err(format!("{variant} projected a delegatable packet:\n{out}"));
            }
            // #9103: an unrouted variant is offered no gap-bound start, and
            // starting one by hand is refused by the shared validator.
            if out.contains("ripr agent repair --gap-id") {
                return Err(format!("{variant} was offered a gap-bound start:\n{out}"));
            }
            // The selected (top) finding is the one the check screen offered
            // no start for; other findings of the same diff keep their own
            // independent packet decisions.
            let variant_gap = selected_typescript_gap(&out)?;
            let (start_ok, start) = ripr(
                &vdecoy,
                &[
                    "agent",
                    "repair",
                    "--gap-id",
                    &variant_gap,
                    "--base",
                    "HEAD~1",
                    "--root",
                    &vroot_arg,
                    "--phase",
                    "before",
                ],
            )?;
            expect_exit(start_ok, false, variant)?;
            need(&start, "repair packet is not ready", variant)?;
            if attempt_manifests(&vroot)? != 0 || vroot.join("target/ripr/workflow").exists() {
                return Err(format!("{variant} left an attempt behind:\n{start}"));
            }
            decoy_untouched(&vdecoy)?;
        } else if !projected {
            return Err(format!(
                "{variant} now fails closed; flip its row to true (#9107):\n{out}"
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_excluded_by_config_is_not_a_clean_result() -> Result<(), String> {
    // Pinned to Rust only so no-config project detection cannot enable TypeScript.
    let (_guard, root, decoy) = materialize(
        "blind_journey_installed_typescript",
        "head",
        "tsoff",
        Some("[languages]\nenabled = [\"rust\"]\n"),
    )?;
    let root_arg = root.to_string_lossy().into_owned();
    let (out_ok, out) = ripr(&decoy, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
    expect_exit(out_ok, true, "check with TypeScript excluded")?;
    need(&out, "analysis incomplete", "incomplete analysis")?;
    need(
        &out,
        "enabled = [\"typescript\"]",
        "names the enabling config",
    )?;
    Ok(())
}

#[cfg(feature = "lang-typescript")]
#[test]
fn typescript_check_findings_artifacts_verify_by_gap_id_and_reject_forgery() -> Result<(), String> {
    let toml = "[languages]\nenabled = [\"typescript\"]\n";
    let (_guard, root, decoy) = materialize(
        "blind_journey_installed_typescript",
        "head",
        "ts-verify",
        Some(toml),
    )?;
    let root_arg = root.to_string_lossy().into_owned();
    let base = fixture_git_output(&root, &["rev-parse", "HEAD~1"])?
        .trim()
        .to_string();
    let snapshot_dir = root.join("target").join("ripr");
    std::fs::create_dir_all(&snapshot_dir).map_err(|error| format!("snapshot dir: {error}"))?;
    let snapshot = |name: &str| snapshot_dir.join(name);
    let produce = |name: &str| -> Result<PathBuf, String> {
        // Stdout only: the artifact is the producer's exact bytes.
        let output = ripr_output(
            &decoy,
            &[
                "check",
                "--root",
                &root_arg,
                "--base",
                &base,
                "--format",
                "check-findings-json",
            ],
        )?;
        expect_exit(output.status.success(), true, "check-findings-json")?;
        let path = snapshot(name);
        std::fs::write(&path, &output.stdout).map_err(|error| format!("write {name}: {error}"))?;
        Ok(path)
    };

    let before = produce("before.json")?;
    let before_text =
        std::fs::read_to_string(&before).map_err(|error| format!("read before: {error}"))?;
    need(&before_text, "\"kind\":\"check_findings\"", "producer kind")?;
    need(&before_text, "gap:typescript:", "typescript gap identity")?;

    // The repair: pin the equality boundary the packet names.
    let test_file = root.join("test").join("pricing.test.ts");
    let original =
        std::fs::read_to_string(&test_file).map_err(|error| format!("read test: {error}"))?;
    let repaired = original.replacen(
        "  it(\"discounts an above-threshold subtotal\"",
        "  it(\"discounts exactly at the threshold\", () => {\n    expect(discountedTotal(10000)).toBe(9500);\n  });\n\n  it(\"discounts an above-threshold subtotal\"",
        1,
    );
    if repaired == original {
        return Err("repair edit did not apply".to_string());
    }
    std::fs::write(&test_file, repaired).map_err(|error| format!("write test: {error}"))?;
    fixture_git_ok(&root, &["add", "-A"])?;
    fixture_git_ok(&root, &["commit", "-q", "-m", "pin the threshold"])?;
    let after = produce("after.json")?;

    let verify_args = |before: &Path, after: &Path| -> Vec<String> {
        [
            "agent",
            "verify",
            "--json",
            "--root",
            &root_arg,
            "--before",
            &before.to_string_lossy(),
            "--after",
            &after.to_string_lossy(),
        ]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect()
    };
    let run_verify = |before: &Path, after: &Path| -> Result<(bool, String), String> {
        let args = verify_args(before, after);
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        ripr(&decoy, &refs)
    };

    let (ok, verified) = run_verify(&before, &after)?;
    expect_exit(
        ok,
        true,
        &format!("verify over producer artifacts: {verified}"),
    )?;
    need(&verified, "gap:typescript:", "verify names the gap")?;
    if verified.contains("\"bound_gap\"") {
        return Err(format!("unbound verify carried a bound_gap:\n{verified}"));
    }

    // Gap-bound verify (#9103): the printed gap closes between the pair, and
    // a gap that did not close is a named refusal, never a success.
    let gap_id = before_text
        .match_indices("gap:typescript:typescript_preview:")
        .next()
        .map(|(start, _)| {
            before_text[start..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_'))
                .collect::<String>()
        })
        .ok_or_else(|| "no typescript gap id in the before artifact".to_string())?;
    let bound = |gap: &str, before: &Path, after: &Path| -> Result<std::process::Output, String> {
        let mut args = verify_args(before, after);
        args.push("--gap-id".to_string());
        args.push(gap.to_string());
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        ripr_output(&decoy, &refs)
    };
    let closed = bound(&gap_id, &before, &after)?;
    let closed_text = String::from_utf8_lossy(&closed.stdout).into_owned();
    expect_exit(closed.status.success(), true, "gap-bound verify")?;
    need(&closed_text, "\"bound_gap\"", "bound gap reported")?;
    need(&closed_text, "\"closed\": true", "closure stated")?;
    need(&closed_text, &gap_id, "bound gap is the named gap")?;
    let refused = |gap: &str, before: &Path, after: &Path, expected: &str| -> Result<(), String> {
        let output = bound(gap, before, after)?;
        if output.status.code() != Some(3) {
            return Err(format!(
                "gap-bound refusal must exit 3, got {:?}",
                output.status.code()
            ));
        }
        if !output.stdout.is_empty() {
            return Err("gap-bound refusal must leave stdout empty".to_string());
        }
        need(
            &String::from_utf8_lossy(&output.stderr),
            expected,
            "named refusal",
        )
        .map(|_| ())
    };
    refused(
        "gap:typescript:typescript_preview:00000000",
        &before,
        &after,
        "is absent from the before artifact",
    )?;

    // Forgery 1: a hand-edited "after" with a flipped finding breaks the commitment.
    let after_text =
        std::fs::read_to_string(&after).map_err(|error| format!("read after: {error}"))?;
    let forged = after_text.replacen(
        "\"classification\": \"exposed\"",
        "\"classification\": \"weakly_exposed\"",
        1,
    );
    let forged_path = snapshot("forged.json");
    std::fs::write(&forged_path, forged).map_err(|error| format!("write forged: {error}"))?;
    let (ok, out) = run_verify(&before, &forged_path)?;
    expect_exit(ok, false, "edited artifact")?;
    need(&out, "content commitment mismatch", "edit is detected")?;

    // Forgery 2: an artifact without the producer envelope is not accepted.
    let bare = ripr_output(
        &decoy,
        &[
            "check", "--root", &root_arg, "--base", &base, "--format", "json",
        ],
    )?;
    let bare_path = snapshot("bare.json");
    std::fs::write(&bare_path, &bare.stdout).map_err(|error| format!("write bare: {error}"))?;
    let (ok, _) = run_verify(&bare_path, &after)?;
    expect_exit(ok, false, "unenveloped before snapshot")?;
    // A gap-bound verify over an unenveloped snapshot is an operational error
    // (exit 2), never a typed gap refusal (exit 3).
    let not_findings = bound(&gap_id, &bare_path, &after)?;
    if not_findings.status.code() != Some(2) {
        return Err(format!(
            "--gap-id over a non-findings snapshot must exit 2, got {:?}",
            not_findings.status.code()
        ));
    }
    need(
        &String::from_utf8_lossy(&not_findings.stderr),
        "not a canonical repo-exposure artifact",
        "operational error named",
    )?;

    // Forgery 3: reversed lineage is incomparable.
    let (ok, out) = run_verify(&after, &before)?;
    expect_exit(ok, false, "reversed pair")?;
    need(&out, "incomparable", "reversed pair named")?;
    decoy_untouched(&decoy)
}
