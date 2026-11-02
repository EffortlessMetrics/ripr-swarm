//! #12338: a Windows shard that wedges until the job cap must still leave a
//! named in-flight test (heartbeat) and files the `always()` upload can pick
//! up. The previous 44-minute watchdog counted from step start, so a long
//! warmup let the 80-minute job cap cancel the job first with no log.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SHARDS: &str = ".github/workflows/windows-advisory-shards.yml";
const QUALIFICATION: &str = ".github/workflows/windows-watchdog-qualification.yml";
const NEXTEST: &str = ".config/nextest.toml";

/// `bash` on a Windows PATH can be the WSL launcher in System32, which fails
/// without an installed distribution. Prefer Git for Windows' bash there.
fn bash() -> PathBuf {
    let git_bash = std::env::var_os("ProgramFiles")
        .filter(|_| cfg!(windows))
        .map(|files| {
            PathBuf::from(files)
                .join("Git")
                .join("bin")
                .join("bash.exe")
        })
        .filter(|path| path.exists());
    git_bash.unwrap_or_else(|| PathBuf::from("bash"))
}

fn repo_file(path: &str) -> Result<String, String> {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(path);
    fs::read_to_string(&full).map_err(|e| format!("read {}: {e}", full.display()))
}

fn tests_run_block(text: &str) -> Result<String, String> {
    let lines: Vec<&str> = text
        .lines()
        .skip_while(|line| !line.contains("- name: Run workspace tests"))
        .skip_while(|line| line.trim() != "run: |")
        .skip(1)
        .take_while(|line| line.starts_with("          "))
        .map(|line| line.trim_start())
        .collect();
    if lines.is_empty() {
        return Err("Run workspace tests step not found".to_string());
    }
    Ok(lines.join("\n").replace("${{ matrix.shard }}", "1"))
}

fn polls_math_script(block: &str) -> Result<String, String> {
    let mut chosen = Vec::new();
    for line in block.lines() {
        if line.contains("JOB_CAP_SECS:=") || line.contains("start=${JOB_START") {
            chosen.push(line);
        }
    }
    if chosen.len() != 2 {
        return Err(format!(
            "expected the cap defaults line and the remain/polls line, got {} lines",
            chosen.len()
        ));
    }
    chosen.push("printf '%s\\n' \"$polls\"");
    Ok(chosen.join("\n"))
}

