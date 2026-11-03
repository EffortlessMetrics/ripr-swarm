//! Pin the hosted golden re-bless route at the workflow boundary (#11736,
//! Proposal A).
//!
//! The route is the only automation allowed to write golden expected-files
//! without a human in the loop, so every decision that permits a bless must
//! live in trusted workflow-layer code (this workflow file, as it exists on
//! the base branch) and must stay stricter than the merge-ref classifier's
//! normalized `formatting_only` label. Breaking any invariant below should
//! fail here, not silently turn the route into a semantic-drift bless, a
//! head-branch executor, a fork writer, or an unconfirmed push:
//!
//! - checkout switched to `refs/pull/N/head` → `checkout_uses_the_merge_ref`
//!   fails on the pinned merge-ref expression (head-branch execution is the
//!   core forbidden shape);
//! - the judge trusting `golden-drift.json` alone → `judge_requires_the_byte
//!   _identity_witness` fails on the missing `git diff --no-index` marker;
//! - a partial or mixed-set bless (or accepting `status == "pass"`) →
//!   `judge_refuses_semantic_and_check_json_drift` fails on the warn /
//!   check.json-surface / semantic-array gates;
//! - the post-bless confirmation removed → `confirm_requires_a_clean_second
//!   _check` fails on the second `cargo xtask goldens check`;
//! - workflow-level write scope, workflow dispatch, review-thread mutation or
//!   third-party PR creation → `workflow_stays_inside_its_write_boundary`
//!   fails on the forbidden fragments;
//! - fork heads accepted or mergeability preflight dropped →
//!   `preflight_refuses_forks_and_conflicts` fails on the head-repo and
//!   mergeability markers;
//! - the allowlist budget row removed → `run_block_budget_is_registered`
//!   fails (and `cargo xtask check-workflows` fails the PR).

use std::fs;
use std::path::Path;

const WORKFLOW: &str = ".github/workflows/goldens-rebless.yml";
const ALLOWLIST: &str = "policy/workflow_allowlist.txt";

/// Executable (non-comment) fragments that would let the route gate,
/// dispatch, mutate review state, or exceed its write boundary.
const FORBIDDEN_FRAGMENTS: &[&str] = &[
    "actions: write",
    "gh workflow run",
    "create-pull-request",
    "resolveReviewThread",
    "write-all",
];

fn repo_file(path: &str) -> Result<String, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .ok_or_else(|| "xtask package has a repository parent".to_string())?;
    fs::read_to_string(root.join(path)).map_err(|error| error.to_string())
}

/// Executable lines only: comment prose may explain the contract without
/// satisfying it. Leading indentation is preserved so scope-level checks can
/// tell a workflow-level write from a job-level one.
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
    Err(format!("goldens-rebless is missing `{needle}`: {why}"))
}

fn forbid(lines: &[&str], needle: &str, why: &str) -> Result<(), String> {
    if !lines.iter().any(|line| line.contains(needle)) {
        return Ok(());
    }
    Err(format!(
        "goldens-rebless must not contain `{needle}`: {why}"
    ))
}

fn count_containing(lines: &[&str], needle: &str) -> usize {
    lines.iter().filter(|line| line.contains(needle)).count()
}

#[test]
fn dispatches_with_a_pr_input_and_serializes_re_dispatches() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "workflow_dispatch:",
        "the route exists only when an actor deliberately dispatches it",
    )?;
    require(
        &lines,
        "PR_INPUT: ${{ inputs.pr }}",
        "the PR number reaches scripts through env, never interpolated into them",
    )?;
    require(
        &lines,
        "group: goldens-rebless-${{ inputs.pr || github.run_id }}",
        "one bless attempt per PR at a time",
    )?;
    require(
        &lines,
        "cancel-in-progress: true",
        "a re-dispatch supersedes; nothing durable exists until the one push",
    )
}

#[test]
fn runs_on_the_arbiter_platform_with_a_deadline() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "if: github.repository == 'EffortlessMetrics/ripr-swarm'",
        "a fork running the workflow must not touch the upstream repository",
    )?;
    require(
        &lines,
        "runs-on: ubuntu-latest",
        "Linux is the arbiter platform for goldens; a bless from another platform \
         would institutionalize the recorded Windows/CRLF drift shapes",
    )?;
    require(
        &lines,
        "timeout-minutes: 60",
        "a hung bless is a red signal with an end, not a stuck run",
    )
}

