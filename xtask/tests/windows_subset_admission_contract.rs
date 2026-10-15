//! Bounded source contract and real domain-entry tests; no Windows/CI execution.

#[path = "../src/convergence/types/mod.rs"]
pub mod types;
mod convergence {
    pub use crate::types;
}
#[path = "../src/convergence/domain/admission/windows_subset.rs"]
mod windows_subset;

const LEGACY: &str = include_str!("../../.github/workflows/windows-advisory.yml");
const STAGED: &str = include_str!("../../fixtures/convergence/windows-subset/qualification.yml");
const PRECHECK: &str =
    "      - name: Verify admitted integration source before product execution\n";

fn normalized_job(source: &str) -> Result<String, String> {
    let (_, job) = source
        .split_once("  windows-advisory-subset:\n")
        .ok_or_else(|| "missing Windows subset job".to_owned())?;
    let mut job = job.to_owned();
    if let Some((before, tail)) = job.split_once(PRECHECK) {
        let (_, after) = tail
            .split_once("      - uses: dtolnay/rust-toolchain@1.95\n")
            .ok_or_else(|| "precheck displaced the native toolchain".to_owned())?;
        job = format!("{before}      - uses: dtolnay/rust-toolchain@1.95\n{after}");
    }
    let trust = "github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository";
    job = job.replace(&format!("${{{{ {trust} }}}}"), "\"true\"")
        .replace(&format!("        if: {trust}\n"), "")
        .replace("${{ github.event.pull_request.head.sha || github.sha }}", "${{ inputs.subject_sha }}")
        .replace("          fetch-depth: 0\n", "")
        .replace("            target/windows-subset-source.json\n", "")
        .replace("          name: ripr-windows-advisory-subset-${{ inputs.request_id }}\n", "          name: ripr-windows-advisory-subset\n")
        .replace("      # Bind native subset/consumer proof to the immutable PR head; the required\n      # Rust lane separately owns its integration checkout.",
            "      # Trusted caller admits H/B/T before invoking this inactive template.\n      # Product execution consumes exact T; it has no admission/check-write authority.");
    Ok(job)
}

#[test]
fn inactive_producer_preserves_all_native_commands_oracles_and_cleanup() -> Result<(), String> {
    assert_eq!(normalized_job(STAGED)?, normalized_job(LEGACY)?);
    Ok(())
}

#[test]
fn subset_command_removal_is_detected() -> Result<(), String> {
    let bad = STAGED.replace(
        "Run Cargo-home identity subset",
        "Removed Cargo-home identity subset",
    );
    assert_ne!(bad, STAGED);
    assert_ne!(normalized_job(&bad)?, normalized_job(LEGACY)?);
    Ok(())
}

#[test]
fn staged_source_has_only_call_admission_and_no_write_authority() {
    let header = STAGED.split("\njobs:\n").next().unwrap_or("");
    assert!(header.contains("on:\n  workflow_call:\n"));
    for forbidden in [
        "  pull_request:",
        "  push:",
        "  workflow_dispatch:",
        "  merge_group:",
        "checks: write",
        "actions: write",
        "pull-requests: write",
    ] {
        assert!(
            !header.contains(forbidden),
            "unexpected admission/authority {forbidden}"
        );
    }
    for field in [
        "request_id",
        "head_sha",
        "base_sha",
        "subject_sha",
        "subject_tree",
        "controller_generation",
    ] {
        assert!(header.contains(&format!(
            "      {field}:\n        type: string\n        required: true"
        )));
    }
    assert!(
        !include_str!("../../.github/workflows/windows-advisory.yml")
            .contains("fixtures/convergence/windows-subset/qualification.yml")
    );
}

#[test]
fn source_precheck_precedes_candidate_execution_and_keeps_source_receipt() -> Result<(), String> {
    let check = STAGED
        .find(PRECHECK)
        .ok_or_else(|| "missing source precheck".to_owned())?;
    let toolchain = STAGED
        .find("      - uses: dtolnay/rust-toolchain@1.95\n")
        .ok_or_else(|| "missing toolchain".to_owned())?;
    assert!(check < toolchain);
    for clause in [
        "$actual -cne $env:ADMITTED_SUBJECT",
        "$tree -cne $env:ADMITTED_TREE",
        "$parents -cne",
        "$env:GITHUB_WORKFLOW_SHA -cne $env:ADMITTED_GENERATION",
        "producer_attempt = $env:GITHUB_RUN_ATTEMPT",
        "target/windows-subset-source.json",
    ] {
        assert!(STAGED.contains(clause), "missing source binding {clause}");
    }
    Ok(())
}
