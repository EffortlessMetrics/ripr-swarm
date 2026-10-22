//! Literal installed-binary walks of the Python and TypeScript fixture
//! journeys (#4518, #4519, parent #4508).
//!
//! The scripted `installed_*` corpus rows never execute `ripr`. These tests
//! materialize the committed `blind_journey_installed_{python,typescript}`
//! snapshots as real git repositories under a root containing a space and a
//! non-ASCII character, then run the built binary from a foreign working
//! directory holding a decoy repository, following only product-emitted
//! routes. They pin what first use actually does today, including the
//! points where the delegated repair loop ends in a manual route, so a
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

fn ripr(launch: &Path, args: &[&str]) -> Result<(bool, String), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(launch)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))?;
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

#[cfg(feature = "lang-python")]
#[test]
fn python_flat_layout_journey_stays_in_selected_root_and_ends_in_manual_route() -> Result<(), String>
{
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
    // Observed first-use break: no delegatable repair start for this shape.
    need(
        &check,
        "no repair start offered",
        "check offers no repair start",
    )?;

    // Feed `repair` the exact finding id that `check` printed, not a literal.
    let probe = check
        .split_whitespace()
        .find(|token| token.starts_with("probe:"))
        .ok_or_else(|| format!("check printed no probe id:\n{check}"))?;
    let (repair_ok, repair) = ripr(&decoy, &["repair", probe, "--root", &root_arg])?;
    expect_exit(repair_ok, false, "repair of an unbound finding id")?;
    need(
        &repair,
        "names no seam in the selected root",
        "repair refuses stale or unbound id",
    )?;

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
    // Observed first-use break: complete packet, but no `repair`/`agent repair` start.
    need(
        &check,
        "no repair start offered",
        "check offers no repair start",
    )?;
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
