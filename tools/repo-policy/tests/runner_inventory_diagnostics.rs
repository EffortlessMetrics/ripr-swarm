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

fn run_route(
    status: u8,
    error: &str,
    runners: &str,
    untrusted: bool,
) -> TestResult<(String, String, bool)> {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-runner-query-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir_all(&scratch.0)?;
    let stub = scratch.0.join("gh");
    fs::write(
        &stub,
        "#!/bin/bash\nprintf '%s\\n' \"$*\" > \"$CALLS\"\nprintf '%s' \"$QUERY_ERROR\" >&2\nprintf '%s' \"$RUNNERS\"\nexit \"$QUERY_STATUS\"\n",
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
        .env("QUERY_STATUS", status.to_string())
        .env("QUERY_ERROR", error)
        .env("RUNNERS", runners)
        .output()?;
    assert!(result.status.success(), "actual shell: {result:?}");
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
    let called = calls.exists();
    if called {
        assert_eq!(
            fs::read_to_string(calls)?.trim(),
            "api --paginate orgs/EffortlessMetrics/actions/runners?per_page=100"
        );
    }
    Ok((
        fs::read_to_string(output)?,
        fs::read_to_string(summary)?,
        called,
    ))
}

#[test]
fn failed_inventory_reports_process_status_without_error_text_or_inferred_http() -> TestResult {
    for (status, error) in [
        (1, "Forbidden"),
        (4, "gh: authentication required"),
        (
            7,
            "HTTP 403: forbidden ghp_SECRET\nAuthorization: bearer SECRET\n::error::injected",
        ),
        (127, "client failure"),
    ] {
        let (output, summary, called) = run_route(status, error, "", false)?;
        assert!(called);
        assert!(output.contains("router_target=github\nrouter_reason=runner_api_failed\n"));
        assert!(output.contains("runner_query=failed\n"));
        assert!(
            output.contains(&format!("runner_query_exit_code={status}\n")),
            "{output}"
        );
        assert!(summary.contains("inventory operation: `gh api --paginate orgs/EffortlessMetrics/actions/runners?per_page=100`"));
        assert!(summary.contains(&format!("runner command exit code: `{status}`")));
        assert!(summary.contains("HTTP status and underlying cause are unknown"));
        for secret in [
            "Forbidden",
            "authentication required",
            "403",
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
fn successful_empty_inventory_records_zero_without_changing_route() -> TestResult {
    let (output, summary, called) =
        run_route(0, "ignored secret warning", "{\"runners\":[]}", false)?;
    assert!(called);
    for expected in [
        "runner_query_exit_code=0\n",
        "runner_query=ok\n",
        "router_target=github\n",
        "router_reason=no_idle_runner\n",
        "runner_total=0\n",
    ] {
        assert!(output.contains(expected), "{output}");
    }
    assert!(!summary.contains("underlying cause are unknown"));
    assert!(!summary.contains("ignored secret warning"));
    Ok(())
}

#[test]
fn successful_inventory_preserves_capability_selection() -> TestResult {
    let runners = r#"{"runners":[{"status":"online","busy":false,"labels":[{"name":"em-ci"},{"name":"cx43"},{"name":"rust-medium"}]}]}"#;
    let (output, _, called) = run_route(0, "", runners, false)?;
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
    let (output, summary, called) = run_route(99, "SECRET", "invalid JSON", true)?;
    assert!(!called);
    for expected in [
        "runner_query_exit_code=not_attempted\n",
        "runner_query=skipped_untrusted_pr\n",
        "router_reason=fork_or_untrusted_pr\n",
        "router_target=github\n",
    ] {
        assert!(output.contains(expected), "{output}");
    }
    assert!(!summary.contains("underlying cause are unknown"));
    Ok(())
}
