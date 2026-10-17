use super::*;

#[test]
fn routed_rust_docs_gate_runs_full_precommit_table() -> Result<(), String> {
    let workflow = routed_rust_workflow_text()?;
    let docs_gate = workflow
        .split("\n  docs-gate:")
        .nth(1)
        .ok_or("routed-rust.yml must declare a docs-gate job")?;
    let docs_gate = docs_gate
        .split("\n  result:")
        .next()
        .ok_or("docs-gate job must precede the result job")?;
    let lines: Vec<String> = docs_gate.lines().map(str::to_string).collect();
    require_single_bare_precommit_line(
        &lines,
        "docs-gate job (docs-only PRs must run the full gate table)",
    )?;
    Ok(())
}

#[test]
fn routed_rust_live_contract_rejects_reviewed_semantic_regressions() {
    let workflow = include_str!("../../../.github/workflows/routed-rust.yml");
    let reusable = include_str!("../../../.github/workflows/rust-gates.yml");
    let settings = include_str!("../../../.github/settings.yml");
    let lane = include_str!("../../../policy/ci-lane-whitelist.toml");

    let violations = routed_rust_workflow_contract_violations_with_reusable(
        workflow,
        Some(reusable),
        Some(settings),
        Some(lane),
    );
    assert!(
        violations.is_empty(),
        "live workflow contract drift: {violations:#?}"
    );

    let regressions: Vec<(&str, String, String, &str)> = vec![
        (
            "delegated runner-config input",
            workflow.replace(
                "      runner-config: '{\"group\":\"em-ci-small\",\"labels\":[\"self-hosted\",\"linux\",\"x64\",\"em-ci\",\"cx43\",\"rust-medium\",\"trusted-pr\"]}'",
                "      runner-config-missing: '{\"group\":\"em-ci-small\",\"labels\":[\"self-hosted\",\"linux\",\"x64\",\"em-ci\",\"cx43\",\"rust-medium\",\"trusted-pr\"]}'",
            ),
            reusable.to_string(),
            "delegated job `rust-cx43`",
        ),
        (
            "delegated JSON-string runner-config count",
            workflow.replace(
                "      runner-config: '\"ubuntu-latest\"'",
                "      runner-config: ubuntu-latest",
            ),
            reusable.to_string(),
            "JSON-string runner-config values",
        ),
        (
            "runner input type",
            workflow.to_string(),
            reusable.replace(
                "runner-config:\n        description: JSON string or object accepted by jobs.<job_id>.runs-on.\n        required: true\n        type: string",
                "runner-config:\n        description: JSON string or object accepted by jobs.<job_id>.runs-on.\n        required: true\n        type: boolean",
            ),
            "required string contract",
        ),
        (
            "runner conversion",
            workflow.to_string(),
            reusable.replace(
                "runs-on: ${{ fromJSON(inputs.runner-config) }}",
                "runs-on: ${{ inputs.runner-config }}",
            ),
            "fromJSON",
        ),
        (
            "CPX42 label",
            workflow.replace(
                "cpx42\",\"rust-medium\",\"rust-16gb",
                "cpx42\",\"rust-16gb",
            ),
            reusable.to_string(),
            "rust-medium capacity",
        ),
        (
            "docs artifact retention",
            workflow.replace(
                "- name: Upload docs-gate reports\n        if: always()",
                "- name: Upload docs-gate reports\n        if: failure()",
            ),
            reusable.to_string(),
            "docs-gate artifacts",
        ),
        (
            "advisory gating",
            workflow.to_string(),
            reusable.replace(
                "if: success() && inputs.run-advisory-reports",
                "if: inputs.run-advisory-reports",
            ),
            "advisory reports",
        ),
        (
            "failure artifact retention",
            workflow.to_string(),
            reusable.replace(
                "if: failure() || inputs.upload-success-artifacts",
                "if: inputs.upload-success-artifacts",
            ),
            "failure artifacts",
        ),
    ];

    for (label, mutated_workflow, mutated_reusable, expected) in regressions {
        let violations = routed_rust_workflow_contract_violations_with_reusable(
            &mutated_workflow,
            Some(&mutated_reusable),
            Some(settings),
            Some(lane),
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains(expected)),
            "reviewed workflow regression `{label}` was not rejected by its expected contract: {violations:#?}"
        );
    }
}

#[test]
fn routed_rust_precommit_invocation_count_matches_delegated_owner() -> Result<(), String> {
    let workflow = routed_rust_workflow_text()?;
    let reusable = reusable_rust_workflow_text()?;
    let routed_count = workflow.matches("cargo xtask precommit").count();
    let reusable_count = reusable.matches("cargo xtask ci-policy").count();
    if routed_count != 1 || reusable_count != 1 {
        return Err(format!(
            "delegated Rust proof must invoke `cargo xtask precommit` once in routed-rust.yml (docs-gate) and once as ci-policy in rust-gates.yml, found routed={routed_count}, reusable={reusable_count}"
        ));
    }
    Ok(())
}

#[test]
fn routed_rust_ready_event_matrix_withholds_draft_and_label_context() {
    let workflow = include_str!("../../../.github/workflows/routed-rust.yml");
    let cases = [
        (
            "pull_request",
            Some("ready_for_review"),
            None,
            RoutedRustEventRoute::LaunchFullGate,
        ),
        (
            "pull_request",
            Some("opened"),
            None,
            RoutedRustEventRoute::WorkflowNotTriggered,
        ),
        (
            "pull_request",
            Some("reopened"),
            None,
            RoutedRustEventRoute::WorkflowNotTriggered,
        ),
        (
            "pull_request",
            Some("synchronize"),
            None,
            RoutedRustEventRoute::WorkflowNotTriggered,
        ),
        (
            "pull_request",
            Some("labeled"),
            Some("full-ci"),
            RoutedRustEventRoute::WorkflowNotTriggered,
        ),
        (
            "pull_request",
            Some("labeled"),
            Some("windows-ci"),
            RoutedRustEventRoute::WorkflowNotTriggered,
        ),
        (
            "pull_request",
            Some("unlabeled"),
            Some("full-ci"),
            RoutedRustEventRoute::WorkflowNotTriggered,
        ),
        ("push", None, None, RoutedRustEventRoute::LaunchFullGate),
        (
            "workflow_dispatch",
            None,
            None,
            RoutedRustEventRoute::LaunchFullGate,
        ),
    ];
    for (event_name, action, label, expected) in cases {
        let actual = routed_rust_event_route(workflow, event_name, action, label);
        assert_eq!(
            actual, expected,
            "event={event_name} action={action:?} label={label:?}"
        );
    }

    let draft_resurrected = workflow.replace(
        "    types: [ready_for_review]",
        "    types: [ready_for_review, synchronize]",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&draft_resurrected)
            .iter()
            .any(|violation| {
                violation.contains("must be exactly") && violation.contains("synchronize")
            }),
        "re-admitting synchronize must fail the Ready-only contract: {:?}",
        routed_rust_ready_event_contract_violations(&draft_resurrected)
    );

    let label_resurrected = workflow.replace(
        "    types: [ready_for_review]",
        "    types: [ready_for_review, labeled]",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&label_resurrected)
            .iter()
            .any(|violation| {
                violation.contains("must be exactly") && violation.contains("labeled")
            }),
        "re-admitting label events must fail the Ready-only contract: {:?}",
        routed_rust_ready_event_contract_violations(&label_resurrected)
    );

    let edited_resurrected = workflow.replace(
        "    types: [ready_for_review]",
        "    types: [ready_for_review, edited]",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&edited_resurrected)
            .iter()
            .any(|violation| {
                violation.contains("must be exactly") && violation.contains("edited")
            }),
        "re-admitting the edited activity type must fail the Ready-only contract: {:?}",
        routed_rust_ready_event_contract_violations(&edited_resurrected)
    );

    let ready_dropped = workflow.replace(
        "    types: [ready_for_review]",
        "    types: [opened, synchronize, reopened]",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&ready_dropped)
            .iter()
            .any(|violation| violation.contains("must be exactly")),
        "dropping the Ready transition must fail closed: {:?}",
        routed_rust_ready_event_contract_violations(&ready_dropped)
    );

    let missing_types = workflow.replace("    types: [ready_for_review]\n", "");
    assert!(
        routed_rust_ready_event_contract_violations(&missing_types)
            .iter()
            .any(|violation| violation.contains("inline pull_request types array")),
        "removing types must fail closed: {:?}",
        routed_rust_ready_event_contract_violations(&missing_types)
    );

    let cancellation_disabled = workflow.replace(
        "  cancel-in-progress: ${{ github.event_name == 'pull_request' }}",
        "  cancel-in-progress: false",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&cancellation_disabled)
            .iter()
            .any(|violation| violation.contains("cancel-in-progress")),
        "disabling Ready-run cancellation must fail: {:?}",
        routed_rust_ready_event_contract_violations(&cancellation_disabled)
    );

    let uncancellable_fallback = workflow.replace("      !cancelled() &&", "      always() &&");
    assert_ne!(
        uncancellable_fallback, workflow,
        "fixture must actually swap the hosted fallback condition"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&uncancellable_fallback)
            .iter()
            .any(|violation| violation.contains("`rust-github`") && violation.contains("always()")),
        "a job-level always() on an implementation job must fail: it survives Ready-run cancellation: {:?}",
        routed_rust_ready_event_contract_violations(&uncancellable_fallback)
    );

    let spaced_uppercase = workflow.replace("      !cancelled() &&", "      Always () &&");
    assert!(
        routed_rust_ready_event_contract_violations(&spaced_uppercase)
            .iter()
            .any(|violation| violation.contains("`rust-github`")),
        "expression names are case-insensitive and may carry spaces: {:?}",
        routed_rust_ready_event_contract_violations(&spaced_uppercase)
    );

    let step_level_cleanup = workflow.replace(
        "    uses: ./.github/workflows/rust-gates.yml\n    with:\n      runner-config: '\"ubuntu-latest\"'",
        "    uses: ./.github/workflows/rust-gates.yml\n    # cleanup may use always()\n    with:\n      runner-config: '\"ubuntu-latest\"'",
    );
    assert_ne!(step_level_cleanup, workflow, "fixture must add the comment");
    assert!(
        !routed_rust_ready_event_contract_violations(&step_level_cleanup)
            .iter()
            .any(|violation| violation.contains("must not use `always()`")),
        "always() outside the job condition must not be rejected: {:?}",
        routed_rust_ready_event_contract_violations(&step_level_cleanup)
    );

    let shared_group = workflow.replace(
        "  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}-${{ github.event_name }}",
        "  group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&shared_group)
            .iter()
            .any(|violation| violation.contains("event-qualified concurrency group")),
        "sharing the push/manual concurrency group must fail: {:?}",
        routed_rust_ready_event_contract_violations(&shared_group)
    );

    let draft_guard = workflow.replace(
        "    name: Route Ripr Rust Small",
        "    if: github.event.pull_request.draft != true\n    name: Route Ripr Rust Small",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&draft_guard)
            .iter()
            .any(|violation| violation.contains("github.event.pull_request.draft")),
        "a draft job guard must fail; a skipped required job reports success: {:?}",
        routed_rust_ready_event_contract_violations(&draft_guard)
    );

    let pseudo_result = workflow.replace(
        "    name: Ripr Rust Small Result",
        "    name: ${{ github.event_name == 'pull_request' && 'Ripr Rust Small Ignored Label Event' || 'Ripr Rust Small Result' }}",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&pseudo_result)
            .iter()
            .any(|violation| violation.contains("Ignored Label Event")),
        "resurrecting the ignored-label pseudo-result must fail: {:?}",
        routed_rust_ready_event_contract_violations(&pseudo_result)
    );

    let renamed_result = workflow.replace(
        "    name: Ripr Rust Small Result",
        "    name: Ripr Rust Small Draft Result",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&renamed_result)
            .iter()
            .any(|violation| violation.contains("must post the static")),
        "renaming the required result context must fail: {:?}",
        routed_rust_ready_event_contract_violations(&renamed_result)
    );
}

/// The routed Rust lanes must delegate the required gate table to the shared
/// reusable workflow, not inline a lane-only gate command where it could drift
/// from `.github/workflows/rust-gates.yml`. The reusable workflow enumerates
/// each gate as its own named per-producer step; the per-step shape (exact
/// command, unconditional, ordered, outcome-reported) is owned by
/// `xtask/tests/rust_gate_workflow_contract.rs`. `cargo xtask precommit` and
/// the lightweight `cargo policy preflight` stay inline in the docs-gate job,
/// which `routed_rust_docs_gate_runs_full_precommit_table` covers.
#[test]
fn routed_rust_required_lanes_run_full_precommit_table() -> Result<(), String> {
    let workflow = routed_rust_workflow_text()?;
    for lane_only in [
        "cargo xtask check-evidence-promotion-honesty",
        "cargo xtask check-dependencies",
        "cargo xtask check-process-policy",
        "cargo xtask check-network-policy",
        "cargo xtask goldens check",
        "cargo xtask fixtures",
    ] {
        if workflow.contains(lane_only) {
            return Err(format!(
                "routed-rust.yml must delegate `{lane_only}` to rust-gates.yml, not inline it"
            ));
        }
    }
    let reusable = reusable_rust_workflow_text()?;
    for lane_only in [
        "cargo xtask check-evidence-promotion-honesty",
        "cargo xtask check-dependencies",
        "cargo xtask check-process-policy",
        "cargo xtask check-network-policy",
        "cargo xtask goldens check",
        "cargo xtask fixtures",
    ] {
        let count = reusable.matches(lane_only).count();
        if count != 1 {
            return Err(format!(
                "rust-gates.yml must enumerate lane-only gate `{lane_only}` exactly once, found {count}"
            ));
        }
    }
    Ok(())
}

#[test]
fn routed_rust_workflow_contract_accepts_reusable_implementation_authority() {
    let workflow = r#"
jobs:
  route:
    name: Route Ripr Rust Small
    timeout-minutes: 10
    steps:
      - name: Select runner
        env:
          GH_TOKEN: ${{ secrets.EM_RUNNER_READ_TOKEN || github.token }}
        run: |
          if [ "$EVENT_NAME" = "pull_request" ] && [ "$HEAD_REPO" != "$REPOSITORY" ]; then
            reason=fork_or_untrusted_pr
          fi
          idle() { printf '%s' "$runners" | jq -s -e --arg model "$1" --arg cap "$2" '[.[].runners[]?] | length > 0'; }
          gh api --paginate orgs/EffortlessMetrics/actions/runners
          reason=runner_api_failed
          reason=no_idle_runner
          reason=runner_capacity_unavailable
          reason=cx43_idle
          reason=cpx42_idle
          reason=cx53_idle
          echo rust-medium rust-16gb rust-large
  detect-docs-only:
    name: Detect Docs-Only Surface
    timeout-minutes: 10
  rust-cx43:
    if: needs.route.outputs.router_target == 'cx43'
    uses: ./.github/workflows/rust-gates.yml
    with:
      runner-config: '{"labels":["rust-medium"]}'
      disk-guard-threshold: 35
  rust-cpx42:
    if: needs.route.outputs.router_target == 'cpx42'
    uses: ./.github/workflows/rust-gates.yml
    with:
      runner-config: '{"labels":["rust-medium","rust-16gb"]}'
      disk-guard-threshold: 35
  rust-cx53:
    if: needs.route.outputs.router_target == 'cx53'
    uses: ./.github/workflows/rust-gates.yml
    with:
      runner-config: '{"labels":["rust-large"]}'
      disk-guard-threshold: 50
  rust-github:
    if: >-
      needs.detect-docs-only.result == 'success' &&
      needs.route.outputs.router_target == 'github' ||
      needs.rust-cx43.outputs.scratch_status == 'tempfail' ||
      needs.rust-cpx42.outputs.scratch_status == 'tempfail' ||
      needs.rust-cx53.outputs.scratch_status == 'tempfail'
    uses: ./.github/workflows/rust-gates.yml
    with:
      runner-config: '"ubuntu-latest"'
  docs-gate:
    name: Ripr Docs Gate
    timeout-minutes: 20
    steps:
      - name: Upload docs-gate reports
        if: always()
  result:
    name: Ripr Rust Small Result
    timeout-minutes: 10
    env:
      DOCS_DETECT_RESULT: ${{ needs.detect-docs-only.result }}
    steps:
      - run: echo "disk-guard tempfailed; GitHub-hosted fallback succeeded"
      - run: echo "docs-surface detection result was $DOCS_DETECT_RESULT"
"#;
    let reusable = r#"
on:
  workflow_call:
    inputs:
      runner-config:
        description: JSON string or object accepted by jobs.<job_id>.runs-on.
        required: true
        type: string
      disk-guard-threshold:
        type: number
        required: false
    outputs:
      scratch_status:
        value: ${{ jobs.rust-gates.outputs.scratch_status }}
jobs:
  rust-gates:
    runs-on: ${{ fromJSON(inputs.runner-config) }}
    timeout-minutes: 90
    outputs:
      scratch_status: ${{ steps.scratch.outputs.status }}
    env:
      CARGO_HOME: /mnt/ci-scratch/cargo-home/${{ github.run_id }}-${{ github.run_attempt }}
    steps:
      - name: Prepare toolchain temp
        run: mkdir -p "$TMPDIR"
      - name: Prepare scratch
        run: ci-disk-guard /mnt/ci-scratch "${{ inputs.disk-guard-threshold }}"
      - name: Proof route dry-run (advisory)
        run: cargo xtask proof route --base "$BASE_SHA" --head "$HEAD_SHA" || true
      - name: Clean scratch
        run: rm -rf "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TMPDIR"
      - name: Advisory reports
        if: success() && inputs.run-advisory-reports
      - name: Upload RIPR reports
        if: failure() || inputs.upload-success-artifacts
defaults:
  run:
    shell: bash
"#;
    let settings = r#"
repository:
  name: ripr-swarm
branches:
  - name: main
    protection:
      required_status_checks:
        contexts:
          - Ripr Rust Small Result
"#;
    let lane = r#"
[[lane]]
id = "routed-rust-small"
workflow = ".github/workflows/routed-rust.yml"
jobs = ["Ripr Rust Small Result"]
"#;

    let violations = routed_rust_workflow_contract_violations_with_reusable(
        workflow,
        Some(reusable),
        Some(settings),
        Some(lane),
    );
    assert!(violations.is_empty(), "{violations:#?}");

    let missing = routed_rust_workflow_contract_violations_with_reusable(
        workflow,
        None,
        Some(settings),
        Some(lane),
    );
    assert!(
        missing
            .iter()
            .any(|violation| violation.contains("delegates implementation jobs to missing"))
    );

    let no_jobs = reusable.replacen("jobs:\n", "missing-jobs:\n", 1);
    let no_jobs_violations = routed_rust_workflow_contract_violations_with_reusable(
        workflow,
        Some(&no_jobs),
        Some(settings),
        Some(lane),
    );
    assert!(
        no_jobs_violations
            .iter()
            .any(|violation| violation.contains("analyzed zero `jobs:` entries"))
    );

    let no_deadline = reusable.replace("    timeout-minutes: 90\n", "");
    let deadline_violations = routed_rust_workflow_contract_violations_with_reusable(
        workflow,
        Some(&no_deadline),
        Some(settings),
        Some(lane),
    );
    assert!(deadline_violations.iter().any(|violation| {
        violation.contains("rust-gates.yml job `rust-gates` must set an explicit `timeout-minutes`")
    }));

    let no_output = reusable.replace(
        "    outputs:\n      scratch_status:\n        value: ${{ jobs.rust-gates.outputs.scratch_status }}\n",
        "",
    );
    let output_violations = routed_rust_workflow_contract_violations_with_reusable(
        workflow,
        Some(&no_output),
        Some(settings),
        Some(lane),
    );
    assert!(
        output_violations
            .iter()
            .any(|violation| { violation.contains("workflow_call scratch-status output") })
    );

    let wrong_output_key = reusable.replacen(
        "      scratch_status:\n        value: ${{ jobs.rust-gates.outputs.scratch_status }}\n",
        "      renamed_status:\n        value: ${{ jobs.rust-gates.outputs.scratch_status }}\n",
        1,
    );
    let output_key_violations = routed_rust_workflow_contract_violations_with_reusable(
        workflow,
        Some(&wrong_output_key),
        Some(settings),
        Some(lane),
    );
    assert!(
        output_key_violations
            .iter()
            .any(|violation| violation.contains("workflow_call scratch-status output"))
    );

    let partial = workflow.replacen(
        "    uses: ./.github/workflows/rust-gates.yml\n",
        "    runs-on: ubuntu-latest\n",
        1,
    );
    let partial_violations = routed_rust_workflow_contract_violations_with_reusable(
        &partial,
        Some(reusable),
        Some(settings),
        Some(lane),
    );
    assert!(partial_violations.iter().any(|violation| {
        violation.contains("must keep all four implementation jobs inline or delegate all four")
    }));

    let wrong_threshold = workflow
        .replacen("disk-guard-threshold: 35", "disk-guard-threshold: swap", 1)
        .replacen("disk-guard-threshold: 50", "disk-guard-threshold: 35", 1)
        .replacen("disk-guard-threshold: swap", "disk-guard-threshold: 50", 1);
    let threshold_violations = routed_rust_workflow_contract_violations_with_reusable(
        &wrong_threshold,
        Some(reusable),
        Some(settings),
        Some(lane),
    );
    assert!(
        threshold_violations
            .iter()
            .any(|violation| violation.contains("job `rust-cx43` must pass"))
    );
    assert!(
        threshold_violations
            .iter()
            .any(|violation| violation.contains("job `rust-cx53` must pass"))
    );
}

