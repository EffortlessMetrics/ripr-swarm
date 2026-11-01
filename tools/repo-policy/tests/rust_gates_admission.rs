//! Static contract for the reusable Rust gate's optional admitted-subject inputs.
//! Source-shape only: no shell replay, hosted run or caller is exercised here.

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const WORKFLOW: &str = include_str!("../../../.github/workflows/rust-gates.yml");

fn input_block<'a>(name: &str) -> TestResult<&'a str> {
    let marker = format!("\n      {name}:\n");
    let (_, tail) = WORKFLOW.split_once(&marker).ok_or("input exists")?;
    let end = tail.find("\n      ").map_or(tail.len(), |_| {
        tail.match_indices("\n      ")
            .find(|(i, _)| !tail[*i + 7..].starts_with(' '))
            .map_or(tail.len(), |(i, _)| i)
    });
    Ok(&tail[..end])
}

#[test]
fn existing_callers_keep_working_without_admitted_inputs() -> TestResult {
    // Main's caller passes only runner-config; every new input must be optional.
    let profile = input_block("proof-profile")?;
    assert!(profile.contains("required: false"), "{profile}");
    assert!(profile.contains("default: legacy"), "{profile}");
    for name in [
        "head-sha",
        "base-sha",
        "subject-sha",
        "subject-tree",
        "workflow-generation",
        "subject-json",
        "subject-identity",
    ] {
        let block = input_block(name)?;
        assert!(block.contains("required: false"), "{name}: {block}");
        assert!(block.contains("default: \"\""), "{name}: {block}");
    }
    Ok(())
}

#[test]
fn admitted_subject_is_verified_before_any_proof_step() -> TestResult {
    let checkout = WORKFLOW
        .find("ref: ${{ inputs.subject-sha || github.sha }}")
        .ok_or("checkout pins the admitted subject")?;
    let verify = WORKFLOW
        .find("- name: Verify admitted integration before proof")
        .ok_or("verification step exists")?;
    let first_gate = WORKFLOW
        .find("kernel flock lease")
        .ok_or("scratch lease comment exists")?;
    assert!(checkout < verify && verify < first_gate);
    let step = &WORKFLOW[verify..first_gate];
    assert!(step.contains("id: admitted"));
    assert!(step.contains("set -euo pipefail"));
    for (var, input) in [
        ("PROFILE", "proof-profile"),
        ("SUBJECT_JSON", "subject-json"),
        ("ADMITTED_HEAD", "head-sha"),
        ("ADMITTED_BASE", "base-sha"),
        ("ADMITTED_MERGE", "subject-sha"),
        ("ADMITTED_TREE", "subject-tree"),
        ("GENERATION", "workflow-generation"),
        ("IDENTITY", "subject-identity"),
    ] {
        let line = format!("          {var}: ${{{{ inputs.{input} }}}}\n");
        assert!(step.contains(&line), "{var} must map to inputs.{input}");
    }
    // Partial qualification cannot ride the legacy profile.
    assert!(step.contains("Partial qualification cannot use legacy profile"));
    // Qualification binds subject JSON, digest, generation, commit, tree and parents.
    for needle in [
        "GITHUB_WORKFLOW_SHA",
        "sha256sum",
        "git rev-parse 'HEAD^{tree}'",
        "git show -s --format=%P HEAD",
        ".parents == [$base,$head]",
        "Unknown source profile",
    ] {
        assert!(step.contains(needle), "missing {needle}");
    }
    Ok(())
}

#[test]
fn pr_evidence_and_summary_prefer_admitted_identities() {
    assert!(WORKFLOW.contains(
        "BASE_SHA: ${{ steps.admitted.outputs.base || github.event.pull_request.base.sha"
    ));
    assert!(WORKFLOW.contains(
        "HEAD_SHA: ${{ steps.admitted.outputs.head || github.event.pull_request.head.sha"
    ));
    assert!(WORKFLOW.contains("INTEGRATION_TREE: ${{ steps.admitted.outputs.tree }}"));
}

#[test]
fn main_only_nextest_archive_measurement_is_preserved() {
    assert!(WORKFLOW.contains("- name: Measure nextest archive"));
    assert!(WORKFLOW.contains("- name: Upload nextest archive (measurement)"));
}

#[test]
fn legacy_profile_publishes_no_outputs_and_keeps_main_expressions() {
    // Main's expressions are the fallbacks after the admitted output.
    assert!(WORKFLOW.contains("SUBJECT_HEAD: ${{ steps.admitted.outputs.head || github.event.pull_request.head.sha || github.sha }}"));
    assert!(
        WORKFLOW.contains(
            "|| github.event.pull_request.base.sha || github.event.merge_group.base_sha }}"
        )
    );
    assert!(
        WORKFLOW.contains(
            "|| github.event.pull_request.head.sha || github.event.merge_group.head_sha }}"
        )
    );
    // The legacy branch must not write head/base outputs (merge_group would then
    // publish merge_group.head_sha as the summary subject head instead of github.sha).
    let (_, tail) = WORKFLOW
        .split_once("            legacy)\n")
        .expect("legacy arm");
    let (legacy, _) = tail.split_once("              ;;").expect("legacy arm end");
    assert!(!legacy.contains("GITHUB_OUTPUT"), "{legacy}");
    // The integration lines appear only for an admitted subject.
    assert!(WORKFLOW.contains("[[ -z \"$INTEGRATION_TREE\" ]] || printf"));
}
