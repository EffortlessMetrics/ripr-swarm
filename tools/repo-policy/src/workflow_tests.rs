use super::*;

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
fn workflow_review_thread_mutation_policy_rejects_graphql_resolver() {
    let verb = "resolve";
    let noun = "ReviewThread";
    let resolver_name = format!("{verb}{noun}");
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
      - run: const resolver = '__head__' + '__tail__'; gh api graphql "$resolver"
"#
    .replace("__head__", "resolve")
    .replace("__tail__", "ReviewThread");

    let violations =
        workflow_review_thread_mutation_violations(".github/workflows/review.yml", &workflow);

    assert_eq!(violations.len(), 1);
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

#[test]
fn routed_rust_contract_catches_deadline_bypass_with_stray_occurrences() {
    // #2230 review: a global `timeout-minutes:` occurrence count passes even
    // when a named job lost its deadline but stray tokens (comments, other
    // jobs, duplicates) keep the total at eight. The per-job check must fire
    // on the job that lost its deadline.
    let workflow = r#"
name: Routed Rust Small
jobs:
  route:
    runs-on: ubuntu-latest
    timeout-minutes: 10
  detect-docs-only:
    runs-on: ubuntu-latest
    timeout-minutes: 10
  rust-cx43:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    # timeout-minutes: 60 (stray occurrence in a comment)
  rust-cpx42:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    timeout-minutes: 60
  rust-cx53:
    runs-on: ubuntu-latest
    timeout-minutes: 60
  rust-github:
    runs-on: ubuntu-latest
    timeout-minutes: 90
  docs-gate:
    runs-on: ubuntu-latest
    timeout-minutes: 20
  result:
    runs-on: ubuntu-latest
"#;
    let violations = routed_rust_workflow_contract_violations(workflow, None, None);
    assert!(
        violations.iter().any(|violation| violation
            .contains("job `result` must set an explicit `timeout-minutes` job deadline")),
        "missing per-job deadline must fire even with 8 stray occurrences: {violations:?}"
    );
    assert!(
        !violations
            .iter()
            .any(|violation| violation.contains("job `route` must set an explicit")),
        "a job that kept its deadline must not be flagged: {violations:?}"
    );
}
