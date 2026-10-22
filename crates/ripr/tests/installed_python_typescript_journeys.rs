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

    // Forgery 3: reversed lineage is incomparable.
    let (ok, out) = run_verify(&after, &before)?;
    expect_exit(ok, false, "reversed pair")?;
    need(&out, "incomparable", "reversed pair named")?;
    decoy_untouched(&decoy)
}
