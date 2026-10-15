//! Bounded source contract and real domain-entry tests; no Windows/CI execution.

const LEGACY: &str = include_str!("../../fixtures/convergence/windows-subset/qualification.yml");
const STAGED: &str = include_str!("../../.github/workflows/windows-subset-qualification.yml");
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
            "      # Native merge-ready preflight admits H/B/T before invoking this inactive template.\n      # Product execution consumes exact T; it has no admission/check-write authority.");
    // Extraction changes the embedded source-test owner, not the consumer oracle.
    job = job.replace("WORKFLOW = Path.cwd() / '.github/workflows/windows-advisory.yml'",
        "WORKFLOW = Path.cwd() / '.github/workflows/windows-subset-qualification.yml'");
    Ok(job)
}

#[test]
fn production_producer_preserves_all_native_commands_oracles_and_cleanup() -> Result<(), String> {
    assert!(STAGED.contains("WORKFLOW = Path.cwd() / '.github/workflows/windows-subset-qualification.yml'"));
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
fn production_callee_has_only_call_admission_and_no_write_authority() {
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
        "subject_sha",
        "workflow_generation",
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

const NATIVE: &str = include_str!("../../.github/workflows/routed-rust.yml");
const RESULT_NAME: &str = "    name: ${{ github.event_name == 'pull_request' && github.event.action == 'labeled' && github.event.label.name == 'merge-ready' && 'Ripr Rust Small Result' || 'Merge-ready Nonrequest' }}";

#[test]
fn native_request_and_nonrequest_names_are_event_bound() {
    assert!(NATIVE.contains("  pull_request:\n    types: [labeled]"));
    assert!(!NATIVE.contains("    types: [ready_for_review]"));
    assert!(NATIVE.contains(RESULT_NAME));
    assert!(!NATIVE.contains("    name: Ripr Rust Small Result\n"));
    assert!(NATIVE.contains("  cancel-in-progress: false"));
    assert!(NATIVE.contains("format('metadata-{0}', github.run_id)"));
    assert!(NATIVE.contains(
        "format('qualification-{0}-{1}', github.repository, github.event.pull_request.number)"
    ));
    assert!(NATIVE.contains(".run_number < $number"));
    assert!(NATIVE.contains(".display_title == $key"));
}

#[test]
fn static_required_name_mutation_is_rejected() {
    let bad = NATIVE.replace(RESULT_NAME, "    name: Ripr Rust Small Result");
    assert_ne!(bad, NATIVE);
    assert!(!bad.contains(RESULT_NAME));
}

#[test]
fn requested_result_evaluates_admission_and_selected_obligations() {
    assert!(NATIVE.contains("    if: always()"));
    for fragment in [
        "[ \"$ADMISSION_RESULT\" = success ]",
        "[ \"$WINDOWS_RESULT\" = success ]",
        "[ \"$PRIOR_RESULT\" = success ]",
        ".head.sha == $head and .base.sha == $base",
        ".merge_commit_sha == $subject",
    ] {
        assert!(
            NATIVE.contains(fragment),
            "missing required-result boundary {fragment}"
        );
    }
}
