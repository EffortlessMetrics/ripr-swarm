use super::*;

#[test]
fn coverage_failure_capture_preserves_original_command_and_failure() -> Result<(), String> {
    let root = crate::test_support::workspace_root()?;
    let text = fs::read_to_string(root.join(".github/workflows/coverage.yml"))
        .map_err(|error| error.to_string())?;
    assert!(text.contains("id: generate-coverage\n        run: cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info"));
    assert!(!text.contains("continue-on-error:"));
    assert!(text.contains("failure() && steps.generate-coverage.outcome == 'failure'"));
    assert!(text.contains("always() && steps.generate-coverage.outcome == 'failure'"));
    assert!(text.contains("name: coverage-failure-diagnostics"));
    Ok(())
}

#[test]
fn linux_llvm_proof_routes_prepare_the_pinned_component_before_tests() -> Result<(), String> {
    let root = crate::test_support::workspace_root()?;
    let routes = [
        (
            "rust-gates.yml",
            "cargo nextest run --workspace --profile ci",
        ),
        (
            "test-analytics.yml",
            "cargo nextest run --workspace --all-features --profile ci",
        ),
        (
            "coverage.yml",
            "cargo test -p repo-policy --lib coverage_diagnostics",
        ),
        ("ub-review.yml", "uses: EffortlessMetrics/ub-review@"),
    ];
    for (route, command) in routes {
        let action = if route == "ub-review.yml" {
            "e2a55d2ffb04f378e9626c28d38b36d230d1e12f # v1"
        } else {
            "v1"
        };
        let prepared = |text: &str, command: &str| {
            let setup = format!(
                "      - uses: dtolnay/rust-toolchain@{action}\n        with:\n          toolchain: 1.95.0\n          components: llvm-tools-preview"
            );
            match (text.find(setup.as_str()), text.find(command)) {
                (Some(setup), Some(test)) => {
                    setup < test
                        && text[setup..]
                            .split("\n      - ")
                            .next()
                            .is_some_and(|step| {
                                !step.lines().any(|line| line.starts_with("        if:"))
                            })
                }
                _ => false,
            }
        };
        let text = fs::read_to_string(root.join(".github/workflows").join(route))
            .map_err(|error| error.to_string())?;
        assert!(
            prepared(&text, command),
            "{route}: mandatory LLVM proof lacks pinned preparation"
        );
        assert!(!prepared(
            &text.replace("components: llvm-tools-preview", "components: clippy"),
            command
        ));
        assert!(!prepared(
            &text.replace("components: llvm-tools-preview", ""),
            command
        ));
        assert!(!prepared(
            &text.replace("toolchain: 1.95.0", "toolchain: stable"),
            command
        ));
        let test_first = format!("{command}\n{text}");
        assert!(!prepared(&test_first, command));
        if route == "ub-review.yml" {
            assert!(!prepared(&text.replace(action, "v1"), command));
        }
        assert!(!prepared(
            &text.replace(
                "components: llvm-tools-preview",
                "components: llvm-tools-preview\n        if: false",
            ),
            command
        ));
    }
    Ok(())
}

