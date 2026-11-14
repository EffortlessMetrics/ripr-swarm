//! RIPR-SPEC-0439 (#4987): replay manual security subjects without GitHub or product work.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const WORKFLOW: &str = include_str!("../../../.github/workflows/security.yml");
const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const RUN: &str = "cccccccccccccccccccccccccccccccccccccccc";
static NEXT: AtomicUsize = AtomicUsize::new(0);
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn acquire_scratch(path: PathBuf) -> TestResult<Scratch> {
    fs::create_dir(&path)?;
    Ok(Scratch(path))
}

fn step_shell(source: &str, name: &str) -> TestResult<String> {
    let marker = format!("      - name: {name}\n");
    let (_, tail) = source.split_once(&marker).ok_or("missing named step")?;
    let step = tail.split("\n      - ").next().ok_or("missing step")?;
    let (_, body) = step.split_once("        run: |\n").ok_or("missing shell")?;
    let shell = body
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!shell.trim().is_empty());
    Ok(shell)
}

fn run(shell: &str, overrides: &[(&str, &str)]) -> TestResult<(i32, String, String)> {
    let temp_root = std::env::temp_dir();
    fs::create_dir_all(&temp_root)?;
    let scratch = acquire_scratch(temp_root.join(format!(
        "ripr-security-subject-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )))?;
    let output = scratch.0.join("output");
    let summary = scratch.0.join("summary");
    fs::write(&output, "")?;
    fs::write(&summary, "")?;
    // The checkout verifier reads Git; only this fixed read is available here.
    let script = format!(
        "git() {{ [[ \"$*\" == 'rev-parse HEAD' ]] || return 90; printf '%s\\n' \"$ACTUAL_SHA\"; }}\n{shell}"
    );
    let result = Command::new("timeout")
        .args([
            "--kill-after=1s",
            "5s",
            "bash",
            "--noprofile",
            "--norc",
            "-c",
            &script,
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("EVENT_NAME", "workflow_dispatch")
        .env("RUN_SHA", RUN)
        .env("INPUT_BASE_SHA", BASE)
        .env("INPUT_HEAD_SHA", HEAD)
        .env("PR_BASE_SHA", BASE)
        .env("PR_HEAD_SHA", HEAD)
        .env("BASE_SHA", BASE)
        .env("HEAD_SHA", HEAD)
        .env("ACTUAL_SHA", HEAD)
        .env("GITHUB_OUTPUT", &output)
        .env("GITHUB_STEP_SUMMARY", &summary)
        .envs(overrides.iter().copied())
        .current_dir(&scratch.0)
        .output()?;
    let status = result.status.code().ok_or("shell terminated by signal")?;
    assert_ne!(status, 124, "security fixture exceeded its deadline");
    Ok((
        status,
        fs::read_to_string(output)?,
        fs::read_to_string(summary)?,
    ))
}

#[test]
fn fixture_collision_preserves_the_unacquired_directory() -> TestResult {
    let temp_root = std::env::temp_dir();
    fs::create_dir_all(&temp_root)?;
    let owned = acquire_scratch(temp_root.join(format!(
        "ripr-security-collision-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )))?;
    let existing = owned.0.join("existing");
    fs::create_dir(&existing)?;
    let sentinel = existing.join("sentinel");
    fs::write(&sentinel, "keep")?;
    assert!(acquire_scratch(existing).is_err());
    assert_eq!(fs::read_to_string(sentinel)?, "keep");
    Ok(())
}

#[test]
fn cargo_scan_binds_candidate_and_preserves_unselected_event_subjects() -> TestResult {
    let shell = step_shell(WORKFLOW, "Select immutable cargo-deny subject")?;
    let (status, output, _) = run(&shell, &[])?;
    assert_eq!(status, 0);
    assert_eq!(output, format!("head_sha={HEAD}\n"));
    for event in ["workflow_dispatch", "schedule", "push", "pull_request"] {
        let (status, output, _) = run(
            &shell,
            &[
                ("EVENT_NAME", event),
                ("INPUT_BASE_SHA", ""),
                ("INPUT_HEAD_SHA", ""),
            ],
        )?;
        assert_eq!(status, 0, "{event}");
        assert_eq!(output, format!("head_sha={RUN}\n"), "{event}");
    }
    Ok(())
}

#[test]
fn manual_security_refuses_partial_mutable_equal_and_injected_subjects() -> TestResult {
    for name in [
        "Select immutable cargo-deny subject",
        "Select immutable dependency comparison",
    ] {
        let shell = step_shell(WORKFLOW, name)?;
        for (base, head) in [
            ("", HEAD),
            (BASE, ""),
            ("main", HEAD),
            (BASE, "refs/heads/candidate"),
            (BASE, BASE),
            (
                BASE,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nhead_sha=main",
            ),
            (BASE, "$(touch injected)"),
        ] {
            let (status, output, _) = run(
                &shell,
                &[("INPUT_BASE_SHA", base), ("INPUT_HEAD_SHA", head)],
            )?;
            assert_eq!(status, 1, "{name}: {base:?} -> {head:?}");
            assert!(output.is_empty(), "refused input must publish no subject");
        }
    }
    Ok(())
}

#[test]
fn dependency_comparison_uses_manual_pair_or_actual_pr_context() -> TestResult {
    let shell = step_shell(WORKFLOW, "Select immutable dependency comparison")?;
    let (status, output, _) = run(&shell, &[])?;
    assert_eq!(status, 0);
    assert_eq!(output, format!("base_sha={BASE}\nhead_sha={HEAD}\n"));
    let (status, output, _) = run(
        &shell,
        &[("EVENT_NAME", "pull_request"), ("PR_HEAD_SHA", RUN)],
    )?;
    assert_eq!(status, 0);
    assert_eq!(output, format!("base_sha={BASE}\nhead_sha={RUN}\n"));
    // Normal no-input manual scans must not become an empty successful comparison.
    for overrides in [
        vec![("INPUT_BASE_SHA", ""), ("INPUT_HEAD_SHA", "")],
        vec![("EVENT_NAME", "schedule")],
    ] {
        let (status, output, _) = run(&shell, &overrides)?;
        assert_eq!(status, 1);
        assert!(output.is_empty());
    }
    Ok(())
}

#[test]
fn both_consumers_refuse_the_wrong_checkout() -> TestResult {
    for name in [
        "Verify cargo-deny checkout",
        "Verify dependency-review checkout",
    ] {
        let shell = step_shell(WORKFLOW, name)?;
        let (status, _, summary) = run(&shell, &[])?;
        assert_eq!(status, 0, "{name}");
        assert!(summary.contains(HEAD));
        let (status, _, summary) = run(&shell, &[("ACTUAL_SHA", RUN)])?;
        assert_eq!(status, 1, "{name}");
        assert!(summary.is_empty());
        // Removing the live checkout comparison recreates the false receipt.
        let bypassed = shell.replace("[[ \"$(git rev-parse HEAD)\" == \"$HEAD_SHA\" ]]", "true");
        assert_ne!(shell, bypassed);
        assert_eq!(run(&bypassed, &[("ACTUAL_SHA", RUN)])?.0, 0);
    }
    Ok(())
}

fn consumer_bindings(source: &str) -> bool {
    let Some((_, cargo)) = source.split_once("\n  cargo-deny:\n") else {
        return false;
    };
    let Some((cargo, _)) = cargo.split_once("\n  deny-tracker:\n") else {
        return false;
    };
    let Some((_, review)) = source.split_once("\n  dependency-review:\n") else {
        return false;
    };
    let bound = [cargo, review].iter().all(|job| {
        job.contains("ref: ${{ steps.subject.outputs.head_sha }}")
            && job.contains("HEAD_SHA: ${{ steps.subject.outputs.head_sha }}")
            && job.contains("INPUT_BASE_SHA: ${{ inputs.base_sha }}")
            && job.contains("INPUT_HEAD_SHA: ${{ inputs.head_sha }}")
            && job.contains("persist-credentials: false")
            && !job.contains("continue-on-error:")
    });
    bound
        && cargo.contains("RUN_SHA: ${{ github.sha }}")
        && cargo.lines().any(|line| {
            let line = line.trim();
            line.strip_prefix("- ").unwrap_or(line)
                == "run: cargo-deny check advisories licenses bans sources"
        })
        && review.contains(
            "github.event_name == 'workflow_dispatch' && (inputs.base_sha != '' || inputs.head_sha != '')",
        )
        && review.contains("base-ref: ${{ steps.subject.outputs.base_sha }}")
        && review.contains("head-ref: ${{ steps.subject.outputs.head_sha }}")
        && review.contains("fail-on-severity: high")
        && review.contains("deny-licenses: GPL-2.0, GPL-3.0, AGPL-1.0, AGPL-3.0")
        && !review.contains("warn-only:")
}

#[test]
fn selected_subjects_reach_checkout_and_the_real_security_consumers() {
    assert!(consumer_bindings(WORKFLOW));
    for (old, new) in [
        ("ref: ${{ steps.subject.outputs.head_sha }}", "ref: main"),
        (
            "INPUT_HEAD_SHA: ${{ inputs.head_sha }}",
            "INPUT_HEAD_SHA: ${{ github.sha }}",
        ),
        (
            "base-ref: ${{ steps.subject.outputs.base_sha }}",
            "base-ref: main",
        ),
        (
            "head-ref: ${{ steps.subject.outputs.head_sha }}",
            "head-ref: ${{ github.sha }}",
        ),
        ("inputs.base_sha != '' || inputs.head_sha != ''", "false"),
        (
            "cargo-deny check advisories licenses bans sources",
            "cargo-deny check licenses bans sources",
        ),
        (
            "run: cargo-deny check advisories licenses bans sources",
            "run: cargo-deny check advisories licenses bans sources || true",
        ),
        ("fail-on-severity: high", "fail-on-severity: critical"),
        ("persist-credentials: false", "persist-credentials: true"),
    ] {
        assert!(WORKFLOW.contains(old), "mutation must affect source");
        assert!(
            !consumer_bindings(&WORKFLOW.replace(old, new)),
            "accepted {old}"
        );
    }
}
