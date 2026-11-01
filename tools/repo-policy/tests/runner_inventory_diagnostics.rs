//! #8858: execute the actual route shell with a local gh stub, never the runner API.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const WORKFLOW: &str = include_str!("../../../.github/workflows/routed-rust.yml");
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `fail_first` inventory calls exit `status`; later calls succeed with `runners`.
fn run_route(
    status: u8,
    fail_first: u32,
    error: &str,
    runners: &str,
    untrusted: bool,
) -> TestResult<(String, String, bool, u32)> {
    run_route_env(status, fail_first, error, runners, untrusted, &[])
}

fn run_route_env(
    status: u8,
    fail_first: u32,
    error: &str,
    runners: &str,
    untrusted: bool,
    extra: &[(&str, &str)],
) -> TestResult<(String, String, bool, u32)> {
    let (output, summary, called, attempts, stdout) =
        run_route_stdout(status, fail_first, error, runners, untrusted, extra)?;
    // Only the token-rejection warning may reach the job log; error text never does.
    assert!(
        stdout.is_empty()
            || stdout.starts_with("::warning title=Runner inventory token rejected::"),
        "unexpected log output: {stdout}"
    );
    Ok((output, summary, called, attempts))
}

fn run_route_stdout(
    status: u8,
    fail_first: u32,
    error: &str,
    runners: &str,
    untrusted: bool,
    extra: &[(&str, &str)],
) -> TestResult<(String, String, bool, u32, String)> {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-runner-query-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir_all(&scratch.0)?;
    let stub = scratch.0.join("gh");
    fs::write(
        &stub,
        "#!/bin/bash\nn=$(cat \"$COUNT\" 2>/dev/null || echo 0)\nn=$((n + 1))\nprintf '%s' \"$n\" > \"$COUNT\"\nprintf '%s\\n' \"$*\" > \"$CALLS\"\nv=ERR_$n\nif [ -n \"${!v+x}\" ]; then printf '%s' \"${!v}\" >&2; else printf '%s' \"$QUERY_ERROR\" >&2; fi\n[ \"$n\" -gt \"$FAIL_FIRST\" ] || exit \"$QUERY_STATUS\"\nprintf '%s' \"$RUNNERS\"\n",
    )?;
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o700))?;
    let (_, route) = WORKFLOW.split_once("\n  route:\n").ok_or("route job")?;
    let (route, _) = route
        .split_once("\n  detect-docs-only:\n")
        .ok_or("route end")?;
    let (_, body) = route.split_once("        run: |\n").ok_or("route shell")?;
    let shell = body
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!shell.trim().is_empty());
    // Redirect the existing private error file into the fixture to avoid collisions.
    let shell = shell.replace(
        "/tmp/runners.err",
        &scratch.0.join("query.err").to_string_lossy(),
    );
    let output = scratch.0.join("output");
    let summary = scratch.0.join("summary");
    let calls = scratch.0.join("calls");
    let count = scratch.0.join("count");
    let mut paths = vec![scratch.0.clone()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").ok_or("PATH")?,
    ));
    // GNU timeout bounds descendants as well as the actual Bash command.
    let result = Command::new("timeout")
        .args([
            "--kill-after=1s",
            "5s",
            "bash",
            "--noprofile",
            "--norc",
            "-e",
            "-o",
            "pipefail",
            "-c",
            &shell,
        ])
        .env_clear()
        .env("PATH", std::env::join_paths(paths)?)
        .env("EVENT_NAME", "pull_request")
        .env(
            "HEAD_REPO",
            if untrusted {
                "fork/repo"
            } else {
                "EffortlessMetrics/ripr-swarm"
            },
        )
        .env("REPOSITORY", "EffortlessMetrics/ripr-swarm")
        .env("RUN_ID", "42")
        .env("GITHUB_OUTPUT", &output)
        .env("GITHUB_STEP_SUMMARY", &summary)
        .env("CALLS", &calls)
        .env("COUNT", &count)
        .env("FAIL_FIRST", fail_first.to_string())
        .env("RUNNER_QUERY_RETRY_DELAY", "0")
        .envs(extra.iter().copied())
        .env("QUERY_STATUS", status.to_string())
        .env("QUERY_ERROR", error)
        .env("RUNNERS", runners)
        .output()?;
    assert!(result.status.success(), "actual shell: {result:?}");
    assert!(result.stderr.is_empty());
    let called = calls.exists();
    if called {
        assert_eq!(
            fs::read_to_string(calls)?.trim(),
            "api --paginate orgs/EffortlessMetrics/actions/runners?per_page=100"
        );
    }
    let attempts = if count.exists() {
        fs::read_to_string(count)?.trim().parse()?
    } else {
        0
    };
    Ok((
        fs::read_to_string(output)?,
        fs::read_to_string(summary)?,
        called,
        attempts,
        String::from_utf8(result.stdout)?,
    ))
}

