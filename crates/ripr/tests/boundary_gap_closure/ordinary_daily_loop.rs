//! Ordinary developer loop from Quickstart, without a governed repair attempt.
//! Reuse the existing boundary fixture and process owner; the native test run
//! and static evidence remain separate observations (#10841, parent #10690).

use super::assert_nonzero_intended_execution;
use super::boundary_gap::{
    AFTER_TESTS, EQUALITY_SELECTOR, InstalledCandidate, JourneyRoots, LIB_CLOSED, SIBLING_TESTS,
    boundary_finding, decoy_was_invoked, install_candidate, journey_roots, parse_json, path_arg,
    plant_path_decoy, require_success, run_focused_cargo_test, run_installed, run_printed_ripr,
    stdout_text, two_commit_before_repo,
};
use serde_json::Value;
use std::process::Output;

fn check(
    candidate: &InstalledCandidate,
    roots: &JourneyRoots,
    base: &str,
    format: &str,
) -> Result<Output, String> {
    let output = run_installed(
        candidate,
        &roots.decoy,
        &roots.decoy,
        &[
            "check",
            "--root",
            &path_arg(&roots.fixture),
            "--base",
            base,
            "--worktree",
            "--format",
            format,
        ],
    )?;
    require_success("ordinary check --base --worktree", &output)?;
    Ok(output)
}

fn report(
    candidate: &InstalledCandidate,
    roots: &JourneyRoots,
    base: &str,
) -> Result<Value, String> {
    parse_json(
        "ordinary check",
        &stdout_text("ordinary check", &check(candidate, roots, base, "json")?)?,
    )
}

fn assert_same_subject(before: &Value, after: &Value) -> Result<(), String> {
    let before_gap = boundary_finding(before)?;
    let after_gap = boundary_finding(after)?;
    assert_eq!(after.get("base_commit"), before.get("base_commit"));
    assert_eq!(after.get("head"), before.get("head"));
    assert_eq!(after.get("root"), before.get("root"));
    assert_eq!(after.get("mode"), before.get("mode"));
    assert_eq!(
        after_gap.get("canonical_gap"),
        before_gap.get("canonical_gap")
    );
    assert_eq!(after_gap.get("probe"), before_gap.get("probe"));
    assert_eq!(after_gap["source_currentness"], "candidate_current");
    assert_eq!(after["analysis_outcome"]["analysis_complete"], true);
    assert_eq!(after["summary"]["probes"], 1);
    assert_ne!(
        after.pointer("/analysis_outcome/outcome/identity/input_identity"),
        before.pointer("/analysis_outcome/outcome/identity/input_identity"),
        "same-HEAD test edits must refresh the analysis input"
    );
    Ok(())
}