#[test]
fn automatic_pr_plan_placeholder_is_retired() -> Result<(), String> {
    let root = crate::test_support::workspace_root()?;
    let workflow = ".github/workflows/pr-plan.yml";
    assert!(
        !root
            .join(workflow)
            .try_exists()
            .map_err(|e| e.to_string())?,
        "the retired PR Plan placeholder must not spend automatic runners"
    );
    let allowance = root.join("policy/workflow_allowlist.txt");
    let text = fs::read_to_string(&allowance).map_err(|e| e.to_string())?;
    let budgets = parse_workflow_budgets("policy/workflow_allowlist.txt", &text)?;
    assert!(
        budgets.iter().all(|budget| budget.path != workflow),
        "the retired PR Plan placeholder must not retain a workflow allowance"
    );
    Ok(())
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
on:
  merge_group:
    types: [checks_requested]
env:
  GROUP_HEAD_REF: ${{ github.event.merge_group.head_ref }}
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
on:
  merge_group:
    types: [checks_requested]
env:
  GROUP_HEAD_REF: ${{ github.event.merge_group.head_ref }}
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
            Some("labeled"),
            Some("merge-ready"),
            RoutedRustEventRoute::LaunchFullGate,
        ),
        (
            "pull_request",
            Some("labeled"),
            Some("Merge-Ready"),
            RoutedRustEventRoute::LaunchFullGate,
        ),
        (
            "pull_request",
            Some("ready_for_review"),
            None,
            RoutedRustEventRoute::WorkflowNotTriggered,
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
            RoutedRustEventRoute::NonrequestLabel,
        ),
        (
            "pull_request",
            Some("labeled"),
            Some("windows-ci"),
            RoutedRustEventRoute::NonrequestLabel,
        ),
        (
            "pull_request",
            Some("labeled"),
            Some("macos-ci"),
            RoutedRustEventRoute::NonrequestLabel,
        ),
        (
            "pull_request",
            Some("unlabeled"),
            Some("full-ci"),
            RoutedRustEventRoute::WorkflowNotTriggered,
        ),
        (
            "merge_group",
            Some("checks_requested"),
            None,
            RoutedRustEventRoute::LaunchFullGate,
        ),
        (
            "merge_group",
            Some("destroyed"),
            None,
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

    let draft_resurrected =
        workflow.replace("    types: [labeled]", "    types: [labeled, synchronize]");
    assert!(
        routed_rust_ready_event_contract_violations(&draft_resurrected)
            .iter()
            .any(|violation| {
                violation.contains("must be exactly") && violation.contains("synchronize")
            }),
        "re-admitting synchronize must fail the merge-ready request contract: {:?}",
        routed_rust_ready_event_contract_violations(&draft_resurrected)
    );

    let ready_resurrected = workflow.replace(
        "    types: [labeled]",
        "    types: [labeled, ready_for_review]",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&ready_resurrected)
            .iter()
            .any(|violation| {
                violation.contains("must be exactly") && violation.contains("ready_for_review")
            }),
        "re-admitting the Ready transition must fail the merge-ready contract: {:?}",
        routed_rust_ready_event_contract_violations(&ready_resurrected)
    );

    let edited_resurrected =
        workflow.replace("    types: [labeled]", "    types: [labeled, edited]");
    assert!(
        routed_rust_ready_event_contract_violations(&edited_resurrected)
            .iter()
            .any(|violation| {
                violation.contains("must be exactly") && violation.contains("edited")
            }),
        "re-admitting the edited activity type must fail the merge-ready request contract: {:?}",
        routed_rust_ready_event_contract_violations(&edited_resurrected)
    );

    let ready_dropped = workflow.replace(
        "    types: [labeled]",
        "    types: [opened, synchronize, reopened]",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&ready_dropped)
            .iter()
            .any(|violation| violation.contains("must be exactly")),
        "dropping the Ready transition must fail closed: {:?}",
        routed_rust_ready_event_contract_violations(&ready_dropped)
    );

    let missing_types = workflow.replace("    types: [labeled]\n", "");
    assert!(
        routed_rust_ready_event_contract_violations(&missing_types)
            .iter()
            .any(|violation| violation.contains("inline pull_request types array")),
        "removing types must fail closed: {:?}",
        routed_rust_ready_event_contract_violations(&missing_types)
    );

    let no_triggers = workflow
        .replace("  pull_request:\n    types: [labeled]\n", "")
        .replace("  merge_group:\n", "  merge_group_removed:\n");
    assert_ne!(no_triggers, workflow, "fixture must remove both triggers");
    assert!(
        routed_rust_ready_event_contract_violations(&no_triggers)
            .iter()
            .any(|violation| violation.contains("inline merge_group types array")),
        "a workflow with neither trigger must still fail the queue contract: {:?}",
        routed_rust_ready_event_contract_violations(&no_triggers)
    );
    let queue_dropped = workflow.replace("    types: [checks_requested]\n", "");
    assert_ne!(
        queue_dropped, workflow,
        "fixture must remove the queue types"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&queue_dropped)
            .iter()
            .any(|violation| violation.contains("inline merge_group types array")),
        "removing the merge-queue types must fail closed: {:?}",
        routed_rust_ready_event_contract_violations(&queue_dropped)
    );
    let queue_widened = workflow.replace(
        "    types: [checks_requested]",
        "    types: [checks_requested, destroyed]",
    );
    assert_ne!(
        queue_widened, workflow,
        "fixture must widen the queue types"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&queue_widened)
            .iter()
            .any(|violation| violation.contains("merge_group types must be exactly")),
        "extra merge_group activity types must fail: {:?}",
        routed_rust_ready_event_contract_violations(&queue_widened)
    );
    let queue_unbound = workflow.replace(
        "GROUP_HEAD_REF: ${{ github.event.merge_group.head_ref }}",
        "GROUP_HEAD_REF: ''",
    );
    assert_ne!(queue_unbound, workflow, "fixture must unbind the queued PR");
    assert!(
        routed_rust_ready_event_contract_violations(&queue_unbound)
            .iter()
            .any(|violation| violation.contains("resolve the queued PR")),
        "admission must stay bound to the queued PR: {:?}",
        routed_rust_ready_event_contract_violations(&queue_unbound)
    );

    for (needle, snippet) in [
        (
            "forward the queued PR's labels",
            "pr-labels: ${{ needs.admission.outputs.labels }}",
        ),
        (
            "fork guard to queued PRs",
            "|| [ \"$EVENT_NAME\" = \"merge_group\" ]; } && [ \"$HEAD_REPO\"",
        ),
        (
            "more than one queued PR",
            "[ \"$parents\" = \"1\" ] && [ \"$parent\" = \"$GROUP_BASE_SHA\" ] && [[ \"$subject\" == *\"(#${pr})\" ]]",
        ),
        (
            "bind the queue ref sha",
            "[ \"$ref_sha\" = \"$head\" ] || [ \"$ref_sha\" = \"$GROUP_BASE_SHA\" ]",
        ),
        (
            "check the live head is not newer",
            "[ \"$ref_sha\" != \"$head\" ]",
        ),
    ] {
        let mutated = workflow.replace(snippet, "removed");
        assert_ne!(mutated, workflow, "fixture must remove {needle}");
        assert!(
            routed_rust_ready_event_contract_violations(&mutated)
                .iter()
                .any(|violation| violation.contains(needle)),
            "dropping the {needle} pin must fail: {:?}",
            routed_rust_ready_event_contract_violations(&mutated)
        );
    }
    let cancellation_disabled = workflow.replace(
        "  cancel-in-progress: ${{ github.event_name == 'pull_request' }}",
        "  cancel-in-progress: false",
    );
    assert!(
        routed_rust_ready_event_contract_violations(&cancellation_disabled)
            .iter()
            .any(|violation| violation.contains("cancel-in-progress")),
        "disabling merge-ready cancellation must fail: {:?}",
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
        "format('{0}-{1}-{2}', github.workflow, github.event.pull_request.number || github.ref, github.event_name)",
        "format('{0}-{1}', github.workflow, github.event.pull_request.number || github.ref)",
    );
    assert_ne!(shared_group, workflow, "fixture must rewrite the group");
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
        "'Merge-ready Nonrequest' }}",
        "'Ripr Rust Small Ignored Label Event' }}",
    );
    assert_ne!(
        pseudo_result, workflow,
        "fixture must rewrite the nonrequest name"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&pseudo_result)
            .iter()
            .any(|violation| violation.contains("Ignored Label Event")),
        "resurrecting the ignored-label pseudo-result must fail: {:?}",
        routed_rust_ready_event_contract_violations(&pseudo_result)
    );

    let renamed_result = workflow.replace(
        "&& 'Ripr Rust Small Result' ||",
        "&& 'Ripr Rust Small Draft Result' ||",
    );
    assert_ne!(
        renamed_result, workflow,
        "fixture must rename the required context"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&renamed_result)
            .iter()
            .any(|violation| violation.contains("result job must be named by exactly")),
        "renaming the required result context must fail: {:?}",
        routed_rust_ready_event_contract_violations(&renamed_result)
    );

    // Negative experiments for the skipped-required-job hazard: each mutation
    // would let a nonrequest event produce, or a request skip, the required
    // context, and must be rejected by name.
    let static_required_name = workflow.replace(
        "    name: ${{ (github.event_name != 'pull_request' || github.event.label.name == 'merge-ready') && 'Ripr Rust Small Result' || 'Merge-ready Nonrequest' }}",
        "    name: Ripr Rust Small Result",
    );
    assert_ne!(
        static_required_name, workflow,
        "fixture must rewrite the result name"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&static_required_name)
            .iter()
            .any(|violation| violation.contains("result job must be named by exactly")),
        "a static result name posts the required context for ordinary labels: {:?}",
        routed_rust_ready_event_contract_violations(&static_required_name)
    );
    let any_label_request = workflow.replace(
        "github.event.label.name == 'merge-ready') && 'Ripr Rust Small Result'",
        "github.event.label.name != '') && 'Ripr Rust Small Result'",
    );
    assert_ne!(
        any_label_request, workflow,
        "fixture must widen the request label"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&any_label_request)
            .iter()
            .any(|violation| violation.contains("result job must be named by exactly")),
        "any label must not earn the required context: {:?}",
        routed_rust_ready_event_contract_violations(&any_label_request)
    );
    let skippable_result = workflow.replace(
        "    if: always()\n    runs-on: ubuntu-latest\n    timeout-minutes: 10\n    steps:\n      - name: Summarize routed result",
        "    if: success()\n    runs-on: ubuntu-latest\n    timeout-minutes: 10\n    steps:\n      - name: Summarize routed result",
    );
    assert_ne!(
        skippable_result, workflow,
        "fixture must make the result skippable"
    );
    assert!(
        routed_rust_ready_event_contract_violations(&skippable_result)
            .iter()
            .any(|violation| violation.contains("job-level `if: always()`")),
        "a skippable result reports success when skipped: {:?}",
        routed_rust_ready_event_contract_violations(&skippable_result)
    );
    for job in ["route", "detect-docs-only", "admission"] {
        let unguarded = workflow.replace(
            "github.event_name != 'pull_request' || github.event.label.name == 'merge-ready'\n",
            "true\n",
        )
        .replace(
            "github.event_name != 'pull_request' || github.event.label.name == 'merge-ready')\n",
            "true)\n",
        );
        assert_ne!(
            unguarded, workflow,
            "fixture must remove the request guards"
        );
        let violations = routed_rust_ready_event_contract_violations(&unguarded);
        assert!(
            violations.iter().any(|violation| violation
                .contains(&format!("job `{job}` must carry the request guard"))),
            "job {job} must be guarded against nonrequest labels: {violations:?}"
        );
    }
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