#[test]
fn persistent_failure_retries_three_times_then_reports_without_error_text() -> TestResult {
    for (status, error, http) in [
        (1, "Forbidden", "none"),
        (4, "gh: authentication required", "none"),
        (
            7,
            "HTTP 403: forbidden ghp_SECRET\nAuthorization: bearer SECRET\n::error::injected",
            "403",
        ),
        (127, "client failure", "none"),
    ] {
        let (output, summary, called, attempts) = run_route(status, 99, error, "", false)?;
        assert!(called);
        assert_eq!(attempts, 3, "bounded retry");
        assert!(output.contains("router_target=github\nrouter_reason=runner_api_failed\n"));
        assert!(output.contains("runner_query=failed\n"));
        assert!(
            output.contains(&format!("runner_query_exit_code={status}\n")),
            "{output}"
        );
        assert!(summary.contains("inventory operation: `gh api --paginate orgs/EffortlessMetrics/actions/runners?per_page=100`"));
        assert!(summary.contains(&format!("runner command exit code: `{status}`")));
        assert!(summary.contains("attempts: `3` of 3"));
        assert!(summary.contains(&format!("last HTTP status reported by gh: `{http}`")));
        assert!(summary.contains("underlying cause is not claimed"));
        for secret in [
            "Forbidden",
            "authentication required",
            "SECRET",
            "::error::",
            "client failure",
        ] {
            assert!(!format!("{output}{summary}").contains(secret));
        }
    }
    Ok(())
}

#[test]
fn transient_failures_recover_within_the_retry_bound() -> TestResult {
    let runners = r#"{"runners":[{"status":"online","busy":false,"labels":[{"name":"em-ci"},{"name":"cx43"},{"name":"rust-medium"}]}]}"#;
    for fail_first in [1, 2] {
        let (output, summary, called, attempts) =
            run_route(1, fail_first, "HTTP 502: bad gateway", runners, false)?;
        assert!(called);
        assert_eq!(attempts, fail_first + 1);
        for expected in [
            "runner_query=ok\n",
            "runner_query_exit_code=0\n",
            "router_target=cx43\n",
            "router_reason=cx43_idle\n",
            "runner_total=1\n",
        ] {
            assert!(output.contains(expected), "{output}");
        }
        assert!(!output.contains("runner_api_failed"));
        assert!(!summary.contains("inventory operation"));
        assert!(!summary.contains("bad gateway"));
    }
    // One more failure than the bound allows is a recorded failure, not a retry loop.
    let (output, _, _, attempts) = run_route(1, 3, "", runners, false)?;
    assert_eq!(attempts, 3);
    assert!(output.contains("runner_query=failed\n"), "{output}");
    Ok(())
}