#[test]
fn routed_rust_workflow_contract_accepts_swarm_shape() {
    let workflow = r#"
jobs:
  route:
    name: Route Ripr Rust Small
    timeout-minutes: 10
    steps:
      - name: Select runner
        env:
          GH_TOKEN: ${{ secrets.EM_RUNNER_READ_TOKEN || github.token }}
        run: |
          if [ "$EVENT_NAME" = "pull_request" ] && [ "$HEAD_REPO" != "$REPOSITORY" ]; then
            reason=fork_or_untrusted_pr
          fi
          idle() { printf '%s' "$runners" | jq -s -e --arg model "$1" --arg cap "$2" '[.[].runners[]?] | length > 0'; }
          gh api --paginate orgs/EffortlessMetrics/actions/runners
          reason=runner_api_failed
          reason=no_idle_runner
          reason=runner_capacity_unavailable
          reason=cx43_idle
          reason=cpx42_idle
          reason=cx53_idle
          echo rust-medium rust-16gb rust-large
  detect-docs-only:
    name: Detect Docs-Only Surface
    timeout-minutes: 10
  rust-cx43:
    if: needs.route.outputs.router_target == 'cx43'
    with:
      runner-config: '{"labels":["rust-medium"]}'
    timeout-minutes: 60
    outputs:
      scratch_status: ${{ steps.scratch.outputs.status }}
    env:
      CARGO_HOME: /mnt/ci-scratch/cargo-home/${{ github.run_id }}-${{ github.run_attempt }}
    steps:
      - name: Prepare toolchain temp
        run: mkdir -p "$TMPDIR"
      - name: Prepare scratch
        run: ci-disk-guard /mnt/ci-scratch 35
      - name: Proof route dry-run (advisory)
        run: cargo xtask proof route --base "$BASE_SHA" --head "$HEAD_SHA" || true
      - name: Clean scratch
        run: rm -rf "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TMPDIR"
  rust-cpx42:
    if: needs.route.outputs.router_target == 'cpx42'
    with:
      runner-config: '{"labels":["rust-medium","rust-16gb"]}'
    timeout-minutes: 60
    outputs:
      scratch_status: ${{ steps.scratch.outputs.status }}
    env:
      CARGO_HOME: /mnt/ci-scratch/cargo-home/${{ github.run_id }}-${{ github.run_attempt }}
    steps:
      - name: Prepare toolchain temp
        run: mkdir -p "$TMPDIR"
      - name: Prepare CPX42 scratch
        run: ci-disk-guard /mnt/ci-scratch 35
      - name: Proof route dry-run (advisory)
        run: cargo xtask proof route --base "$BASE_SHA" --head "$HEAD_SHA" || true
      - name: Clean scratch
        run: rm -rf "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TMPDIR"
  rust-cx53:
    if: needs.route.outputs.router_target == 'cx53'
    with:
      runner-config: '{"labels":["rust-large"]}'
    timeout-minutes: 60
    outputs:
      scratch_status: ${{ steps.scratch.outputs.status }}
    env:
      CARGO_HOME: /mnt/ci-scratch/cargo-home/${{ github.run_id }}-${{ github.run_attempt }}
    steps:
      - name: Prepare toolchain temp
        run: mkdir -p "$TMPDIR"
      - name: Prepare scratch
        run: ci-disk-guard /mnt/ci-scratch 50
      - name: Proof route dry-run (advisory)
        run: cargo xtask proof route --base "$BASE_SHA" --head "$HEAD_SHA" || true
      - name: Clean scratch
        run: rm -rf "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TMPDIR"
  rust-github:
    if: >-
      needs.detect-docs-only.result == 'success' &&
      needs.route.outputs.router_target == 'github' ||
      needs.rust-cx43.outputs.scratch_status == 'tempfail' ||
      needs.rust-cpx42.outputs.scratch_status == 'tempfail' ||
      needs.rust-cx53.outputs.scratch_status == 'tempfail'
    timeout-minutes: 90
    with:
      runner-config: '"ubuntu-latest"'
    steps:
      - name: Proof route dry-run (advisory)
        run: cargo xtask proof route --base "$BASE_SHA" --head "$HEAD_SHA" || true
  docs-gate:
    name: Ripr Docs Gate
    timeout-minutes: 20
    steps:
      - name: Upload docs-gate reports
        if: always()
      - name: Advisory reports
        if: success() && inputs.run-advisory-reports
      - name: Upload RIPR reports
        if: failure() || inputs.upload-success-artifacts
  result:
    name: Ripr Rust Small Result
    timeout-minutes: 10
    env:
      DOCS_DETECT_RESULT: ${{ needs.detect-docs-only.result }}
      CX43_SCRATCH_STATUS: ${{ needs.rust-cx43.outputs.scratch_status }}
    steps:
      - run: echo "disk-guard tempfailed; GitHub-hosted fallback succeeded"
      - run: echo "docs-surface detection result was $DOCS_DETECT_RESULT"
"#;
    let settings = r#"
repository:
  name: ripr-swarm
branches:
  - name: main
    protection:
      required_status_checks:
        contexts:
          - Ripr Rust Small Result
"#;
    let lane = r#"
[[lane]]
id = "routed-rust-small"
workflow = ".github/workflows/routed-rust.yml"
jobs = ["Ripr Rust Small Result"]
"#;

    assert!(
        routed_rust_workflow_contract_violations(workflow, Some(settings), Some(lane),).is_empty()
    );
}

#[test]
fn routed_rust_workflow_contract_rejects_unsafe_drift() {
    let workflow = r#"
jobs:
  route:
    steps:
      - run: |
          gh api repos/$REPOSITORY/actions/runners
          reason=no_idle_runner
  rust-cx53:
    if: github.event.pull_request.head.repo.full_name == github.repository
  result:
    name: Ripr Rust Small Result
"#;
    let settings = r#"
repository:
  name: ripr-swarm
branches:
  - name: main
    protection:
      required_status_checks:
        contexts:
          - Ripr Rust Small Result
          - Ripr Rust Small on CX53
"#;
    let lane = r#"
[[lane]]
id = "routed-rust-small"
workflow = ".github/workflows/routed-rust.yml"
jobs = ["Ripr Rust Small Result", "Ripr Rust Small on CX53"]
"#;

    let violations = routed_rust_workflow_contract_violations(workflow, Some(settings), Some(lane));

    assert!(violations.iter().any(|violation| {
        violation.contains("organization runner discovery")
            || violation.contains("repo-local runner discovery")
    }));
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("slurped idle runner query") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("hosted fallback docs-detection guard") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("self-hosted scratch tempfail output") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("CX43 tempfail fallback predicate") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("CPX42 tempfail fallback predicate") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("CX53 tempfail fallback predicate") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("normalized tempfail fallback result") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("normalized docs detection failure") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("Prepare toolchain temp") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("scratch CARGO_HOME") })
    );
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("guard pull_request events from forks") })
    );
    assert!(violations.iter().any(|violation| {
        violation.contains("must not require conditional implementation job")
    }));
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("must list only `Ripr Rust Small Result`") })
    );
    assert!(violations.iter().any(|violation| {
        violation.contains("must set an explicit `timeout-minutes` job deadline")
    }));
}

#[test]
fn rust_conversion_candidates_classify_unretained_non_rust_source() -> Result<(), String> {
    let Some(candidate) = super::super::non_rust_source_conversion_candidate("scripts/check.py")
    else {
        return Err("unretained non-Rust source should be a candidate".to_string());
    };

    assert_eq!(candidate.priority, "high");
    assert_eq!(
        candidate.kind,
        "non_rust_programming_without_retention_rule"
    );
    assert!(candidate.recommendation.contains("xtask"));
    Ok(())
}

#[test]
fn rust_conversion_candidates_identify_workflow_shell_logic() -> Result<(), String> {
    let block = super::super::RunBlock {
        line_number: 12,
        non_empty_lines: 1,
        text:
            "mkdir -p target/ripr/reports && git diff --name-only > target/ripr/reports/changes.txt"
                .to_string(),
    };

    let Some(candidate) =
        super::super::workflow_run_conversion_candidate(".github/workflows/pr-plan.yml", &block)
    else {
        return Err("repo-owned workflow shell should be a migration candidate".to_string());
    };

    assert_eq!(candidate.priority, "medium");
    assert_eq!(candidate.line, Some(12));
    assert_eq!(candidate.kind, "workflow_shell_logic");
    Ok(())
}

#[test]
fn rust_conversion_candidates_json_reports_counts_and_items() -> Result<(), String> {
    let candidates = sample_rust_conversion_candidates();

    let json = super::super::rust_conversion_candidates_json(&candidates)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("rust conversion JSON should parse: {err}"))?;

    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["actionable_count"], 2);
    assert_eq!(value["retained_count"], 1);
    assert_eq!(value["candidates"][0]["path"], "scripts/check.py");
    assert_eq!(value["candidates"][1]["line"], 42);
    assert_eq!(value["candidates"][2]["priority"], "retained");
    Ok(())
}

#[test]
fn rust_conversion_candidates_markdown_handles_empty_report() {
    let markdown = super::super::rust_conversion_candidates_markdown(&[]);

    assert!(markdown.contains("Actionable candidates: 0"));
    assert!(markdown.contains("Retained boundaries inspected: 0"));
    assert!(markdown.contains(
        "No unretained non-Rust source files or workflow shell migration candidates were found."
    ));
    assert!(markdown.contains("No retained non-Rust runtime or fixture boundaries were found."));
}

#[test]
fn rust_conversion_candidates_markdown_reports_actionable_and_retained_counts() {
    let candidates = sample_rust_conversion_candidates();

    let markdown = super::super::rust_conversion_candidates_markdown(&candidates);

    assert!(markdown.contains("Status: advisory"));
    assert!(markdown.contains("Actionable candidates: 2"));
    assert!(markdown.contains("Retained boundaries inspected: 1"));
    assert!(markdown.contains("workflow_shell_logic"));
    assert!(markdown.contains("VS Code extension TypeScript"));
    assert!(markdown.contains("cargo xtask check-file-policy"));
}

#[test]
fn rust_conversion_candidates_retain_the_portable_consumer_python_runtime() -> Result<(), String> {
    let Some(consumer) = super::super::non_rust_source_conversion_candidate(
        "tools/python/portable-ripr-consumer/run.py",
    ) else {
        return Err("portable consumer python should be assessed".to_string());
    };

    assert_eq!(consumer.priority, "retained");
    assert_eq!(consumer.kind, "retained_external_runtime");
    Ok(())
}

#[test]
fn rust_conversion_candidates_retains_external_workflow_tooling() -> Result<(), String> {
    let block = super::super::RunBlock {
        line_number: 21,
        non_empty_lines: 1,
        text: "npm --prefix editors/vscode run compile".to_string(),
    };

    let Some(candidate) =
        super::super::workflow_run_conversion_candidate(".github/workflows/ci.yml", &block)
    else {
        return Err("external workflow tooling should be retained".to_string());
    };

    assert_eq!(candidate.priority, "retained");
    assert_eq!(candidate.kind, "retained_external_runtime");
    assert_eq!(candidate.line, Some(21));
    assert!(candidate.current_surface.contains("external toolchain"));
    Ok(())
}

#[test]
fn rust_conversion_candidates_retains_fixture_and_editor_boundaries() -> Result<(), String> {
    let Some(fixture) = super::super::non_rust_source_conversion_candidate(
        "fixtures/python_boundary_gap/input/src/discount.py",
    ) else {
        return Err("fixture input should be assessed".to_string());
    };
    let Some(editor) =
        super::super::non_rust_source_conversion_candidate("editors/vscode/src/extension.ts")
    else {
        return Err("editor adapter should be assessed".to_string());
    };

    assert_eq!(fixture.priority, "retained");
    assert_eq!(fixture.kind, "retained_fixture_input");
    assert_eq!(editor.priority, "retained");
    assert_eq!(editor.kind, "retained_external_runtime");
    Ok(())
}

#[test]
fn rust_conversion_candidates_skip_xtask_workflow_calls() {
    let block = super::super::RunBlock {
        line_number: 34,
        non_empty_lines: 1,
        text: "cargo xtask reports index || true".to_string(),
    };

    assert!(
        super::super::workflow_run_conversion_candidate(".github/workflows/ci.yml", &block)
            .is_none()
    );
}

#[test]
fn rust_repair_complete_report_with_real_movement_does_not_bypass_interim_cap() {
    let report = serde_json::json!({
        "status": "complete",
        "eligible_attempt_count": 20,
        "requirements": {"minimum_attempts": 20},
        "movement_counts": {"closed": 1, "improved": 4}
    });
    let violation = super::super::rust_gap_repair_interim_cap_violation("usable", Some(&report));
    assert!(
        violation.as_deref().is_some_and(|violation| {
            violation.contains("exceeds the interim `usable alpha` cap")
                && violation.contains("`complete` with 20 eligible, 4 improved, and 1 closed")
                && violation.contains("trust report alone is not promotion authority")
                && violation.contains("#1702")
        }),
        "{violation:#?}"
    );
}

#[test]
fn sarif_policy_args_parse_mode_threshold_and_missing_baseline() -> Result<(), String> {
    let args = vec![
        "--current".to_string(),
        "current.sarif.json".to_string(),
        "--baseline".to_string(),
        "baseline.sarif.json".to_string(),
        "--mode".to_string(),
        "fail-on-new-warning".to_string(),
        "--threshold".to_string(),
        "note".to_string(),
        "--missing-baseline".to_string(),
        "error".to_string(),
    ];

    let parsed = parse_sarif_policy_args(&args)?;

    assert_eq!(parsed.current, PathBuf::from("current.sarif.json"));
    assert_eq!(parsed.baseline, Some(PathBuf::from("baseline.sarif.json")));
    assert_eq!(parsed.mode, SarifPolicyMode::FailOnNewWarning);
    assert_eq!(parsed.threshold, SarifPolicyThreshold::Note);
    assert_eq!(parsed.missing_baseline, SarifMissingBaseline::Error);
    Ok(())
}

#[test]
fn sarif_policy_flags_new_warning_result() {
    let current = vec![sarif_policy_result(
        "ripr.seam.weakly_gripped",
        "warning",
        "new",
    )];
    let baseline = vec![sarif_policy_result(
        "ripr.seam.weakly_gripped",
        "warning",
        "old",
    )];
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    assert_eq!(report.status, "new_results");
    assert_eq!(report.new_results.len(), 1);
    assert_eq!(report.new_results[0].fingerprint, "new");
}

#[test]
fn sarif_policy_ignores_note_when_threshold_warning() {
    let current = vec![sarif_policy_result("ripr.seam.opaque", "note", "new-note")];
    let baseline: Vec<SarifPolicyResult> = Vec::new();
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    assert_eq!(report.status, "pass");
    assert!(report.new_results.is_empty());
    assert_eq!(report.current_compared_results, 0);
}

#[test]
fn sarif_policy_missing_baseline_is_advisory_by_default() {
    let current = vec![sarif_policy_result("ripr.seam.ungripped", "warning", "new")];
    let report = build_sarif_policy_report(
        SarifPolicyMode::FailOnNewWarning,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("missing-baseline.sarif.json".to_string()),
        &current,
        None,
        true,
    );

    assert_eq!(report.status, "advisory_missing_baseline");
    assert!(report.new_results.is_empty());
    assert!(report.baseline_missing);
}

#[test]
fn sarif_policy_parses_results_and_skips_suppressions() -> Result<(), String> {
    let text = sarif_policy_test_sarif(vec![
        sarif_policy_json_result("ripr.finding.weakly_exposed", "warning", "visible", false),
        sarif_policy_json_result("ripr.finding.weakly_exposed", "warning", "hidden", true),
    ])?;
    let results = parse_sarif_policy_results(&text, "test SARIF")?;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].fingerprint, "visible");
    assert_eq!(results[0].uri, "src/lib.rs");
    assert_eq!(results[0].line, Some(12));
    Ok(())
}

#[test]
fn sarif_policy_passes_when_no_new_results() {
    let current = vec![sarif_policy_result(
        "ripr.finding.weakly_exposed",
        "warning",
        "same",
    )];
    let baseline = current.clone();
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    assert_eq!(report.status, "pass");
    assert!(report.new_results.is_empty());
    assert_eq!(report.current_compared_results, 1);
    assert_eq!(report.baseline_compared_results, 1);
}

#[test]
fn sarif_policy_report_json_and_markdown_are_structured() -> Result<(), String> {
    let current = vec![sarif_policy_result(
        "ripr.seam.weakly_gripped",
        "warning",
        "new",
    )];
    let baseline: Vec<SarifPolicyResult> = Vec::new();
    let report = build_sarif_policy_report(
        SarifPolicyMode::BaselineCheck,
        SarifPolicyThreshold::Warning,
        "current.sarif.json".to_string(),
        Some("baseline.sarif.json".to_string()),
        &current,
        Some(&baseline),
        false,
    );

    let json = sarif_policy_report_json(&report)?;
    let markdown = sarif_policy_report_markdown(&report);

    assert!(json.contains("\"schema_version\": \"0.1\""));
    assert!(json.contains("\"new_results_total\": 1"));
    assert!(markdown.contains("# ripr SARIF policy report"));
    assert!(markdown.contains("ripr.seam.weakly_gripped"));
    Ok(())
}

#[test]
fn scratch_gc_concurrency_policy_accepts_per_pool_matrix_queue() {
    let workflow = r#"
jobs:
  scratch-gc:
    strategy:
      matrix:
        pool: [cx43, cpx42, cx53]
    concurrency:
      group: scratch-gc-${{ github.repository }}-${{ matrix.pool }}
      cancel-in-progress: false
"#;

    assert!(
        scratch_gc_concurrency_violations(".github/workflows/scratch-gc.yml", workflow,).is_empty()
    );
}

#[test]
fn scratch_gc_concurrency_policy_rejects_workflow_queue() {
    let workflow = r#"
concurrency:
  group: scratch-gc-${{ github.repository }}
  cancel-in-progress: false
jobs:
  scratch-gc:
    strategy:
      matrix:
        pool: [cx43, cpx42, cx53]
    concurrency:
      group: scratch-gc-${{ github.repository }}-${{ matrix.pool }}
      cancel-in-progress: true
  unrelated:
    concurrency:
      cancel-in-progress: false
"#;

    let violations =
        scratch_gc_concurrency_violations(".github/workflows/scratch-gc.yml", workflow);

    assert_eq!(violations.len(), 2);
    assert!(violations[0].contains("workflow-level concurrency"));
    assert!(violations[1].contains("matrix.pool"));
}