fn run_polls_math(script: &str, env: &[(&str, String)]) -> Result<u32, String> {
    let dir = std::env::temp_dir().join(format!(
        "ripr-shard-polls-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("polls.sh");
    fs::write(
        &path,
        format!("#!/usr/bin/env bash\nset -euo pipefail\n{script}\n"),
    )
    .map_err(|e| e.to_string())?;
    let mut cmd = Command::new(bash());
    cmd.arg(&path).current_dir(&dir);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let _ = fs::remove_dir_all(&dir);
    if !output.status.success() {
        return Err(format!(
            "polls math failed: status={} stdout={stdout:?} stderr={stderr:?}",
            output.status
        ));
    }
    stdout
        .parse()
        .map_err(|e| format!("polls was not an integer {stdout:?}: {e}"))
}

#[test]
fn windows_advisory_profile_still_terminates_a_slow_test() -> Result<(), String> {
    let text = repo_file(NEXTEST)?;
    let profile = text
        .split("[profile.windows-advisory]")
        .nth(1)
        .ok_or("missing [profile.windows-advisory]")?;
    let profile = profile.split('[').next().unwrap_or(profile);
    if !profile.contains("slow-timeout") || !profile.contains("terminate-after") {
        return Err(format!(
            "windows-advisory must terminate a hung test, not only label it SLOW:\n{profile}"
        ));
    }
    Ok(())
}

#[test]
fn shard_workflow_keeps_cancel_upload_and_heartbeat_contract() -> Result<(), String> {
    let text = repo_file(SHARDS)?;
    for needle in [
        "JOB_CAP_SECS:=4800",
        "UPLOAD_RESERVE_SECS:=480",
        "WATCHDOG_POLLS:=264",
        "POLL_SECS:=10",
        "JOB_START",
        "target/shard-$shard.heartbeat",
        "target/shard-${{ matrix.shard }}.heartbeat",
        "if: ${{ always() }}",
        "timeout-minutes: 3",
        "timeout-minutes: 55",
        "timeout-minutes: 80",
    ] {
        if !text.contains(needle) {
            return Err(format!("workflow lost `{needle}`"));
        }
    }
    if text.contains("seq 1 264") {
        return Err(
            "fixed 264-poll watchdog is back; remaining job time must shorten it".to_string(),
        );
    }
    let upload = text
        .split("- name: Upload shard logs")
        .nth(1)
        .ok_or("missing Upload shard logs")?;
    if !upload.contains("if: ${{ always() }}") {
        return Err(
            "upload must run under always() so a failed step still leaves the artifact".into(),
        );
    }
    if !upload.contains("target/shard-${{ matrix.shard }}.heartbeat") {
        return Err("upload must include the heartbeat file".into());
    }
    Ok(())
}

#[test]
fn qualification_timer_substitutions_still_match_the_shipped_wrapper() -> Result<(), String> {
    let shards = repo_file(SHARDS)?;
    let qual = repo_file(QUALIFICATION)?;
    for needle in ["WATCHDOG_POLLS:=264", "POLL_SECS:=10"] {
        if !shards.contains(needle) {
            return Err(format!("shipped wrapper lost `{needle}`"));
        }
    }
    for needle in [
        "s/WATCHDOG_POLLS:=264/WATCHDOG_POLLS:=12/",
        "s/POLL_SECS:=10/POLL_SECS:=1/",
        "WATCHDOG_POLLS:=12",
        "POLL_SECS:=1",
    ] {
        if !qual.contains(needle) {
            return Err(format!(
                "qualification no longer substitutes the shipped timer (`{needle}` missing)"
            ));
        }
    }
    if qual.contains("s/seq 1 264/") || qual.contains("s/sleep 10/") {
        return Err(
            "qualification still patches the old seq/sleep literals; those are no longer in the wrapper"
                .into(),
        );
    }
    Ok(())
}

#[test]
fn remaining_job_time_shortens_the_watchdog_after_a_long_warmup() -> Result<(), String> {
    let block = tests_run_block(&repo_file(SHARDS)?)?;
    let script = polls_math_script(&block)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let after_warmup = run_polls_math(
        &script,
        &[("JOB_START", (now.saturating_sub(4500)).to_string())],
    )?;
    if after_warmup != 1 {
        return Err(format!(
            "75 minutes into an 80-minute job with an 8-minute upload reserve must leave one poll, got {after_warmup}"
        ));
    }
    let fresh = run_polls_math(&script, &[("JOB_START", now.to_string())])?;
    if fresh != 264 {
        return Err(format!(
            "a fresh job must still use the 44-minute max (264 polls), got {fresh}"
        ));
    }
    Ok(())
}

#[test]
fn fixed_step_start_watchdog_would_not_shorten_after_warmup() -> Result<(), String> {
    // Negative control: the pre-#12338 loop `seq 1 264` ignores remaining job
    // time. If that loop returned, the 75-minute-elapsed case above would
    // still be 264 polls and this assertion would catch a revert that drops
    // the remain/polls assignment.
    let block = tests_run_block(&repo_file(SHARDS)?)?;
    if block.contains("seq 1 264") && !block.contains("remain=") {
        return Err("old fixed 264-poll loop is the watchdog again".into());
    }
    if !block.contains("remain=") || !block.contains("polls=") {
        return Err("watchdog no longer computes polls from remaining job time".into());
    }
    Ok(())
}

#[test]
fn remaining_time_watchdog_leaves_heartbeat_and_status_for_upload() -> Result<(), String> {
    let mut block = tests_run_block(&repo_file(SHARDS)?)?;
    let cargo = "cargo nextest run --profile windows-advisory --workspace --no-fail-fast --partition count:$shard/4 -E \"not $ACL_TESTS\" > target/shard-$shard.log 2>&1 &";
    if !block.contains(cargo) {
        return Err(format!("did not find the shipped nextest launch:\n{block}"));
    }
    block = block.replace(
        cargo,
        "bash target/fixture.sh > target/shard-$shard.log 2>&1 &",
    );

    let dir = std::env::temp_dir().join(format!(
        "ripr-shard-hang-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("target")).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("target/fixture.sh"),
        "#!/usr/bin/env bash\nprintf '%s\\n' '        SLOW [> 120.0s] xtask hung::forever'\nwhile true; do :; done\n",
    )
    .map_err(|e| e.to_string())?;
    let script = dir.join("wrapper.sh");
    fs::write(
        &script,
        format!("#!/usr/bin/env bash\nset -eo pipefail\n{block}\n"),
    )
    .map_err(|e| e.to_string())?;

    let output = Command::new(bash())
        .arg(&script)
        .current_dir(&dir)
        .env("JOB_CAP_SECS", "3")
        .env("UPLOAD_RESERVE_SECS", "2")
        .env("POLL_SECS", "1")
        .env("WATCHDOG_POLLS", "264")
        .output()
        .map_err(|e| e.to_string())?;
    let heartbeat = fs::read_to_string(dir.join("target/shard-1.heartbeat")).unwrap_or_default();
    let status = fs::read_to_string(dir.join("target/shard-1.status")).unwrap_or_default();
    let watchdog = fs::read_to_string(dir.join("target/shard-1.watchdog")).unwrap_or_default();
    let log = fs::read_to_string(dir.join("target/shard-1.log")).unwrap_or_default();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = fs::remove_dir_all(&dir);

    if status.trim() != "124" {
        return Err(format!(
            "expected status 124 after remaining-time fire, got {status:?} exit={} stderr={stderr} heartbeat={heartbeat:?} watchdog={watchdog:?} log={log:?}",
            output.status
        ));
    }
    if !watchdog.contains("fired") {
        return Err(format!("watchdog marker missing: {watchdog:?}"));
    }
    if !log.contains("hung::forever") {
        return Err(format!("log lost the in-flight SLOW row: {log:?}"));
    }
    if !heartbeat.contains("hung::forever") {
        return Err(format!(
            "heartbeat did not name the in-flight test: {heartbeat:?}"
        ));
    }
    Ok(())
}

#[test]
fn ordinary_exit_does_not_write_a_watchdog_marker() -> Result<(), String> {
    let mut block = tests_run_block(&repo_file(SHARDS)?)?;
    let cargo = "cargo nextest run --profile windows-advisory --workspace --no-fail-fast --partition count:$shard/4 -E \"not $ACL_TESTS\" > target/shard-$shard.log 2>&1 &";
    if !block.contains(cargo) {
        return Err(format!("did not find the shipped nextest launch:\n{block}"));
    }
    block = block.replace(
        cargo,
        "bash target/fixture.sh > target/shard-$shard.log 2>&1 &",
    );
    let dir = std::env::temp_dir().join(format!(
        "ripr-shard-ok-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("target")).map_err(|e| e.to_string())?;
    fs::write(
        dir.join("target/fixture.sh"),
        "#!/usr/bin/env bash\nprintf '%s\\n' '     Summary [  0.1s] 1 test run: 1 passed'\n",
    )
    .map_err(|e| e.to_string())?;
    let script = dir.join("wrapper.sh");
    fs::write(
        &script,
        format!("#!/usr/bin/env bash\nset -eo pipefail\n{block}\n"),
    )
    .map_err(|e| e.to_string())?;
    let output = Command::new(bash())
        .arg(&script)
        .current_dir(&dir)
        .env("POLL_SECS", "1")
        .env("WATCHDOG_POLLS", "264")
        .output()
        .map_err(|e| e.to_string())?;
    let status = fs::read_to_string(dir.join("target/shard-1.status")).unwrap_or_default();
    let watchdog_exists = dir.join("target/shard-1.watchdog").exists();
    let log = fs::read_to_string(dir.join("target/shard-1.log")).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    if !output.status.success() || status.trim() != "0" {
        return Err(format!(
            "ordinary fixture should exit 0, got wrapper={} status={status:?} log={log:?}",
            output.status
        ));
    }
    if watchdog_exists {
        return Err("watchdog marker must not be written on a clean nextest exit".into());
    }
    Ok(())
}

#[test]
fn removing_remaining_time_math_is_what_the_elapsed_case_rejects() -> Result<(), String> {
    // Discriminator: keep the shipped default assignment but drop the remain
    // clamp. That is the pre-#12338 watchdog, and it still reports 264 polls
    // after a 75-minute warmup, which `remaining_job_time_shortens_the_watchdog_after_a_long_warmup`
    // rejects.
    let block = tests_run_block(&repo_file(SHARDS)?)?;
    let defaults = block
        .lines()
        .find(|line| line.contains("WATCHDOG_POLLS:="))
        .ok_or("missing WATCHDOG_POLLS default assignment")?;
    let script = format!("{defaults}\npolls=$WATCHDOG_POLLS\nprintf '%s\\n' \"$polls\"");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let polls = run_polls_math(
        &script,
        &[("JOB_START", (now.saturating_sub(4500)).to_string())],
    )?;
    if polls != 264 {
        return Err(format!(
            "the no-remain mutation must still be 264 polls so the positive case is a real discriminator, got {polls}"
        ));
    }
    Ok(())
}
