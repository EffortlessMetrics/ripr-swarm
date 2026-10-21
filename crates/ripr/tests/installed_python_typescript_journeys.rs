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

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::{fixture_git_ok, fixture_git_output};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> Result<PathBuf, String> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ripr-installed-{label}-{}-{counter}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    Ok(dir)
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
) -> Result<(PathBuf, PathBuf), String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = manifest_dir
        .join("../../fixtures")
        .join(fixture)
        .join("repository");
    let launch = temp_dir(label)?;
    let root = launch.join("sélected root").join("repo");
    copy_tree(&source.join("base"), &root)?;
    fixture_git_ok(&root, &["init", "-q", "-b", "main"])?;
    fixture_git_ok(&root, &["config", "core.autocrlf", "false"])?;
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
    Ok((root, decoy))
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

fn need<'a>(text: &'a str, needle: &str, label: &str) -> Result<&'a str, String> {
    if text.contains(needle) {
        return Ok(text);
    }
    Err(format!("{label}: expected `{needle}` in:\n{text}"))
}

#[test]
fn python_flat_layout_journey_stays_in_selected_root_and_ends_in_manual_route() -> Result<(), String>
{
    let (root, decoy) = materialize("blind_journey_installed_python", "head", "py", None)?;
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

    let (_, repair) = ripr(
        &decoy,
        &[
            "repair",
            "probe:pricing_core.py:python_preview:6705c5d5",
            "--root",
            &root_arg,
        ],
    )?;
    need(
        &repair,
        "names no seam in the selected root",
        "repair refuses stale or unbound id",
    )?;

    let (_, pilot) = ripr(&decoy, &["pilot", "--root", &root_arg])?;
    need(
        &pilot,
        "no_python_findings",
        "pilot reports no Python findings for a committed-diff gap",
    )?;

    let (_, status) = ripr(&decoy, &["agent", "status", "--root", &root_arg])?;
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

#[test]
fn typescript_literal_threshold_packet_is_complete_and_controls_fail_closed() -> Result<(), String>
{
    let toml = "[languages]\nenabled = [\"typescript\"]\n";
    let (root, decoy) = materialize(
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
    let (_, status) = ripr(&decoy, &["agent", "status", "--root", &root_arg])?;
    // Before pilot, status routes to pilot; pilot routes to check; status then
    // terminates with the manual route instead of cycling.
    need(&status, "ripr pilot", "status routes to pilot first")?;
    let (_, pilot) = ripr(&decoy, &["pilot", "--root", &root_arg])?;
    need(
        &pilot,
        "analyze the TypeScript diff: ripr check",
        "pilot routes to check",
    )?;
    let (_, status) = ripr(&decoy, &["agent", "status", "--root", &root_arg])?;
    need(
        &status,
        "Status selects no next command",
        "status terminates after pilot",
    )?;
    decoy_untouched(&decoy)?;

    for variant in [
        "variant-let-binding",
        "variant-imported-constant",
        "variant-reach-barrier",
    ] {
        let (vroot, vdecoy) = materialize(
            "blind_journey_installed_typescript",
            variant,
            "tsv",
            Some(toml),
        )?;
        let vroot_arg = vroot.to_string_lossy().into_owned();
        let (_, out) = ripr(
            &vdecoy,
            &["check", "--root", &vroot_arg, "--base", "HEAD~1"],
        )?;
        need(&out, "repair packet is not ready", variant)?;
        if out.contains("the repair packet is complete") {
            return Err(format!("{variant} projected a delegatable packet:\n{out}"));
        }
    }
    Ok(())
}

#[test]
fn typescript_without_enabling_config_is_not_a_clean_result() -> Result<(), String> {
    // Pinned to Rust only so no-config project detection cannot enable TypeScript.
    let (root, decoy) = materialize(
        "blind_journey_installed_typescript",
        "head",
        "tsoff",
        Some("[languages]\nenabled = [\"rust\"]\n"),
    )?;
    let root_arg = root.to_string_lossy().into_owned();
    let (_, out) = ripr(&decoy, &["check", "--root", &root_arg, "--base", "HEAD~1"])?;
    need(&out, "analysis incomplete", "incomplete analysis")?;
    need(
        &out,
        "enabled = [\"typescript\"]",
        "names the enabling config",
    )?;
    Ok(())
}
