//! The trusted admission bundle must be pinned to the base of the judged tree:
//! the first parent of the pull_request merge ref. The anchor is local and
//! immutable for the run, so a moving base branch cannot change it. There is
//! no `gh` call in this step, so there is no empty-`gh` case to exercise.
#![cfg(target_os = "linux")]

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const WORKFLOW: &str = include_str!("../../../.github/workflows/routed-rust.yml");
const MERGE: &str = "cd6f0d2faaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "3b280022bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HEAD: &str = "041b9d44cccccccccccccccccccccccccccccccc";
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn step_shell(source: &str, name: &str) -> Result<String, String> {
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

fn run(shell: &str, parents: &str, overrides: &[(&str, &str)]) -> Result<(i32, String), String> {
    let temp_root = std::env::temp_dir();
    fs::create_dir_all(&temp_root).map_err(|e| e.to_string())?;
    let scratch = temp_root.join(format!(
        "ripr-trusted-source-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&scratch).map_err(|e| e.to_string())?;
    let output = scratch.join("output");
    fs::write(&output, "").map_err(|e| e.to_string())?;
    // The anchor is local: fetch succeeds and rev-list returns the scripted
    // parent line. No gh call exists in this step.
    let script = format!(
        "git() {{ case \"$1\" in fetch) return 0 ;; rev-list) printf '%s\\n' \"$PARENTS\" ;; *) return 90 ;; esac; }}\n{shell}"
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
        .env("EVENT_NAME", "pull_request")
        .env("EVENT_SHA", MERGE)
        .env("GROUP_BASE_SHA", "")
        .env("PARENTS", parents)
        .env("GITHUB_WORKSPACE", &scratch)
        .env("GITHUB_OUTPUT", &output)
        .envs(overrides.iter().copied())
        .current_dir(&scratch)
        .output()
        .map_err(|e| e.to_string())?;
    let status = result.status.code().ok_or("shell terminated by signal")?;
    assert_ne!(status, 124, "trusted-source fixture exceeded its deadline");
    let out = fs::read_to_string(&output).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&scratch);
    Ok((status, out))
}

#[test]
fn pull_request_pins_the_base_of_the_judged_tree() -> Result<(), String> {
    let shell = step_shell(WORKFLOW, "Select trusted admission source")?;
    let (status, output) = run(&shell, &format!("{MERGE} {BASE} {HEAD}"), &[])?;
    assert_eq!(status, 0, "step failed: {output}");
    assert_eq!(
        output,
        format!("sha={BASE}\n"),
        "the trusted bundle must follow the judged tree's base"
    );
    Ok(())
}

#[test]
fn a_one_parent_commit_is_refused() -> Result<(), String> {
    let shell = step_shell(WORKFLOW, "Select trusted admission source")?;
    let (status, output) = run(&shell, &format!("{HEAD} {BASE}"), &[])?;
    assert_ne!(status, 0, "a one-parent commit must refuse: {output}");
    Ok(())
}

#[test]
fn a_three_parent_commit_is_refused() -> Result<(), String> {
    let shell = step_shell(WORKFLOW, "Select trusted admission source")?;
    let (status, output) = run(&shell, &format!("{MERGE} {BASE} {HEAD} {HEAD}"), &[])?;
    assert_ne!(status, 0, "a three-parent commit must refuse: {output}");
    Ok(())
}

#[test]
fn merge_group_still_pins_the_group_base() -> Result<(), String> {
    let shell = step_shell(WORKFLOW, "Select trusted admission source")?;
    let (status, output) = run(
        &shell,
        &format!("{MERGE} {BASE} {HEAD}"),
        &[("EVENT_NAME", "merge_group"), ("GROUP_BASE_SHA", BASE)],
    )?;
    assert_eq!(status, 0, "step failed: {output}");
    assert_eq!(output, format!("sha={BASE}\n"));
    Ok(())
}
