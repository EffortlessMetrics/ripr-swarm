//! Rendered contract for the progressively disclosed xtask help surface (#3502).
//!
//! These tests exercise the built binary rather than matching help source text,
//! so parser and dispatch routing are part of the contract.

use std::process::Output;

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

fn run_xtask(args: &[&str]) -> Result<Output, String> {
    xtask_bin::output(|command| {
        command.args(args);
    })
    .map_err(|error| format!("failed to run xtask {args:?}: {error}"))
}

fn rendered_help(args: &[&str]) -> Result<String, String> {
    let output = run_xtask(args)?;
    if !output.status.success() {
        return Err(format!(
            "xtask {args:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("xtask {args:?} emitted non-UTF-8 stdout: {error}"))
}

fn rendered_error(args: &[&str]) -> Result<String, String> {
    let output = run_xtask(args)?;
    if output.status.success() {
        return Err(format!("xtask {args:?} unexpectedly succeeded"));
    }
    Ok(format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn default_help_routes_through_the_bounded_contributor_path() -> Result<(), String> {
    let mut rendered = Vec::new();
    let variants: &[&[&str]] = &[&[], &["help"], &["--help"], &["-h"]];
    for args in variants {
        let stdout = normalized(&rendered_help(args)?);
        for needle in [
            "cargo xtask doctor",
            "cargo xtask first-pr",
            "cargo xtask pr-ready",
            "cargo xtask check-pr",
            "cargo xtask cockpit",
            "cargo xtask help --all",
        ] {
            if !stdout.contains(needle) {
                return Err(format!(
                    "default help for {args:?} lost the route `{needle}`\nstdout:\n{stdout}"
                ));
            }
        }
        if stdout.contains("xtask commands:") || stdout.contains("check-static-language [CI]") {
            return Err(format!(
                "default help for {args:?} expanded into the exhaustive command reference\nstdout:\n{stdout}"
            ));
        }
        rendered.push(stdout);
    }

    if rendered.windows(2).any(|pair| pair[0] != pair[1]) {
        return Err("bare, help, --help, and -h rendered different start screens".to_string());
    }
    Ok(())
}

#[test]
fn exhaustive_help_preserves_the_ci_marked_command_catalog() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["help", "--all"])?);
    for needle in [
        "xtask commands:",
        "check-static-language [CI]",
        "goldens check [CI]",
        "Unmarked commands are advisory or local-only.",
        "`cargo xtask help` for common starting points",
    ] {
        if !stdout.contains(needle) {
            return Err(format!(
                "exhaustive help lost `{needle}`\nstdout:\n{stdout}"
            ));
        }
    }
    if stdout.contains("check-supply-chain [CI]") {
        return Err(
            "exhaustive help marked advisory check-supply-chain as CI-enforced".to_string(),
        );
    }
    Ok(())
}

#[test]
fn command_help_keeps_mutability_and_enforcement_details() -> Result<(), String> {
    let stdout = normalized(&rendered_help(&["help", "check-pr"])?);
    for needle in [
        "Usage: cargo xtask check-pr",
        "Mutability:",
        "Writes:",
        "Judgment required:",
        "CI enforced:",
        "Notes:",
        "cargo xtask help --all",
    ] {
        if !stdout.contains(needle) {
            return Err(format!(
                "per-command help lost `{needle}`\nstdout:\n{stdout}"
            ));
        }
    }
    Ok(())
}

#[test]
fn check_fast_is_discoverable_with_its_local_proof_boundary() -> Result<(), String> {
    let exhaustive = normalized(&rendered_help(&["help", "--all"])?);
    if !exhaustive.split_whitespace().any(|word| word == "check-fast") {
        return Err("exhaustive help omitted the local check-fast command".to_string());
    }
    if exhaustive.contains("check-fast [CI]") {
        return Err("local check-fast was marked as CI-enforced".to_string());
    }

    let detail = normalized(&rendered_help(&["help", "check-fast"])?);
    for needle in [
        "Usage: cargo xtask check-fast",
        "Mutability: non_mutating_check",
        "target/ripr/reports/check-fast.md",
        "target/ripr/reports/check-fast-selector.md",
        "Judgment required: false",
        "CI enforced: false",
        "committed origin/main...HEAD",
        "live worktree",
        "unstaged, staged, or untracked",
        "git fetch origin main",
        "builds xtask",
        "workspace/all-target Clippy",
        "does not replace precommit or required hosted CI",
    ] {
        if !detail.contains(needle) {
            return Err(format!(
                "check-fast help lost `{needle}`\nstdout:\n{detail}"
            ));
        }
    }
    Ok(())
}

#[test]
fn unknown_and_retired_commands_route_to_exhaustive_help() -> Result<(), String> {
    for command in ["chek-pr", "goals"] {
        let stderr = normalized(&rendered_error(&[command])?);
        if !stderr.contains("cargo xtask help --all") {
            return Err(format!(
                "unknown route for `{command}` lost exhaustive help guidance\noutput:\n{stderr}"
            ));
        }
        if stderr.contains("`cargo xtask help` for the full list") {
            return Err(format!(
                "unknown route for `{command}` still points at bounded help as the full list\noutput:\n{stderr}"
            ));
        }
    }

    let stderr = normalized(&rendered_error(&["help", "chek-pr"])?);
    if !stderr.contains("cargo xtask help --all")
        || stderr.contains("`cargo xtask help` for the full list")
    {
        return Err(format!(
            "unknown help target lost exhaustive help guidance\noutput:\n{stderr}"
        ));
    }
    Ok(())
}