#[test]
fn server_archive_qualification_workflow_is_sha_bound_and_credential_free() -> Result<(), String> {
    const QUALIFICATION_COMMIT_TYPE_COMMAND: &str = r#"git -C "${GITHUB_WORKSPACE}" cat-file -t"#;
    let workflow_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".github/workflows/server-archive-qualification.yml");
    let workflow = fs::read_to_string(&workflow_path)
        .map_err(|err| format!("read {}: {err}", workflow_path.display()))?;

    let validate = |candidate: &str| -> Result<(), String> {
        if candidate.contains("actions/checkout@")
            || candidate
                .matches("git init \"${GITHUB_WORKSPACE}\"")
                .count()
                != 4
            || candidate
                .matches("-c credential.helper= -c http.extraheader= fetch")
                .count()
                != 4
            || !candidate.contains("git -c credential.helper= -c http.extraheader= ls-remote")
            || candidate
                .matches("https://github.com/EffortlessMetrics/ripr-swarm.git")
                .count()
                != 5
        {
            return Err(
                "candidate source must use four isolated unauthenticated git fetches".to_owned(),
            );
        }
        if !candidate.contains("permissions:\n  contents: read")
            || candidate.contains("contents: write")
        {
            return Err("workflow permissions must be read-only contents".to_owned());
        }
        if !candidate.contains("candidate_sha=\"${CANDIDATE_SHA,,}\"")
            || !candidate.contains("git -c credential.helper= -c http.extraheader= ls-remote")
            || !candidate.contains(QUALIFICATION_COMMIT_TYPE_COMMAND)
            || !candidate.contains("test \"${CANDIDATE_TAG}\" = \"${tag}\"")
            || !candidate.contains("^ripr-release-[0-9]+\\.[0-9]+\\.[0-9]+$")
        {
            return Err("candidate tag must be strict, remote-bound, and commit-typed".to_owned());
        }
        if candidate.contains("rulesets?includes_parents=true")
            || candidate.contains("message=${message}")
            || candidate.contains("ruleset_shape=")
            || !candidate.contains(
                "https://api.github.com/repos/EffortlessMetrics/ripr-swarm/rulesets/${ruleset_id}",
            )
            || !candidate.contains("ruleset_id=\"20661783\"")
            || !candidate.contains("(.id == $expected_id)")
            || !candidate.contains(".target == \"tag\"")
            || !candidate.contains(".enforcement == \"active\"")
            || !candidate.contains("type == \"array\" and length == 1")
            || !candidate.contains(".conditions.ref_name.exclude")
            || !candidate.contains("index(\"update\")")
            || !candidate.contains("index(\"deletion\")")
        {
            return Err(
                "fixed ruleset endpoint or strict protected tag shape checks are missing"
                    .to_owned(),
            );
        }
        let targets = [
            "x86_64-pc-windows-msvc",
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ];
        let Some(matrix) = candidate
            .split("matrix:\n")
            .nth(1)
            .and_then(|rest| rest.split_once("\n\n    steps:"))
            .map(|(matrix, _)| matrix)
        else {
            return Err("matrix block delimiters were not found".to_owned());
        };
        if targets
            .iter()
            .filter(|target| matrix.matches(*target).count() == 1)
            .count()
            != 5
        {
            return Err(
                "the exact five-target matrix inventory is missing or duplicated".to_owned(),
            );
        }
        if candidate.contains("continue-on-error") {
            return Err("qualification steps must not continue on error".to_owned());
        }
        if candidate
            .matches("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a")
            .count()
            != 5
            || candidate
                .matches("actions/download-artifact@9000827ccba6bdab643e8b6fd33ac0654aef8333")
                .count()
                != 3
            || candidate.contains("release-upload-assets")
            || candidate.contains("gh release")
            || candidate.contains("gh api")
            || candidate.contains("GH_TOKEN")
            || candidate.contains("github.token")
            || candidate.contains("secrets.")
        {
            return Err(
                "workflow output must be Actions-artifact-only with no publication or secrets"
                    .to_owned(),
            );
        }
        for marker in [
            "expected-archive-entries.txt",
            "diff -u expected-archive-entries.txt actual-archive-entries.txt",
            "Compare-Object $expectedEntries $actualEntries",
            "expected-dist-files.txt",
            "diff -u expected-dist-files.txt actual-dist-files.txt",
            "archive: tar.gz",
            "archive: zip",
            "refs/tags/${tag}^{}",
            "resolved_sha=\"${peeled_sha:-${remote_sha}}\"",
            "actual_sha=\"$(sha256sum",
            "assets[$target].sha256",
            "(.assets | keys == ($targets | sort))",
            "curl --silent --show-error --location --connect-timeout 10 --max-time 20 --dump-header",
            "ruleset_transport_ok",
            "^[0-9]{1,20}$",
            "ruleset verification request failed",
            "ruleset verification shape mismatch",
            "ruleset_safe_summary",
            "json_object",
            "include_count",
            "exclude_count",
            "rule_count",
            "rate_limit_remaining",
            "rate_limit_reset",
            "response_sha256",
            "ruleset_mode",
            "ruleset_source_url",
            "ruleset_response_sha256",
            "ruleset_id",
            "--arg repository \"${REPOSITORY}\"",
            "release_assets_created: false",
            "archive_readback_verified = $true",
            "(.archive_readback_verified == true) and",
            "archive_readback_verified: true",
            "RECEIPT_INSTRUMENT_REPOSITORY: ${{ job.workflow_repository }}",
            "RECEIPT_INSTRUMENT_SHA: ${{ job.workflow_sha }}",
            "endswith(\" / \" + $suffix)",
            "cargo xtask release-server-archive",
            "os: ubuntu-22.04\n",
            "os: ubuntu-22.04-arm\n",
            "GLIBC_FLOOR: \"2.34\"",
            "sed -n '/^Version needs section/,/^Version .* section/p'",
            "if [ \"${highest}\" != \"${GLIBC_FLOOR}\" ]; then",
            "requires glibc ${required}, above the ${GLIBC_FLOOR} floor\"\n            exit 1\n          fi",
            "needs a non-numeric glibc version that the ${GLIBC_FLOOR} floor cannot bound\"\n            exit 1\n          fi",
            "if grep -v -x -E '[0-9][0-9.]*' glibc-need-names.txt; then",
            "      - name: Verify Linux glibc floor\n        if: runner.os == 'Linux'\n",
        ] {
            if !candidate.contains(marker) {
                return Err(format!(
                    "structured runtime inventory marker missing `{marker}`"
                ));
            }
        }
        Ok(())
    };

    validate(&workflow)?;
    for (name, broken) in [
        (
            "writable permissions",
            workflow.replacen("contents: read", "contents: write", 1),
        ),
        (
            "credential clearing",
            workflow.replacen(
                "-c credential.helper= -c http.extraheader= fetch",
                "fetch",
                1,
            ),
        ),
        (
            "fixed public repository",
            workflow.replacen(
                "https://github.com/EffortlessMetrics/ripr-swarm.git",
                "https://github.com/${REPOSITORY}.git",
                1,
            ),
        ),
        (
            "publication",
            workflow.replacen(
                "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
                "actions/upload-release-asset@deadbeef",
                1,
            ),
        ),
        (
            "tag protection",
            workflow.replacen("index(\"deletion\")", "index(\"create\")", 1),
        ),
        (
            "manifest inventory",
            workflow.replacen(
                "diff -u expected-dist-files.txt actual-dist-files.txt",
                "true",
                1,
            ),
        ),
        (
            "tag syntax",
            workflow.replacen(
                "^ripr-release-[0-9]+\\.[0-9]+\\.[0-9]+$",
                "^ripr-release-.*$",
                1,
            ),
        ),
        (
            "matrix inventory",
            workflow.replacen(
                "          - target: x86_64-pc-windows-msvc\n",
                "          - target: x86_64-pc-windows-msvc\n          - target: x86_64-pc-windows-msvc\n",
                1,
            ),
        ),
        (
            "required marker",
            workflow.replacen(
                "release_assets_created: false",
                "release_assets_created: true",
                1,
            ),
        ),
        (
            "authenticated API",
            workflow.replacen("curl --silent --show-error", "gh api", 1),
        ),
        (
            "bypassed delegated server builder",
            workflow.replace(
                "cargo xtask release-server-archive",
                "cargo build --release -p ripr",
            ),
        ),
        (
            "newer glibc runner",
            workflow.replacen("os: ubuntu-22.04\n", "os: ubuntu-latest\n", 1),
        ),
        (
            "newer arm glibc runner",
            workflow.replacen("os: ubuntu-22.04-arm\n", "os: ubuntu-24.04-arm\n", 1),
        ),
        (
            "glibc floor that cannot fail",
            workflow.replacen(
                "requires glibc ${required}, above the ${GLIBC_FLOOR} floor\"\n            exit 1",
                "requires glibc ${required}, above the ${GLIBC_FLOOR} floor\"\n            true",
                1,
            ),
        ),
        (
            "glibc floor skipped on Linux",
            workflow.replacen(
                "      - name: Verify Linux glibc floor\n        if: runner.os == 'Linux'\n",
                "      - name: Verify Linux glibc floor\n        if: runner.os == 'Solaris'\n",
                1,
            ),
        ),
        (
            "named glibc need that cannot fail",
            workflow.replacen(
                "needs a non-numeric glibc version that the ${GLIBC_FLOOR} floor cannot bound\"\n            exit 1",
                "needs a non-numeric glibc version that the ${GLIBC_FLOOR} floor cannot bound\"\n            true",
                1,
            ),
        ),
        (
            "named glibc needs ignored",
            workflow.replacen(
                "if grep -v -x -E '[0-9][0-9.]*' glibc-need-names.txt; then",
                "if false; then",
                1,
            ),
        ),
        (
            "private glibc need allowed",
            workflow.replacen(
                "if grep -v -x -E '[0-9][0-9.]*' glibc-need-names.txt; then",
                "if grep -v -x -E '[0-9][0-9.]*|PRIVATE' glibc-need-names.txt; then",
                1,
            ),
        ),
        (
            "glibc floor continues on error",
            workflow.replacen(
                "      - name: Verify Linux glibc floor\n",
                "      - name: Verify Linux glibc floor\n        continue-on-error: true\n",
                1,
            ),
        ),
        (
            "raised glibc floor",
            workflow.replacen("GLIBC_FLOOR: \"2.34\"", "GLIBC_FLOOR: \"2.39\"", 1),
        ),
        (
            "token credential",
            workflow.replacen("ruleset_mode", "GH_TOKEN", 1),
        ),
        (
            "caller SHA receipt instrument",
            workflow.replacen(
                "RECEIPT_INSTRUMENT_SHA: ${{ job.workflow_sha }}",
                "RECEIPT_INSTRUMENT_SHA: ${{ github.sha }}",
                1,
            ),
        ),
        (
            "exact-only matrix job lookup",
            workflow.replacen(
                "select(.name == $suffix or (.name | endswith(\" / \" + $suffix)))",
                "select(.name == $suffix)",
                1,
            ),
        ),
        (
            "implicit archive readback",
            workflow.replacen(
                "archive_readback_verified: true",
                "archive_shape_verified: true",
                1,
            ),
        ),
        (
            "false archive readback identity",
            workflow.replacen(
                "archive_readback_verified = $true",
                "archive_readback_verified = $false",
                1,
            ),
        ),
    ] {
        if validate(&broken).is_ok() {
            return Err(format!("negative fixture `{name}` was not rejected"));
        }
    }
    Ok(())
}

#[test]
fn server_archive_ruleset_shape_fixtures_are_strict_and_discriminating() -> Result<(), String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest should have a repository parent".to_string())?;
    let workflow =
        fs::read_to_string(repo_root.join(".github/workflows/server-archive-qualification.yml"))
            .map_err(|error| format!("failed to read qualification workflow: {error}"))?;
    let predicate_marker = "if jq -e --argjson expected_id \"${ruleset_id}\" '";
    let predicate_start = workflow
        .find(predicate_marker)
        .map(|offset| offset + predicate_marker.len())
        .ok_or_else(|| "qualification workflow jq predicate marker is missing".to_string())?;
    let predicate_end = workflow[predicate_start..]
        .find("' \"${ruleset_file}\"")
        .ok_or_else(|| "qualification workflow jq predicate terminator is missing".to_string())?;
    let predicate = workflow[predicate_start..predicate_start + predicate_end].trim();
    if predicate.is_empty() {
        return Err("qualification workflow jq predicate is empty".to_string());
    }

    let fixture_root = temp_dir("server-ruleset-jq");
    let run_predicate = |ruleset: &Value, name: &str| -> Result<bool, String> {
        let path = fixture_root.join(name);
        let input = serde_json::to_vec(ruleset)
            .map_err(|error| format!("failed to serialize ruleset fixture: {error}"))?;
        fs::write(&path, input)
            .map_err(|error| format!("failed to write ruleset fixture: {error}"))?;
        let path_text = path
            .to_str()
            .ok_or_else(|| "ruleset fixture path was not UTF-8".to_string())?;
        let args = vec![
            "-e".to_string(),
            "--argjson".to_string(),
            "expected_id".to_string(),
            "20661783".to_string(),
            predicate.to_string(),
            path_text.to_string(),
        ];
        let _cwd_guard = super::super::acquire_test_cwd_read_guard();
        command_success_owned("jq", &args)
    };

    let positive: Value = serde_json::from_str(include_str!(
        "../../../fixtures/release_control/server-ruleset-20661783.json"
    ))
    .map_err(|error| format!("failed to parse positive ruleset fixture: {error}"))?;
    if !run_predicate(&positive, "positive.json")? {
        return Err("workflow jq predicate rejected the actual positive fixture".to_string());
    }

    let negative: Value = serde_json::from_str(include_str!(
        "../../../fixtures/release_control/server-ruleset-20661783-invalid.json"
    ))
    .map_err(|error| format!("failed to parse negative ruleset fixture: {error}"))?;
    if run_predicate(&negative, "negative.json")? {
        return Err("workflow jq predicate accepted the negative fixture".to_string());
    }

    let mut wrong_exclude = positive.clone();
    wrong_exclude["conditions"]["ref_name"]["exclude"] = serde_json::json!(["refs/tags/old"]);
    if run_predicate(&wrong_exclude, "wrong-exclude.json")? {
        return Err("workflow jq predicate accepted a nonempty exclude mutation".to_string());
    }

    let mut wrong_id = positive.clone();
    wrong_id["id"] = serde_json::json!(20_661_784);
    if run_predicate(&wrong_id, "wrong-id.json")? {
        return Err("workflow jq predicate accepted an unexpected ruleset id".to_string());
    }

    let wrong_shape = serde_json::json!({"id": 20_661_783, "target": "tag"});
    if run_predicate(&wrong_shape, "wrong-shape.json")? {
        return Err("workflow jq predicate accepted a malformed ruleset shape".to_string());
    }
    Ok(())
}

#[test]
fn server_archive_terminal_job_name_filter_accepts_direct_and_reusable_names() -> Result<(), String>
{
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest should have a repository parent".to_string())?;
    let workflow =
        fs::read_to_string(repo_root.join(".github/workflows/server-archive-qualification.yml"))
            .map_err(|error| format!("failed to read qualification workflow: {error}"))?;
    let marker = "jq -r --arg suffix \"build and qualify ${target}\" '";
    let program_start = workflow
        .find(marker)
        .map(|offset| offset + marker.len())
        .ok_or_else(|| "terminal job-name jq marker is missing".to_string())?;
    let program_end = workflow[program_start..]
        .find("' \"${response}\"")
        .ok_or_else(|| "terminal job-name jq terminator is missing".to_string())?;
    let program = workflow[program_start..program_start + program_end].trim();
    if program.is_empty() {
        return Err("terminal job-name jq program is empty".to_string());
    }
    if !program.contains(".name == $suffix") || !program.contains("endswith(\" / \" + $suffix)") {
        return Err(
            "terminal job-name jq program must accept direct and reusable caller names".to_string(),
        );
    }

    let suffix = "build and qualify x86_64-unknown-linux-gnu";
    let fixture_root = temp_dir("server-terminal-job-names");
    let run = |names: &[&str]| -> Result<String, String> {
        let jobs: Vec<Value> = names
            .iter()
            .map(|name| serde_json::json!({ "name": name, "conclusion": "success" }))
            .collect();
        let path = fixture_root.join(format!("jobs-{}.json", jobs.len()));
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({ "jobs": jobs }))
                .map_err(|error| format!("serialize workflow-jobs fixture: {error}"))?,
        )
        .map_err(|error| format!("write workflow-jobs fixture: {error}"))?;
        let path_text = path
            .to_str()
            .ok_or_else(|| "workflow-jobs fixture path was not UTF-8".to_string())?;
        let _cwd_guard = super::super::acquire_test_cwd_read_guard();
        let stdout =
            crate::run_output("jq", &["-r", "--arg", "suffix", suffix, program, path_text])
                .map_err(|error| format!("run terminal job-name jq filter: {error}"))?;
        Ok(stdout.trim().to_string())
    };

    if run(&[suffix])? != "success" {
        return Err(
            "terminal job-name jq filter rejected the direct dispatch job name".to_string(),
        );
    }
    let reusable = format!("rehearse / {suffix}");
    if run(&[reusable.as_str()])? != "success" {
        return Err(
            "terminal job-name jq filter rejected the reusable-caller job name".to_string(),
        );
    }
    if run(&["build and qualify"])? != "missing" {
        return Err("terminal job-name jq filter matched an unrelated job name".to_string());
    }
    if run(&[suffix, reusable.as_str()])? != "duplicate" {
        return Err(
            "terminal job-name jq filter did not report a duplicated target job".to_string(),
        );
    }
    ignore_remove_dir_all(&fixture_root);
    Ok(())
}

#[test]
fn shape_rewrite_notice_lists_rewritten_files() {
    assert_eq!(super::super::shape_rewrite_notice(&[]), None);
    let notice = super::super::shape_rewrite_notice(&[
        "policy/a.txt".to_string(),
        ".ripr/b.txt".to_string(),
    ]);
    assert!(
        notice.is_some(),
        "expected a rewrite notice for two sorted files"
    );
    if let Some(notice) = notice {
        assert!(notice.contains("2 tracked allowlist file(s)"));
        assert!(notice.contains("policy/a.txt"));
        assert!(notice.contains(".ripr/b.txt"));
        assert!(notice.contains("review before `git add`"));
    }
}

#[test]
fn should_scan_static_language_path_combines_candidate_check_and_allowlist() {
    let allowlist = allowlist_with_docs_markdown_globs();

    // Non-candidate files (not in the watched extensions list) are never
    // scanned, regardless of allowlist contents.
    assert!(!should_scan_static_language_path(
        &allowlist,
        "docs/generated/output.png"
    ));
    assert!(!should_scan_static_language_path(
        &allowlist,
        "fixtures/boundary_gap/expected/output.bin"
    ));

    // Editor-extension source is part of the static-language policy surface.
    assert!(should_scan_static_language_path(
        &allowlist,
        "editors/vscode/src/extension.ts"
    ));
    assert!(should_scan_static_language_path(
        &allowlist,
        "editors/vscode/src/component.tsx"
    ));
    assert!(should_scan_static_language_path(
        &allowlist,
        "editors/vscode/scripts/package.js"
    ));
    assert!(should_scan_static_language_path(
        &allowlist,
        "editors/vscode/scripts/component.jsx"
    ));

    // Candidate files covered by an exact allowlist entry are not scanned.
    assert!(!should_scan_static_language_path(&allowlist, "AGENTS.md"));

    // Candidate files covered by the docs Markdown globs are not scanned.
    assert!(!should_scan_static_language_path(
        &allowlist,
        "docs/BADGE_POLICY.md"
    ));
    assert!(!should_scan_static_language_path(
        &allowlist,
        "docs/specs/RIPR-SPEC-0004-test-efficiency.md"
    ));

    // Non-allowlisted candidate Rust source IS scanned.
    assert!(should_scan_static_language_path(
        &allowlist,
        "crates/ripr/src/lib.rs"
    ));
    assert!(should_scan_static_language_path(
        &allowlist,
        "fixtures/boundary_gap/input/src/lib.rs"
    ));
}

#[test]
fn should_skip_path_ignores_agent_worktree_state() {
    // Nested agent worktrees under `.claude/worktrees/` contain full repo
    // copies; FS-walking checks must not descend into them and double-count
    // or false-fail policy gates (#1030). `.claude/` is gitignored, so
    // `git ls-files`-based checks are already safe; this guards the
    // `collect_files()` filesystem walkers.
    assert!(should_skip_path(".claude"));
    assert!(should_skip_path(".claude/settings.local.json"));
    assert!(should_skip_path(
        ".claude/worktrees/agent-1/crates/ripr/src/lib.rs"
    ));
    // A real source path that merely contains the substring elsewhere must
    // still be scanned.
    assert!(!should_skip_path("crates/ripr/src/lib.rs"));
    assert!(!should_skip_path("docs/claude-notes.md"));
}

#[test]
fn should_skip_path_ignores_generated_editor_test_artifacts() {
    assert!(should_skip_path("editors/vscode/.vscode-test"));
    assert!(should_skip_path(
        "editors/vscode/.vscode-test/vscode-win32-x64-archive-1.119.0/resources/app/package.json"
    ));
    assert!(should_skip_path(
        "editors/vscode/node_modules/vscode/package.json"
    ));
    assert!(should_skip_path("editors/vscode/out/src/extension.js"));
    assert!(should_skip_path("editors/vscode/dist/ripr-0.3.0.vsix"));
    assert!(!should_skip_path("editors/vscode/src/config.ts"));
}

#[test]
fn should_skip_path_ignores_nested_target_artifacts() {
    assert!(should_skip_path("crates/ripr/target"));
    assert!(should_skip_path(
        "crates/ripr/target/ripr/review/run-receipt.json"
    ));
    assert!(!should_skip_path("crates/ripr/src/targeting.rs"));
}

#[test]
fn sorted_allowlist_content_preserves_header_and_sorts_entries() {
    let input = "# Header\n# More\n\nz|kind|owner|reason\na|kind|owner|reason\n";
    let sorted = sorted_allowlist_content(input);

    assert_eq!(
        sorted,
        "# Header\n# More\n\na|kind|owner|reason\nz|kind|owner|reason\n"
    );
}

#[test]
fn sorted_capability_blocks_content_preserves_prefix_and_whole_blocks() {
    let prefix = "# Capabilities\n\n# human notes stay above generated ordering\n\n";
    let block_b = "[[capability]]\nid = \"beta_capability\"\nspec = \"RIPR-SPEC-0002\"\n# block comment stays with beta\nname = \"Beta\"\n\n";
    let block_a = "[[capability]]\nid = \"alpha_capability\"\nspec = \"RIPR-SPEC-0001\"\nname = \"Alpha\"\nfixtures = [\n  \"fixtures/alpha\",\n]\n\n";
    let input = format!("{prefix}{block_b}{block_a}");
    let sorted = sorted_capability_blocks_content(&input);

    assert!(sorted.starts_with(prefix));
    assert_eq!(sorted, format!("{prefix}{block_a}{block_b}"));
}

#[test]
fn sorted_capability_blocks_content_skips_unsafe_manifests() {
    let duplicate = "[[capability]]\nid = \"alpha\"\nspec = \"RIPR-SPEC-0001\"\n\n[[capability]]\nid = \"alpha\"\nspec = \"RIPR-SPEC-0002\"\n";
    let missing = "[[capability]]\nname = \"A\"\nspec = \"RIPR-SPEC-0001\"\n\n[[capability]]\nid = \"beta\"\nspec = \"RIPR-SPEC-0002\"\n";
    let invalid_id = "[[capability]]\nid = \"NotSnake\"\nspec = \"RIPR-SPEC-0001\"\n\n[[capability]]\nid = \"beta\"\nspec = \"RIPR-SPEC-0002\"\n";
    let invalid_spec = "[[capability]]\nid = \"beta\"\nspec = \"not-a-spec\"\n\n[[capability]]\nid = \"alpha\"\nspec = \"RIPR-SPEC-0002\"\n";
    let duplicate_key = "[[capability]]\nid = \"beta\"\nspec = \"RIPR-SPEC-0002\"\nevidence = []\nevidence = [\n  \"example\",\n]\n\n[[capability]]\nid = \"alpha\"\nspec = \"RIPR-SPEC-0001\"\n";
    let unterminated_array = "[[capability]]\nid = \"beta\"\nspec = \"RIPR-SPEC-0002\"\nevidence = [\n  \"example\",\n\n[[capability]]\nid = \"alpha\"\nspec = \"RIPR-SPEC-0001\"\n";
    let malformed_inline_array = "[[capability]]\nid = \"beta\"\nspec = \"RIPR-SPEC-0002\"\nevidence = [example]\n\n[[capability]]\nid = \"alpha\"\nspec = \"RIPR-SPEC-0001\"\n";

    assert_eq!(sorted_capability_blocks_content(duplicate), duplicate);
    assert_eq!(sorted_capability_blocks_content(missing), missing);
    assert_eq!(sorted_capability_blocks_content(invalid_id), invalid_id);
    assert_eq!(sorted_capability_blocks_content(invalid_spec), invalid_spec);
    assert_eq!(
        sorted_capability_blocks_content(duplicate_key),
        duplicate_key
    );
    assert_eq!(
        sorted_capability_blocks_content(unterminated_array),
        unterminated_array
    );
    assert_eq!(
        sorted_capability_blocks_content(malformed_inline_array),
        malformed_inline_array
    );
}