#[test]
fn checkout_uses_the_merge_ref() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "ref: refs/pull/${{ steps.preflight.outputs.pr_number }}/merge",
        "the job executes the two-parent integration tree the required \
         qualification already runs; a `refs/pull/N/head` checkout would \
         execute and bless an unmerged, unadmitted head",
    )?;
    require(
        &lines,
        "persist-credentials: false",
        "fixture runs execute PR-authored build scripts; no token may sit in \
         .git/config while they run",
    )
}

#[test]
fn preflight_refuses_forks_and_conflicts() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "^[0-9]+$",
        "the PR number is validated before it is used anywhere",
    )?;
    require(
        &lines,
        "head.repo.full_name",
        "a fork head is refused where GITHUB_TOKEN holds no write",
    )?;
    require(
        &lines,
        "mergeable",
        "the merge ref only exists while the PR merges cleanly; dispatching \
         against a missing ref must fail the preflight",
    )?;
    require(
        &lines,
        "mergeable_state",
        "the PR state is read, not assumed",
    )?;
    require(
        &lines,
        "\"dirty\"",
        "a conflicting PR is refused instead of judged against a stale merge ref",
    )
}

#[test]
fn claims_convergence_before_touching_the_pr() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "Converging this PR:",
        "the bot records its claim in the PR_AUTOMATION.md house format",
    )?;
    require(
        &lines,
        "14400",
        "another actor's convergence claim younger than 4h owns the PR; the \
         route refuses instead of racing it",
    )?;
    require(
        &lines,
        "goldens-rebless-claim",
        "the bot's own claim is marked, so a re-dispatch is not blocked by \
         itself while foreign claims still are",
    )
}

#[test]
fn fetches_the_same_locked_manifests_as_the_required_gate() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml",
        "the locked manifest the gate fetches explicitly",
    )?;
    require(
        &lines,
        "cargo fetch --locked --manifest-path fixtures/owner_return_pin_json_real_literal/input/Cargo.toml",
        "the second locked manifest the gate fetches explicitly",
    )?;
    require(
        &lines,
        "cargo fetch --locked --manifest-path crates/ripr/tests/fixtures/proptest_singleton/input/Cargo.toml",
        "a preparation gap must classify as fail, never silently bless an \
         incomplete run",
    )
}

#[test]
fn judge_refuses_semantic_and_check_json_drift() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "!= \"warn\"",
        "only drift-only status is blessable: accepting `pass` blesses nothing \
         and accepting `fail` blesses a broken run",
    )?;
    require(
        &lines,
        ".entries | length",
        "a zero-entry dispatch is a no-op, not a bless",
    )?;
    require(
        &lines,
        "select(.surface == \"check.json\")",
        "check.json drift is refused whole: the machine contract is never \
         auto-blessed, which is what keeps a partial or mixed set out",
    )?;
    require(
        &lines,
        "added_finding_ids",
        "semantic arrays gate the bless; an added finding must never be blessed",
    )?;
    require(
        &lines,
        "static_language_terms",
        "static-language boundary terms gate the bless",
    )?;
    require(
        &lines,
        "changed_stop_reasons",
        "stop-reason changes gate the bless",
    )
}

#[test]
fn judge_requires_the_byte_identity_witness() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "git diff --no-index",
        "the judge must not trust the merge-ref classifier's normalized \
         formatting-only label alone; the check.json machine contract must be \
         byte-identical before anything is blessed",
    )?;
    require(
        &lines,
        "expected/check.json",
        "the byte compare reads the committed contract side",
    )?;
    require(
        &lines,
        "target/ripr/fixtures",
        "the byte compare reads the freshly rendered actual side",
    )
}

#[test]
fn bless_reasons_derive_from_each_fixtures_own_spec() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "cargo xtask goldens bless",
        "the bless runs the real producer, whose own integrity and spec-id \
         guards still execute",
    )?;
    require(
        &lines,
        "Spec: RIPR-SPEC-",
        "each reason is derived from that fixture's SPEC.md id, so the bless \
         command's spec validation passes without inventing a reason",
    )?;
    require(
        &lines,
        "for f in $FIXTURES",
        "every judged fixture is blessed; blessing a subset would strand the \
         rest of a judged drift set as a half-blessed red PR",
    )
}

