//! Contract for the explicitly requested main-health incident probe.
//! The retired timer must not return; bounded execution and truthful evidence
//! still govern a selected incident request under #4984/#4842.

use std::fs;
use std::path::Path;

const WORKFLOW: &str = ".github/workflows/main-health-sentinel.yml";
const ALLOWLIST: &str = "policy/workflow_allowlist.txt";
const SMOKE_SELECTOR: &str = "version_is_exact_and_precedes_help_or_output_flags";
const TRACKER_MARKER: &str = "main-health-sentinel-tracker";

/// Executable (non-comment) fragments that would let the sentinel gate,
/// authorize, or mutate anything beyond its own advisory run.
const FORBIDDEN_FRAGMENTS: &[&str] = &[
    "actions: write",
    "gh workflow run",
    "required_status_checks",
    "required_pull_request_reviews",
    "/protection",
    "/rules/",
    "create-pull-request",
    "/rerun",
];

fn repo_file(path: &str) -> Result<String, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

/// Executable lines only: comment prose may explain the contract without
/// satisfying it.
fn executable_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect()
}

fn require(lines: &[&str], needle: &str, why: &str) -> Result<(), String> {
    if lines.iter().any(|line| line.contains(needle)) {
        return Ok(());
    }
    Err(format!("main-health sentinel is missing `{needle}`: {why}"))
}

fn forbid(lines: &[&str], needle: &str, why: &str) -> Result<(), String> {
    if !lines.iter().any(|line| line.contains(needle)) {
        return Ok(());
    }
    Err(format!(
        "main-health sentinel must not contain `{needle}`: {why}"
    ))
}

#[test]
fn incident_probe_requires_an_explicit_question_and_has_no_timer() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let declarations = repo_policy::workflow_lifecycle::event_declarations(&source)?;
    assert_eq!(
        declarations.keys().map(String::as_str).collect::<Vec<_>>(),
        ["workflow_dispatch"]
    );
    let lines = executable_lines(&source);
    require(
        &lines,
        "INCIDENT: ${{ inputs.incident }}",
        "an existing incident supplies the scope",
    )?;
    require(
        &lines,
        "QUESTION: ${{ inputs.question }}",
        "the selected proof answers a named question",
    )?;
    require(
        &lines,
        "[[ \"$INCIDENT\" =~ [^[:space:]] && \"$QUESTION\" =~ [^[:space:]] ]]",
        "blank requests cannot allocate proof",
    )?;
    let old = source.replacen("on:\n", "on:\n  schedule:\n    - cron: '9,39 * * * *'\n", 1);
    assert_ne!(old, source);
    let violations = repo_policy::workflow_lifecycle::violations(WORKFLOW, &old);
    assert!(
        violations.iter().any(|v| v.contains("`schedule`")),
        "expected a schedule violation, got {violations:?}"
    );
    Ok(())
}

#[test]
fn sentinel_serializes_overlapping_fires() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "group: main-health-sentinel-${{ github.repository }}",
        "one probe at a time, so tracker reconciliation cannot race itself",
    )?;
    require(
        &lines,
        "cancel-in-progress: false",
        "an in-progress probe must finish writing its summary and tracker state",
    )
}

#[test]
fn sentinel_is_bounded_and_repository_scoped() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "if: github.repository == 'EffortlessMetrics/ripr-swarm' && github.ref == 'refs/heads/main'",
        "a fork running the workflow must not open issues on the upstream tracker, \
         and a dispatch from a non-main ref must not judge that ref as main health",
    )?;
    require(
        &lines,
        "timeout-minutes: 30",
        "a hung probe is a red signal with an end, not a stuck run",
    )
}

#[test]
fn sentinel_stays_advisory_detect_only() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    for fragment in FORBIDDEN_FRAGMENTS {
        forbid(
            &lines,
            fragment,
            "the sentinel detects; it never gates or dispatches",
        )?;
    }
    // The only write authority is the tracking issue. Workflow-level
    // permissions stay read-only so no secret-holding job inherits a broad
    // token.
    forbid(
        &lines,
        "contents: write",
        "the sentinel never pushes commits or opens PRs",
    )?;
    require(
        &lines,
        "issues: write",
        "the main-red tracker is the durable record",
    )?;
    require(
        &lines,
        "GITHUB_STEP_SUMMARY",
        "the per-probe summary table is the visible audit trail",
    )
}

