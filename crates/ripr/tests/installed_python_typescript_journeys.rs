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
    // #9103: the pilot/status route does not cycle. Status names the route back
    // to `ripr check` (which printed the gap-bound start above) and offers no
    // `ripr pilot` command, so pilot -> status -> pilot cannot recur.
    need(
        &status,
        "pilot_routed_to_check_no_repair_target",
        "status names why it selects no command",
    )?;
    if status.contains("ripr pilot --root") && !status.contains("ripr check --root") {
        return Err(format!("status cycled back to pilot:\n{status}"));
    }
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
    // The persisted next step is the executable gap-bound after phase: the
    // manifest carries the gap and its base, so the command needs no extra flag.
    let persisted_next = manifest["next_command"].as_str().unwrap_or_default();
    if !persisted_next.contains("--phase after")
        || !persisted_next.contains("--attempt ")
        || persisted_next.contains("--seam-id")
    {
        return Err(format!(
            "persisted next step is not the gap-bound after phase: {persisted_next}"
        ));
    }
    let resolved_base = fixture_git_output(&root, &["rev-parse", "HEAD~1"])?
        .trim()
        .to_string();
    if manifest["gap_binding"]["base_commit"] != resolved_base.as_str() {
        return Err(format!(
            "manifest does not pin the resolved base commit:\n{manifest_text}"
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

/// A started gap-bound attempt over the installed TypeScript fixture.
#[cfg(feature = "lang-typescript")]
struct GapAttempt {
    _guard: TempDirGuard,
    root: PathBuf,
    root_arg: String,
    decoy: PathBuf,
    gap: String,
    attempt_id: String,
}

#[cfg(feature = "lang-typescript")]
impl GapAttempt {
    fn start(label: &str) -> Result<Self, String> {
        let toml = "[languages]\nenabled = [\"typescript\"]\n";
        let (guard, root, decoy) = materialize(
            "blind_journey_installed_typescript",
            "head",
            label,
            Some(toml),
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
        let attempt_id = attempt_id_of(&manifest)?;
        Ok(Self {
            attempt_id,
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

    fn after(&self, json: bool) -> Result<std::process::Output, String> {
        let mut args = vec![
            "agent",
            "repair",
            "--root",
            &self.root_arg,
            "--phase",
            "after",
            "--attempt",
            &self.attempt_id,
        ];
        if json {
            args.push("--json");
        }
        ripr_output(&self.decoy, &args)
    }

    /// Commit an edit of the focused test that pins the equality boundary the
    /// packet names, so the gap reads exposed.
    fn commit_closing_test(&self) -> Result<(), String> {
        self.commit_test_edit(
            "  it(\"discounts exactly at the threshold\", () => {\n    expect(discountedTotal(10000)).toBe(9500);\n  });\n\n  it(\"discounts an above-threshold subtotal\"",
            "pin the threshold",
        )
    }

    /// Commit a focused-test edit that leaves the boundary unpinned.
    fn commit_non_discriminating_test(&self, label: &str) -> Result<(), String> {
        self.commit_test_edit(
            &format!(
                "  it(\"{label}\", () => {{\n    expect(discountedTotal(5)).toBe(5);\n  }});\n\n  it(\"discounts an above-threshold subtotal\""
            ),
            label,
        )
    }

    fn commit_test_edit(&self, insertion: &str, message: &str) -> Result<(), String> {
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
        std::fs::write(&test_file, edited).map_err(|error| format!("write test: {error}"))?;
        // Only the paths the edit touched: `target/ripr` artifacts and the
        // untracked `ripr.toml` are not part of the committed edit.
        fixture_git_ok(&self.root, &["add", "test/pricing.test.ts"])?;
        if self.root.join("src/extra.ts").is_file() {
            fixture_git_ok(&self.root, &["add", "src/extra.ts"])?;
        }
        fixture_git_ok(&self.root, &["commit", "-q", "-m", message])
    }

    fn state(&self) -> Result<String, String> {
        Ok(self.manifest_for_only_attempt()?["state"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }
}

#[cfg(feature = "lang-typescript")]
fn text_of(output: &std::process::Output) -> (String, String) {
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// #9103 (RIPR-SPEC-0311): the gap-bound after phase closes a gap only on
/// positive evidence, finishes the attempt through the edit cage, and names
/// no seam and no receipt.
#[cfg(feature = "lang-typescript")]
#[test]
fn gap_bound_after_phase_closes_the_gap_on_positive_evidence() -> Result<(), String> {
    let attempt = GapAttempt::start("ts-after-ok")?;
    attempt.commit_closing_test()?;
    let output = attempt.after(true)?;
    let (stdout, stderr) = text_of(&output);
    expect_exit(
        output.status.success(),
        true,
        &format!("gap-bound after phase:\n{stdout}\n{stderr}"),
    )?;
    let document: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|error| format!("stdout json: {error}\n{stdout}"))?;
    if document["kind"] != "repair_after_result"
        || document["verify"]["bound_gap"]["gap_id"] != attempt.gap.as_str()
        || document["verify"]["bound_gap"]["after_classification"] != "exposed"
        || document["verify"]["bound_gap"]["closed"] != true
    {
        return Err(format!("closure is not reported:\n{stdout}"));
    }
    if attempt.state()? != "ready_to_finish" {
        return Err(format!("unexpected state {}", attempt.state()?));
    }
    if !attempt
        .root
        .join("target/ripr/workflow/after.check-findings.json")
        .is_file()
    {
        return Err("after.check-findings.json was not written".to_string());
    }
    if attempt
        .root
        .join("target/ripr/workflow/after.repo-exposure.json")
        .exists()
    {
        return Err("a gap-bound attempt wrote a repo-exposure after snapshot".to_string());
    }
    for text in [&stdout, &stderr] {
        if text.contains("--seam-id") {
            return Err(format!("gap-bound after output leaks --seam-id:\n{text}"));
        }
    }
    // The receipt is bound to the closed gap and to this attempt.
    let receipt_path = attempt.root.join("target/ripr/reports/agent-receipt.json");
    let receipt: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&receipt_path)
            .map_err(|error| format!("read receipt: {error}"))?,
    )
    .map_err(|error| format!("receipt json: {error}"))?;
    if receipt["status"] != "advisory"
        || receipt["analysis_outcome_status"] != "complete"
        || receipt["seam"]["seam_id"] != attempt.gap.as_str()
        || receipt["provenance"]["seam_id"] != attempt.gap.as_str()
        || receipt["bound_gap"]["gap_id"] != attempt.gap.as_str()
        || receipt["bound_gap"]["closed"] != true
        || receipt["bound_gap"]["before_classification"] != "weakly_exposed"
        || receipt["bound_gap"]["after_classification"] != "exposed"
        || receipt["repair_attempt"]["attempt_id"] != attempt.attempt_id.as_str()
        || receipt["repair_attempt"]["seam_id"] != attempt.gap.as_str()
        || receipt["repair_attempt"]["gap_binding"]["base"] != "HEAD~1"
        || receipt["verification"]["status"] != "verification_not_run"
        || receipt["provenance"]["limits"]["runtime_mutation_execution"] != false
        || receipt["provenance"]["limits"]["runtime_adequacy_claim"] != false
    {
        return Err(format!(
            "receipt is not bound to the closed gap:\n{receipt}"
        ));
    }
    let manifest = attempt.manifest_for_only_attempt()?;
    if receipt["repair_attempt"]["gap_binding"]["base_commit"]
        != manifest["gap_binding"]["base_commit"]
    {
        return Err("receipt does not record the attempt's resolved base commit".to_string());
    }
    // The attempt retained the receipt and its verify document.
    let roles: Vec<&str> = manifest["terminal_artifacts"]
        .as_array()
        .map(|artifacts| {
            artifacts
                .iter()
                .filter_map(|a| a["role"].as_str())
                .collect()
        })
        .unwrap_or_default();
    if !roles.contains(&"agent_receipt") || !roles.contains(&"agent_verify") {
        return Err(format!("terminal evidence not retained: {roles:?}"));
    }
    // Status reads the retained receipt as showing this gap closed and offers
    // no seam-bound command.
    let (status_ok, status) = ripr(
        &attempt.decoy,
        &["agent", "status", "--root", &attempt.root_arg, "--json"],
    )?;
    expect_exit(status_ok, true, "agent status after a gap-bound finish")?;
    let status_doc: serde_json::Value =
        serde_json::from_str(&status).map_err(|error| format!("status json: {error}\n{status}"))?;
    let attempt_view = &status_doc["repair_attempts"][0];
    if attempt_view["receipt"]["issued_for_attempt"] != true
        || attempt_view["receipt"]["shows_gap_closed"] != true
        || attempt_view["receipt"]["status"] != "advisory"
    {
        return Err(format!("status does not read the receipt:\n{status}"));
    }
    if status.contains("--seam-id") || status.contains("repair_receipt_unconfirmed") {
        return Err(format!(
            "status offers a seam route or doubts the receipt:\n{status}"
        ));
    }
    // A finished attempt cannot run again, and its restart names the gap.
    let again = attempt.after(false)?;
    let (_, again_err) = text_of(&again);
    expect_exit(again.status.success(), false, "second after phase")?;
    need(&again_err, "--gap-id", "restart names the gap start")?;
    decoy_untouched(&attempt.decoy)
}

/// Human (non `--json`) gap-bound after output names the closure and this
/// attempt's receipt, and replaces a stale seam receipt planted at the
/// receipt path rather than summarizing it.
#[cfg(feature = "lang-typescript")]
#[test]
fn gap_bound_after_phase_human_output_names_closure_and_replaces_a_stale_receipt()
-> Result<(), String> {
    let attempt = GapAttempt::start("ts-after-human")?;
    let reports = attempt.root.join("target/ripr/reports");
    std::fs::create_dir_all(&reports).map_err(|error| format!("mkdir: {error}"))?;
    std::fs::write(
        reports.join("agent-receipt.json"),
        "{\"stale_seam_receipt_marker\": true}",
    )
    .map_err(|error| format!("plant receipt: {error}"))?;
    attempt.commit_closing_test()?;
    let output = attempt.after(false)?;
    let (stdout, stderr) = text_of(&output);
    expect_exit(
        output.status.success(),
        true,
        &format!("gap-bound human after phase:\n{stdout}\n{stderr}"),
    )?;
    need(&stdout, "closed", "stdout names the gap closure")?;
    need(&stdout, attempt.gap.as_str(), "stdout names the gap")?;
    need(&stdout, "receipt:", "stdout names the receipt it issued")?;
    for text in [&stdout, &stderr] {
        if text.contains("stale_seam_receipt_marker") || text.contains("--seam-id") {
            return Err(format!(
                "gap-bound output is not this attempt's output\n{text}"
            ));
        }
    }
    let receipt = std::fs::read_to_string(reports.join("agent-receipt.json"))
        .map_err(|error| format!("read receipt: {error}"))?;
    if receipt.contains("stale_seam_receipt_marker") {
        return Err("the stale receipt was not replaced by the gap-bound finish".to_string());
    }
    need(&receipt, attempt.gap.as_str(), "receipt names the gap")?;
    decoy_untouched(&attempt.decoy)
}

/// `ripr agent receipt` for a gap is fail-closed on closure evidence: the
/// real bound verify document issues it, while an unbound pair (no
/// `--gap-id`), a forged `bound_gap` naming a gap that never closed, and an
/// edited closure are each refused.
#[cfg(feature = "lang-typescript")]
#[test]
fn gap_bound_receipt_requires_the_verify_closure_and_rejects_forgery() -> Result<(), String> {
    let attempt = GapAttempt::start("ts-receipt-forgery")?;
    attempt.commit_closing_test()?;
    let finished = attempt.after(true)?;
    let (stdout, stderr) = text_of(&finished);
    expect_exit(
        finished.status.success(),
        true,
        &format!("closing finish:\n{stdout}\n{stderr}"),
    )?;
    let workflow = attempt.root.join("target/ripr/workflow");
    let verify_text = std::fs::read_to_string(workflow.join("agent-verify.json"))
        .map_err(|error| format!("read verify: {error}"))?;
    let verify: serde_json::Value =
        serde_json::from_str(&verify_text).map_err(|error| format!("verify json: {error}"))?;
    let before = verify["inputs"]["before"].as_str().unwrap_or_default();
    let after = verify["inputs"]["after"].as_str().unwrap_or_default();

    let receipt_for = |verify_path: &Path, label: &str| -> Result<std::process::Output, String> {
        let out = workflow.join(format!("{label}.receipt.json"));
        ripr_output(
            &attempt.decoy,
            &[
                "agent",
                "receipt",
                "--root",
                &attempt.root_arg,
                "--verify-json",
                &verify_path.to_string_lossy(),
                "--seam-id",
                &attempt.gap,
                "--attempt",
                &attempt.attempt_id,
                "--json",
                "--out",
                &out.to_string_lossy(),
            ],
        )
    };

    // Positive control: the bound verify document the finish wrote issues the
    // receipt again.
    let again = receipt_for(&workflow.join("agent-verify.json"), "control")?;
    expect_exit(
        again.status.success(),
        true,
        &format!("receipt over the bound verify:\n{:?}", text_of(&again)),
    )?;
    let control = std::fs::read_to_string(workflow.join("control.receipt.json"))
        .map_err(|error| format!("read control receipt: {error}"))?;
    need(
        &control,
        "\"closed\": true",
        "control receipt carries closure",
    )?;

    // Negative 1: an unbound verify over the same pair carries no closure.
    let unbound = ripr_output(
        &attempt.decoy,
        &[
            "agent",
            "verify",
            "--json",
            "--root",
            &attempt.root_arg,
            "--before",
            before,
            "--after",
            after,
        ],
    )?;
    expect_exit(unbound.status.success(), true, "unbound verify")?;
    let unbound_path = workflow.join("unbound-verify.json");
    std::fs::write(&unbound_path, &unbound.stdout)
        .map_err(|error| format!("write unbound verify: {error}"))?;
    let refused = receipt_for(&unbound_path, "unbound")?;
    let (_, refused_err) = text_of(&refused);
    expect_exit(
        refused.status.success(),
        false,
        "receipt over an unbound pair",
    )?;
    need(
        &refused_err,
        "requires an agent verify document bound to that gap",
        "unbound pair refused",
    )?;
    if workflow.join("unbound.receipt.json").exists() {
        return Err("a receipt was written over an unbound pair".to_string());
    }

    // Negative 2: a forged closure naming a gap that never closed.
    let mut forged = verify.clone();
    forged["bound_gap"]["gap_id"] = serde_json::json!("gap:typescript:typescript_preview:deadbeef");
    let forged_path = workflow.join("forged-verify.json");
    std::fs::write(
        &forged_path,
        serde_json::to_string_pretty(&forged).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("write forged verify: {error}"))?;
    let refused = receipt_for(&forged_path, "forged")?;
    let (_, refused_err) = text_of(&refused);
    expect_exit(refused.status.success(), false, "receipt over a forged gap")?;
    need(
        &refused_err,
        "gap binding refused",
        "forged closure refused",
    )?;

    // Negative 3: the real gap with an edited classification is not the
    // canonical render of its snapshots.
    let mut edited = verify;
    edited["bound_gap"]["before_classification"] = serde_json::json!("exposed");
    let edited_path = workflow.join("edited-verify.json");
    std::fs::write(
        &edited_path,
        serde_json::to_string_pretty(&edited).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("write edited verify: {error}"))?;
    let refused = receipt_for(&edited_path, "edited")?;
    let (_, refused_err) = text_of(&refused);
    expect_exit(
        refused.status.success(),
        false,
        "receipt over an edited closure",
    )?;
    need(&refused_err, "not_canonical", "edited closure refused")?;
    for label in ["forged", "edited"] {
        if workflow.join(format!("{label}.receipt.json")).exists() {
            return Err(format!("a receipt was written over the {label} verify"));
        }
    }
    decoy_untouched(&attempt.decoy)
}

/// A committed test that does not pin the boundary leaves the gap open: a
/// typed refusal (exit 3) recorded on the attempt, which stays retryable, and
/// the corrected edit then closes it.
#[cfg(feature = "lang-typescript")]
#[test]
fn gap_bound_after_phase_refuses_a_gap_that_is_not_closed_and_stays_retryable() -> Result<(), String>
{
    let attempt = GapAttempt::start("ts-after-open")?;
    attempt.commit_non_discriminating_test("covers a small subtotal")?;
    let output = attempt.after(true)?;
    let (stdout, stderr) = text_of(&output);
    if output.status.code() != Some(3) {
        return Err(format!(
            "an open gap must exit 3, got {:?}:\n{stdout}\n{stderr}",
            output.status.code()
        ));
    }
    need(&stderr, "agent verify gap binding refused", "named refusal")?;
    need(&stderr, "still reads", "the gap is still open")?;
    need(
        &stderr,
        "remains `awaiting_edit`",
        "attempt stays retryable",
    )?;
    let document: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|error| format!("stdout json: {error}\n{stdout}"))?;
    if document["kind"] != "repair_after_refusal" || stdout.contains("\"closed\": true") {
        return Err(format!("refusal document claims closure:\n{stdout}"));
    }
    if attempt.state()? != "awaiting_edit" {
        return Err(format!("unexpected state {}", attempt.state()?));
    }
    if attempt
        .root
        .join("target/ripr/reports/agent-receipt.json")
        .exists()
    {
        return Err("a receipt was issued for a gap that did not close".to_string());
    }
    let manifest = attempt.manifest_for_only_attempt()?;
    if manifest["after_refusal_count"] != 1 {
        return Err(format!("refusal not counted:\n{manifest}"));
    }
    if stderr.contains("--seam-id") {
        return Err(format!("refusal leaks --seam-id:\n{stderr}"));
    }
    // The retry with the discriminating test closes the gap.
    attempt.commit_closing_test()?;
    // The persisted next command is the executable after phase.
    let persisted = manifest["next_command"].as_str().unwrap_or_default();
    need(persisted, "--phase after", "persisted next command")?;
    let retry = attempt.after(true)?;
    let (retry_out, retry_err) = text_of(&retry);
    expect_exit(
        retry.status.success(),
        true,
        &format!("retry after the corrected edit:\n{retry_out}\n{retry_err}"),
    )?;
    need(&retry_out, "\"closed\": true", "retry reports closure").map(|_| ())
}

/// Absence is never closure: reverting the production change removes the gap
/// from the after artifact, which the verifier refuses as a typed refusal.
#[cfg(feature = "lang-typescript")]
#[test]
fn gap_bound_after_phase_does_not_treat_an_absent_gap_as_closed() -> Result<(), String> {
    let attempt = GapAttempt::start("ts-after-absent")?;
    let base = fixture_git_output(&attempt.root, &["rev-parse", "HEAD~1"])?
        .trim()
        .to_string();
    fixture_git_ok(&attempt.root, &["checkout", &base, "--", "src/pricing.ts"])?;
    fixture_git_ok(&attempt.root, &["commit", "-q", "-m", "drop the change"])?;
    let output = attempt.after(true)?;
    let (stdout, stderr) = text_of(&output);
    if output.status.code() != Some(3) {
        return Err(format!(
            "an absent gap must exit 3, got {:?}:\n{stdout}\n{stderr}",
            output.status.code()
        ));
    }
    need(
        &stderr,
        "is absent from the after artifact",
        "absence named",
    )?;
    need(
        &stderr,
        "absence is not closure evidence",
        "absence is not closure",
    )?;
    if stdout.contains("\"closed\": true") || stdout.contains("\"bound_gap\"") {
        return Err(format!("absence was reported as closure:\n{stdout}"));
    }
    if attempt.state()? != "awaiting_edit" {
        return Err(format!("unexpected state {}", attempt.state()?));
    }
    if attempt
        .root
        .join("target/ripr/reports/agent-receipt.json")
        .exists()
    {
        return Err("a receipt was issued for a gap that did not close".to_string());
    }
    Ok(())
}

/// The edit cage is enforced: a closing test committed beside a file outside
/// the test surface ends the attempt terminally and reports no closure.
#[cfg(feature = "lang-typescript")]
#[test]
fn gap_bound_after_phase_enforces_the_edit_cage() -> Result<(), String> {
    let attempt = GapAttempt::start("ts-after-cage")?;
    std::fs::write(
        attempt.root.join("src/extra.ts"),
        "export const EXTRA = 1;\n",
    )
    .map_err(|error| format!("write extra: {error}"))?;
    attempt.commit_closing_test()?;
    let output = attempt.after(true)?;
    let (stdout, stderr) = text_of(&output);
    expect_exit(
        output.status.success(),
        false,
        &format!("cage violation:\n{stdout}\n{stderr}"),
    )?;
    need(&stderr, "src/extra.ts", "the refused path is named")?;
    let document: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|error| format!("stdout json: {error}\n{stdout}"))?;
    if document["kind"] != "repair_after_failure" || stdout.contains("\"closed\": true") {
        return Err(format!("cage refusal reads as a repair:\n{stdout}"));
    }
    let state = attempt.state()?;
    if state == "awaiting_edit" || state == "ready_to_finish" {
        return Err(format!("cage violation left a live attempt: {state}"));
    }
    if attempt
        .root
        .join("target/ripr/reports/agent-receipt.json")
        .exists()
    {
        return Err("a receipt was issued for a gap that the cage refused".to_string());
    }
    Ok(())
}

/// The attempt authority's retry budget applies to gap-bound refusals.
#[cfg(feature = "lang-typescript")]
#[test]
fn gap_bound_after_phase_stops_when_the_retry_budget_is_spent() -> Result<(), String> {
    let attempt = GapAttempt::start("ts-after-budget")?;
    attempt.commit_non_discriminating_test("covers a small subtotal")?;
    for round in 1..=5 {
        let output = attempt.after(false)?;
        let (_, stderr) = text_of(&output);
        if output.status.code() != Some(3) {
            return Err(format!(
                "refusal {round} must exit 3, got {:?}:\n{stderr}",
                output.status.code()
            ));
        }
        need(&stderr, "gap binding refused", "named refusal")?;
    }
    let stopped = attempt.after(false)?;
    let (_, stderr) = text_of(&stopped);
    expect_exit(stopped.status.success(), false, "after the budget")?;
    need(&stderr, "retry budget", "budget stop is named")?;
    need(&stderr, "--gap-id", "recovery restarts with the gap")?;
    if stderr.contains("--seam-id") {
        return Err(format!("budget stop leaks --seam-id:\n{stderr}"));
    }
    Ok(())
}