#[test]
fn sorted_capability_blocks_content_sorts_blocks_by_spec_then_id() {
    let prefix = "# Capabilities\n\n";
    let block_c = "[[capability]]\n# keep c\nid = \"alpha_capability\"\nspec = \"RIPR-SPEC-0001\"\nname = \"Alpha\"\n\n";
    let block_b = "[[capability]]\n# keep b\nid = \"zeta_capability\"\nspec = \"RIPR-SPEC-0001\"\nname = \"Zeta\"\n\n";
    let block_a = "[[capability]]\n# keep a\nid = \"beta_capability\"\nspec = \"RIPR-SPEC-0002\"\nname = \"Beta\"\n\n";
    let input = format!("{prefix}{block_a}{block_b}{block_c}");
    let sorted = sorted_capability_blocks_content(&input);

    assert_eq!(sorted, format!("{prefix}{block_c}{block_b}{block_a}"));
}

#[test]
fn sorted_command_catalog_content_skips_entries_without_quoted_command() {
    let input = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            SHAPE_COMMAND,
            "mutating",
            "source files",
            false,
            "Runs deterministic shaping.",
        ),
        command_entry(
            "check-pr",
            "non_mutating_check",
            "target/ripr/reports",
            false,
            "Review-ready gate.",
        ),
    ]
}
"#;

    assert_eq!(sorted_command_catalog_content(input), input);
}

#[test]
fn sorted_command_catalog_content_skips_non_catalog_or_incomplete_catalogs() {
    let no_catalog = "pub(crate) fn unrelated() {}\n";
    let incomplete = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            "shape",
            "mutating",
            "source files",
            false,
            "Runs deterministic shaping.",
        ),
"#;
    let non_entry_body = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        CommandCatalogEntry {
            command: "shape",
            mutability: "mutating",
            writes: "source files",
            judgment_required: false,
            notes: "Runs deterministic shaping.",
        },
    ]
}
"#;
    let single_entry = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            "shape",
            "mutating",
            "source files",
            false,
            "Runs deterministic shaping.",
        ),
    ]
}
"#;

    assert_eq!(sorted_command_catalog_content(no_catalog), no_catalog);
    assert_eq!(sorted_command_catalog_content(incomplete), incomplete);
    assert_eq!(
        sorted_command_catalog_content(non_entry_body),
        non_entry_body
    );
    assert_eq!(sorted_command_catalog_content(single_entry), single_entry);
}

#[test]
fn sorted_command_catalog_content_skips_unknown_or_malformed_catalogs() {
    let unknown = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            "not-a-real-command",
            "report_only",
            "target/ripr/reports",
            false,
            "Unknown command.",
        ),
        command_entry(
            "shape",
            "mutating",
            "source files",
            false,
            "Runs deterministic shaping.",
        ),
    ]
}
"#;
    let malformed = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            "check-pr",
            "non_mutating_check",
            "target/ripr/reports",
            false,
            "Review-ready gate.",
        )
    ]
}
"#;

    assert_eq!(sorted_command_catalog_content(unknown), unknown);
    assert_eq!(sorted_command_catalog_content(malformed), malformed);
}

#[test]
fn sorted_command_catalog_content_sorts_commands_with_shared_roots() {
    let input = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            "goldens bless <name> --reason <reason>",
            "mutating",
            "fixtures/**/expected/**",
            true,
            "Updates golden expected outputs.",
        ),
        command_entry(
            "goldens check",
            "non_mutating_check",
            "target/ripr/reports/goldens.md",
            false,
            "Checks golden drift.",
        ),
    ]
}
"#;
    let sorted = sorted_command_catalog_content(input);

    assert!(
        sorted
            .find("\"goldens check\"")
            .zip(sorted.find("\"goldens bless <name> --reason <reason>\""))
            .is_some_and(|(check, bless)| check < bless)
    );
}

#[test]
fn sorted_command_catalog_content_sorts_entries_by_help_order() {
    let input = r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            "check-pr",
            "non_mutating_check",
            "target/ripr/reports",
            false,
            "Review-ready gate.",
        ),
        command_entry(
            "shape",
            "mutating",
            "source files",
            false,
            "Runs deterministic shaping.",
        ),
    ]
}
"#;
    let sorted = sorted_command_catalog_content(input);

    assert!(
        sorted
            .find("\"shape\"")
            .zip(sorted.find("\"check-pr\""))
            .is_some_and(|(shape, check_pr)| shape < check_pr)
    );
}

#[test]
fn sorted_markdown_index_table_content_sorts_index_rows() {
    let input = "# Specs\n\n## Index\n\n| Spec | Status | Topic |\n| --- | --- | --- |\n| [RIPR-SPEC-0002](RIPR-SPEC-0002-b.md) | proposed | B |\n| [RIPR-SPEC-0001](RIPR-SPEC-0001-a.md) | accepted | A |\n\nAfter.\n";
    let sorted = sorted_markdown_index_table_content(input);

    assert_eq!(
        sorted,
        "# Specs\n\n## Index\n\n| Spec | Status | Topic |\n| --- | --- | --- |\n| [RIPR-SPEC-0001](RIPR-SPEC-0001-a.md) | accepted | A |\n| [RIPR-SPEC-0002](RIPR-SPEC-0002-b.md) | proposed | B |\n\nAfter.\n"
    );
}

#[test]
fn sorted_traceability_behavior_blocks_content_skips_unsafe_manifests() {
    let duplicate =
        "[[behavior]]\nid = \"RIPR-SPEC-0001\"\n\n[[behavior]]\nid = \"RIPR-SPEC-0001\"\n";
    let missing = "[[behavior]]\nname = \"A\"\n\n[[behavior]]\nid = \"RIPR-SPEC-0002\"\n";
    let invalid = "[[behavior]]\nid = \"not-a-spec\"\n\n[[behavior]]\nid = \"RIPR-SPEC-0002\"\n";
    let duplicate_key = "[[behavior]]\nid = \"RIPR-SPEC-0002\"\ntests = []\ntests = [\n  \"example\",\n]\n\n[[behavior]]\nid = \"RIPR-SPEC-0001\"\n";
    let unterminated_array = "[[behavior]]\nid = \"RIPR-SPEC-0002\"\ntests = [\n  \"example\",\n\n[[behavior]]\nid = \"RIPR-SPEC-0001\"\n";
    let malformed_inline_array = "[[behavior]]\nid = \"RIPR-SPEC-0002\"\ntests = [\"example\"\n\n[[behavior]]\nid = \"RIPR-SPEC-0001\"\n";

    assert_eq!(
        sorted_traceability_behavior_blocks_content(duplicate),
        duplicate
    );
    assert_eq!(
        sorted_traceability_behavior_blocks_content(missing),
        missing
    );
    assert_eq!(
        sorted_traceability_behavior_blocks_content(invalid),
        invalid
    );
    assert_eq!(
        sorted_traceability_behavior_blocks_content(duplicate_key),
        duplicate_key
    );
    assert_eq!(
        sorted_traceability_behavior_blocks_content(unterminated_array),
        unterminated_array
    );
    assert_eq!(
        sorted_traceability_behavior_blocks_content(malformed_inline_array),
        malformed_inline_array
    );
}

#[test]
fn sorted_traceability_behavior_blocks_content_sorts_blocks_by_id() {
    let prefix = "# Traceability\n\n";
    let block_b = "[[behavior]]\n# keep b\nid = \"RIPR-SPEC-0002\"\nname = \"B\"\n\n";
    let block_a = "[[behavior]]\n# keep a\nid = \"RIPR-SPEC-0001\"\nname = \"A\"\n\n";
    let input = format!("{prefix}{block_b}{block_a}");
    let sorted = sorted_traceability_behavior_blocks_content(&input);

    assert_eq!(sorted, format!("{prefix}{block_a}{block_b}"));
}

#[test]
fn spec_id_helpers_accept_only_ripr_spec_ids() {
    assert!(is_spec_id("RIPR-SPEC-0001"));
    assert!(!is_spec_id("RIPR-SPEC-001"));
    assert!(!is_spec_id("SPEC-0001"));
    assert_eq!(
        spec_id_from_path(Path::new("docs/specs/RIPR-SPEC-0004-predicate-boundary.md")),
        Some("RIPR-SPEC-0004".to_string())
    );
    assert_eq!(spec_id_from_path(Path::new("docs/specs/README.md")), None);
}

#[test]
fn spec_ids_in_text_extracts_four_digit_ids_only() {
    let ids = spec_ids_in_text(
        "RIPR-SPEC-0001 RIPR-SPEC-001 RIPR-SPEC-9999 RIPR-SPEC-abcd RIPR-SPEC-12345",
    );
    assert!(ids.contains("RIPR-SPEC-0001"));
    assert!(ids.contains("RIPR-SPEC-9999"));
    assert!(!ids.contains("RIPR-SPEC-001"));
    assert!(!ids.contains("RIPR-SPEC-1234"));
}

#[test]
fn spec_numbering_detects_duplicate_spec_ids() -> Result<(), String> {
    let root = temp_dir("spec-numbering-duplicate");
    write(
        &root.join("docs/specs/RIPR-SPEC-0001-one.md"),
        "# RIPR-SPEC-0001: One\n",
    );
    write(
        &root.join("docs/specs/RIPR-SPEC-0001-two.md"),
        "# RIPR-SPEC-0001: Two\n",
    );
    write(
        &root.join("docs/specs/README.md"),
        "| Spec | Status | Title |\n| --- | --- | --- |\n| [RIPR-SPEC-0001](RIPR-SPEC-0001-one.md) | proposed | One |\n",
    );

    let violations = spec_numbering_violations(&root)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("RIPR-SPEC-0001 is used by multiple spec files"))
    );
    Ok(())
}

#[test]
fn spec_numbering_detects_missing_index_and_reference_drift() -> Result<(), String> {
    let root = temp_dir("spec-numbering-drift");
    write(
        &root.join("docs/specs/RIPR-SPEC-0001-one.md"),
        "# RIPR-SPEC-0001: One\n",
    );
    write(
        &root.join("docs/specs/RIPR-SPEC-0002-two.md"),
        "# RIPR-SPEC-0002: Two\n",
    );
    write(
        &root.join("docs/specs/README.md"),
        "| Spec | Status | Title |\n| --- | --- | --- |\n| [RIPR-SPEC-0001](RIPR-SPEC-0001-one.md) | proposed | One |\n| [RIPR-SPEC-9999](missing.md) | proposed | Missing |\n",
    );
    write(
        &root.join(".ripr/traceability.toml"),
        "spec = \"RIPR-SPEC-9998\"\n",
    );
    write(
        &root.join("metrics/capabilities.toml"),
        "spec = \"RIPR-SPEC-9997\"\n",
    );
    write(
        &root.join("docs/CAPABILITY_MATRIX.md"),
        "| Capability | RIPR-SPEC-9996 |\n",
    );

    let violations = spec_numbering_violations(&root)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("missing index link `[RIPR-SPEC-0002]"))
    );
    assert!(violations.iter().any(|violation| {
        violation.contains("README.md references missing spec `RIPR-SPEC-9999`")
    }));
    assert!(violations.iter().any(|violation| {
        violation.contains(".ripr/traceability.toml references missing spec `RIPR-SPEC-9998`")
    }));
    assert!(violations.iter().any(|violation| {
        violation.contains("metrics/capabilities.toml references missing spec `RIPR-SPEC-9997`")
    }));
    assert!(violations.iter().any(|violation| {
        violation.contains("docs/CAPABILITY_MATRIX.md references missing spec `RIPR-SPEC-9996`")
    }));
    Ok(())
}

#[test]
fn spec_system_profile_is_current_v2_without_goal_root() -> Result<(), String> {
    let profile =
        std::fs::read_to_string(slice_test_repo_root().join(".allow/profiles/spec-system.toml"))
            .map_err(|err| format!("read spec-system profile: {err}"))?;
    if !profile.contains("generation = \"current-v2\"") {
        return Err("profile must declare current-v2 generation".to_string());
    }
    if profile.lines().any(|line| line.starts_with("goals =")) {
        return Err("current-v2 profile must not declare a goals root".to_string());
    }
    if profile.contains("active_goal_required") {
        return Err("current-v2 profile must not require an active goal".to_string());
    }
    Ok(())
}

#[test]
fn specs_command_rejects_missing_or_unknown_subcommand() {
    let missing = match specs(&[]) {
        Ok(()) => "unexpected pass".to_string(),
        Err(err) => err,
    };
    assert!(missing.contains("cargo xtask specs next"));

    let unknown = match specs(&["current".to_string()]) {
        Ok(()) => "unexpected pass".to_string(),
        Err(err) => err,
    };
    assert!(unknown.contains("unknown specs command"));
    assert!(unknown.contains("cargo xtask specs next"));
}

#[test]
fn specs_next_id_is_mechanical() {
    assert_eq!(
        next_spec_id_from_ids(["RIPR-SPEC-0001", "RIPR-SPEC-0046"].into_iter()),
        "RIPR-SPEC-0047"
    );
    assert_eq!(
        next_spec_id_from_ids(std::iter::empty::<&str>()),
        "RIPR-SPEC-0001"
    );
}

#[test]
fn static_language_allowlist_covers_exact_paths_and_scoped_doc_globs() {
    let allowlist = allowlist_with_docs_markdown_globs();

    assert!(static_language_allowlist_covers(&allowlist, "AGENTS.md"));
    assert!(static_language_allowlist_covers(
        &allowlist,
        "docs/BADGE_POLICY.md"
    ));
    assert!(static_language_allowlist_covers(
        &allowlist,
        "docs/specs/RIPR-SPEC-0004-test-efficiency.md"
    ));
    assert!(!static_language_allowlist_covers(
        &allowlist,
        "crates/ripr/src/lib.rs"
    ));
    assert!(!static_language_allowlist_covers(
        &allowlist,
        "fixtures/boundary_gap/input/src/lib.rs"
    ));
    assert!(!static_language_allowlist_covers(
        &allowlist,
        "docs/generated/output.json"
    ));
}

#[test]
fn static_language_allowlist_covers_handles_empty_and_glob_only_lists() {
    // An empty allowlist covers nothing.
    let empty: Vec<StaticLanguageAllowEntry> = Vec::new();
    assert!(!static_language_allowlist_covers(&empty, "AGENTS.md"));
    assert!(!static_language_allowlist_covers(&empty, "docs/X.md"));

    // Glob-only allowlist: only paths matching the glob are covered.
    let globs_only = vec![StaticLanguageAllowEntry::new_glob(
        "docs/**/*.md",
        "test",
        "test fixture",
    )];
    assert!(static_language_allowlist_covers(
        &globs_only,
        "docs/specs/X.md"
    ));
    assert!(static_language_allowlist_covers(
        &globs_only,
        "docs/BADGE_POLICY.md"
    ));
    assert!(!static_language_allowlist_covers(&globs_only, "AGENTS.md"));
    assert!(!static_language_allowlist_covers(
        &globs_only,
        "crates/ripr/README.md"
    ));

    // Exact-only allowlist: no glob behavior applied to non-glob entries.
    let exact_only = vec![
        StaticLanguageAllowEntry::new_path("AGENTS.md", "test", "test fixture"),
        StaticLanguageAllowEntry::new_path("README.md", "test", "test fixture"),
    ];
    assert!(static_language_allowlist_covers(&exact_only, "AGENTS.md"));
    assert!(!static_language_allowlist_covers(
        &exact_only,
        "docs/AGENTS.md"
    ));
    assert!(!static_language_allowlist_covers(&exact_only, "docs/X.md"));
}

#[test]
fn static_language_allowlist_parses_reasoned_entries() {
    let text = r#"
schema_version = 1

[[allow]]
path = "AGENTS.md"
owner = "maintainers"
reason = "Agent instructions define the static-language boundary."

[[allow]]
glob = "docs/**/*.md"
owner = "docs"
reason = "Nested documentation may describe policy vocabulary."
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].matcher,
        StaticLanguageMatcher::Path("AGENTS.md".to_string())
    );
    assert_eq!(entries[0].owner, "maintainers");
    assert_eq!(
        entries[1].matcher,
        StaticLanguageMatcher::Glob("docs/**/*.md".to_string())
    );
    assert_eq!(entries[1].owner, "docs");
}

