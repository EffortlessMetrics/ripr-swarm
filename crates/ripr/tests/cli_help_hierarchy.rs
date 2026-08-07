//! Drift guard for the public command hierarchy (#2930).
//!
//! These tests exercise the *rendered* help surfaces — the stdout of the built
//! binary — rather than the help source files. A substring check against
//! `src/cli/help/overview.rs` passes even when a route disappears from the
//! default screen, because that one file holds the default overview, the
//! exhaustive reference, and prose; only the rendered output is the public
//! contract. The doc asserts below pin the same canonical role vocabulary in
//! the three human-facing docs so README, Quickstart, and the hierarchy page
//! cannot drift away from the help the binary actually prints.

use std::process::Command;

const COMMAND_HIERARCHY_DOC: &str = include_str!("../../../docs/COMMAND_HIERARCHY.md");
const ROOT_README: &str = include_str!("../../../README.md");
const QUICKSTART_DOC: &str = include_str!("../../../docs/QUICKSTART.md");

fn rendered_help(args: &[&str]) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .output()
        .map_err(|error| format!("failed to run ripr {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ripr {args:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("ripr {args:?} emitted non-UTF-8 stdout: {error}"))
}

fn assert_contains(surface: &str, text: &str, needle: &str) -> Result<(), String> {
    if text.contains(needle) {
        return Ok(());
    }
    Err(format!("{surface} lost the canonical route `{needle}`"))
}

/// Collapse all whitespace runs so a doc needle survives line reflows; the
/// vocabulary, not the wrapping, is the contract.
fn normalized(doc: &str) -> String {
    doc.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn default_help_keeps_the_task_roles_distinct() -> Result<(), String> {
    for args in [["--help"].as_slice(), ["help"].as_slice()] {
        let stdout = rendered_help(args)?;
        for needle in [
            "Diagnose setup        ripr doctor",
            "Inspect one change    ripr check",
            "Guided repo adoption  ripr pilot --root .",
            "Repair one named gap  ripr agent repair",
            "Compose PR evidence   ripr first-pr",
            "Adopt advisory CI     ripr init --ci github",
        ] {
            assert_contains(
                "default help (`ripr --help` / `ripr help`)",
                &stdout,
                needle,
            )?;
        }
        assert_contains(
            "default help (`ripr --help` / `ripr help`)",
            &stdout,
            "ripr help --all",
        )?;
    }
    Ok(())
}

#[test]
fn exhaustive_help_keeps_the_same_roles_and_boundaries() -> Result<(), String> {
    let stdout = rendered_help(&["help", "--all"])?;
    for needle in [
        "Diagnose setup        ripr doctor",
        "Inspect one change    ripr check --base origin/main",
        "Guided repo adoption  ripr pilot --root .",
        "Repair one named gap  ripr agent repair --seam-id ID --phase before|after",
        "Compose PR evidence   ripr first-pr --root . --base origin/main --head HEAD",
        "Adopt advisory CI     ripr init --ci github",
        "`ripr check` is the ordinary first-value analysis; `ripr pilot` is the guided repo-adoption workflow.",
        "`ripr first-pr` and `ripr start-here` compose `target/ripr/reports/start-here.{json,md}` from existing artifacts; they do not run analysis or repair a gap.",
    ] {
        assert_contains("exhaustive help (`ripr help --all`)", &stdout, needle)?;
    }
    Ok(())
}

#[test]
fn agent_help_makes_repair_primary_without_removing_control_surfaces() -> Result<(), String> {
    for args in [["agent", "--help"].as_slice(), ["agent"].as_slice()] {
        let stdout = rendered_help(args)?;
        assert_contains("agent help", &stdout, "Primary workflow:")?;
        assert_contains(
            "agent help",
            &stdout,
            "repair    Run the two-phase before/edit/after repair transaction for one seam.",
        )?;
        assert_contains(
            "agent help",
            &stdout,
            "status    Report existing agent-loop artifacts and the exact next command.",
        )?;
        let advanced = stdout
            .split("Advanced and compatibility workflows:")
            .nth(1)
            .ok_or_else(|| {
                "agent help lost the explicit advanced/compatibility boundary".to_string()
            })?;
        // Match each advanced command as the first token of a listed entry, so
        // a dropped entry cannot hide behind a substring of another command's
        // name or description (`verify` inside `verify-execute`, and so on).
        for command in [
            "start",
            "brief",
            "packet",
            "verify",
            "verify-execute",
            "receipt",
            "review-summary",
        ] {
            let listed = advanced
                .lines()
                .any(|line| line.split_whitespace().next() == Some(command));
            if !listed {
                return Err(format!(
                    "agent help lost the advanced command entry `{command}`"
                ));
            }
        }
    }
    Ok(())
}

/// The hierarchy page, the README, and the Quickstart must keep the same role
/// vocabulary the rendered help prints. Whitespace is normalized so a reflow
/// does not break the pin; rewording a role does.
#[test]
fn docs_keep_the_canonical_role_vocabulary() -> Result<(), String> {
    let hierarchy = normalized(COMMAND_HIERARCHY_DOC);
    for needle in [
        "Ordinary first value: analyze the selected diff and name the top gap",
        "Guided repository analysis and materialization.",
        "Repair one named gap",
        "Composes existing artifacts into the start-here packet. It does not run analysis or repair a gap.",
    ] {
        assert_contains("docs/COMMAND_HIERARCHY.md", &hierarchy, needle)?;
    }

    let readme = normalized(ROOT_README);
    for needle in [
        "`ripr check` is the ordinary first-value command",
        "use the dedicated two-phase transaction",
        "`ripr pilot --root .` remains the guided repository-adoption workflow.",
        "composes existing artifacts into PR-facing evidence; it is not the analyzer or repair driver.",
    ] {
        assert_contains("README.md", &readme, needle)?;
    }

    let quickstart = normalized(QUICKSTART_DOC);
    for needle in [
        "`check` is ordinary first value, `pilot` is guided repo adoption, `agent repair` is the repair transaction, and `first-pr` composes PR evidence.",
        "use the primary repair command",
        "Ask RIPR what local artifacts already exist when resuming or diagnosing: ```bash ripr agent status --root .",
    ] {
        assert_contains("docs/QUICKSTART.md", &quickstart, needle)?;
    }
    Ok(())
}
