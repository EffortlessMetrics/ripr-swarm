//! #5394: execute the checked-in Result shell, including a real manual
//! admission producer, without dispatching a workflow or creating a check.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let path = parent.join(format!(
            "ripr-dispatch-result-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn workflow() -> Result<String, String> {
    fs::read_to_string(root().join(".github/workflows/routed-rust.yml")).map_err(|e| e.to_string())
}

fn result_step(source: &str) -> Result<&str, String> {
    let (_, job) = source
        .split_once("\n  result:\n")
        .ok_or("missing actual result job")?;
    let (_, step) = job
        .split_once("      - name: Summarize routed result\n")
        .ok_or("missing actual Result step")?;
    Ok(step)
}

fn result_shell(step: &str) -> Result<String, String> {
    let (_, body) = step
        .split_once("        run: |\n")
        .ok_or("missing Result shell")?;
    body.lines()
        .map(|line| {
            if line.is_empty() {
                Ok("")
            } else {
                line.strip_prefix("          ")
                    .ok_or("unexpected Result shell nesting")
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|lines| lines.join("\n"))
        .map_err(str::to_string)
}

fn run_result(
    source: &str,
    event: &str,
    reference: &str,
    admission: &str,
    status: &str,
    overrides: &[(&str, &str)],
) -> Result<(bool, String), String> {
    let scratch = Scratch::new()?;
    let step = result_step(source)?;
    let mut env = BTreeMap::new();
    // Derive the consumed inputs from the actual step, then give its child
    // outcomes a successful fixture. Tests vary authoritative inputs below.
    for line in step
        .split("        run: |\n")
        .next()
        .ok_or("missing env")?
        .lines()
    {
        if let Some((name, _)) = line
            .strip_prefix("          ")
            .and_then(|l| l.split_once(':'))
        {
            env.insert(name.to_string(), "success".to_string());
        }
    }
    for (name, value) in [
        ("EVENT_NAME", event),
        ("WORKFLOW_REF", reference),
        ("ADMISSION_RESULT", admission),
        ("ADMISSION_STATUS", status),
        ("ADMISSION_REASON", "fixture"),
        ("TARGET", "github"),
        ("DOCS_ONLY", "false"),
        ("LABEL_NAME", "merge-ready"),
    ]
    .into_iter()
    .chain(overrides.iter().copied())
    {
        env.insert(name.to_string(), value.to_string());
    }
    let summary = scratch.0.join("summary");
    let output = Command::new("bash")
        .args(["-e", "-c", &result_shell(step)?])
        .env_clear()
        .envs(env)
        .env("PATH", std::env::var_os("PATH").ok_or("PATH unavailable")?)
        .env("GITHUB_STEP_SUMMARY", &summary)
        .output()
        .map_err(|e| e.to_string())?;
    let report =
        fs::read_to_string(summary).map_err(|e| format!("missing summary: {e}; {output:?}"))?;
    assert!(report.contains("### Ripr Rust Small Result"));
    Ok((output.status.success(), report))
}

#[test]
fn branch_dispatch_cannot_qualify_even_when_every_child_succeeds() -> Result<(), String> {
    for reference in ["refs/heads/fix/5394", "refs/tags/v0.11.0", "", "main"] {
        let (success, report) = run_result(
            &workflow()?,
            "workflow_dispatch",
            reference,
            "success",
            "not_applicable",
            &[],
        )?;
        assert!(
            !success,
            "diagnostic dispatch must not qualify: {reference}: {report}"
        );
        assert!(
            report.contains("diagnostic-only"),
            "must explain the event refusal: {report}"
        );
    }
    Ok(())
}

#[test]
fn ready_and_main_authorities_keep_real_child_result_checks() -> Result<(), String> {
    for (event, reference, status) in [
        ("pull_request", "refs/pull/10/merge", "admitted"),
        (
            "merge_group",
            "refs/heads/gh-readonly-queue/main/pr-10-aaaa",
            "admitted",
        ),
        ("workflow_dispatch", "refs/heads/main", "not_applicable"),
        ("workflow_dispatch", "refs/heads/master", "not_applicable"),
        ("push", "refs/heads/main", "not_applicable"),
    ] {
        assert!(run_result(&workflow()?, event, reference, "success", status, &[])?.0);
        assert!(
            !run_result(
                &workflow()?,
                event,
                reference,
                "success",
                status,
                &[("GITHUB_RESULT", "failure")]
            )?
            .0
        );
    }
    for (admission, status) in [
        ("failure", "admitted"),
        ("success", "refused"),
        ("success", ""),
    ] {
        assert!(
            !run_result(
                &workflow()?,
                "pull_request",
                "refs/pull/10/merge",
                admission,
                status,
                &[]
            )?
            .0
        );
    }
    assert!(
        run_result(
            &workflow()?,
            "pull_request",
            "refs/pull/10/merge",
            "success",
            "admitted",
            &[("DOCS_ONLY", "true")]
        )?
        .0
    );
    assert!(
        !run_result(
            &workflow()?,
            "pull_request",
            "refs/pull/10/merge",
            "success",
            "admitted",
            &[("DOCS_ONLY", "true"), ("DOCS_GATE_RESULT", "skipped")]
        )?
        .0
    );
    Ok(())
}

#[test]
fn refused_admission_summary_names_the_concrete_detail() -> Result<(), String> {
    let detail = "missing or placeholder PR body line(s): Governing issue or accepted claim:, Acceptance delta (one sentence):";
    let (success, report) = run_result(
        &workflow()?,
        "pull_request",
        "refs/pull/10/merge",
        "failure",
        "refused",
        &[
            ("ADMISSION_REASON", "claim_identity_missing"),
            ("ADMISSION_DETAIL", detail),
        ],
    )?;
    assert!(!success, "refused admission must fail the result: {report}");
    assert!(
        report.contains("claim_identity_missing") && report.contains(detail),
        "result summary must name the single concrete refusal reason, got: {report}"
    );
    Ok(())
}

#[test]
fn manual_admission_producer_does_not_supply_pr_qualification() -> Result<(), String> {
    let scratch = Scratch::new()?;
    let action = root().join(".github/actions/pr-candidate-admission");
    let source = fs::read_to_string(action.join("action.yml")).map_err(|e| e.to_string())?;
    let (_, body) = source
        .split_once("      run: |\n")
        .ok_or("missing action shell")?;
    let shell = body
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let outputs = scratch.0.join("outputs");
    let output = Command::new("bash")
        .args(["-e", "-c", &shell])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").ok_or("PATH unavailable")?)
        .env("GITHUB_ACTION_PATH", action)
        .env("GITHUB_WORKSPACE", root())
        .env("GITHUB_OUTPUT", &outputs)
        .env("GITHUB_STEP_SUMMARY", scratch.0.join("summary"))
        .env("ADMISSION_MODE", "admit")
        .env("ADMISSION_PR_NUMBER", "")
        .output()
        .map_err(|e| e.to_string())?;
    assert!(output.status.success(), "{output:?}");
    let outputs = fs::read_to_string(outputs).map_err(|e| e.to_string())?;
    let status = outputs
        .lines()
        .find_map(|line| line.strip_prefix("status="))
        .ok_or("no producer status")?;
    assert_eq!(status, "not_applicable");
    assert!(outputs.contains("cut_reason=cut_non_pull_request_invocation"));
    assert!(
        !run_result(
            &workflow()?,
            "workflow_dispatch",
            "refs/heads/fix/5394",
            "success",
            status,
            &[]
        )?
        .0
    );
    assert!(
        run_result(
            &workflow()?,
            "workflow_dispatch",
            "refs/heads/main",
            "success",
            status,
            &[]
        )?
        .0
    );
    Ok(())
}

#[test]
fn event_identity_is_wired_from_github_not_candidate_inputs() -> Result<(), String> {
    let source = workflow()?;
    let step = result_step(&source)?;
    assert!(step.contains("          EVENT_NAME: ${{ github.event_name }}\n"));
    assert!(step.contains("          WORKFLOW_REF: ${{ github.ref }}\n"));
    assert!(!source.contains("    continue-on-error: true"));
    Ok(())
}