#[test]
fn static_language_allowlist_rejects_absolute_path() {
    let unix = r#"
schema_version = 1

[[allow]]
path = "/abs/path.md"
owner = "maintainers"
reason = "Absolute path attempt."
"#;
    let (entries, violations) = parse_static_language_allowlist(unix);
    assert!(entries.is_empty());
    assert!(
        violations.iter().any(|v| v.contains("is absolute")),
        "expected absolute-path violation, got {violations:?}"
    );

    // Windows drive letter form is also rejected. The drive prefix is
    // assembled at runtime so the literal pattern never appears in this
    // source file (`cargo xtask check-local-context` flags `<alpha>:/`).
    let drive = "Z";
    let sep = ":/";
    let win = format!(
        r#"
schema_version = 1

[[allow]]
path = "{drive}{sep}abs/path.md"
owner = "maintainers"
reason = "Windows absolute path attempt."
"#
    );
    let (entries, violations) = parse_static_language_allowlist(&win);
    assert!(entries.is_empty());
    assert!(
        violations.iter().any(|v| v.contains("is absolute")),
        "expected windows-absolute-path violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_rejects_backslash_path() {
    let text = r#"
schema_version = 1

[[allow]]
path = "docs\\BADGE_POLICY.md"
owner = "maintainers"
reason = "Backslash path attempt."
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert!(entries.is_empty());
    assert!(
        violations.iter().any(|v| v.contains("backslashes")),
        "expected backslash-path violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_rejects_blank_reason() {
    let text = r#"
schema_version = 1

[[allow]]
path = "AGENTS.md"
owner = "maintainers"
reason = "   "
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert!(entries.is_empty());
    assert!(
        violations.iter().any(|v| v.contains("`reason` is blank")),
        "expected blank-reason violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_rejects_duplicate_matcher() {
    let text = r#"
schema_version = 1

[[allow]]
path = "AGENTS.md"
owner = "maintainers"
reason = "First declaration."

[[allow]]
path = "AGENTS.md"
owner = "maintainers"
reason = "Second declaration."
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert_eq!(entries.len(), 2, "both entries should still parse");
    assert!(
        violations
            .iter()
            .any(|v| v.contains("`AGENTS.md` is duplicated")),
        "expected duplicate violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_rejects_missing_path_and_glob() {
    let text = r#"
schema_version = 1

[[allow]]
owner = "maintainers"
reason = "Entry without a matcher is meaningless."
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must declare either `path` or `glob`")),
        "expected missing-matcher violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_rejects_path_and_glob_together() {
    let text = r#"
schema_version = 1

[[allow]]
path = "AGENTS.md"
glob = "docs/*.md"
owner = "maintainers"
reason = "Mixed entry should be rejected."
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("both `path` and `glob`")),
        "expected path+glob violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_rejects_repo_wide_markdown_glob() {
    let cases = [
        ("*.md", "repo-wide single-segment markdown glob"),
        ("**/*.md", "repo-wide recursive markdown glob"),
        ("crates/**/*.md", "non-docs glob outside the scoped set"),
    ];
    for (glob, label) in cases {
        let text = format!(
            r#"
schema_version = 1

[[allow]]
glob = "{glob}"
owner = "maintainers"
reason = "{label}"
"#
        );
        let (entries, violations) = parse_static_language_allowlist(&text);
        assert!(entries.is_empty(), "glob `{glob}` should not parse");
        assert!(
            violations
                .iter()
                .any(|v| v.contains("not in the scoped set")),
            "expected scoped-glob violation for `{glob}`, got {violations:?}"
        );
    }
}

#[test]
fn static_language_allowlist_requires_owner() {
    let text = r#"
schema_version = 1

[[allow]]
path = "AGENTS.md"
reason = "Agent instructions describe policy vocabulary."
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing required `owner`")),
        "expected missing-owner violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_requires_reason() {
    let text = r#"
schema_version = 1

[[allow]]
path = "AGENTS.md"
owner = "maintainers"
"#;
    let (entries, violations) = parse_static_language_allowlist(text);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing required `reason`")),
        "expected missing-reason violation, got {violations:?}"
    );
}

#[test]
fn static_language_allowlist_requires_schema_version() {
    let text = r#"
[[allow]]
path = "AGENTS.md"
owner = "maintainers"
reason = "Agent instructions describe policy vocabulary."
"#;
    let (_, violations) = parse_static_language_allowlist(text);
    assert!(
        violations.iter().any(|v| v.contains("schema_version = 1")),
        "expected schema_version violation, got {violations:?}"
    );
}

#[test]
fn static_language_violation_message_advertises_inline_allow() {
    let message =
        static_language_violation_message("policy/x.txt", 3, "adequate", "# this set is adequate");
    assert!(
        message.contains("ripr-allow: static-language:"),
        "message must advertise the inline-allow escape, got: {message}"
    );
}

#[test]
fn static_language_violation_message_is_actionable() {
    let message = static_language_violation_message(
        ".github/workflows/scratch-gc.yml",
        4,
        "proven",
        "# Mirrors the proven inline \"Prepare scratch\" step",
    );
    // file:line and the term.
    assert!(
        message.starts_with(".github/workflows/scratch-gc.yml:4"),
        "message must lead with path:line, got: {message}"
    );
    assert!(
        message.contains("`proven`"),
        "message must name the term, got: {message}"
    );
    // The offending snippet, so the author sees the context inline.
    assert!(
        message.contains("Mirrors the proven inline"),
        "message must quote the offending line, got: {message}"
    );
    // The key fix hint: the gate scans prose, not just output.
    assert!(
        message.contains("scans all tracked prose"),
        "message must explain the prose-vs-output scope, got: {message}"
    );
    // A plain synonym for the prose case.
    assert!(
        message.contains("demonstrated/established"),
        "message must suggest a synonym, got: {message}"
    );
}

#[test]
fn static_language_violation_message_truncates_long_lines() {
    let long = "x".repeat(200);
    let line = format!("// {long} adequate");
    let message = static_language_violation_message("docs/notes.md", 7, "adequate", &line);
    // Snippet is bounded so the report stays scannable.
    assert!(
        message.contains("..."),
        "long snippet must be truncated, got len {}",
        message.len()
    );
    assert!(
        message.contains("sufficient/enough"),
        "message must suggest a synonym for `adequate`, got: {message}"
    );
}

#[test]
fn strip_yaml_comment_handles_double_backslash_before_quote() {
    assert_eq!(
        strip_yaml_comment("key: \"value\\\\\" # real comment"),
        "key: \"value\\\\\" "
    );
}

#[test]
fn strip_yaml_comment_handles_escaped_quote() {
    assert_eq!(
        strip_yaml_comment("key: \"value\\\" # not a comment"),
        "key: \"value\\\" # not a comment"
    );
}

#[test]
fn strip_yaml_comment_handles_full_comment_line() {
    assert_eq!(strip_yaml_comment("# this is a comment"), "");
}

#[test]
fn strip_yaml_comment_preserves_hash_in_strings() {
    assert_eq!(
        strip_yaml_comment("key: \"value#with hash\""),
        "key: \"value#with hash\""
    );
    assert_eq!(
        strip_yaml_comment("key: 'value#with hash'"),
        "key: 'value#with hash'"
    );
}

#[test]
fn strip_yaml_comment_preserves_line_without_comment() {
    assert_eq!(
        strip_yaml_comment("review_model: \"custom:MiniMax-M3-0\""),
        "review_model: \"custom:MiniMax-M3-0\""
    );
}

#[test]
fn strip_yaml_comment_removes_trailing_comment() {
    assert_eq!(strip_yaml_comment("key: value # comment"), "key: value ");
}

#[test]
fn suggested_fixes_patch_is_empty_when_allowlists_are_sorted() -> Result<(), String> {
    with_temp_cwd("suggested-fixes-empty", |root| {
        write(
            &root.join(".ripr/generated.txt"),
            "# header\n\nalpha\nbeta\n",
        );
        write(
            &root.join("docs/adr/README.md"),
            "# Architecture Decision Records\n\n## Index\n\n| ADR | Status | Decision |\n| --- | --- | --- |\n| [0001](0001-a.md) | accepted | A. |\n| [0002](0002-b.md) | proposed | B. |\n",
        );
        write(
            &root.join("metrics/capabilities.toml"),
            "# Capabilities\n\n[[capability]]\nid = \"alpha_capability\"\nspec = \"RIPR-SPEC-0001\"\nname = \"Alpha\"\n\n[[capability]]\nid = \"beta_capability\"\nspec = \"RIPR-SPEC-0002\"\nname = \"Beta\"\n",
        );

        let (patch, files) = suggested_fixes_patch()?;

        assert!(patch.is_empty());
        assert!(files.is_empty());
        Ok(())
    })
}

#[test]
fn suggested_fixes_patch_sorts_allowlists_doc_indexes_traceability_capabilities_and_catalog()
-> Result<(), String> {
    with_temp_cwd("suggested-fixes-allowlist", |root| {
        write(
            &root.join(".ripr/generated.txt"),
            "# header\n\nzeta\nalpha\n",
        );
        write(
            &root.join(".ripr/traceability.toml"),
            "# Traceability\n\n[[behavior]]\nid = \"RIPR-SPEC-0002\"\nname = \"B\"\n\n[[behavior]]\nid = \"RIPR-SPEC-0001\"\nname = \"A\"\n",
        );
        write(
            &root.join("metrics/capabilities.toml"),
            "# Capabilities\n\n[[capability]]\nid = \"zeta_capability\"\nspec = \"RIPR-SPEC-0002\"\nname = \"Zeta\"\n\n[[capability]]\nid = \"alpha_capability\"\nspec = \"RIPR-SPEC-0001\"\nname = \"Alpha\"\n",
        );
        write(&root.join("policy/process.txt"), "gamma\nbeta\n");
        write(
            &root.join("docs/specs/README.md"),
            "# Specs\n\n## Index\n\n| Spec | Status | Topic |\n| --- | --- | --- |\n| [RIPR-SPEC-0002](RIPR-SPEC-0002-b.md) | proposed | B |\n| [RIPR-SPEC-0001](RIPR-SPEC-0001-a.md) | accepted | A |\n",
        );
        write(
            &root.join("xtask/src/command.rs"),
            r#"pub(crate) fn command_catalog() -> Vec<CommandCatalogEntry> {
    vec![
        command_entry(
            "check-pr",
            "non_mutating_check",
            "target/ripr/reports",
            false,
            "Review-ready gate.",
        ),
        command_entry(
            "shape",
            "mutating",
            "source files",
            false,
            "Runs deterministic shaping.",
        ),
    ]
}
"#,
        );
        write(
            &root.join("badges/ripr.json"),
            r#"{"schemaVersion":1,"label":"ripr","message":"999","color":"red"}"#,
        );

        let (patch, files) = suggested_fixes_patch()?;

        assert_eq!(
            files,
            vec![
                ".ripr/generated.txt".to_string(),
                ".ripr/traceability.toml".to_string(),
                "docs/specs/README.md".to_string(),
                "metrics/capabilities.toml".to_string(),
                "policy/process.txt".to_string(),
                "xtask/src/command.rs".to_string()
            ]
        );
        assert!(patch.contains("diff --git a/.ripr/generated.txt b/.ripr/generated.txt"));
        assert!(patch.contains("+alpha"));
        assert!(patch.contains("+zeta"));
        assert!(
            patch
                .find("+id = \"RIPR-SPEC-0001\"")
                .zip(patch.find("+id = \"RIPR-SPEC-0002\""))
                .is_some_and(|(first, second)| first < second)
        );
        assert!(
            patch
                .find("+id = \"alpha_capability\"")
                .zip(patch.find("+id = \"zeta_capability\""))
                .is_some_and(|(first, second)| first < second)
        );
        assert!(patch.contains("diff --git a/docs/specs/README.md b/docs/specs/README.md"));
        assert!(
            patch
                .find("+| [RIPR-SPEC-0001](RIPR-SPEC-0001-a.md) | accepted | A |")
                .zip(patch.find("+| [RIPR-SPEC-0002](RIPR-SPEC-0002-b.md) | proposed | B |"))
                .is_some_and(|(first, second)| first < second)
        );
        assert!(patch.contains("diff --git a/policy/process.txt b/policy/process.txt"));
        assert!(!patch.contains("badges/ripr.json"));
        assert!(!patch.contains("999"));
        assert!(
            patch
                .find("+            \"shape\"")
                .zip(patch.find("+            \"check-pr\""))
                .is_some_and(|(shape, check_pr)| shape < check_pr)
        );
        Ok(())
    })
}

#[test]
fn support_tiers_accept_valid_fixture() -> Result<(), String> {
    with_temp_cwd("support-tiers-valid", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `usable alpha` | CLI and editor | `cargo xtask rust-repair-trust-report` | Governed promotion authority is incomplete. |\n| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Registered graph only. |\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(violations.is_empty(), "{violations:#?}");
        Ok(())
    })
}

#[test]
fn support_tiers_allow_specs_with_no_support_tier_impact() -> Result<(), String> {
    with_temp_cwd("support-tiers-spec-none", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `usable alpha` | CLI and editor | `cargo xtask rust-repair-trust-report` | Governed promotion authority is incomplete. |\n| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Registered graph only. |\n",
        );
        write(
            &root.join("docs/specs/RIPR-SPEC-0001-alpha.md"),
            "# RIPR-SPEC-0001: Alpha\n\nSupport-tier impact:\n\n- None.\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(violations.is_empty(), "{violations:#?}");
        Ok(())
    })
}

#[test]
fn support_tiers_command_accepts_valid_fixture() -> Result<(), String> {
    with_temp_cwd("support-tiers-command-valid", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `usable alpha` | CLI and editor | `cargo xtask rust-repair-trust-report` | Governed promotion authority is incomplete. |\n| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Registered graph only. |\n",
        );

        check_support_tiers()
    })
}