#[test]
fn sentinel_probe_is_bounded_and_executes_something() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "cargo check --locked -p ripr",
        "the compile-health half of the probe",
    )?;
    // The full test command is pinned, not just the selector name: the name
    // also appears in the summary row, so a name-only check would pass even
    // if the cargo test invocation were removed.
    require(
        &lines,
        "cargo test --locked -p ripr --test cli_smoke",
        "the smoke half actually invokes cargo test on the built binary",
    )?;
    require(
        &lines,
        &format!("{SMOKE_SELECTOR} -- --exact"),
        "the smoke half runs exactly one focused test, nothing broader",
    )?;
    // Evidence discipline: native exit statuses are captured next to the logs
    // so a failure inside a pipe cannot vanish, a hung step classifies as the
    // timeout status instead of skipping, and a zero-subject run fails.
    require(
        &lines,
        "PIPESTATUS",
        "a failure inside a tee pipe must survive",
    )?;
    require(
        &lines,
        "2>/dev/null || echo 124",
        "a probe terminated at its step timeout has no status file and must read as red (124), not as success",
    )?;
    require(
        &lines,
        "^test result:",
        "the judge reads the native test-result line, not cargo's success",
    )?;
    require(
        &lines,
        "executed zero tests",
        "a probe that ran nothing proves nothing and must fail",
    )
}

#[test]
fn sentinel_tracker_preserves_full_qualification_obligations() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "main-red: scheduled main-health sentinel found a broken main",
        "the tracker title is the findable red record",
    )?;
    require(
        &lines,
        TRACKER_MARKER,
        "the marker marks sentinel provenance in the tracker body and comments",
    )?;
    require(
        &lines,
        "select(.title == \\\"$title\\\")",
        "tracker creation deduplicates by exact title among open issues, not by the body marker",
    )?;
    forbid(
        &lines,
        "state=closed",
        "a bounded probe cannot close full-gate obligations",
    )?;
    forbid(
        &lines,
        "Recovered:",
        "bounded success is not incident recovery",
    )?;
    require(
        &lines,
        "steps.judge.outputs.verdict != ''",
        "only a judge that actually classified main may touch the tracker: a failure \
         before the judge (checkout, toolchain) is a visible red run, not a main verdict",
    )
}

#[test]
fn sentinel_run_block_budget_is_registered() -> Result<(), String> {
    let allowlist = repo_file(ALLOWLIST)?;
    if allowlist
        .lines()
        .any(|line| line.starts_with(".github/workflows/main-health-sentinel.yml|"))
    {
        return Ok(());
    }
    Err(
        "check-workflows requires a run-block budget row for every workflow; \
         the sentinel's row in policy/workflow_allowlist.txt is part of its contract"
            .to_string(),
    )
}

