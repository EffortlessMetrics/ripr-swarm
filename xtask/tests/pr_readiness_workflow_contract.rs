//! Pin the native draft/ready lifecycle before any automatic PR runner starts.
//! Actual expression evaluation remains part of workflow-change review.

use std::fs;
use std::path::Path;

const DRAFT_GUARD: &str =
    "(github.event_name != 'pull_request' || github.event.pull_request.draft == false)";

fn readiness_findings(source: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let Some((events, jobs)) = source.split_once("\njobs:\n") else {
        return vec!["missing jobs".to_owned()];
    };
    if !events.contains("ready_for_review") {
        findings.push("missing ready transition".to_owned());
    }
    let mut job = None;
    let mut guarded = false;
    for line in jobs.lines().chain(std::iter::once("  end:")) {
        if line.starts_with("  ")
            && !line.starts_with("   ")
            && line.ends_with(':')
            && !line.trim_start().starts_with('#')
        {
            if let Some(name) = job
                && !guarded
            {
                findings.push(format!("{name}: missing server-side draft guard"));
            }
            job = Some(line.trim().trim_end_matches(':'));
            guarded = false;
        } else if let Some(expression) = line.strip_prefix("    if: ${{ ") {
            guarded = expression.starts_with(DRAFT_GUARD);
        }
    }
    findings
}

#[test]
fn every_automatic_pr_job_is_guarded_and_has_a_ready_transition()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows");
    let mut workflows = 0;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        // Explicit @droid invocations are requests, not automatic PR CI.
        if path.file_name().is_some_and(|name| name == "droid.yml") {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        if !source.contains("\n  pull_request:") {
            continue;
        }
        workflows += 1;
        let findings = readiness_findings(&source);
        assert!(findings.is_empty(), "{}: {findings:?}", path.display());
    }
    assert!(
        workflows >= 14,
        "automatic PR workflow inventory disappeared"
    );
    Ok(())
}

#[test]
fn readiness_contract_rejects_missing_gate_even_on_finalizer() {
    let source = include_str!("../../.github/workflows/routed-rust.yml");
    let changed = source.replace(
        &format!("    if: ${{{{ {DRAFT_GUARD} && (always()) }}}}"),
        "    if: always()",
    );
    assert_ne!(changed, source, "result guard mutation must engage");
    assert!(
        readiness_findings(&changed)
            .iter()
            .any(|finding| { finding == "result: missing server-side draft guard" })
    );
}

#[test]
fn readiness_contract_rejects_missing_ready_transition() {
    let source = include_str!("../../.github/workflows/security.yml");
    let changed = source.replace(", ready_for_review", "");
    assert_ne!(changed, source, "ready event mutation must engage");
    assert!(readiness_findings(&changed).contains(&"missing ready transition".to_owned()));
}