#[test]
fn advisory_review_prepares_locked_fixture_before_sensors() -> Result<(), String> {
    let root = crate::test_support::workspace_root()?;
    let text = fs::read_to_string(root.join(".github/workflows/ub-review.yml"))
        .map_err(|error| error.to_string())?;
    let header = "name: Prepare locked owner-return fixture dependencies";
    let fetch = "        run: cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml";
    let preparation = format!("      - {header}\n{fetch}\n");
    let prepared = |source: &str| {
        let steps = source.split("\n      - ").collect::<Vec<_>>();
        let preparations = steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.lines().next() == Some(header))
            .collect::<Vec<_>>();
        let [(index, step)] = preparations.as_slice() else {
            return false;
        };
        // The action sanitizes sensor environments. Both commands must use
        // the runner default Cargo home, with no workflow override.
        let lines = step
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        !source.lines().any(|line| {
            let line = line.trim();
            line.starts_with("CARGO_HOME:") || line.starts_with("HOME:")
        }) && lines == [header, fetch]
            && steps[..*index]
                .iter()
                .any(|step| step.starts_with("uses: dtolnay/rust-toolchain@"))
            && steps[*index + 1..].iter().any(|step| {
                step.lines().any(|line| {
                    line.trim()
                        .starts_with("uses: EffortlessMetrics/ub-review@")
                })
            })
    };
    assert!(
        prepared(&text),
        "advisory sensors require locked fixture preparation"
    );
    for wrong in [
        text.replace(&preparation, ""),
        text.replace(
            "cargo fetch --locked --manifest-path",
            "cargo fetch --manifest-path",
        ),
        text.replace(fetch, &format!("        if: false\n{fetch}")),
        text.replace(fetch, &format!("        continue-on-error: true\n{fetch}")),
        text.replace(fetch, &format!("{fetch} || true")),
        text.replace(
            "--manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml",
            "--manifest-path Cargo.toml",
        ),
        format!("env:\n  CARGO_HOME: other\n{text}"),
        format!("env:\n  HOME: other\n{text}"),
        text.replace(
            fetch,
            &format!("{fetch}\n        env:\n          CARGO_HOME: other"),
        ),
        format!("{}\n{preparation}", text.replace(&preparation, "")),
        text.replace(&preparation, &format!("{preparation}\n{preparation}")),
    ] {
        assert!(
            !prepared(&wrong),
            "invalid fixture preparation must be rejected"
        );
    }
    Ok(())
}