#[test]
fn support_tiers_reject_stable_claim_with_only_markdown_links() -> Result<(), String> {
    with_temp_cwd("support-tiers-link-only-proof", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `usable` | CLI | [workflow](../FIRST_PR_WORKFLOW.md), [capability matrix](../CAPABILITY_MATRIX.md) | Links alone are not proof commands. |\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("must name a known proof command or proof artifact")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn support_tiers_reject_stable_claim_without_proof() -> Result<(), String> {
    with_temp_cwd("support-tiers-missing-proof", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `usable` | CLI |  | Missing proof should fail. |\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("empty `Proof`")),
            "{violations:#?}"
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("with tier `usable` must name proof")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn support_tiers_reject_stronger_rust_repair_tier_bypass() -> Result<(), String> {
    with_temp_cwd("support-tiers-rust-repair-stable-bypass", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `stable building block` | CLI and editor | `cargo xtask rust-repair-trust-report` | Interim cap applies. |\n",
        );
        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(
            violations.iter().any(|violation| violation
                .contains("`stable building block` exceeds the interim `usable alpha` cap")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn support_tiers_reject_unknown_xtask_proof_command() -> Result<(), String> {
    with_temp_cwd("support-tiers-unknown-command", |root| {
        write_support_tier_fixture(
            root,
            "| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask nope` | Registered graph only. |\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("unknown xtask command `nope`")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn support_tiers_reject_usable_rust_repair_claim_without_promotion_authority() -> Result<(), String>
{
    with_temp_cwd("support-tiers-rust-repair-no-promotion-authority", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `usable` | CLI and editor | `cargo xtask rust-repair-trust-report` | Governed evidence required. |\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(
            violations.iter().any(|violation| {
                violation.contains("Rust gap repair loop")
                    && violation.contains("cannot claim `usable`")
                    && violation.contains("trust report alone is not promotion authority")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn support_tiers_require_readme_support_language_to_link_status_doc() -> Result<(), String> {
    with_temp_cwd("support-tiers-readme-link", |root| {
        write_support_tier_fixture(
            root,
            "| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Registered graph only. |\n",
        );
        write(
            &root.join("README.md"),
            "This stable building block is ready for users.\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("README.md names support-tier language")),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn support_tiers_require_specs_with_impact_to_link_status_doc() -> Result<(), String> {
    with_temp_cwd("support-tiers-spec-link", |root| {
        write_support_tier_fixture(
            root,
            "| Rust gap repair loop | `usable alpha` | CLI and editor | `cargo xtask rust-repair-trust-report` | Governed promotion authority is incomplete. |\n| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Registered graph only. |\n",
        );
        write(
            &root.join("docs/specs/RIPR-SPEC-0001-alpha.md"),
            "# RIPR-SPEC-0001: Alpha\n\nSupport-tier impact:\n\n- Stable claim changes.\n",
        );

        let violations =
            super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
        // Gate-feedback contract: the violation names the offending spec, the
        // missing SUPPORT_TIERS.md reference, a reason code, and a next action.
        assert!(
            violations.iter().any(|violation| {
                violation.contains("RIPR-SPEC-0001")
                    && violation.contains("docs/status/SUPPORT_TIERS.md")
                    && violation.contains("spec_support_tier_reference_missing")
                    && violation.contains("next:")
            }),
            "{violations:#?}"
        );
        Ok(())
    })
}

#[test]
fn support_tiers_require_unique_canonical_rust_repair_row() -> Result<(), String> {
    for (name, rows, expected) in [
        (
            "missing",
            "| Source-of-truth artifact graph | `stable building block` | docs | `cargo xtask check-doc-artifacts` | Registered graph only. |\n",
            "row is missing or renamed",
        ),
        (
            "renamed",
            "| Rust repair loop | `usable alpha` | CLI | `cargo xtask rust-repair-trust-report` | Renamed identity. |\n",
            "row is missing or renamed",
        ),
        (
            "duplicate",
            "| Rust gap repair loop | `usable alpha` | CLI | `cargo xtask rust-repair-trust-report` | First. |\n| Rust gap repair loop | `usable alpha` | editor | `cargo xtask rust-repair-trust-report` | Second. |\n",
            "found 2 rows",
        ),
    ] {
        with_temp_cwd(
            &format!("support-tiers-rust-repair-{name}"),
            |root| -> Result<(), String> {
                write_support_tier_fixture(root, rows);
                let violations =
                    super::super::support_tier_violations(root, &root.join(SUPPORT_TIERS_PATH))?;
                assert!(
                    violations.iter().any(|violation| {
                        violation.contains("exactly one canonical support-tier row")
                            && violation.contains(expected)
                    }),
                    "{name}: {violations:#?}"
                );
                Ok(())
            },
        )?;
    }
    Ok(())
}

#[test]
fn support_tiers_resolve_workflow_proof_relative_to_root() -> Result<(), String> {
    with_temp_cwd("support-tiers-root-relative-proof", |root| {
        let nested = root.join("nested-repo");
        write_support_tier_fixture(
            &nested,
            "| Rust gap repair loop | `usable alpha` | CLI and editor | `cargo xtask rust-repair-trust-report` | Governed promotion authority is incomplete. |\n| Source-of-truth workflow | `stable building block` | CI | `.github/workflows/source-of-truth.yml` | Workflow file exists in repo. |\n",
        );
        write(
            &nested.join(".github/workflows/source-of-truth.yml"),
            "name: Source of Truth\n",
        );

        let violations =
            super::super::support_tier_violations(&nested, &nested.join(SUPPORT_TIERS_PATH))?;
        assert!(violations.is_empty(), "{violations:#?}");
        Ok(())
    })
}

#[test]
fn suspicious_runtime_file_names_include_current_state_notes() {
    assert!(
        suspicious_runtime_file_names()
            .iter()
            .any(|name| name == concat!("CURRENT", "_STATE.md"))
    );
}

#[test]
fn swarm_server_binary_rehearsal_cannot_publish() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest should have a repository parent".to_string())?;
    let path = root.join(".github/workflows/release-server-binaries.yml");
    let workflow = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let validate = |candidate: &str| -> Result<(), String> {
        if !candidate.contains("permissions:\n  contents: read")
            || !candidate.contains("uses: ./.github/workflows/server-archive-qualification.yml")
            || !candidate.contains("candidate_sha: ${{ github.sha }}")
            || !candidate.contains("version: ${{ inputs.version }}")
            || candidate.contains("contents: write")
            || candidate.contains("release-upload-assets")
            || candidate.contains("gh release")
            || candidate.contains("GH_TOKEN")
            || candidate.contains("github.token")
            || candidate.contains("secrets.")
            || candidate.contains("run:")
            || candidate.contains("steps:")
            || candidate.contains("push:")
        {
            return Err("swarm server-binary entrypoint must delegate exact-SHA read-only qualification without a publication path".to_string());
        }
        Ok(())
    };
    validate(&workflow)?;
    for broken in [
        workflow.replace("contents: read", "contents: write"),
        format!("{workflow}\n# release-upload-assets"),
        workflow.replace(
            "candidate_sha: ${{ github.sha }}",
            "candidate_sha: ${{ github.ref }}",
        ),
    ] {
        if validate(&broken).is_ok() {
            return Err("rehearsal publication negative control was accepted".to_string());
        }
    }
    Ok(())
}

#[test]
fn targeted_test_outcome_args_parse_before_and_after() -> Result<(), String> {
    let args = vec![
        "--before".to_string(),
        "before.json".to_string(),
        "--after".to_string(),
        "after.json".to_string(),
    ];
    let parsed = parse_targeted_test_outcome_args(&args)?;
    assert_eq!(parsed.before, PathBuf::from("before.json"));
    assert_eq!(parsed.after, PathBuf::from("after.json"));
    Ok(())
}

#[test]
fn targeted_test_outcome_command_writes_markdown_and_json() -> Result<(), String> {
    with_temp_cwd("targeted-test-outcome", |_root| {
        let before = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 42,
      "grip_class": "weakly_gripped",
      "related_tests": [
        {"oracle_kind": "exact_value", "oracle_strength": "weak"}
      ],
      "observed_values": ["50"],
      "missing_discriminators": [
        {"value": "threshold equality", "reason": "not observed"}
      ]
    }
  ]
}"#;
        let after = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 42,
      "grip_class": "strongly_gripped",
      "related_tests": [
        {"oracle_kind": "exact_value", "oracle_strength": "strong"}
      ],
      "observed_values": ["50", "100"],
      "missing_discriminators": []
    }
  ]
}"#;
        write(Path::new("before.json"), before);
        write(Path::new("after.json"), after);
        targeted_test_outcome(&[
            "--before".to_string(),
            "before.json".to_string(),
            "--after".to_string(),
            "after.json".to_string(),
        ])?;
        let markdown = fs::read_to_string("target/ripr/reports/targeted-test-outcome.md")
            .map_err(|err| format!("failed to read targeted-test outcome markdown: {err}"))?;
        let json = fs::read_to_string("target/ripr/reports/targeted-test-outcome.json")
            .map_err(|err| format!("failed to read targeted-test outcome JSON: {err}"))?;
        assert!(markdown.contains("# ripr targeted-test outcome report"));
        assert!(markdown.contains("## Review Receipt"));
        assert!(json.contains("\"schema_version\": \"0.1\""));
        assert!(json.contains("\"moved\": 1"));
        assert!(json.contains("\"review_receipt\""));
        Ok(())
    })
}

#[test]
fn targeted_test_outcome_json_and_markdown_are_structured() -> Result<(), String> {
    let before = vec![
        targeted_static_seam("seam-a", "weakly_gripped"),
        targeted_static_seam("seam-same", "weakly_gripped"),
    ];
    let mut after_same = targeted_static_seam("seam-same", "weakly_gripped");
    after_same.observed_values = vec!["50".to_string(), "100".to_string()];
    let after = vec![
        targeted_static_seam("seam-a", "strongly_gripped"),
        after_same,
    ];
    let report = build_targeted_test_outcome_report(
        &before,
        &after,
        "target/ripr/before.json".to_string(),
        "target/ripr/after.json".to_string(),
    )?;

    let json = targeted_test_outcome_report_json(&report)?;
    let value: Value = serde_json::from_str(&json)
        .map_err(|err| format!("targeted-test outcome JSON should parse: {err}"))?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["status"], "advisory");
    assert_eq!(value["summary"]["moved"], 1);
    assert_eq!(
        value["review_receipt"]["movement_after_verification"][0],
        "1 improved, 0 changed without ranking higher, 0 regressed, 1 unchanged."
    );
    assert!(
        value["review_receipt"]["focused_proof_added"][0]
            .as_str()
            .is_some_and(|text| text.contains("new observed value: 100"))
    );
    assert!(
        value["review_receipt"]["reviewer_should_not_believe"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item == "Merge approval."))
    );

    let markdown = targeted_test_outcome_report_markdown(&report);
    assert!(markdown.contains("# ripr targeted-test outcome report"));
    assert!(markdown.contains("| moved | 1 |"));
    assert!(markdown.contains("## Unchanged"));
    assert!(markdown.contains("seam-same"));
    assert!(markdown.contains("new observed value: 100"));
    assert!(markdown.contains("## Review Receipt"));
    assert!(markdown.contains("### What focused proof changed?"));
    assert!(markdown.contains("### Reviewer should not believe"));
    assert!(markdown.contains("weakly_gripped -> strongly_gripped"));
    Ok(())
}

#[test]
fn targeted_test_outcome_report_buckets_seam_movement() -> Result<(), String> {
    let mut before_moved = targeted_static_seam("seam-moved", "weakly_gripped");
    before_moved.missing_discriminators = vec!["threshold equality".to_string()];
    before_moved.oracle_strength = "weak".to_string();
    let before = vec![
        before_moved,
        targeted_static_seam("seam-regressed", "weakly_gripped"),
        targeted_static_seam("seam-same", "strongly_gripped"),
        targeted_static_seam("seam-removed", "ungripped"),
    ];

    let mut after_moved = targeted_static_seam("seam-moved", "strongly_gripped");
    after_moved.observed_values = vec!["50".to_string(), "100".to_string()];
    after_moved.oracle_strength = "strong".to_string();
    let after = vec![
        after_moved,
        targeted_static_seam("seam-regressed", "ungripped"),
        targeted_static_seam("seam-same", "strongly_gripped"),
        targeted_static_seam("seam-new", "weakly_gripped"),
    ];

    let report = build_targeted_test_outcome_report(
        &before,
        &after,
        "before.json".to_string(),
        "after.json".to_string(),
    )?;
    assert_eq!(report.moved.len(), 1);
    assert_eq!(report.moved[0].seam_id, "seam-moved");
    assert_eq!(report.moved[0].direction, "improved");
    assert!(
        report.moved[0]
            .evidence_delta
            .iter()
            .any(|delta| delta.contains("missing discriminator no longer reported"))
    );
    assert!(
        report.moved[0]
            .evidence_delta
            .iter()
            .any(|delta| delta.contains("stronger related oracle visible"))
    );
    assert_eq!(report.regressed.len(), 1);
    assert_eq!(report.unchanged.len(), 1);
    assert_eq!(report.new.len(), 1);
    assert_eq!(report.removed.len(), 1);
    assert_eq!(report.before_counts.get("weakly_gripped"), Some(&2));
    assert_eq!(report.after_counts.get("strongly_gripped"), Some(&2));
    Ok(())
}

#[test]
fn text_encoding_candidates_cover_issue_text_classes() -> Result<(), String> {
    // The three #7091 BOM sites were .txt, .rs, and .md; extensionless
    // tracked files (LICENSE files) are scanned fail-closed. Extension
    // matching is case-insensitive: the tracked `Makefile.PL` fixture and
    // any future `README.MD`-style path must be scanned too (#7154).
    for candidate in [
        "policy/process_allowlist.txt",
        "crates/ripr/tests/agent_stub_compiles.rs",
        "fixtures/rust-verdict-corpus/SPEC.md",
        "Cargo.lock",
        "LICENSE-MIT",
        "editors/vscode/.vscodeignore",
        "fixtures/perl_cpan_alpha/input/Makefile.PL",
        "notes.TXT",
    ] {
        if !super::super::is_text_encoding_candidate(candidate) {
            return Err(format!("text path must be scanned: {candidate}"));
        }
    }
    for skipped in ["assets/logo.png", "fixtures/corpus.gz"] {
        if super::super::is_text_encoding_candidate(skipped) {
            return Err(format!("binary path must be skipped: {skipped}"));
        }
    }
    Ok(())
}

#[test]
fn text_encoding_clean_tree_passes() -> Result<(), String> {
    with_repo_cwd(super::super::check_text_encoding)
}

#[test]
fn text_encoding_detects_only_bom_prefix() -> Result<(), String> {
    if !super::super::has_utf8_bom(&[0xEF, 0xBB, 0xBF, b'#']) {
        return Err("a leading EF BB BF must be detected as a BOM".to_string());
    }
    if !super::super::has_utf8_bom(&[0xEF, 0xBB, 0xBF]) {
        return Err("a BOM-only file must be detected as a BOM".to_string());
    }
    for clean in [
        Vec::new(),
        vec![0xEF, 0xBB],
        b"# heading".to_vec(),
        b"\x00\x01\x02".to_vec(),
        [b'x', 0xEF, 0xBB, 0xBF].to_vec(),
    ] {
        if super::super::has_utf8_bom(&clean) {
            return Err(format!("non-BOM prefix must pass: {clean:?}"));
        }
    }
    Ok(())
}

#[test]
fn text_encoding_reports_a_tracked_bom_file() -> Result<(), String> {
    // The load-bearing direction: a tracked file starting with EF BB BF
    // must fail the real check with the file named. Without this test,
    // deleting the reporting branch leaves every other text-encoding
    // test green (#7154).
    with_temp_cwd("text-encoding-tracked-bom", |root| -> Result<(), String> {
        badge_fixture_git(&["init", "--initial-branch=main", "--quiet"])?;
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"# heading\n");
        std::fs::write(root.join("bom.md"), &bytes)
            .map_err(|err| format!("failed to write BOM fixture: {err}"))?;
        badge_fixture_git(&["add", "bom.md"])?;
        // Setup assertion: enumeration must see the fixture before the
        // check runs, so a git-layout failure cannot masquerade as a
        // clean scan or a passing red test.
        let listed = super::super::tracked_files()?;
        if !listed.iter().any(|path| path == "bom.md") {
            return Err(format!(
                "BOM fixture must be git-tracked before the check runs: {listed:?}"
            ));
        }
        let Err(err) = super::super::check_text_encoding() else {
            return Err("a tracked BOM file must fail check_text_encoding".to_string());
        };
        if !err.contains("bom.md") || !err.contains("UTF-8 BOM") {
            return Err(format!(
                "BOM failure must name the file and the mark: {err}"
            ));
        }
        Ok(())
    })
}

#[test]
fn text_encoding_violation_names_file_and_both_strip_commands() -> Result<(), String> {
    let violation = super::super::text_encoding_violation("policy/process_allowlist.txt");
    for required in [
        "policy/process_allowlist.txt",
        "UTF-8 BOM",
        "strip (POSIX):",
        "tail -c +4 'policy/process_allowlist.txt'",
        "strip (PowerShell):",
        "[IO.File]::ReadAllBytes",
        "[IO.File]::WriteAllBytes",
    ] {
        if !violation.contains(required) {
            return Err(format!(
                "BOM violation must contain {required:?}: {violation:?}"
            ));
        }
    }
    // An apostrophe in the path must stay inside each shell's quoting:
    // POSIX closes the quote around '\'' and PowerShell doubles it.
    let quoted = super::super::text_encoding_violation("docs/it's.md");
    for required in ["tail -c +4 'docs/it'\\''s.md'", "$p='docs/it''s.md'"] {
        if !quoted.contains(required) {
            return Err(format!(
                "BOM violation must quote apostrophes ({required:?}): {quoted:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn traceability_array_parser_reads_inline_values() {
    assert_eq!(
        parse_inline_array("[\"one\", \"two\"]"),
        Ok(vec!["one".to_string(), "two".to_string()])
    );
    assert_eq!(parse_inline_array("[]"), Ok(Vec::new()));
    parse_inline_array("[one]").expect_err("unquoted token should fail array parse");
}

#[test]
fn traceability_failure_report_renders_recommended_fixes() -> Result<(), String> {
    with_temp_cwd("traceability-recommended-fixes-report", |_| {
        let error = finish_traceability_report(&["invalid spec reference".to_string()], &[])
            .expect_err("violations should fail the traceability report");
        let report = fs::read_to_string("target/ripr/reports/traceability.md")
            .map_err(|err| format!("read traceability report: {err}"))?;

        assert!(error.contains("check-traceability failed"));
        assert!(report.contains("cargo xtask specs next"));
        assert!(report.contains("docs/specs/README.md"));
        assert!(report.contains("valid RIPR-SPEC-NNNN IDs (exactly 4 digits)"));
        Ok(())
    })
}

#[test]
fn traceability_missing_path_remains_blocking() -> Result<(), String> {
    let references = vec!["xtask/path-that-does-not-exist.rs::tests::example".to_string()];
    let mut violations = Vec::new();
    let mut advisories = Vec::new();

    super::super::validate_trace_paths(
        "RIPR-SPEC-9999",
        "tests",
        &references,
        &mut violations,
        &mut advisories,
    );

    if violations.len() != 1 || !violations[0].contains("path does not exist") {
        return Err(format!(
            "missing path should remain blocking, got {violations:?}"
        ));
    }
    if !advisories.is_empty() {
        return Err(format!(
            "missing path should not also emit an advisory: {advisories:?}"
        ));
    }
    Ok(())
}

#[test]
fn traceability_pass_report_discloses_registered_only_scope() -> Result<(), String> {
    with_temp_cwd("traceability-pass-scope-report", |_| {
        finish_traceability_report(
            &[],
            &["RIPR-SPEC-0027 symbol suffix unverified".to_string()],
        )?;
        let report = fs::read_to_string("target/ripr/reports/traceability.md")
            .map_err(|err| format!("read traceability pass report: {err}"))?;

        assert!(report.contains("Status: pass"));
        assert!(report.contains("## Scope of this result"));
        assert!(report.contains("does not enumerate Rust tests"));
        assert!(report.contains("does not require every newly added test"));
        assert!(report.contains("a test role, ran, or establishes the behavior"));
        assert!(report.contains("Advisories (non-blocking)"));
        assert!(report.contains("symbol suffix unverified"));
        Ok(())
    })
}

#[test]
fn traceability_recommended_fixes_explain_spec_allocation_and_format() {
    let fixes = traceability_recommended_fixes();
    assert!(
        fixes
            .iter()
            .any(|fix| fix.contains("cargo xtask specs next"))
    );
    assert!(fixes.iter().any(|fix| fix.contains("exactly 4 digits")));
    assert!(fixes.iter().any(|fix| fix.contains("docs/specs/README.md")));
}

#[test]
fn traceability_symbol_suffix_is_advisory_without_blocking() -> Result<(), String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let references = vec![format!("{}::tests::example", source.display())];
    let mut violations = Vec::new();
    let mut advisories = Vec::new();

    super::super::validate_trace_paths(
        "RIPR-SPEC-9999",
        "tests",
        &references,
        &mut violations,
        &mut advisories,
    );

    if !violations.is_empty() {
        return Err(format!(
            "existing-file symbol suffix should not block: {violations:?}"
        ));
    }
    if advisories.len() != 1 || !advisories[0].contains("symbol suffix") {
        return Err(format!(
            "expected one visible symbol advisory, got {advisories:?}"
        ));
    }
    Ok(())
}

#[test]
fn typescript_bun_ub_calibration_cases_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let cases = super::super::typescript_bun_ub_calibration_cases();
        for required in TYPESCRIPT_BUN_UB_CALIBRATION_REQUIRED_CASES {
            assert!(
                cases
                    .iter()
                    .any(|case| { case.name == required.0 && case.expected_verdict == required.1 }),
                "{} TypeScript Bun UB calibration case should be checked as {}",
                required.0,
                required.1
            );
        }

        let verdicts = cases
            .iter()
            .map(|case| case.expected_verdict.as_str())
            .collect::<BTreeSet<_>>();
        for required in [
            "ts_discriminated",
            "ts_missing_shared",
            "ts_missing_resizable",
            "ts_missing_external_oracle",
            "ts_mention_not_observer",
            "bridge_unknown",
        ] {
            assert!(
                verdicts.contains(required),
                "TypeScript Bun UB calibration should include {required}"
            );
        }

        for case in cases {
            let errors = super::super::typescript_bun_ub_calibration_case_errors(&case);
            assert!(
                errors.is_empty(),
                "{} TypeScript Bun UB calibration case should validate: {:?}",
                case.name,
                errors
            );
        }

        Ok(())
    })
}

#[test]
fn typescript_bun_ub_calibration_rejects_duplicate_and_required_case_drift() -> Result<(), String> {
    let root = temp_dir("typescript-bun-ub-calibration-required-cases");
    let mut duplicate = valid_typescript_bun_ub_calibration_case();
    duplicate.name = "duplicated".to_string();
    let duplicate_case = typescript_bun_ub_calibration_case_json(&duplicate);
    let duplicate_path = root.join("duplicate.json");
    write(
        &duplicate_path,
        &typescript_bun_ub_calibration_corpus_json(&[duplicate_case.clone(), duplicate_case]),
    );

    let mut violations = Vec::new();
    super::super::validate_typescript_bun_ub_calibration_fixture_corpus_at(
        &duplicate_path,
        &mut violations,
    )?;
    assert_contains_error(&violations, "case duplicated is duplicated");
    assert_contains_error(
        &violations,
        "TypeScript Bun UB calibration corpus is missing case bun_blob_shared_and_resizable_present",
    );
    assert_contains_error(
        &violations,
        "TypeScript Bun UB calibration corpus must include verdict ts_missing_resizable",
    );

    let wrong_required_path = root.join("wrong-required.json");
    let mut wrong_required = valid_typescript_bun_ub_calibration_case();
    wrong_required.name = "bun_blob_shared_and_resizable_present".to_string();
    wrong_required.expected_verdict = "ts_missing_resizable".to_string();
    wrong_required.resizable_array_buffer = false;
    wrong_required
        .expected_missing_discriminators
        .push("resizable_array_buffer".to_string());
    wrong_required.suggested_test_file = "test/js/web/fetch/blob.test.ts".to_string();
    write(
        &wrong_required_path,
        &typescript_bun_ub_calibration_corpus_json(&[typescript_bun_ub_calibration_case_json(
            &wrong_required,
        )]),
    );

    let mut violations = Vec::new();
    super::super::validate_typescript_bun_ub_calibration_fixture_corpus_at(
        &wrong_required_path,
        &mut violations,
    )?;
    assert_contains_error(
        &violations,
        "case bun_blob_shared_and_resizable_present must have expected_verdict ts_discriminated, got ts_missing_resizable",
    );

    Ok(())
}

#[test]
fn typescript_bun_ub_calibration_rejects_invalid_case_fields() {
    let mut case = valid_typescript_bun_ub_calibration_case();
    case.name = " ".to_string();
    case.language = "javascript".to_string();
    case.language_status = "stable".to_string();
    case.authority_boundary = "gate_ready".to_string();
    case.repair_packet_ready = true;
    case.expected_verdict = "not_a_verdict".to_string();
    case.bridge_confidence = "guessed".to_string();
    case.rust_file = "src/lib.rs".to_string();
    case.rust_owner = "Blob::from_js".to_string();
    case.rust_boundary = "array_buffer.shared".to_string();
    case.ts_test_file = "test/js/web/fetch/blob.test.js".to_string();
    case.suggested_test_file = "src/jsc/Blob.rs".to_string();
    case.reason.clear();
    case.non_claims.clear();

    let errors = super::super::typescript_bun_ub_calibration_case_errors(&case);
    for expected in [
        "case id must be present",
        "reason must be present",
        "language must be typescript",
        "language_status must be preview",
        "authority_boundary must be preview_advisory_only",
        "repair_packet_ready must remain false",
        "expected_verdict must be a Bun UB calibration verdict",
        "bridge_confidence must be configured_hint, heuristic, or unknown",
        "rust_file must identify the Bun Blob Rust seam",
        "rust_owner must pin Blob::from_js_without_defer_gc",
        "rust_boundary must include array_buffer.shared and array_buffer.resizable",
        "ts_test_file must be a Bun test/js TypeScript test path",
        "suggested_test_file must be not_applicable or a Bun test/js path",
        "non_claims must keep preview boundary denials visible",
        "non_claims must deny provider",
    ] {
        assert_contains_error(&errors, expected);
    }
}

#[test]
fn typescript_bun_ub_calibration_rejects_invalid_verdict_shapes() {
    let mut discriminated = valid_typescript_bun_ub_calibration_case();
    discriminated.shared_array_buffer = false;
    discriminated
        .expected_missing_discriminators
        .push("shared_array_buffer".to_string());
    discriminated.suggested_test_file = "test/js/web/fetch/blob.test.ts".to_string();
    let errors = super::super::typescript_bun_ub_calibration_case_errors(&discriminated);
    assert_contains_error(
        &errors,
        "ts_discriminated requires shared, resizable, Blob input, and stable-byte oracle facts",
    );
    assert_contains_error(
        &errors,
        "ts_discriminated must not name missing discriminators",
    );
    assert_contains_error(&errors, "ts_discriminated must not suggest a new test file");

    let mut missing_resizable = valid_typescript_bun_ub_calibration_case();
    missing_resizable.expected_verdict = "ts_missing_resizable".to_string();
    missing_resizable.shared_array_buffer = false;
    missing_resizable.resizable_array_buffer = true;
    missing_resizable.suggested_test_file = "test/js/web/fetch/blob.test.ts".to_string();
    let errors = super::super::typescript_bun_ub_calibration_case_errors(&missing_resizable);
    assert_contains_error(
        &errors,
        "ts_missing_resizable requires shared present and resizable absent",
    );
    assert_contains_error(
        &errors,
        "ts_missing_resizable must include missing discriminator resizable_array_buffer",
    );

    let mut missing_shared = valid_typescript_bun_ub_calibration_case();
    missing_shared.expected_verdict = "ts_missing_shared".to_string();
    missing_shared.shared_array_buffer = true;
    missing_shared.resizable_array_buffer = false;
    missing_shared.suggested_test_file = "test/js/web/fetch/blob.test.ts".to_string();
    let errors = super::super::typescript_bun_ub_calibration_case_errors(&missing_shared);
    assert_contains_error(
        &errors,
        "ts_missing_shared requires shared absent and resizable present",
    );
    assert_contains_error(
        &errors,
        "ts_missing_shared must include missing discriminator shared_array_buffer",
    );

    let mut missing_both = valid_typescript_bun_ub_calibration_case();
    missing_both.expected_verdict = "ts_missing_shared_and_resizable".to_string();
    missing_both.shared_array_buffer = true;
    missing_both.resizable_array_buffer = true;
    missing_both.suggested_test_file = "not_applicable".to_string();
    let errors = super::super::typescript_bun_ub_calibration_case_errors(&missing_both);
    assert_contains_error(
        &errors,
        "ts_missing_shared_and_resizable requires both boundary facts absent",
    );
    assert_contains_error(
        &errors,
        "ts_missing_shared_and_resizable must include missing discriminator shared_array_buffer",
    );
    assert_contains_error(
        &errors,
        "ts_missing_shared_and_resizable must include missing discriminator resizable_array_buffer",
    );
    assert_contains_error(
        &errors,
        "ts_missing_shared_and_resizable must keep suggested_test_file=test/js/web/fetch/blob.test.ts",
    );

    let mut missing_external_oracle = valid_typescript_bun_ub_calibration_case();
    missing_external_oracle.expected_verdict = "ts_missing_external_oracle".to_string();
    missing_external_oracle.expected_missing_discriminators =
        vec!["resizable_array_buffer".to_string()];
    missing_external_oracle.view_backed_blob_input = true;
    missing_external_oracle.stable_byte_copy_oracle = true;
    missing_external_oracle.suggested_test_file = "test/js/web/fetch/blob.test.ts".to_string();
    let errors = super::super::typescript_bun_ub_calibration_case_errors(&missing_external_oracle);
    assert_contains_error(
        &errors,
        "ts_missing_external_oracle requires a missing Blob input or stable-byte oracle",
    );
    assert_contains_error(
        &errors,
        "ts_missing_external_oracle must not name boundary discriminators",
    );
    assert_contains_error(
        &errors,
        "ts_missing_external_oracle must keep suggested_test_file=not_applicable",
    );

    let mut mention_only = valid_typescript_bun_ub_calibration_case();
    mention_only.expected_verdict = "ts_mention_not_observer".to_string();
    mention_only.max_byte_length_mention_only = false;
    mention_only.view_backed_blob_input = true;
    mention_only.stable_byte_copy_oracle = true;
    mention_only.suggested_test_file = super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string();
    let errors = super::super::typescript_bun_ub_calibration_case_errors(&mention_only);
    assert_contains_error(
        &errors,
        "ts_mention_not_observer must record max_byte_length_mention_only=true",
    );
    assert_contains_error(
        &errors,
        "ts_mention_not_observer must not count Blob input or stable-byte oracle facts",
    );
    assert_contains_error(
        &errors,
        "ts_mention_not_observer must keep suggested_test_file=not_applicable",
    );

    let mut bridge_unknown = valid_typescript_bun_ub_calibration_case();
    bridge_unknown.expected_verdict = "bridge_unknown".to_string();
    bridge_unknown.bridge_confidence = "configured_hint".to_string();
    bridge_unknown.suggested_test_file =
        super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE.to_string();
    let errors = super::super::typescript_bun_ub_calibration_case_errors(&bridge_unknown);
    assert_contains_error(&errors, "bridge_unknown requires bridge_confidence=unknown");
    assert_contains_error(
        &errors,
        "bridge_unknown must keep suggested_test_file=not_applicable",
    );
}

#[test]
fn typescript_bun_ub_calibration_rejects_malformed_corpus_files() -> Result<(), String> {
    let root = temp_dir("typescript-bun-ub-calibration-malformed");
    let missing_path = root.join("missing.json");
    let mut violations = Vec::new();
    super::super::validate_typescript_bun_ub_calibration_fixture_corpus_at(
        &missing_path,
        &mut violations,
    )?;
    assert_contains_error(
        &violations,
        "TypeScript Bun UB calibration corpus is missing",
    );

    for (name, body, reason) in [
        (
            "bad-schema",
            r#"{"schema_version":"0.2","kind":"typescript_bun_ub_calibration_corpus","spec":"RIPR-SPEC-0027","cases":[]}"#,
            "schema_version must be 0.1",
        ),
        (
            "bad-kind",
            r#"{"schema_version":"0.1","kind":"other","spec":"RIPR-SPEC-0027","cases":[]}"#,
            "kind must be typescript_bun_ub_calibration_corpus",
        ),
        (
            "bad-spec",
            r#"{"schema_version":"0.1","kind":"typescript_bun_ub_calibration_corpus","spec":"RIPR-SPEC-9999","cases":[]}"#,
            "spec must be RIPR-SPEC-0027",
        ),
        (
            "missing-cases",
            r#"{"schema_version":"0.1","kind":"typescript_bun_ub_calibration_corpus","spec":"RIPR-SPEC-0027"}"#,
            "missing cases array",
        ),
    ] {
        let path = root.join(format!("{name}.json"));
        write(&path, body);
        let cases = super::super::typescript_bun_ub_calibration_cases_at(&path);
        assert_contains_error(std::slice::from_ref(&cases[0].reason), reason);

        let mut violations = Vec::new();
        super::super::validate_typescript_bun_ub_calibration_fixture_corpus_at(
            &path,
            &mut violations,
        )?;
        assert_contains_error(
            &violations,
            "TypeScript Bun UB calibration case corpus: language must be present",
        );
    }

    Ok(())
}

#[test]
fn typescript_preview_false_actionable_audit_cases_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let cases = super::super::typescript_preview_false_actionable_audit_cases();
        for required in TYPESCRIPT_PREVIEW_FALSE_ACTIONABLE_AUDIT_REQUIRED_CASES {
            assert!(
                cases
                    .iter()
                    .any(|case| { case.name == required.0 && case.disposition == required.1 }),
                "{} TypeScript preview false-actionable audit case should be checked as {}",
                required.0,
                required.1
            );
        }

        let dispositions = cases
            .iter()
            .map(|case| case.disposition.as_str())
            .collect::<BTreeSet<_>>();
        for required in [
            "safe_advisory",
            "named_static_limitation",
            "candidate_future_support",
            "must_remain_non_actionable",
        ] {
            assert!(
                dispositions.contains(required),
                "TypeScript preview false-actionable audit should include {required}"
            );
        }

        for case in cases {
            let errors = super::super::typescript_preview_false_actionable_audit_case_errors(&case);
            assert!(
                errors.is_empty(),
                "{} TypeScript preview false-actionable audit case should validate: {:?}",
                case.name,
                errors
            );
        }

        Ok(())
    })
}

#[test]
fn unknown_command_message_includes_help_without_nearby_match() {
    let message = unknown_command_message("totally-unknown-command");
    assert!(!message.contains("Did you mean"));
    assert!(message.contains("cargo xtask help"));
}

#[test]
fn unknown_command_message_suggests_nearest_match() {
    let message = unknown_command_message("chek-pr");
    assert!(message.contains("Did you mean `check-pr`?"));
}

#[test]
fn validate_bless_reason_rejects_unknown_spec_ids() -> Result<(), String> {
    with_repo_cwd(|| {
        validate_bless_reason("RIPR-SPEC-0001: valid existing spec")?;
        let error = match validate_bless_reason("RIPR-SPEC-9999: not a repository spec") {
            Ok(()) => return Err("unknown spec id must not be accepted".to_string()),
            Err(error) => error,
        };
        if !error.contains("RIPR-SPEC-9999") {
            return Err(format!("unknown spec error omitted the id: {error}"));
        }
        Ok(())
    })
}

#[test]
fn validate_shields_endpoint_bytes_accepts_minimal_shape() -> Result<(), String> {
    super::super::validate_shields_endpoint_bytes(
        b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"0\",\"color\":\"brightgreen\"}",
        "ripr+",
    )
}

#[test]
fn validate_shields_endpoint_bytes_rejects_extra_fields() {
    let err = super::super::validate_shields_endpoint_bytes(
            b"{\"schemaVersion\":1,\"label\":\"ripr+\",\"message\":\"0\",\"color\":\"brightgreen\",\"extra\":true}",
            "ripr+",
        )
        .expect_err("extra fields must be rejected");
    assert!(err.contains("must contain only"));
}

#[test]
fn verify_packaged_vsix_accepts_real_repository_notice_bytes() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .ok_or_else(|| "failed to resolve repo root from CARGO_MANIFEST_DIR".to_string())?;
    let notices = super::super::load_repository_license_notices(repo_root)?;
    let mit = String::from_utf8(notices.mit.clone()).map_err(|err| err.to_string())?;
    let apache = String::from_utf8(notices.apache.clone()).map_err(|err| err.to_string())?;
    with_temp_cwd("vsix-real-notices", |root| {
        let vsix = root.join("real.vsix");
        let license_txt = packaged_license_txt(&mit, &apache);
        write_packaging_test_vsix(
            &vsix,
            &[
                ("extension/package.json", "{}"),
                ("extension/LICENSE.txt", &license_txt),
            ],
        )?;
        super::super::verify_packaged_vsix(&vsix, &notices)?;
        Ok(())
    })
}

#[test]
fn verify_source_vscode_license_rejects_pointer_and_accepts_complete_texts() -> Result<(), String> {
    with_temp_cwd("source-vscode-license", |root| {
        let notices = complete_license_notices();
        let path = root.join("LICENSE");
        write_license_fixture(root, "LICENSE", POINTER_LICENSE)?;
        let Err(pointer) = super::super::verify_source_vscode_license(&path, &notices) else {
            return Err(
                "the historic LICENSE pointer must not satisfy the source contract".to_string(),
            );
        };
        assert!(
            pointer.contains("editors/vscode/LICENSE pointer"),
            "{pointer}"
        );

        write_license_fixture(
            root,
            "LICENSE",
            &packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE),
        )?;
        super::super::verify_source_vscode_license(&path, &notices)?;
        Ok(())
    })
}

#[test]
fn vscode_command_literal_extraction_finds_ripr_commands() {
    let commands = ripr_command_literals_in_text(
        "await vscode.commands.executeCommand('ripr.copyContext');\ncommand: \"ripr.collectContext\"",
    );
    assert_eq!(
        commands,
        vec![
            "ripr.collectContext".to_string(),
            "ripr.copyContext".to_string()
        ]
    );
}

#[test]
fn vscode_commands_use_extension_cwd_and_local_bins() {
    let extension_dir = vscode_extension_dir();
    let node_bin_extension = if cfg!(windows) { ".cmd" } else { "" };

    assert_eq!(
        vscode_compile_command(),
        CwdCommand {
            program: extension_dir
                .join("node_modules")
                .join(".bin")
                .join(format!("tsc{node_bin_extension}")),
            args: vec!["-p".to_string(), "./".to_string()],
            cwd: extension_dir.clone(),
        }
    );
    assert_eq!(
        vscode_package_command("0.4.0"),
        CwdCommand {
            program: extension_dir
                .join("node_modules")
                .join(".bin")
                .join(format!("vsce{node_bin_extension}")),
            args: vec![
                "package".to_string(),
                "--out".to_string(),
                "dist/ripr-0.4.0.vsix".to_string(),
            ],
            cwd: extension_dir.clone(),
        }
    );
    assert_eq!(
        vscode_test_e2e_command(),
        CwdCommand {
            program: PathBuf::from("node"),
            args: vec!["out/test/runTest.js".to_string()],
            cwd: extension_dir,
        }
    );
}

#[test]
fn vscode_e2e_preserves_installed_server_selection() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-vscode-server-selection-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).map_err(|err| format!("create test directory: {err}"))?;
    let installed = root.join("installed-ripr.exe");
    let built = root.join("target-ripr.exe");
    fs::write(&installed, b"packaged").map_err(|err| format!("write installed fixture: {err}"))?;

    let selected = select_vscode_test_server(Some(&installed), &built)?;
    assert_eq!(selected, (installed.clone(), false));
    assert_eq!(select_vscode_test_server(None, &built)?, (built, true));
    let missing = root.join("missing-ripr.exe");
    let error = select_vscode_test_server(Some(&missing), &installed).unwrap_err();
    assert!(error.contains("installed server binary"));

    fs::remove_dir_all(&root).map_err(|err| format!("remove test directory: {err}"))?;
    Ok(())
}

#[test]
fn vscode_host_diagnostics_ci_job_retains_failure_only() -> Result<(), String> {
    let root = repo_root()?;
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml"))
        .map_err(|err| format!("read ci.yml: {err}"))?;
    let job = vscode_job_block(&workflow)?;
    let violations = vscode_host_diagnostics_contract_violations(job);
    assert!(
        violations.is_empty(),
        "vscode host diagnostics contract: {violations:?}"
    );

    let runner = fs::read_to_string(root.join("editors/vscode/test/runTest.ts"))
        .map_err(|err| format!("read runTest.ts: {err}"))?;
    assert!(
        runner.contains("resolveUserDataDir"),
        "runTest.ts must pin user-data through resolveUserDataDir"
    );
    assert!(
        runner.contains("finalizeHostRun"),
        "runTest.ts must preserve the original host exit through finalizeHostRun"
    );
    assert!(
        runner.contains("resetHostLogDir(userDataPath)"),
        "runTest.ts must call resetHostLogDir(userDataPath) before setup"
    );
    assert!(
        !runner.contains("vscode-test-user-data', runId"),
        "user-data must be a known folder, not a pid-suffixed path"
    );

    let collector = fs::read_to_string(root.join("editors/vscode/test/hostDiagnostics.ts"))
        .map_err(|err| format!("read hostDiagnostics.ts: {err}"))?;
    assert!(collector.contains("HOST_DIAGNOSTICS_RETENTION_DAYS = 5"));
    assert!(collector.contains("MAX_HOST_DIAGNOSTICS_BYTES = 5 * 1024 * 1024"));
    assert!(collector.contains("USER_DATA_DIR_NAME = 'vscode-test-user-data'"));
    assert!(collector.contains("HOST_DIAGNOSTICS_DIR_NAME = 'vscode-test-host-diagnostics'"));
    Ok(())
}

#[test]
fn vscode_host_diagnostics_contract_rejects_success_uploads_and_exit_laundering() {
    let success_upload = r#"
  vscode:
    steps:
      - run: xvfb-run -a npm run test:e2e
      - name: Upload VS Code test host diagnostics
        if: always()
        uses: actions/upload-artifact@v7
        with:
          path: target/ripr/vscode-test-host-diagnostics/
          retention-days: 5
          if-no-files-found: ignore
"#;
    assert!(
        vscode_host_diagnostics_contract_violations(success_upload)
            .iter()
            .any(|violation| violation.contains("must not run on success")),
        "success uploads must fail the contract"
    );

    let laundered_exit = r#"
  vscode:
    steps:
      - run: xvfb-run -a npm run test:e2e
        continue-on-error: true
      - name: Upload VS Code test host diagnostics
        if: failure()
        uses: actions/upload-artifact@v7
        with:
          path: target/ripr/vscode-test-host-diagnostics/
          retention-days: 5
          if-no-files-found: ignore
"#;
    assert!(
        vscode_host_diagnostics_contract_violations(laundered_exit)
            .iter()
            .any(|violation| violation.contains("launder")),
        "continue-on-error must fail the contract"
    );

    let success_upload_before_later_failure = r#"
  vscode:
    steps:
      - run: xvfb-run -a npm run test:e2e
      - name: Upload VS Code test host diagnostics
        if: success()
        uses: actions/upload-artifact@v7
        with:
          path: target/ripr/vscode-test-host-diagnostics/
          retention-days: 5
          if-no-files-found: ignore
      - name: later summary
        if: failure()
        run: echo later
"#;
    let success_before_later =
        vscode_host_diagnostics_contract_violations(success_upload_before_later_failure);
    assert!(
        success_before_later
            .iter()
            .any(|violation| violation.contains("exactly failure()")),
        "a later failure() step must not credit a success() upload: {success_before_later:?}"
    );
}

#[test]
fn vscode_package_version_reads_extension_manifest() -> Result<(), String> {
    with_temp_cwd("vscode-package-version", |root| {
        let package_json = root.join("package.json");
        fs::write(&package_json, r#"{"version":"0.4.0"}"#)
            .map_err(|err| format!("failed to write {}: {err}", package_json.display()))?;
        assert_eq!(vscode_package_version(&package_json)?, "0.4.0");
        Ok(())
    })
}

#[test]
fn vscode_verify_vsix_path_requires_exactly_one_path() -> Result<(), String> {
    let Err(missing) = super::super::vscode_verify_vsix_path(&[]) else {
        return Err("vscode-verify-vsix must require a VSIX path".to_string());
    };
    assert!(missing.contains("requires a VSIX path"), "{missing}");
    let Err(extra) = super::super::vscode_verify_vsix_path(&[
        "--".to_string(),
        "one.vsix".to_string(),
        "two.vsix".to_string(),
    ]) else {
        return Err("vscode-verify-vsix must reject a second path".to_string());
    };
    assert!(extra.contains("exactly one VSIX path"), "{extra}");
    assert_eq!(
        super::super::vscode_verify_vsix_path(&[
            "--".to_string(),
            "dist/ripr-0.11.0.vsix".to_string()
        ])?,
        PathBuf::from("dist/ripr-0.11.0.vsix")
    );
    Ok(())
}

#[test]
fn vscode_verify_vsix_reuses_sealed_archive_license_check() -> Result<(), String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .ok_or_else(|| "failed to resolve repo root from CARGO_MANIFEST_DIR".to_string())?;
    let notices = super::super::load_repository_license_notices(repo_root)?;
    let mit = String::from_utf8(notices.mit.clone()).map_err(|err| err.to_string())?;
    let apache = String::from_utf8(notices.apache.clone()).map_err(|err| err.to_string())?;
    with_temp_cwd("vscode-verify-vsix-reuse", |root| {
        let good = root.join("good.vsix");
        let license_txt = packaged_license_txt(&mit, &apache);
        write_packaging_test_vsix(&good, &[("extension/LICENSE.txt", &license_txt)])?;
        super::super::vscode_verify_vsix(&[good.to_string_lossy().into_owned()])?;

        let pointer = root.join("pointer.vsix");
        write_packaging_test_vsix(&pointer, &[("extension/LICENSE.txt", POINTER_LICENSE)])?;
        let Err(error) =
            super::super::vscode_verify_vsix(&[pointer.to_string_lossy().into_owned()])
        else {
            return Err("vscode-verify-vsix must reject a pointer-only LICENSE.txt".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_inventory_bounds_entry_count_and_unpacked_size() -> Result<(), String> {
    let three = vec![
        vsix_entry("extension/package.json", 10),
        vsix_entry("extension/out/a.js", 10),
        vsix_entry("extension/out/b.js", 10),
    ];
    super::super::check_vsix_inventory(&three, 3, 30)?;
    let Err(count) = super::super::check_vsix_inventory(&three, 2, 30) else {
        return Err("an entry count above the bound must be rejected".to_string());
    };
    assert!(
        count.contains("3 entries, above the 2-entry bound"),
        "{count}"
    );
    let Err(size) = super::super::check_vsix_inventory(&three, 3, 29) else {
        return Err("an unpacked size above the bound must be rejected".to_string());
    };
    assert!(size.contains("30 bytes, above the 29-byte bound"), "{size}");
    // The production bounds sit between the observed 0.11 package (about 410
    // entries, 3 MiB) and the #1775 defect (2,805 entries, about 2.3 GB).
    const { assert!(super::super::VSIX_MAX_ENTRIES > 410 && super::super::VSIX_MAX_ENTRIES < 2_805) };
    const {
        assert!(
            super::super::VSIX_MAX_UNCOMPRESSED_BYTES > 3 * 1024 * 1024
                && super::super::VSIX_MAX_UNCOMPRESSED_BYTES < 2_300 * 1024 * 1024
        )
    };
    Ok(())
}

#[test]
fn vsix_inventory_fails_closed_on_missing_or_non_zip_package() -> Result<(), String> {
    with_temp_cwd("vsix-inventory-unreadable", |root| {
        // A package step that produced no archive must fail, not pass an empty inventory.
        let missing = root.join("missing.vsix");
        let Err(error) = super::super::read_vsix_inventory(&missing) else {
            return Err("a missing VSIX must not yield an inventory".to_string());
        };
        assert!(error.contains("is missing"), "{error}");

        let not_zip = root.join("not-zip.vsix");
        fs::write(&not_zip, "not a zip archive")
            .map_err(|err| format!("failed to write {}: {err}", not_zip.display()))?;
        let Err(error) = super::super::read_vsix_inventory(&not_zip) else {
            return Err("a non-zip VSIX must not yield an inventory".to_string());
        };
        assert!(error.contains("is not a zip"), "{error}");

        // An intact central directory over a corrupt member header must fail
        // at the member read, not produce a partial inventory.
        let corrupt = root.join("corrupt-member.vsix");
        write_packaging_test_vsix(&corrupt, &[("extension/package.json", "{}")])?;
        let mut bytes = fs::read(&corrupt)
            .map_err(|err| format!("failed to read {}: {err}", corrupt.display()))?;
        assert_eq!(bytes.get(..4), Some(&b"PK\x03\x04"[..]));
        bytes[..2].copy_from_slice(b"XX");
        fs::write(&corrupt, &bytes)
            .map_err(|err| format!("failed to write {}: {err}", corrupt.display()))?;
        let Err(error) = super::super::read_vsix_inventory(&corrupt) else {
            return Err("a corrupt VSIX member must not yield an inventory".to_string());
        };
        assert!(error.contains("member 0"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_inventory_rejects_cargo_artifacts_outside_target() -> Result<(), String> {
    for name in [
        "extension/out/libripr-0123.rlib",
        "extension/out/libripr-0123.rmeta",
        "extension/build/.fingerprint/ripr-0123/lib-ripr",
        "extension/build/incremental/ripr-0123/s-abc/query-cache.bin",
    ] {
        let entries = vec![
            vsix_entry("extension/package.json", 2),
            vsix_entry(name, 10),
        ];
        let Err(error) = super::super::check_vsix_inventory(&entries, 10, 1_000) else {
            return Err(format!("{name} must be rejected as build output"));
        };
        assert!(error.contains(name), "{error}");
    }
    Ok(())
}

#[test]
fn vsix_inventory_rejects_workspace_build_output_sentinel() -> Result<(), String> {
    with_temp_cwd("vsix-inventory-sentinel", |root| {
        let approved = [
            ("[Content_Types].xml", "<Types/>"),
            ("extension.vsixmanifest", "<PackageManifest/>"),
            ("extension/package.json", "{}"),
            ("extension/distribution.json", "{\"schema\":2}"),
            ("extension/out/src/client.js", "exports.x = 1;"),
            // A dependency's own `target/` directory is not workspace output.
            (
                "extension/node_modules/dep/target/index.js",
                "module.exports = 1;",
            ),
        ];
        let clean = root.join("clean.vsix");
        write_packaging_test_vsix(&clean, &approved)?;
        let entries = super::super::read_vsix_inventory(&clean)?;
        assert_eq!(entries.len(), approved.len());
        super::super::check_vsix_inventory(
            &entries,
            super::super::VSIX_MAX_ENTRIES,
            super::super::VSIX_MAX_UNCOMPRESSED_BYTES,
        )?;
        // The production path `vscode-package` runs after `vsce package`.
        let summary = super::super::verify_packaged_vsix_inventory(&clean)?;
        assert!(
            summary.starts_with(&format!("VSIX inventory: {} entries, ", approved.len())),
            "{summary}"
        );

        let sentinel = "extension/target/debug/ripr-1775-sentinel.bin";
        let mut polluted_members = approved.to_vec();
        polluted_members.push((sentinel, "cargo build output"));
        let polluted = root.join("polluted.vsix");
        write_packaging_test_vsix(&polluted, &polluted_members)?;
        let entries = super::super::read_vsix_inventory(&polluted)?;
        let Err(error) = super::super::check_vsix_inventory(
            &entries,
            super::super::VSIX_MAX_ENTRIES,
            super::super::VSIX_MAX_UNCOMPRESSED_BYTES,
        ) else {
            return Err("a packaged editors/vscode/target sentinel must be rejected".to_string());
        };
        assert!(error.contains(sentinel), "{error}");
        assert!(error.contains("1 workspace build-output"), "{error}");
        let Err(error) = super::super::verify_packaged_vsix_inventory(&polluted) else {
            return Err("vscode-package must reject the polluted VSIX".to_string());
        };
        assert!(error.contains("packaged VSIX"), "{error}");
        assert!(error.contains(sentinel), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_accept_license_txt_containing_both_notices() -> Result<(), String> {
    with_temp_cwd("vsix-license-accept", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("licensed.vsix");
        let license_txt = packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE);
        write_packaging_test_vsix(
            &vsix,
            &[
                ("[Content_Types].xml", "<Types/>"),
                ("extension.vsixmanifest", "<PackageManifest/>"),
                ("extension/package.json", "{}"),
                ("extension/LICENSE.txt", &license_txt),
            ],
        )?;
        let summary = super::super::verify_packaged_vsix(&vsix, &notices)?;
        assert!(
            summary.starts_with("VSIX inventory: 4 entries, "),
            "{summary}"
        );
        super::super::verify_packaged_vsix_license_notices(&vsix, &notices)?;
        Ok(())
    })
}

#[test]
fn vsix_license_notices_accept_non_unix_and_permission_only_license_txt() -> Result<(), String> {
    with_temp_cwd("vsix-license-ordinary-types", |root| {
        let notices = complete_license_notices();
        let license_txt = packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE);
        let cases = [
            ("zero-attrs.vsix", ZIP_SYSTEM_DOS, 0),
            ("perm-only.vsix", ZIP_SYSTEM_UNIX, 0o644 << 16),
            ("dos-archive.vsix", ZIP_SYSTEM_DOS, DOS_ARCHIVE_ATTRIBUTE),
            (
                "unix-reg.vsix",
                ZIP_SYSTEM_UNIX,
                (UNIX_S_IFREG | 0o644) << 16,
            ),
        ];
        for (name, system, attributes) in cases {
            let vsix = root.join(name);
            write_typed_license_txt_vsix(&vsix, &license_txt, system, attributes)?;
            super::super::verify_packaged_vsix_license_notices(&vsix, &notices).map_err(|err| {
                format!("{name} must stay accepted as a regular LICENSE.txt: {err}")
            })?;
        }
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_dos_directory_bit_on_unix_regular_mode() -> Result<(), String> {
    with_temp_cwd("vsix-license-dos-dir-on-reg", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("dos-dir-on-reg.vsix");
        let license_txt = packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE);
        // High bits look like a regular Unix file; the DOS directory bit is
        // still enough to reject. zip::unix_mode prefers the high bits.
        write_typed_license_txt_vsix(
            &vsix,
            &license_txt,
            ZIP_SYSTEM_DOS,
            (UNIX_S_IFREG | 0o644) << 16 | DOS_DIRECTORY_ATTRIBUTE,
        )?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err(
                "a LICENSE.txt member with DOS directory metadata must be rejected".to_string(),
            );
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("not a regular file"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_dos_directory_typed_license_txt() -> Result<(), String> {
    with_temp_cwd("vsix-license-dos-dir", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("dos-dir.vsix");
        let license_txt = packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE);
        write_typed_license_txt_vsix(&vsix, &license_txt, ZIP_SYSTEM_DOS, DOS_DIRECTORY_ATTRIBUTE)?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err("a LICENSE.txt member marked DOS directory must be rejected".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("not a regular file"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_fifo_typed_license_txt() -> Result<(), String> {
    with_temp_cwd("vsix-license-fifo", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("fifo.vsix");
        let license_txt = packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE);
        write_typed_license_txt_vsix(
            &vsix,
            &license_txt,
            ZIP_SYSTEM_UNIX,
            (UNIX_S_IFIFO | 0o644) << 16,
        )?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err("a LICENSE.txt member marked FIFO must be rejected".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("not a regular file"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_license_txt_missing_apache() -> Result<(), String> {
    with_temp_cwd("vsix-license-missing-apache", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("mit-only.vsix");
        write_packaging_test_vsix(&vsix, &[("extension/LICENSE.txt", COMPLETE_MIT_NOTICE)])?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err("a LICENSE.txt that omits the Apache notice must be rejected".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("LICENSE-APACHE"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_missing_license_txt() -> Result<(), String> {
    with_temp_cwd("vsix-license-missing-txt", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("no-license-txt.vsix");
        write_packaging_test_vsix(
            &vsix,
            &[
                ("extension/package.json", "{}"),
                ("extension/LICENSE-MIT", COMPLETE_MIT_NOTICE),
                ("extension/LICENSE-APACHE", COMPLETE_APACHE_NOTICE),
            ],
        )?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err("a VSIX missing extension/LICENSE.txt must be rejected".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("missing"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_mutated_notice_bytes() -> Result<(), String> {
    with_temp_cwd("vsix-license-mutated", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("mutated.vsix");
        let mutated_apache =
            COMPLETE_APACHE_NOTICE.replace("TERMS AND CONDITIONS", "TERMS AND CONDITIONZ");
        let mutated = packaged_license_txt(COMPLETE_MIT_NOTICE, &mutated_apache);
        write_packaging_test_vsix(&vsix, &[("extension/LICENSE.txt", &mutated)])?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err("a VSIX with mutated Apache notice bytes must be rejected".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("LICENSE-APACHE"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_pointer_license_txt_even_when_extra_notice_files_exist()
-> Result<(), String> {
    with_temp_cwd("vsix-license-pointer-with-extras", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("pointer-plus-staged.vsix");
        write_packaging_test_vsix(
            &vsix,
            &[
                ("extension/LICENSE.txt", POINTER_LICENSE),
                ("extension/LICENSE-MIT", COMPLETE_MIT_NOTICE),
                ("extension/LICENSE-APACHE", COMPLETE_APACHE_NOTICE),
            ],
        )?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err(
                "extra LICENSE-MIT/LICENSE-APACHE members must not satisfy a pointer LICENSE.txt"
                    .to_string(),
            );
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_pointer_only_license_txt() -> Result<(), String> {
    with_temp_cwd("vsix-license-pointer", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("pointer.vsix");
        write_packaging_test_vsix(
            &vsix,
            &[
                ("[Content_Types].xml", "<Types/>"),
                ("extension.vsixmanifest", "<PackageManifest/>"),
                ("extension/package.json", "{}"),
                ("extension/LICENSE.txt", POINTER_LICENSE),
            ],
        )?;
        // Inventory bounds still pass; the pointer LICENSE.txt is the defect.
        super::super::verify_packaged_vsix_inventory(&vsix)?;
        let Err(error) = super::super::verify_packaged_vsix(&vsix, &notices) else {
            return Err(
                "a VSIX that ships only the historic LICENSE pointer as LICENSE.txt must be rejected"
                    .to_string(),
            );
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("editors/vscode/LICENSE pointer"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_symlink_typed_license_txt() -> Result<(), String> {
    with_temp_cwd("vsix-license-symlink", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("symlink.vsix");
        let license_txt = packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE);
        // Unix symlink member (mode 0120777) whose stored bytes are the
        // complete notices. Payload match is not a regular-file member.
        write_symlink_license_txt_vsix(&vsix, &license_txt)?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err("a LICENSE.txt member marked Unix symlink must be rejected".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("not a regular file"), "{error}");
        Ok(())
    })
}

#[test]
fn vsix_license_notices_reject_unix_directory_typed_license_txt() -> Result<(), String> {
    with_temp_cwd("vsix-license-unix-dir", |root| {
        let notices = complete_license_notices();
        let vsix = root.join("unix-dir.vsix");
        let license_txt = packaged_license_txt(COMPLETE_MIT_NOTICE, COMPLETE_APACHE_NOTICE);
        write_typed_license_txt_vsix(
            &vsix,
            &license_txt,
            ZIP_SYSTEM_UNIX,
            (UNIX_S_IFDIR | 0o755) << 16,
        )?;
        let Err(error) = super::super::verify_packaged_vsix_license_notices(&vsix, &notices) else {
            return Err("a LICENSE.txt member marked Unix directory must be rejected".to_string());
        };
        assert!(error.contains("extension/LICENSE.txt"), "{error}");
        assert!(error.contains("not a regular file"), "{error}");
        Ok(())
    })
}

#[test]
fn windows_absolute_path_tokens_find_tokens_without_trailing_punctuation() {
    let path = concat!("C:", "\\path\\to\\ripr.exe");
    let line = format!("Use `{path}`.");

    assert_eq!(windows_absolute_path_tokens(&line), vec![path.to_string()]);
}

#[test]
fn workflow_bare_self_hosted_policy_accepts_capacity_group() {
    let workflow = r#"
jobs:
  rust:
    runs-on:
      group: em-ci-small
      labels:
        - self-hosted
        - linux
        - x64
        - em-ci
        - cx43
        - rust-medium
        - trusted-pr
"#;

    let violations =
        workflow_bare_self_hosted_violations(".github/workflows/routed-rust.yml", workflow);

    assert!(violations.is_empty());
}

#[test]
fn workflow_bare_self_hosted_policy_rejects_linux_x64_without_group() {
    let workflow = r#"
jobs:
  inline:
    runs-on: [self-hosted, linux, x64]
  block:
    runs-on:
      - self-hosted
      - linux
      - x64
"#;

    let violations = workflow_bare_self_hosted_violations(".github/workflows/ci.yml", workflow);

    assert_eq!(violations.len(), 2);
}

#[test]
fn workflow_review_thread_mutation_policy_accepts_adjudication_guidance() {
    let workflow = r#"
name: Review guidance
jobs:
  review:
    steps:
      - run: echo "Reply with evidence before resolving review threads"
"#;

    assert!(
        workflow_review_thread_mutation_violations(".github/workflows/review.yml", workflow,)
            .is_empty()
    );
}

#[test]
fn workflow_review_thread_mutation_policy_rejects_graphql_resolver() {
    let resolver_name = ["resolve", "ReviewThread"].concat();
    let workflow = format!(
        r#"
jobs:
  review:
    steps:
      - run: gh api graphql -f query='mutation {{ {resolver_name}(input: {{}}) {{ thread {{ isResolved }} }} }}'
"#
    );

    let violations =
        workflow_review_thread_mutation_violations(".github/workflows/review.yml", &workflow);

    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("review-thread resolution mutation"));
}

#[test]
fn workflow_review_thread_mutation_policy_rejects_runtime_constructed_resolver() {
    let workflow = r#"
jobs:
  review:
    steps:
      - run: const resolver = 'resolve' + 'ReviewThread'; gh api graphql "$resolver"
"#;

    let violations =
        workflow_review_thread_mutation_violations(".github/workflows/review.yml", workflow);

    assert_eq!(violations.len(), 1);
}

#[test]
fn workflow_run_extraction_handles_step_shorthand_and_blocks() {
    let workflow = r#"
jobs:
  test:
    steps:
      - run: cargo fmt --check
      - name: block
        run: |
          cargo check
          cargo test
      - uses: actions/checkout@v4
"#;

    let blocks = extract_workflow_run_blocks(workflow);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].line_number, 5);
    assert_eq!(blocks[0].non_empty_lines, 1);
    assert_eq!(blocks[0].text, "cargo fmt --check");
    assert_eq!(blocks[1].line_number, 7);
    assert_eq!(blocks[1].non_empty_lines, 2);
    assert!(blocks[1].text.contains("cargo check"));
    assert!(blocks[1].text.contains("cargo test"));
}

#[test]
fn workflow_runtime_policy_allows_documented_dependency_review_exception() {
    let workflow = r#"
jobs:
  dependency-review:
    steps:
      - uses: actions/checkout@v6
      - uses: actions/dependency-review-action@v4
"#;
    let mut allowlist = BTreeMap::new();
    allowlist.insert(
        (
            ".github/workflows/security.yml".to_string(),
            "actions/dependency-review-action@v4".to_string(),
        ),
        1,
    );

    assert!(
        workflow_runtime_violations(".github/workflows/security.yml", workflow, &allowlist,)
            .is_empty()
    );
    assert!(
        !workflow_runtime_violations(".github/workflows/security.yml", workflow, &BTreeMap::new(),)
            .is_empty()
    );
}

#[test]
fn workflow_runtime_policy_flags_old_action_refs_and_node20_extension_builds() {
    let workflow = r#"
jobs:
  vscode:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - uses: actions/upload-artifact@v4
"#;
    let violations =
        workflow_runtime_violations(".github/workflows/ci.yml", workflow, &BTreeMap::new());

    assert!(violations.iter().any(|violation| {
        violation.contains("actions/checkout@v4") && violation.contains("actions/checkout@v6")
    }));
    assert!(violations.iter().any(|violation| {
        violation.contains("actions/setup-node@v4") && violation.contains("actions/setup-node@v6")
    }));
    assert!(violations.iter().any(|violation| {
        violation.contains("actions/upload-artifact@v4")
            && violation.contains("actions/upload-artifact@v7")
    }));
    assert!(
        violations
            .iter()
            .any(|violation| { violation.contains("uses Node 20 for extension tooling") })
    );
}

#[test]
fn workflow_runtime_policy_flags_remaining_old_action_refs() {
    let workflow = r#"
jobs:
  release:
    steps:
      - uses: actions/download-artifact@v4
      - uses: codecov/codecov-action@v4
"#;
    let violations = workflow_runtime_violations(
        ".github/workflows/release-server-binaries.yml",
        workflow,
        &BTreeMap::new(),
    );

    assert!(violations.iter().any(|violation| {
        violation.contains("actions/download-artifact@v4")
            && violation.contains("actions/download-artifact@v8")
    }));
    assert!(violations.iter().any(|violation| {
        violation.contains("codecov/codecov-action@v4")
            && violation.contains("codecov/codecov-action@v6")
    }));
}

#[test]
fn workflow_runtime_policy_ignores_node20_outside_extension_workflows() {
    let workflow = r#"
jobs:
  coverage:
    steps:
      - uses: actions/setup-node@v6
        with:
          node-version: 20
"#;

    assert!(
        workflow_runtime_violations(".github/workflows/coverage.yml", workflow, &BTreeMap::new(),)
            .is_empty()
    );
}

#[test]
fn workflow_runtime_policy_rejects_dependency_review_over_allowlisted_count() {
    let workflow = r#"
jobs:
  dependency-review:
    steps:
      - uses: actions/dependency-review-action@v4
      - uses: actions/dependency-review-action@v4
"#;
    let mut allowlist = BTreeMap::new();
    allowlist.insert(
        (
            ".github/workflows/security.yml".to_string(),
            "actions/dependency-review-action@v4".to_string(),
        ),
        1,
    );

    let violations =
        workflow_runtime_violations(".github/workflows/security.yml", workflow, &allowlist);

    assert!(violations.iter().any(|violation| {
        violation.contains("uses `actions/dependency-review-action@v4` 2 time(s), allowed 1")
    }));
}

#[test]
fn workflow_runtime_policy_rejects_unsupported_allowlist_patterns() {
    let workflow = r#"
jobs:
  security:
    steps:
      - uses: actions/checkout@v6
"#;
    let mut allowlist = BTreeMap::new();
    allowlist.insert(
        (
            ".github/workflows/security.yml".to_string(),
            "actions/unknown-action@v1".to_string(),
        ),
        1,
    );

    let violations =
        workflow_runtime_violations(".github/workflows/security.yml", workflow, &allowlist);

    assert!(violations.iter().any(|violation| {
        violation.contains("unsupported exception")
            && violation.contains("actions/unknown-action@v1")
    }));
}

#[test]
fn worktree_command_rejects_missing_and_unknown_subcommands() -> Result<(), String> {
    let Err(missing) = worktree(&[]) else {
        return Err("missing subcommand should fail".to_string());
    };
    assert!(missing.contains("missing worktree command"));
    assert!(missing.contains("cargo xtask worktree doctor"));

    let Err(unknown) = worktree(&["inspect".to_string()]) else {
        return Err("unknown subcommand should fail".to_string());
    };
    assert!(unknown.contains("unknown worktree command"));
    assert!(unknown.contains("cargo xtask worktree doctor"));
    Ok(())
}

#[test]
fn worktree_doctor_clean_report_names_next_gate() -> Result<(), String> {
    with_temp_cwd(
        "worktree-doctor-clean-next-action",
        |root| -> Result<(), String> {
            finish_worktree_doctor_report(&[])?;
            let report_path = root.join("target/ripr/reports/worktree-doctor.md");
            let report = fs::read_to_string(&report_path)
                .map_err(|err| format!("failed to read {}: {err}", report_path.display()))?;
            assert!(report.contains("Status: pass"));
            assert!(report.contains("Next actions:"));
            assert!(report.contains("continue with `cargo xtask precommit`"));
            Ok(())
        },
    )
}

#[test]
fn worktree_doctor_command_writes_clean_report_in_git_worktree() -> Result<(), String> {
    with_temp_cwd("worktree-doctor-command", |root| -> Result<(), String> {
        run("git", &["init", "-b", "feature"])?;
        worktree(&["doctor".to_string()])?;
        let report_path = root.join("target/ripr/reports/worktree-doctor.md");
        let report = fs::read_to_string(&report_path)
            .map_err(|err| format!("failed to read {}: {err}", report_path.display()))?;

        assert!(report.contains("Status: pass"));
        assert!(report.contains("No findings."));
        Ok(())
    })
}

#[test]
fn worktree_doctor_flags_dirty_main_and_behind_branch() {
    let changes = vec![changed_path("docs/README.md", &["M"])];
    let findings = worktree_doctor_findings("main", 2, &changes, false, false, false);

    assert!(doctor_has_error(
        &findings,
        "main branch has uncommitted changes"
    ));
    assert!(doctor_has_error(
        &findings,
        "behind origin/main by 2 commit"
    ));
}

#[test]
fn worktree_doctor_rejects_badge_endpoint_outside_refresh_context() {
    let changes = vec![changed_path("badges/ripr.json", &["M"])];
    let findings = worktree_doctor_findings("badge/diff-policy", 0, &changes, false, false, false);

    assert!(doctor_has_error(
        &findings,
        "generated badge endpoint is dirty"
    ));

    let refresh_findings =
        worktree_doctor_findings("badge-refresh", 0, &changes, false, false, true);
    assert!(
        !doctor_has_error(&refresh_findings, "generated badge endpoint is dirty"),
        "unexpected findings: {refresh_findings:?}"
    );
}

#[test]
fn worktree_doctor_rejects_detached_head() {
    let findings = worktree_doctor_findings("HEAD", 0, &[], false, false, false);

    assert!(doctor_has_error(&findings, "detached HEAD"));
}

#[test]
fn worktree_doctor_rejects_dirty_generated_artifacts() {
    let changes = vec![
        changed_path("target/ripr/reports/policy-operations.json", &["A"]),
        changed_path("crates/ripr/examples/sample/target/debug/sample", &["A"]),
    ];
    let findings = worktree_doctor_findings("feature/generated", 0, &changes, false, false, false);

    assert!(doctor_has_error(
        &findings,
        "generated RIPR target artifact is dirty"
    ));
    assert!(doctor_has_error(
        &findings,
        "sample workspace target artifact is dirty"
    ));

    let deletion = vec![changed_path("target/ripr/reports/old.json", &["D"])];
    let deletion_findings =
        worktree_doctor_findings("feature/generated", 0, &deletion, false, false, false);
    assert!(
        !doctor_has_error(
            &deletion_findings,
            "generated RIPR target artifact is dirty"
        ),
        "unexpected findings: {deletion_findings:?}"
    );
}

#[test]
fn worktree_doctor_report_renders_warnings_and_errors() -> Result<(), String> {
    with_temp_cwd("worktree-doctor-report", |root| -> Result<(), String> {
        let warnings = vec![WorktreeDoctorFinding {
            severity: WorktreeDoctorSeverity::Warning,
            message: "target/ripr exists".to_string(),
        }];
        finish_worktree_doctor_report(&warnings)?;
        let report_path = root.join("target/ripr/reports/worktree-doctor.md");
        let warning_report = fs::read_to_string(&report_path)
            .map_err(|err| format!("failed to read {}: {err}", report_path.display()))?;
        assert!(warning_report.contains("Status: warn"));
        assert!(warning_report.contains("Warnings:"));
        assert!(warning_report.contains("Next actions:"));
        assert!(warning_report.contains("remove local `target/ripr` report artifacts"));
        assert!(warning_report.contains("rerun `cargo xtask worktree doctor`"));

        let errors = vec![WorktreeDoctorFinding {
            severity: WorktreeDoctorSeverity::Error,
            message: "branch is behind origin/main".to_string(),
        }];
        let Err(err) = finish_worktree_doctor_report(&errors) else {
            return Err("error findings should fail the doctor report".to_string());
        };
        assert!(err.contains("worktree doctor found blocking issues"));
        let error_report = fs::read_to_string(&report_path)
            .map_err(|err| format!("failed to read {}: {err}", report_path.display()))?;
        assert!(error_report.contains("Status: fail"));
        assert!(error_report.contains("Errors:"));
        assert!(error_report.contains("refresh the branch from `origin/main`"));
        Ok(())
    })
}

#[test]
fn worktree_doctor_warns_about_broad_diff_without_work_item_marker() {
    let changes = vec![
        changed_path("docs/specs/RIPR-SPEC-9999-example.md", &["A"]),
        changed_path(".ripr/traceability.toml", &["M"]),
        changed_path("metrics/capabilities.toml", &["M"]),
        changed_path("xtask/src/main.rs", &["M"]),
    ];
    let findings = worktree_doctor_findings("feature/broad", 0, &changes, false, false, false);
    assert!(doctor_has_warning(
        &findings,
        "changes span multiple source-of-truth layers"
    ));

    let with_marker = vec![
        changed_path("docs/specs/RIPR-SPEC-9999-example.md", &["A"]),
        changed_path(".ripr/traceability.toml", &["M"]),
        changed_path("metrics/capabilities.toml", &["M"]),
        changed_path("xtask/src/main.rs", &["M"]),
        changed_path(".ripr/goals/active.toml", &["M"]),
    ];
    let marker_findings =
        worktree_doctor_findings("feature/broad", 0, &with_marker, false, false, false);
    assert!(
        !doctor_has_warning(
            &marker_findings,
            "changes span multiple source-of-truth layers"
        ),
        "unexpected findings: {marker_findings:?}"
    );
}

#[test]
fn worktree_doctor_warns_about_ignored_target_residue() {
    let findings = worktree_doctor_findings("feature/x", 0, &[], true, true, false);

    assert!(doctor_has_warning(&findings, "target/ripr exists"));
    assert!(doctor_has_warning(
        &findings,
        "crates/ripr/examples/sample/target exists"
    ));
}

#[test]
fn xtask_run_helpers_report_success_failure_and_optional_output() -> Result<(), String> {
    with_repo_cwd(|| {
        let program = "rustc";
        let invalid_flag = "--definitely-not-a-real-rustc-flag";

        let version = run_output(program, &["--version"])?;
        if !version.contains("rustc") {
            return Err(format!("expected rustc version output, got {version:?}"));
        }

        let owned_version = run_output_owned(program, &["--version".to_string()])?;
        if !owned_version.contains("rustc") {
            return Err(format!(
                "expected owned rustc version output, got {owned_version:?}"
            ));
        }

        let status = run(program, &["--version"])?;
        if !status.success() {
            return Err(format!("expected rustc --version to succeed, got {status}"));
        }

        let captured = capture_output(program, &["--version"], "rustc version")?;
        if !captured.status.success() || !captured.stdout.contains("rustc") {
            return Err(format!(
                "expected captured rustc version output, got status={} stdout={:?}",
                captured.status, captured.stdout
            ));
        }

        let failed_capture = capture_output(program, &[invalid_flag], "rustc invalid flag")?;
        if failed_capture.status.success() {
            return Err("expected invalid rustc flag to fail".to_string());
        }

        let optional = run_output_optional(program, &[invalid_flag])?;
        if !optional.is_empty() {
            return Err(format!(
                "expected optional failure to return empty output, got {optional:?}"
            ));
        }

        let failure = run_output(program, &[invalid_flag]).is_err();
        if !failure {
            return Err("expected run_output to report non-zero exit".to_string());
        }

        let owned_failure = run_output_owned(program, &[invalid_flag.to_string()]).is_err();
        if !owned_failure {
            return Err("expected run_output_owned to report non-zero exit".to_string());
        }

        let missing_program = capture_output(
            "definitely-missing-ripr-test-binary",
            &[],
            "missing test binary",
        );
        if missing_program.is_ok() {
            return Err("expected missing executable to report spawn error".to_string());
        }

        Ok(())
    })
}