#[test]
fn confirm_requires_a_clean_second_check() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    let checks = count_containing(&lines, "cargo xtask goldens check");
    if checks < 2 {
        return Err(format!(
            "goldens-rebless must run `cargo xtask goldens check` twice (pre-bless witness and post-bless confirmation); found {checks}"
        ));
    }
    require(
        &lines,
        "Confirm re-blessed goldens are clean",
        "the second check must actually gate the push, not merely appear",
    )?;
    require(
        &lines,
        "|| status=$?",
        "the pre-bless check's failure is captured as the witness instead of \
         aborting the job before the judge runs",
    )
}

#[test]
fn push_is_bot_authored_guarded_and_token_scoped() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "ls-remote",
        "the runbook's writer guard runs immediately before the push; a moved \
         head is reconciled, never pushed over",
    )?;
    require(
        &lines,
        "41898282+github-actions[bot]@users.noreply.github.com",
        "the bless commit is bot-attributed like the rest of the automation",
    )?;
    require(
        &lines,
        "x-access-token",
        "the push authenticates through an ephemeral token URL, not persisted \
         credentials",
    )?;
    require(
        &lines,
        "git push",
        "the one durable write of the whole route",
    )?;
    let token_steps = count_containing(&lines, "GITHUB_TOKEN: ${{ github.token }}");
    if token_steps < 1 {
        return Err("the push step must inject the token".to_string());
    }
    Ok(())
}

#[test]
fn re_arms_qualification_after_the_push() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "labels/merge-ready",
        "the GITHUB_TOKEN push fires no events, so the route re-arms \
         qualification itself by removing the label",
    )?;
    require(
        &lines,
        "labels[]=merge-ready",
        "and re-adding it, so the fresh labeled event re-runs admission and \
         qualification on the new head",
    )?;
    require(
        &lines,
        "was_merge_ready",
        "the label is re-armed only when it was already present; otherwise the \
         route comments the exact command instead of adding a qualification \
         request nobody made",
    )
}

#[test]
fn refusals_are_visible_and_push_nothing() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    require(
        &lines,
        "goldens-rebless-refusal",
        "a refused dispatch comments the classified drift on the PR",
    )?;
    require(
        &lines,
        "steps.judge_gates.outcome == 'failure'",
        "the refusal comment is bound to a judged failure, not to any crash",
    )?;
    forbid(
        &lines,
        "exit 0",
        "a refused dispatch must end red; a green refusal would present the \
         no-bless as success",
    )
}

#[test]
fn workflow_stays_inside_its_write_boundary() -> Result<(), String> {
    let source = repo_file(WORKFLOW)?;
    let lines = executable_lines(&source);
    for fragment in FORBIDDEN_FRAGMENTS {
        forbid(
            &lines,
            fragment,
            "the route blesses goldens; it never gates, \
             dispatches, resolves review threads, or creates PRs",
        )?;
    }
    // Workflow-level write scopes reach every job; the one push scope lives on
    // the job only (repo-policy enforces the same rule independently).
    if lines.iter().any(|line| {
        let indent = line.len() - line.trim_start().len();
        indent == 2 && line.trim_start().ends_with(": write")
    }) {
        return Err(
            "goldens-rebless must keep write scopes on the job; a workflow-level \
             write grants them to every job"
                .to_string(),
        );
    }
    require(
        &lines,
        "contents: write",
        "the job holds exactly one push scope",
    )?;
    require(
        &lines,
        "pull-requests: write",
        "the job's comment and label re-arm authority",
    )
}

#[test]
fn run_block_budget_is_registered() -> Result<(), String> {
    let allowlist = repo_file(ALLOWLIST)?;
    if allowlist
        .lines()
        .any(|line| line.starts_with(".github/workflows/goldens-rebless.yml|"))
    {
        return Ok(());
    }
    Err(
        "check-workflows requires a run-block budget row for every workflow; \
         the goldens-rebless row in policy/workflow_allowlist.txt is part of \
         its contract"
            .to_string(),
    )
}
