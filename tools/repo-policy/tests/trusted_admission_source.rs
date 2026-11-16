//! The trusted admission bundle must be pinned to the live base the judged
//! tree is built on, not the PR's recorded base. A stale recorded base pins an
//! older admission script than the merge ref is judged against.
#![cfg(target_os = "linux")]

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const WORKFLOW: &str = include_str!("../../../.github/workflows/routed-rust.yml");
const RECORDED_BASE: &str = "96cd1c58aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const LIVE_BASE: &str = "290e3850bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
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

fn run(shell: &str, overrides: &[(&str, &str)]) -> Result<(i32, String), String> {
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
    // The step resolves the live base ref through gh; the recorded base is
    // deliberately different so the two pins are distinguishable.
    let script = format!(
        "gh() {{ [[ \"$1\" = api ]] || return 90; [[ \"$2\" == *git/ref/heads/* ]] || return 91; printf '%s\\n' \"$LIVE_BASE\"; }}\n{shell}"
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
        .env("PR_BASE_SHA", RECORDED_BASE)
        .env("PR_BASE_REF", "main")
        .env("GROUP_BASE_SHA", "")
        .env("EVENT_SHA", "")
        .env("REPOSITORY", "EffortlessMetrics/ripr-swarm")
        .env("LIVE_BASE", LIVE_BASE)
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
fn pull_request_pins_the_live_base_not_the_recorded_base() -> Result<(), String> {
    let shell = step_shell(WORKFLOW, "Select trusted admission source")?;
    let (status, output) = run(&shell, &[])?;
    assert_eq!(status, 0, "step failed: {output}");
    assert_eq!(
        output,
        format!("sha={LIVE_BASE}\n"),
        "the trusted bundle must follow the live base, not the recorded base"
    );
    Ok(())
}

#[test]
fn merge_group_still_pins_the_group_base() -> Result<(), String> {
    let shell = step_shell(WORKFLOW, "Select trusted admission source")?;
    let (status, output) = run(
        &shell,
        &[("EVENT_NAME", "merge_group"), ("GROUP_BASE_SHA", LIVE_BASE)],
    )?;
    assert_eq!(status, 0, "step failed: {output}");
    assert_eq!(output, format!("sha={LIVE_BASE}\n"));
    Ok(())
}