#[test]
fn ordinary_loop_preserves_production_subject_and_native_outcome() -> Result<(), String> {
    let roots = journey_roots("ordinary-daily-loop")?;
    let marker = plant_path_decoy(&roots.decoy)?;
    let candidate = install_candidate(&roots.install)?;
    let base = two_commit_before_repo(&roots.fixture, &roots.cargo_target)?;
    let before = report(&candidate, &roots, &base)?;
    let gap = boundary_finding(&before)?;
    assert_eq!(before["base_commit"], base);
    assert_eq!(before["head"]["source"], "working_tree");
    assert_eq!(before["mode"], "draft");
    assert_eq!(before["summary"]["probes"], 1);
    assert_eq!(before["analysis_outcome"]["analysis_complete"], true);
    assert_eq!(gap["classification"], "weakly_exposed");

    let human = stdout_text(
        "ordinary human check",
        &check(&candidate, &roots, &base, "human")?,
    )?;
    for command in ["explain", "context"] {
        let prefix = format!("ripr {command} ");
        let printed = human
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with(&prefix))
            .ok_or_else(|| format!("ordinary check did not print {command}: {human}"))?;
        let output = run_printed_ripr(&candidate, &roots.decoy, &roots.decoy, printed)?;
        require_success(printed, &output)?;
        let text = stdout_text(printed, &output)?;
        if command == "context" {
            let context = parse_json("printed context", &text)?;
            assert_eq!(context["canonical_gap_id"], gap["canonical_gap_id"]);
            assert_eq!(context["source_currentness"], "candidate_current");
            assert_eq!(context["classification"], "weakly_exposed");
            assert_eq!(context["witness"]["before"], "amount > discount_threshold");
            assert_eq!(context["witness"]["after"], "amount >= discount_threshold");
        } else {
            assert!(text.contains("amount > discount_threshold"));
            assert!(text.contains("amount >= discount_threshold"));
        }
    }

    // A passing test at the wrong input does not close the original gap.
    let test_file = roots.fixture.join("tests/pricing.rs");
    std::fs::write(&test_file, SIBLING_TESTS).map_err(|error| error.to_string())?;
    let sibling_run =
        run_focused_cargo_test(&roots.fixture, &roots.cargo_target, EQUALITY_SELECTOR)?;
    assert_nonzero_intended_execution(&sibling_run)?;
    let sibling = report(&candidate, &roots, &base)?;
    assert_same_subject(&before, &sibling)?;
    assert_eq!(
        boundary_finding(&sibling)?["classification"],
        "weakly_exposed"
    );

    // Switching to HEAD hides the committed production change. A quiet
    // test-only comparison is the wrong subject, not a successful improvement.
    let test_only = report(&candidate, &roots, "HEAD")?;
    assert_ne!(test_only["base_commit"], before["base_commit"]);
    assert_eq!(test_only["summary"]["probes"], 0);
    assert_eq!(test_only["findings"], serde_json::json!([]));
    assert_eq!(
        test_only["analysis_outcome"]["outcome"]["kind"],
        "no_behavioral_candidates"
    );

    std::fs::write(&test_file, AFTER_TESTS).map_err(|error| error.to_string())?;
    let execution = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, EQUALITY_SELECTOR)?;
    assert_nonzero_intended_execution(&execution)?;
    assert_eq!(execution.executed, 1);
    assert_eq!(execution.ignored, 0);
    let after = report(&candidate, &roots, &base)?;
    assert_same_subject(&before, &after)?;
    assert_eq!(boundary_finding(&after)?["classification"], "exposed");
    assert_eq!(
        boundary_finding(&after)?["missing_discriminators"],
        serde_json::json!([])
    );

    // The wrong expected value statically contradicts the owner's folded
    // behavior, so the whole-test contradiction control withholds the
    // diff-mode credit exactly as the repo surface does (#7007, #10980):
    // a test that fails at baseline cannot claim the change is noticed.
    // The native equality test still fails, and the report subject itself
    // is unchanged.
    let wrong_expectation = AFTER_TESTS.replace(
        "discounted_total(100, 100), 90",
        "discounted_total(100, 100), 91",
    );
    std::fs::write(&test_file, wrong_expectation).map_err(|error| error.to_string())?;
    let failed = run_focused_cargo_test(&roots.fixture, &roots.cargo_target, EQUALITY_SELECTOR)?;
    assert_eq!(failed.executed, 1);
    assert_eq!(failed.failed, 1);
    assert_eq!(failed.passed, 0);
    assert!(failed.exit_code.is_some_and(|code| code != 0));
    let failed_static = report(&candidate, &roots, &base)?;
    assert_same_subject(&after, &failed_static)?;
    assert_eq!(
        boundary_finding(&failed_static)?["classification"],
        "weakly_exposed"
    );
    assert_eq!(
        std::fs::read_to_string(roots.fixture.join("src/lib.rs"))
            .map_err(|error| error.to_string())?,
        LIB_CLOSED
    );
    assert!(!decoy_was_invoked(&marker));
    Ok(())
}