/// Test-level breaks pass the bounded probe (#13793, #13639, #13783), so the
/// sentinel retains completed main-push observations. Only exact selected-commit
/// failures may make the current verdict red; historical results remain visible.
/// Read-only observation never re-runs, dispatches or rebuilds a gate.
#[test]
fn sentinel_judges_the_latest_completed_main_push_gates() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "actions: read",
        "reading workflow-run results needs only the read scope",
    )?;
    require(
        &lines,
        "for wf in routed-rust.yml ci.yml; do",
        "the required Rust gate and the CI lane (Rust-only features, editor e2e) are both judged",
    )?;
    require(
        &lines,
        "-f branch=main -f event=push -f status=completed",
        "only finished main-push runs are evidence about main",
    )?;
    require(
        &lines,
        "select(.conclusion != \"cancelled\" and .conclusion != \"skipped\")",
        "a superseded run says nothing about main; skip it and read the latest finished one",
    )?;
    require(
        &lines,
        "echo 1 > \"$d/gates.red\"",
        "an exact-selected-commit failed gate raises the red flag",
    )?;
    require(
        &lines,
        "gates_red=\"$(cat \"$d/gates.red\" 2>/dev/null || echo 0)\"",
        "the judge must read the gate red flag into its verdict",
    )?;
    require(
        &lines,
        "grep -qxF \"$fingerprint\"",
        "a red tracker gets one comment per new piece of evidence, not one per fire",
    )?;
    let read_step = source
        .split("- name: Read the latest completed main push gates")
        .nth(1)
        .and_then(|rest| rest.split("- name:").next())
        .ok_or("the gate read step must exist")?;
    if !read_step.contains("continue-on-error: true") {
        return Err("a hung or failed gate read must not skip the probe judge and tracker".into());
    }
    let probe_step = source
        .split("- name: Probe main build and one focused binary smoke test")
        .nth(1)
        .and_then(|rest| rest.split("- name:").next())
        .ok_or("the probe step must exist")?;
    if !probe_step.contains("continue-on-error: true") {
        return Err("a probe terminated at its timeout must still reach the judge".into());
    }
    require(
        &lines,
        "0:success|0:failure|0:timed_out|0:action_required|0:startup_failure) ;;",
        "a failed read or an unexpected conclusion must become unknown, not red",
    )?;
    require(
        &lines,
        "[ \"$(grep -c . \"$d/gates.tsv\" || true)\" -ge 2 ] || echo 1 > \"$d/gates.unknown\"",
        "a missing gate row must mark the gates unknown",
    )?;
    require(
        &lines,
        "unknown) echo 1 > \"$d/gates.unknown\" ;;",
        "an unknown gate row must mark the gates unknown",
    )?;
    require(
        &lines,
        "run=\"$(timeout 60 gh api -X GET",
        "each gate read is bounded so a hung API call becomes an unknown row",
    )?;
    require(
        &lines,
        "[ \"$sha\" != \"$GITHUB_SHA\" ] || scope=selected-commit",
        "full commit identity, not a short prefix, binds current gate evidence",
    )?;
    require(
        &lines,
        "if [ \"$scope\" = selected-commit ]; then echo 1 > \"$d/gates.red\"; fi",
        "historical failure is visible but cannot poison the current verdict",
    )
}

/// Execute the shipped shell decisions without GitHub, Cargo, or a live issue.
/// These tests establish local decision behavior, not hosted qualification.
#[cfg(unix)]
mod replay {
    use super::repo_file;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Same abbreviated prefix: comparing seven characters would misbind history.
    const CURRENT: &str = "aaaaaaa111111111111111111111111111111111";
    const OLD: &str = "aaaaaaa222222222222222222222222222222222";
    const FAKE_GH: &str = r#"#!/bin/bash
set -euo pipefail
printf '%s\n' "$*" >> "$FAKE_DIR/calls"
body=""
for arg in "$@"; do case "$arg" in body=*) body="${arg#body=}" ;; esac; done
case "$*" in
  *state=closed*) echo closed >> "$FAKE_DIR/writes"; exit 0 ;;
  *"/actions/workflows/"*)
    case "$FAKE_MODE" in
      unreadable) exit 1 ;;
      missing) exit 0 ;;
      malformed) printf '41\tfailure\taaaaaaa\thttps://github.com/EffortlessMetrics/ripr-swarm/actions/runs/41\n' ;;
      bad-id) printf 'oops\tfailure\t%s\thttps://github.com/EffortlessMetrics/ripr-swarm/actions/runs/41\n' "$GITHUB_SHA" ;;
      bad-url) printf '41\tfailure\t%s\thttps://example.invalid/run/41\n' "$GITHUB_SHA" ;;
      bad-conclusion) printf '41\tmystery\t%s\thttps://github.com/EffortlessMetrics/ripr-swarm/actions/runs/41\n' "$GITHUB_SHA" ;;
      *) printf '41\t%s\t%s\thttps://github.com/EffortlessMetrics/ripr-swarm/actions/runs/41\n' "$FAKE_CONCLUSION" "$FAKE_SHA" ;;
    esac; exit 0 ;;
  *"/actions/runs/41/jobs"*) echo 'Required test'; exit 0 ;;
  *search/issues*) [ "$FAKE_TRACKER" != present ] || echo 17; exit 0 ;;
  *"/issues"*)
    if [ -n "$body" ]; then
      printf 'body: %s\n' "$body" >> "$FAKE_DIR/writes"
      printf '%s\n' "$body" >> "$FAKE_DIR/stored"
    elif [ -f "$FAKE_DIR/stored" ]; then cat "$FAKE_DIR/stored"; fi
    exit 0 ;;
  *) echo "unexpected fake API call: $*" >&2; exit 1 ;;