#[test]
fn successful_empty_inventory_records_zero_without_changing_route() -> TestResult {
    let (output, summary, called, attempts) =
        run_route(0, 0, "ignored secret warning", "{\"runners\":[]}", false)?;
    assert!(called);
    assert_eq!(attempts, 1);
    for expected in [
        "runner_query_exit_code=0\n",
        "runner_query=ok\n",
        "router_target=github\n",
        "router_reason=no_idle_runner\n",
        "runner_total=0\n",
    ] {
        assert!(output.contains(expected), "{output}");
    }
    assert!(!summary.contains("underlying cause is not claimed"));
    assert!(!summary.contains("ignored secret warning"));
    Ok(())
}

#[test]
fn successful_inventory_preserves_capability_selection() -> TestResult {
    let runners = r#"{"runners":[{"status":"online","busy":false,"labels":[{"name":"em-ci"},{"name":"cx43"},{"name":"rust-medium"}]}]}"#;
    let (output, _, called, _) = run_route(0, 0, "", runners, false)?;
    assert!(called);
    for expected in [
        "runner_query_exit_code=0\n",
        "router_target=cx43\n",
        "router_reason=cx43_idle\n",
        "runner_total=1\n",
    ] {
        assert!(output.contains(expected), "{output}");
    }
    Ok(())
}

#[test]
fn untrusted_pr_never_queries_or_claims_a_command_status() -> TestResult {
    let (output, summary, called, _) = run_route(99, 99, "SECRET", "invalid JSON", true)?;
    assert!(!called);
    for expected in [
        "runner_query_exit_code=not_attempted\n",
        "runner_query=skipped_untrusted_pr\n",
        "router_reason=fork_or_untrusted_pr\n",
        "router_target=github\n",
    ] {
        assert!(output.contains(expected), "{output}");
    }
    assert!(!summary.contains("underlying cause is not claimed"));
    Ok(())
}

#[test]
fn summary_reports_the_final_failed_attempts_http_status() -> TestResult {
    let (_, summary, _, attempts) = run_route_env(
        1,
        99,
        "",
        "",
        false,
        &[
            ("ERR_1", "HTTP 502: a"),
            ("ERR_2", "HTTP 429: b"),
            ("ERR_3", "HTTP 403: c"),
        ],
    )?;
    assert_eq!(attempts, 3);
    assert!(summary.contains("reported by gh: `403`"), "{summary}");
    assert!(!summary.contains("502") && !summary.contains("429"));
    // A final attempt that prints no status must not inherit an earlier one.
    let (_, summary, _, _) = run_route_env(
        1,
        99,
        "",
        "",
        false,
        &[("ERR_1", "HTTP 502: a"), ("ERR_3", "opaque")],
    )?;
    assert!(summary.contains("reported by gh: `none`"), "{summary}");
    Ok(())
}

#[test]
fn malformed_retry_delay_falls_back_without_changing_retries() -> TestResult {
    for bad in ["abc", "-1", "1.5", "999999999999"] {
        let (output, _, _, attempts) = run_route_env(
            1,
            1,
            "",
            r#"{"runners":[]}"#,
            false,
            &[("RUNNER_QUERY_RETRY_DELAY", bad)],
        )?;
        assert_eq!(attempts, 2, "delay {bad:?}");
        assert!(output.contains("runner_query=ok\n"), "{output}");
    }
    Ok(())
}

#[test]
fn rejected_inventory_token_is_a_warning_not_a_failure() -> TestResult {
    for (http, warned) in [("401", true), ("403", true), ("404", false), ("502", false)] {
        let err = format!("HTTP {http}: ghp_SECRET\n::error::injected");
        let (output, _, _, _, stdout) = run_route_stdout(1, 99, &err, "", false, &[])?;
        // Routing is unchanged: the hosted leg is still selected.
        assert!(output.contains("router_target=github\nrouter_reason=runner_api_failed\n"));
        assert_eq!(
            stdout.contains("::warning title=Runner inventory token rejected::"),
            warned,
            "{http}: {stdout}"
        );
        assert!(
            !stdout.contains("::error::") && !stdout.contains("SECRET"),
            "{stdout}"
        );
        if warned {
            assert!(stdout.contains(&format!("HTTP {http}")), "{stdout}");
        }
    }
    Ok(())
}