esac
"#;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn block(source: &str, name: &str) -> Result<String, String> {
        let (_, step) = source
            .split_once(&format!("      - name: {name}\n"))
            .ok_or_else(|| format!("missing step {name}"))?;
        let (_, script) = step
            .split_once("        run: |\n")
            .ok_or_else(|| format!("missing literal script {name}"))?;
        let script = script
            .lines()
            .take_while(|line| line.is_empty() || line.starts_with("          "))
            .map(|line| line.strip_prefix("          ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n");
        if script.trim().is_empty() {
            return Err(format!("empty script {name}"));
        }
        Ok(script
            .replace("${{ github.server_url }}", "https://github.com")
            .replace("${{ github.run_id }}", "99"))
    }

    struct Outcome {
        verdict: String,
        summary: String,
        writes: String,
        gates: String,
        unknown: String,
        gate_evidence: String,
    }

    fn run(
        source: &str,
        mode: &str,
        sha: &str,
        conclusion: &str,
        probe: &str,
        tracker: &str,
    ) -> Result<Outcome, String> {
        run_subjects(source, mode, sha, conclusion, probe, tracker, &[CURRENT])
    }

    fn run_subjects(
        source: &str,
        mode: &str,
        sha: &str,
        conclusion: &str,
        probe: &str,
        tracker: &str,
        selected_subjects: &[&str],
    ) -> Result<Outcome, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let dir = Scratch(root.join(format!(
            "main-health-replay-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&dir.0).map_err(|e| e.to_string())?;
        fs::create_dir(dir.0.join("bin")).map_err(|e| e.to_string())?;
        let evidence = dir.0.join("main-health-sentinel");
        fs::create_dir(&evidence).map_err(|e| e.to_string())?;
        for (name, value) in [
            ("host.log", "fixture toolchain"),
            ("check.log", "fixture compile"),
            ("check.status", "0"),
            ("smoke.status", "0"),
            (
                "smoke.log",
                "test result: ok. 1 passed; 0 failed; 0 ignored",
            ),
        ] {
            fs::write(evidence.join(name), value).map_err(|e| e.to_string())?;
        }
        match probe {
            "compile-failed" => fs::write(evidence.join("check.status"), "101"),
            "smoke-failed" => fs::write(evidence.join("smoke.status"), "101"),
            "zero-tests" => fs::write(
                evidence.join("smoke.log"),
                "test result: ok. 0 passed; 0 failed",
            ),
            "missing-status" => fs::remove_file(evidence.join("check.status")),
            "missing-log" => fs::remove_file(evidence.join("host.log")),
            "passed" => Ok(()),
            _ => return Err("unknown fixture probe".into()),
        }
        .map_err(|e| e.to_string())?;
        fs::write(dir.0.join("bin/gh"), FAKE_GH).map_err(|e| e.to_string())?;
        fs::set_permissions(dir.0.join("bin/gh"), fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let mut verdict = String::new();
        // Replay tracker twice to prove repeated evidence is deduplicated.
        for selected_sha in selected_subjects {
            verdict.clear();
            fs::write(dir.0.join("output"), "").map_err(|e| e.to_string())?;
            for name in [
                "Read the latest completed main push gates",
                "Summarize the main push gates",
                "Judge probe evidence",
                "Reconcile the main-red tracker",
                "Reconcile the main-red tracker",
            ] {
                if mode == "missing-file" && name == "Read the latest completed main push gates" {
                    continue;
                }
                let script = block(source, name)?;
                let output = Command::new("timeout")
                    .args(["10", "bash", "-c", &script])
                    .env(
                        "PATH",
                        format!(
                            "{}:{}",
                            dir.0.join("bin").display(),
                            std::env::var("PATH").unwrap_or_default()
                        ),
                    )
                    .env("RUNNER_TEMP", &dir.0)
                    .env("FAKE_DIR", &dir.0)
                    .env("FAKE_MODE", mode)
                    .env("FAKE_SHA", sha)
                    .env("FAKE_CONCLUSION", conclusion)
                    .env("FAKE_TRACKER", tracker)
                    .env("GITHUB_SHA", selected_sha)
                    .env("GITHUB_SERVER_URL", "https://github.com")
                    .env("REPOSITORY", "EffortlessMetrics/ripr-swarm")
                    .env("GITHUB_STEP_SUMMARY", dir.0.join("summary"))
                    .env("GITHUB_OUTPUT", dir.0.join("output"))
                    .env("VERDICT", &verdict)
                    .output()
                    .map_err(|e| e.to_string())?;
                if name == "Judge probe evidence" {
                    verdict = fs::read_to_string(dir.0.join("output"))
                        .map_err(|e| e.to_string())?
                        .trim()
                        .strip_prefix("verdict=")
                        .ok_or("missing verdict output")?
                        .to_string();
                    if output.status.code() != Some(if verdict == "green" { 0 } else { 1 }) {
                        return Err(format!(
                            "judge status disagrees with {verdict}: {:?}",
                            output.status
                        ));
                    }
                } else if !output.status.success() {
                    return Err(format!(
                        "{name}: {:?}: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
            }
        }
        let read = |path: PathBuf| fs::read_to_string(path).unwrap_or_default();
        Ok(Outcome {
            verdict,
            summary: read(dir.0.join("summary")),
            writes: read(dir.0.join("writes")),
            gates: read(evidence.join("gates.tsv")),
            unknown: read(evidence.join("gates.unknown")),
            gate_evidence: read(evidence.join("gates.md")),
        })
    }

    fn bounded_pass(out: &Outcome) -> Result<(), String> {
        if out.verdict != "green"
            || !out.writes.is_empty()
            || !out
                .summary
                .contains("not full qualification or incident recovery")
        {
            return Err(format!(
                "bounded pass must preserve tracker and scope: {}, {}",
                out.verdict, out.writes
            ));
        }
        Ok(())
    }

    #[test]
    fn historical_failure_is_visible_without_poisoning_selected_commit() -> Result<(), String> {
        let out = run(
            &repo_file(super::WORKFLOW)?,
            "normal",
            OLD,
            "failure",
            "passed",
            "present",
        )?;
        bounded_pass(&out)?;
        assert!(out.summary.contains("| historical | failure |"));
        assert!(out.summary.contains(OLD));
        assert!(out.summary.contains("/actions/runs/41"));
        assert!(out.gates.contains(OLD));
        assert!(out.gate_evidence.contains("historical completed main push"));
        assert_eq!(out.unknown.trim(), "1");
        Ok(())
    }

    #[test]
    fn exact_selected_commit_failure_is_red_and_deduplicated() -> Result<(), String> {
        let source = repo_file(super::WORKFLOW)?;
        let out = run(&source, "normal", CURRENT, "failure", "passed", "present")?;
        assert_eq!(out.verdict, "red");
        assert!(out.summary.contains("| selected-commit | failure |"));
        assert!(out.writes.contains(CURRENT));
        assert!(out.writes.contains("selected-commit completed main push"));
        assert_eq!(out.writes.matches("body: ").count(), 1);
        assert!(!out.writes.contains("Recovered:"));
        assert!(!out.writes.lines().any(|line| line == "closed"));
        let created = run(&source, "normal", CURRENT, "failure", "passed", "absent")?;
        assert!(created.writes.contains(super::TRACKER_MARKER));
        Ok(())
    }

    #[test]
    fn failed_empty_or_missing_probe_stays_red_despite_historical_success() -> Result<(), String> {
        let source = repo_file(super::WORKFLOW)?;
        for probe in [
            "compile-failed",
            "smoke-failed",
            "zero-tests",
            "missing-status",
            "missing-log",
        ] {
            let out = run(&source, "normal", OLD, "success", probe, "present")?;
            assert_eq!(out.verdict, "red", "{probe}");
            assert!(out.writes.contains(CURRENT), "{probe}");
            assert_eq!(out.writes.matches("body: ").count(), 1, "{probe}");
        }
        Ok(())
    }

    #[test]
    fn missing_log_fingerprints_bind_each_selected_commit() -> Result<(), String> {
        let source = repo_file(super::WORKFLOW)?;
        let subjects = [CURRENT, OLD];
        let out = run_subjects(
            &source,
            "normal",
            OLD,
            "success",
            "missing-log",
            "present",
            &subjects,
        )?;
        assert_eq!(out.verdict, "red");
        assert_eq!(
            out.writes.matches("body: ").count(),
            2,
            "each selected commit needs durable evidence"
        );
        for subject in subjects {
            assert!(
                out.writes
                    .contains(&format!("fingerprint: probe={subject}:green"))
            );
        }
        let mutant = source.replace("probe_fp=\"$GITHUB_SHA:green\"", "probe_fp=green");
        assert_ne!(
            mutant, source,
            "mutation must remove full identity from the real branch"
        );
        let wrong = run_subjects(
            &mutant,
            "normal",
            OLD,
            "success",
            "missing-log",
            "present",
            &subjects,
        )?;
        assert_eq!(
            wrong.writes.matches("body: ").count(),
            1,
            "the old fingerprint incorrectly suppresses the second commit"
        );
        Ok(())
    }

    #[test]
    fn unreadable_malformed_and_missing_gates_are_unknown_without_recovery() -> Result<(), String> {
        let source = repo_file(super::WORKFLOW)?;
        for mode in [
            "unreadable",
            "malformed",
            "bad-id",
            "bad-url",
            "bad-conclusion",
            "missing",
            "missing-file",
        ] {
            let out = run(&source, mode, CURRENT, "success", "passed", "present")?;
            bounded_pass(&out)?;
            assert_eq!(out.unknown.trim(), "1", "{mode}");
            assert!(!out.summary.contains("| selected-commit |"), "{mode}");
        }
        Ok(())
    }

    #[test]
    fn even_current_gate_success_never_closes_full_gate_tracker() -> Result<(), String> {
        for sha in [CURRENT, OLD] {
            bounded_pass(&run(
                &repo_file(super::WORKFLOW)?,
                "normal",
                sha,
                "success",
                "passed",
                "present",
            )?)?;
        }
        Ok(())
    }

    #[test]
    fn history_as_current_mutant_is_rejected() -> Result<(), String> {
        let source = repo_file(super::WORKFLOW)?;
        let mutant = source.replace(
            "if [ \"$scope\" = selected-commit ]; then echo 1 > \"$d/gates.red\"; fi",
            "echo 1 > \"$d/gates.red\"",
        );
        assert_ne!(mutant, source, "mutation must touch the real decision");
        assert!(
            bounded_pass(&run(
                &mutant, "normal", OLD, "failure", "passed", "present"
            )?)
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn automatic_closure_mutant_is_rejected() -> Result<(), String> {
        let source = repo_file(super::WORKFLOW)?;
        let mutant = format!(
            "{source}\n          if [ \"$VERDICT\" = green ]; then\n            gh api -X PATCH \"repos/$REPOSITORY/issues/$number\" -f state=closed\n            gh api \"repos/$REPOSITORY/issues/$number/comments\" -f body=\"Recovered: bounded probe passed\"\n          fi\n"
        );
        assert!(
            bounded_pass(&run(
                &mutant, "normal", CURRENT, "success", "passed", "present"
            )?)
            .is_err()
        );
        Ok(())
    }
}
