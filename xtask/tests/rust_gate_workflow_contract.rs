//! Guard the deliberately simple, one-producer-per-required-step CI surface.
//! The repository workflow gate remains responsible for general YAML policy.

const WORKFLOW: &str = include_str!("../../.github/workflows/rust-gates.yml");
const SHARDS: &str = include_str!("../../.github/workflows/rust-test-shards.yml");
const ROUTED: &str = include_str!("../../.github/workflows/routed-rust.yml");
/// The only condition the required test step may carry: the hosted caller passes
/// `test-shards: 4` and runs the same tests in `rust-test-shards.yml` instead (#10507).
const SHARDED_TEST_CONDITION: &str = "        if: inputs.test-shards == 1";
const SHARDED_TEST_COMMAND: &str =
    "cargo nextest run --workspace --profile ci --partition hash:${{ matrix.shard }}/4";
const SHARDED_LIST_COMMAND: &str = "cargo nextest list --workspace --profile ci --partition hash:${{ matrix.shard }}/4 --color never --message-format oneline 2> /dev/null | sort > target/nextest/ci/list-${{ matrix.shard }}.txt";
const SHARDED_RECEIPT_COMMAND: &str = "printf 'profile=ci\\nshard=%s/4\\ncommand=cargo nextest run --workspace --profile ci --partition hash:%s/4\\n' \"${{ matrix.shard }}\" \"${{ matrix.shard }}\"";
const COMBINED_PR_EVIDENCE_PRINTF: &str =
    "printf '| pr_evidence | %s |\\n| pr_evidence scope | %s |\\n' \"${PR_EVIDENCE_RESULT:-not_reported}\" \"${PR_EVIDENCE_SCOPE:-not_reported}\"";
const REQUIRED_GATES: &[(&str, &str)] = &[
    ("formatting", "cargo fmt --check"),
    ("agent_skills", "cargo policy preflight"),
    ("precommit", "cargo xtask ci-policy"),
    (
        "promotion_honesty",
        "cargo xtask check-evidence-promotion-honesty",
    ),
    ("dependencies", "cargo xtask check-dependencies"),
    ("process_policy", "cargo xtask check-process-policy"),
    ("network_policy", "cargo xtask check-network-policy"),
    (
        "all_features_check",
        "cargo check -p ripr --all-targets --all-features",
    ),
    (
        "clippy",
        "cargo clippy --workspace --all-targets -- -D warnings",
    ),
    ("goldens", "cargo xtask goldens check"),
    ("fixtures", "cargo xtask fixtures"),
    (
        "capability_producers",
        "cargo xtask check-capability-producers",
    ),
    ("capability_docs", "cargo xtask capability-docs --check"),
    ("verdict_corpus", "cargo xtask verdict-corpus check-all"),
    // After the fast gates so main-side drift fails within minutes (#10341).
    ("rust-tests", "cargo nextest run --workspace --profile ci"),
];

fn step_id(block: &str) -> Option<&str> {
    block
        .lines()
        .find_map(|line| line.strip_prefix("        id: "))
}

fn contract_violations(workflow: &str) -> Vec<String> {
    let blocks: Vec<_> = workflow.split("\n      - ").skip(1).collect();
    let mut findings = Vec::new();
    let mut previous = None;
    for &(id, command) in REQUIRED_GATES {
        let matches: Vec<_> = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| step_id(block) == Some(id))
            .collect();
        if matches.len() != 1 {
            findings.push(format!("{id}: expected one step"));
            continue;
        }
        let Some(&(index, block)) = matches.first() else {
            continue;
        };
        if !block.lines().any(|line| {
            let line = line.trim();
            line.strip_prefix("run: ").unwrap_or(line) == command
        }) {
            findings.push(format!("{id}: exact command missing"));
        }
        if block.lines().any(|line| {
            (line.starts_with("        if:")
                && !(id == "rust-tests" && line == SHARDED_TEST_CONDITION))
                || line.starts_with("        continue-on-error:")
        }) {
            findings.push(format!("{id}: required step must remain unconditional"));
        }
        let command_count = workflow
            .lines()
            .map(str::trim)
            .map(|line| line.strip_prefix("run: ").unwrap_or(line))
            .filter(|line| *line == command)
            .count();
        if command_count != 1 {
            findings.push(format!("{id}: expected one command invocation"));
        }
        if previous.is_some_and(|prior| prior >= index) {
            findings.push(format!("{id}: gate order changed"));
        }
        previous = Some(index);
    }
    let summaries: Vec<_> = blocks
        .iter()
        .filter(|block| step_id(block) == Some("gate_summary"))
        .collect();
    if summaries.len() != 1 {
        findings.push("summary: expected one step".to_owned());
        return findings;
    }
    let Some(summary) = summaries.first() else {
        return findings;
    };
    if !summary.lines().any(|line| line == "        if: always()") {
        findings.push("summary: must run after non-success".to_owned());
    }
    for id in REQUIRED_GATES
        .iter()
        .map(|(id, _)| *id)
        .chain(std::iter::once("pr_evidence"))
    {
        let variable = format!("{}_RESULT", id.replace('-', "_").to_ascii_uppercase());
        let binding = format!("          {variable}: ${{{{ steps.{id}.outcome }}}}");
        // The two pr_evidence rows share one printf to preserve the workflow's
        // reviewed executable-line budget. Pin both argument positions so the
        // compact form cannot swap or fabricate the required outcome.
        let row = if id == "pr_evidence" {
            format!("            {COMBINED_PR_EVIDENCE_PRINTF}")
        } else {
            format!("            printf '| {id} | %s |\\n' \"${{{variable}:-not_reported}}\"")
        };
        if !summary.lines().any(|line| line == binding) || !summary.lines().any(|line| line == row)
        {
            findings.push(format!("summary: {id} must retain its actual outcome"));
        }
    }
    if !summary
        .lines()
        .any(|line| line == "          PR_EVIDENCE_SCOPE: ${{ steps.pr_evidence.outputs.evidence }}")
    {
        findings.push("summary: pr_evidence scope must retain its actual output".to_owned());
    }
    findings
}

#[test]
fn rust_gate_workflow_preserves_complete_ordered_command_inventory() {
    let findings = contract_violations(WORKFLOW);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn rust_gate_workflow_rejects_missing_required_command() {
    let changed = WORKFLOW.replace(
        "        run: cargo xtask check-network-policy",
        "        run: echo policy omitted",
    );
    let findings = contract_violations(&changed);
    assert!(findings.contains(&"network_policy: exact command missing".to_owned()));
}

#[test]
fn rust_gate_workflow_rejects_duplicate_required_command() {
    let changed = format!(
        "{WORKFLOW}\n      - name: Duplicate tests\n        run: cargo nextest run --workspace --profile ci\n"
    );
    let findings = contract_violations(&changed);
    assert!(findings.contains(&"rust-tests: expected one command invocation".to_owned()));
}

#[test]
fn rust_gate_workflow_rejects_suppressed_or_optional_required_command() {
    for replacement in [
        "        id: rust-tests\n        continue-on-error: true",
        "        id: rust-tests\n        if: false",
    ] {
        let changed = WORKFLOW.replace("        id: rust-tests", replacement);
        let findings = contract_violations(&changed);
        assert!(
            findings.contains(&"rust-tests: required step must remain unconditional".to_owned())
        );
    }
    let changed = WORKFLOW.replace(
        "          cargo nextest run --workspace --profile ci",
        "          cargo nextest run --workspace --profile ci || true",
    );
    let findings = contract_violations(&changed);
    assert!(findings.contains(&"rust-tests: exact command missing".to_owned()));
}

#[test]
fn rust_gate_workflow_rejects_policy_after_workspace_tests() {
    let changed = WORKFLOW
        .replace("        id: precommit", "        id: swap")
        .replace("        id: rust-tests", "        id: precommit")
        .replace("        id: swap", "        id: rust-tests");
    let findings = contract_violations(&changed);
    assert!(
        findings
            .iter()
            .any(|finding| finding.ends_with("gate order changed"))
    );
}

#[test]
fn rust_gate_workflow_pins_fast_gates_before_workspace_tests() {
    for id in [
        "goldens",
        "fixtures",
        "capability_producers",
        "capability_docs",
        "verdict_corpus",
    ] {
        let changed = WORKFLOW
            .replace(&format!("        id: {id}"), "        id: swap")
            .replace("        id: rust-tests", &format!("        id: {id}"))
            .replace("        id: swap", "        id: rust-tests");
        let findings = contract_violations(&changed);
        assert!(
            findings
                .iter()
                .any(|finding| finding.ends_with("gate order changed")),
            "{id} placed after the workspace tests was not rejected: {findings:?}"
        );
    }
}

#[test]
fn rust_gate_workflow_rejects_failure_only_summary() {
    let changed = WORKFLOW.replace(
        "        id: gate_summary\n        if: always()",
        "        id: gate_summary\n        if: failure()",
    );
    let findings = contract_violations(&changed);
    assert!(findings.contains(&"summary: must run after non-success".to_owned()));
}

#[test]
fn rust_gate_workflow_rejects_fabricated_success_or_missing_outcome() {
    for changed in [
        WORKFLOW.replace(
            "RUST_TESTS_RESULT: ${{ steps.rust-tests.outcome }}",
            "RUST_TESTS_RESULT: success",
        ),
        WORKFLOW.replace("${RUST_TESTS_RESULT:-not_reported}", "success"),
        WORKFLOW.replace(
            "${RUST_TESTS_RESULT:-not_reported}",
            "${RUST_TESTS_RESULT:-success}",
        ),
    ] {
        let findings = contract_violations(&changed);
        assert!(
            findings.contains(&"summary: rust-tests must retain its actual outcome".to_owned())
        );
    }
}

#[test]
fn combined_pr_evidence_summary_cannot_swap_or_fabricate_its_outcome() {
    for changed in [
        WORKFLOW.replace(
            COMBINED_PR_EVIDENCE_PRINTF,
            "printf '| pr_evidence | %s |\\n| pr_evidence scope | %s |\\n' \"success\" \"${PR_EVIDENCE_SCOPE:-not_reported}\"",
        ),
        WORKFLOW.replace(
            COMBINED_PR_EVIDENCE_PRINTF,
            "printf '| pr_evidence | %s |\\n| pr_evidence scope | %s |\\n' \"${PR_EVIDENCE_SCOPE:-not_reported}\" \"${PR_EVIDENCE_RESULT:-not_reported}\"",
        ),
        WORKFLOW.replace(
            "PR_EVIDENCE_SCOPE: ${{ steps.pr_evidence.outputs.evidence }}",
            "PR_EVIDENCE_SCOPE: fabricated",
        ),
    ] {
        assert_ne!(changed, WORKFLOW, "negative must mutate the current summary");
        let findings = contract_violations(&changed);
        assert!(
            findings.iter().any(|finding| finding.starts_with("summary: pr_evidence")),
            "fabricated summary was accepted: {findings:?}"
        );
    }
}

#[test]
fn rust_gate_workflow_rejects_local_convenience_clippy_composition() {
    let changed = WORKFLOW.replace("run: cargo xtask ci-policy", "run: cargo xtask precommit");
    assert!(contract_violations(&changed).contains(&"precommit: exact command missing".to_owned()));
}

#[test]
fn rust_gate_workflow_rejects_narrowing_explicit_clippy_coverage() {
    for command in [
        "cargo clippy -p xtask --all-targets -- -D warnings",
        "cargo clippy --workspace -- -D warnings",
        "cargo clippy --workspace --all-targets",
    ] {
        let changed = WORKFLOW.replace(
            "cargo clippy --workspace --all-targets -- -D warnings",
            command,
        );
        assert!(
            contract_violations(&changed).contains(&"clippy: exact command missing".to_owned())
        );
    }
}

const FIXTURE_PREPARATION_NAME: &str = "name: Prepare locked owner-return fixture dependencies";
const FALLIBLE_FETCH: &str = "cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml";
const JSON_FETCH: &str = "cargo fetch --locked --manifest-path fixtures/owner_return_pin_json_real_literal/input/Cargo.toml";
const PROPTEST_FETCH: &str = "cargo fetch --locked --manifest-path crates/ripr/tests/fixtures/proptest_singleton/input/Cargo.toml";

fn prepares_locked_owner_return_fixtures(workflow: &str) -> bool {
    let blocks: Vec<_> = workflow.split("\n      - ").skip(1).collect();
    let preparations: Vec<_> = blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.lines().next() == Some(FIXTURE_PREPARATION_NAME))
        .collect();
    let [(index, block)] = preparations.as_slice() else {
        return false;
    };
    // Exact bare commands retain lock enforcement and failure propagation;
    // no condition, alternate Cargo home, or suppressed failure is accepted.
    let lines: Vec<_> = block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        == [
            FIXTURE_PREPARATION_NAME,
            "run: |",
            FALLIBLE_FETCH,
            JSON_FETCH,
            PROPTEST_FETCH,
        ]
        && blocks[..*index]
            .iter()
            .any(|block| block.starts_with("uses: dtolnay/rust-toolchain@"))
        && ["precommit", "rust-tests"].iter().all(|id| {
            blocks[*index + 1..]
                .iter()
                .any(|block| step_id(block) == Some(id))
        })
        && [FALLIBLE_FETCH, JSON_FETCH, PROPTEST_FETCH]
            .iter()
            .all(|command| {
                workflow
                    .lines()
                    .filter(|line| line.trim() == *command)
                    .count()
                    == 1
            })
}

#[test]
fn rust_gate_workflow_prepares_both_locked_owner_return_fixtures_before_policy() {
    assert!(prepares_locked_owner_return_fixtures(WORKFLOW));
}

#[test]
fn rust_gate_workflow_rejects_missing_unlocked_late_or_suppressed_fixture_preparation() {
    let preparation = format!(
        "      - {FIXTURE_PREPARATION_NAME}\n        run: |\n          {FALLIBLE_FETCH}\n          {JSON_FETCH}\n          {PROPTEST_FETCH}\n"
    );
    for changed in [
        WORKFLOW.replace(&preparation, ""),
        WORKFLOW.replace(
            JSON_FETCH,
            "cargo fetch --locked --manifest-path Cargo.toml",
        ),
        WORKFLOW.replace(JSON_FETCH, &JSON_FETCH.replace(" --locked", "")),
        WORKFLOW.replace(FALLIBLE_FETCH, ""),
        WORKFLOW.replace(JSON_FETCH, &format!("{JSON_FETCH} || true")),
        WORKFLOW.replace(
            FIXTURE_PREPARATION_NAME,
            &format!("{FIXTURE_PREPARATION_NAME}\n        if: false"),
        ),
        WORKFLOW.replace(
            FIXTURE_PREPARATION_NAME,
            &format!("{FIXTURE_PREPARATION_NAME}\n        continue-on-error: true"),
        ),
        WORKFLOW.replace(
            FIXTURE_PREPARATION_NAME,
            &format!("{FIXTURE_PREPARATION_NAME}\n        env:\n          CARGO_HOME: other"),
        ),
        format!("{}\n{preparation}", WORKFLOW.replace(&preparation, "")),
        format!("{WORKFLOW}\n{preparation}"),
    ] {
        assert_ne!(
            changed, WORKFLOW,
            "negative fixture must change the workflow"
        );
        assert!(!prepares_locked_owner_return_fixtures(&changed));
    }
}

#[test]
fn rust_gate_workflow_rejects_missing_unlocked_suppressed_or_duplicate_property_preparation() {
    for changed in [
        WORKFLOW.replace(PROPTEST_FETCH, ""),
        WORKFLOW.replace(PROPTEST_FETCH, &PROPTEST_FETCH.replace(" --locked", "")),
        WORKFLOW.replace(PROPTEST_FETCH, &format!("{PROPTEST_FETCH} || true")),
        WORKFLOW.replace(
            PROPTEST_FETCH,
            &format!("{PROPTEST_FETCH}\n          {PROPTEST_FETCH}"),
        ),
        // This leaves the closed preparation block intact and challenges
        // the independent whole-workflow uniqueness requirement.
        format!(
            "{WORKFLOW}\n      - name: Duplicate property fetch\n        run: |\n          {PROPTEST_FETCH}\n"
        ),
    ] {
        assert_ne!(
            changed, WORKFLOW,
            "negative fixture must change the workflow"
        );
        assert!(!prepares_locked_owner_return_fixtures(&changed));
    }
}

fn shard_contract_violations(shards: &str, routed: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let count = |text: &str, needle: &str| {
        text.lines()
            .map(str::trim)
            .filter(|line| line.contains(needle))
            .count()
    };
    for (label, command) in [
        ("test", SHARDED_TEST_COMMAND),
        ("list", SHARDED_LIST_COMMAND),
        ("receipt", SHARDED_RECEIPT_COMMAND),
    ] {
        if shards.lines().filter(|line| line.trim() == command).count() != 1 {
            findings.push(format!(
                "shards: expected exactly one {label} hash-partition command"
            ));
        }
    }
    if shards
        .lines()
        .filter(|line| line.trim() == "tool: nextest@0.9.149")
        .count()
        != 1
    {
        findings.push("shards: nextest must remain pinned to 0.9.149".to_owned());
    }
    if count(shards, "continue-on-error") != 0 {
        findings.push("shards: no step may continue on error".to_owned());
    }
    if !shards.contains("        shard: [1, 2, 3, 4]\n") {
        findings.push("shards: the matrix must stay shards 1 to 4".to_owned());
    }
    // `verify` is what makes four partitions equal the one command: a missing,
    // cancelled or red shard and a non-disjoint or incomplete cover must all fail.
    for required in [
        "    needs: rust-test-shard\n    if: ${{ !cancelled() }}\n",
        "\"$xtask\" verify-test-shards \\\n",
        "xtask=\"$(find xtask-binary -type f -name xtask | sort -V | tail -n 1)\"\n",
        "pattern: ${{ inputs.artifact-name }}-xtask-*\n          path: xtask-binary\n",
        "name: ${{ inputs.artifact-name }}-xtask-${{ github.run_attempt }}\n          path: target/xtask-binary/xtask\n",
        "        if: matrix.shard == 1\n        run: |\n          mkdir -p target/xtask-binary\n          strip --strip-debug -o target/xtask-binary/xtask target/debug/xtask\n",
        "if-no-files-found: error\n          retention-days: 1\n",
        "            --shards 4 \\\n",
        "            --sha \"$(git rev-parse HEAD)\" \\\n",
        "            --run-id \"$GITHUB_RUN_ID\" \\\n",
        "name: ${{ inputs.artifact-name }}-nextest-${{ github.run_attempt }}\n          path: |\n            target/nextest/ci/junit.xml\n            target/nextest/ci/run-context.txt\n",
    ] {
        if !shards.contains(required) {
            findings.push(format!("shards: verify lost `{required}`"));
        }
    }
    // The hosted route must run the shards behind its gates job and may not report
    // the required result green without them.
    for required in [
        "      test-shards: 4\n",
        "  rust-github-tests:\n    name: Ripr Rust Small tests on GitHub Hosted\n    needs: [rust-github]\n    if: ${{ !cancelled() && needs.rust-github.result == 'success' }}\n    uses: ./.github/workflows/rust-test-shards.yml\n",
        "rust-github, rust-github-tests, docs-gate,",
        "GITHUB_TESTS_RESULT: ${{ needs.rust-github-tests.result }}",
        "elif [ \"$GITHUB_TESTS_RESULT\" != \"success\" ]; then status=fail",
    ] {
        if !routed.contains(required) {
            findings.push(format!("routed: lost `{required}`"));
        }
    }
    if count(
        routed,
        "[ \"$GITHUB_RESULT\" = \"success\" ] && [ \"$GITHUB_TESTS_RESULT\" = \"success\" ]",
    ) != 3
    {
        findings.push("routed: every tempfail fallback must require the sharded tests".to_owned());
    }
    findings
}

#[test]
fn sharded_hosted_tests_keep_the_one_command_selection() {
    let findings = shard_contract_violations(SHARDS, ROUTED);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn sharded_hosted_tests_reject_legacy_and_mixed_partition_commands() {
    for mode in ["count", "slice"] {
        for (label, command) in [
            ("test", SHARDED_TEST_COMMAND),
            ("list", SHARDED_LIST_COMMAND),
            ("receipt", SHARDED_RECEIPT_COMMAND),
        ] {
            let replacement = command.replace("--partition hash:", &format!("--partition {mode}:"));
            let changed = SHARDS.replace(command, &replacement);
            assert_ne!(
                changed, SHARDS,
                "{label}/{mode} did not change the workflow"
            );
            let expected = format!("shards: expected exactly one {label} hash-partition command");
            assert!(
                shard_contract_violations(&changed, ROUTED).contains(&expected),
                "{label}/{mode} was not rejected"
            );
            let decoy = format!("{changed}\n# {command}\n");
            assert!(
                shard_contract_violations(&decoy, ROUTED).contains(&expected),
                "a comment hid the {label}/{mode} drift"
            );
        }
        let changed = SHARDS.replace("--partition hash:", &format!("--partition {mode}:"));
        assert_ne!(changed, SHARDS);
        let findings = shard_contract_violations(&changed, ROUTED);
        for label in ["test", "list", "receipt"] {
            assert!(findings.contains(&format!(
                "shards: expected exactly one {label} hash-partition command"
            )));
        }
    }
}

#[test]
fn sharded_hosted_tests_reject_a_changed_nextest_version() {
    for replacement in ["tool: nextest", "tool: nextest@0.9.148"] {
        let changed = SHARDS.replace("tool: nextest@0.9.149", replacement);
        assert_ne!(changed, SHARDS);
        assert!(
            shard_contract_violations(&changed, ROUTED)
                .contains(&"shards: nextest must remain pinned to 0.9.149".to_owned())
        );
        let decoy = format!("{changed}\n# tool: nextest@0.9.149\n");
        assert!(
            shard_contract_violations(&decoy, ROUTED)
                .contains(&"shards: nextest must remain pinned to 0.9.149".to_owned())
        );
    }
}

#[test]
fn rust_gate_workflow_allows_only_the_shard_condition_on_the_test_step() {
    assert!(WORKFLOW.contains(SHARDED_TEST_CONDITION));
    for other in [
        "        if: inputs.test-shards != 4",
        "        if: always() && inputs.test-shards == 1",
        "        if: false",
    ] {
        let changed = WORKFLOW.replace(SHARDED_TEST_CONDITION, other);
        assert!(
            contract_violations(&changed)
                .contains(&"rust-tests: required step must remain unconditional".to_owned()),
            "{other}"
        );
    }
    // The shard condition does not excuse any other required gate.
    let changed = WORKFLOW.replace(
        "        id: clippy",
        &format!("        id: clippy\n{SHARDED_TEST_CONDITION}"),
    );
    assert!(
        contract_violations(&changed)
            .contains(&"clippy: required step must remain unconditional".to_owned())
    );
}

#[test]
fn sharded_hosted_tests_reject_a_dropped_or_softened_shard_check() {
    for (from, to) in [
        (
            SHARDED_TEST_COMMAND,
            "cargo nextest run --workspace --profile ci",
        ),
        ("fail-fast: false", "continue-on-error: true"),
        ("shard: [1, 2, 3, 4]", "shard: [1, 2, 3]"),
        ("\"$xtask\" verify-test-shards", "true"),
        (
            "find xtask-binary -type f -name xtask | sort -V | tail -n 1",
            "find xtask-binary -type f -name xtask | head -n 1",
        ),
        (
            "if-no-files-found: error\n          retention-days: 1",
            "if-no-files-found: warn\n          retention-days: 1",
        ),
        (
            "-xtask-${{ github.run_attempt }}\n          path: target",
            "-xtask-0\n          path: target",
        ),
        ("            --shards 4 \\\n", "            --shards 3 \\\n"),
        ("            --sha \"$(git rev-parse HEAD)\" \\\n", ""),
        (
            "    if: ${{ !cancelled() }}\n    runs-on",
            "    if: success()\n    runs-on",
        ),
    ] {
        let changed = SHARDS.replace(from, to);
        assert_ne!(
            changed, SHARDS,
            "fixture must change the shard workflow: {from}"
        );
        assert!(
            !shard_contract_violations(&changed, ROUTED).is_empty(),
            "softening `{from}` was not rejected"
        );
    }
}

#[test]
fn routed_result_cannot_pass_without_the_hosted_shards() {
    for (from, to) in [
        ("test-shards: 4\n", "test-shards: 1\n"),
        (
            "elif [ \"$GITHUB_TESTS_RESULT\" != \"success\" ]; then status=fail",
            "elif false; then status=fail",
        ),
        (
            "rust-github, rust-github-tests, docs-gate,",
            "rust-github, docs-gate,",
        ),
        (
            "needs.rust-github.result == 'success' }}\n    uses: ./.github/workflows/rust-test-shards.yml",
            "needs.rust-github.result != 'cancelled' }}\n    uses: ./.github/workflows/rust-test-shards.yml",
        ),
    ] {
        let changed = ROUTED.replace(from, to);
        assert_ne!(changed, ROUTED, "fixture must change the caller: {from}");
        assert!(
            !shard_contract_violations(SHARDS, &changed).is_empty(),
            "weakening `{from}` was not rejected"
        );
    }
}

fn test_build_script(workflow: &str) -> Result<String, String> {
    let step = workflow
        .split_once("      - name: Build the test binaries one memory-heavy crate at a time\n")
        .ok_or_else(|| "missing serialized test-build step".to_owned())?
        .1
        .split("\n      - ")
        .next()
        .ok_or_else(|| "missing test-build step body".to_owned())?;
    let body = step
        .split_once("        run: |\n")
        .ok_or_else(|| "test-build step must keep a literal run block".to_owned())?
        .1;
    Ok(body
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

#[test]
fn hosted_build_metrics_keep_commands_and_the_existing_artifact_root() -> Result<(), String> {
    let script = test_build_script(SHARDS)?;
    let commands: Vec<_> = script
        .lines()
        .filter(|line| line.starts_with("build_phase "))
        .collect();
    assert_eq!(
        commands,
        [
            "build_phase without-xtask cargo test --no-run --locked --workspace --exclude xtask",
            "build_phase workspace cargo test --no-run --locked --workspace",
        ]
    );
    for required in [
        "metrics=target/nextest/ci/build-metrics",
        "time_tool=/usr/bin/time",
        "elapsed_seconds=%e\\nlargest_process_max_rss_kib=%M",
        "rss_scope=largest_command_or_waited_descendant_process_not_aggregate",
        "sample_period_seconds=5",
        "system-memory.tsv",
        "sampled_min_available_mb=",
        "sampled_max_swap_used_mb=",
        "measurement=unavailable",
    ] {
        assert!(
            script.contains(required),
            "missing metric contract: {required}"
        );
    }
    assert!(
        !script.contains("%x"),
        "GNU time's exit field is not native status"
    );
    assert!(!script.contains("$RUNNER_TEMP"));
    assert!(SHARDS.contains("            target/nextest/ci/build-metrics/\n"));
    assert!(SHARDS.contains("            target/nextest/ci/junit.xml\n"));
    assert!(SHARDS.contains("            target/nextest/ci/run-context.txt\n"));
    Ok(())
}

/// Execute the literal workflow Bash with bounded stand-ins. This proves command
/// selection, status and evidence handling, not GNU time or a hosted Rust build.
#[cfg(target_os = "linux")]
mod hosted_build_metrics {
    use super::{SHARDS, test_build_script};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const CARGO: &str = r#"#!/bin/bash
printf '%s\n' "$*" >> "$TRACE"
for ((i=0; i<100; i++)); do
  [ "$CASE" == no-samples ] && break
  [ -s target/nextest/ci/build-metrics/system-memory.tsv ] && break
  /bin/sleep 0.01
done
if [ "$CASE" == first-signal137 ]; then kill -KILL "$$"; fi
if [ "$CASE" == second-signal143 ] && [[ "$*" != *--exclude* ]]; then kill -TERM "$$"; fi
if [[ "$*" == *--exclude* ]]; then exit "$FIRST_EXIT"; else exit "$SECOND_EXIT"; fi
"#;
    const GIT: &str = r#"#!/bin/bash
[[ "$*" == 'rev-parse HEAD' ]] || exit 93
printf '%s\n' "$CHECKOUT_SHA"
"#;
    const FREE: &str = r#"#!/bin/bash
[ "$CASE" == no-samples ] && exit 0
printf '%s\n' 'Mem: 16384 11000 1000 10 4384 4200' 'Swap: 12288 375 11913'
if [ "$CASE" == multiple-samples ]; then
  printf '%s\n' 'Mem: 16384 12000 1000 10 3384 3000' 'Swap: 12288 450 11838'
  printf '%s\n' 'Mem: 16384 11500 1000 10 3884 3500' 'Swap: 12288 500 11788'
fi
"#;
    const AWK: &str = r#"#!/bin/bash
if [[ "$BROKEN_REPORTER" == 1 && ( "$*" == *sampled_min_available_mb* || "$*" == *elapsed_seconds* ) ]]; then exit 92; fi
exec /usr/bin/awk "$@"
"#;
    const TIMER: &str = r#"#!/bin/bash
[[ "$1" == -q && "$2" == -f && "$4" == -o ]] || exit 94
format="$3"; destination="$5"; shift 5
[[ "$format" == 'elapsed_seconds=%e\nlargest_process_max_rss_kib=%M' ]] || exit 95
if "$@"; then status=0; else status=$?; fi
case "$TIMER_MODE" in
  valid) printf 'elapsed_seconds=1.25\nlargest_process_max_rss_kib=12345\n' > "$destination" ;;
  malformed) printf 'elapsed_seconds=broken\nlargest_process_max_rss_kib=0\n' > "$destination" ;;
  empty) : > "$destination" ;;
esac
exit "$status"
"#;

    #[test]
    fn extracted_build_keeps_native_status_when_measurements_fail() -> TestResult {
        let script = test_build_script(SHARDS)?;
        assert_eq!(script.matches("time_tool=/usr/bin/time").count(), 1);
        // Only the instrument's path and Actions' shard expression are supplied
        // by the fixture; the production Bash, Cargo argv and reporters are real.
        let script = script
            .replace("time_tool=/usr/bin/time", "time_tool=\"$TEST_TIMER\"")
            .replace("${{ matrix.shard }}", "2");
        for (name, first, second, mode, reporter_fails) in [
            ("success", 0, 0, "valid", false),
            ("first-fails", 17, 0, "valid", false),
            ("second-fails", 0, 17, "valid", false),
            ("first-signal137", 137, 0, "valid", false),
            ("second-signal143", 0, 143, "valid", false),
            ("missing-timer", 0, 0, "absent", false),
            ("missing-timer-fails", 17, 0, "absent", false),
            ("malformed-timer", 0, 0, "malformed", false),
            ("empty-timer", 0, 0, "empty", false),
            ("multiple-samples", 0, 0, "valid", false),
            ("no-samples", 0, 0, "valid", false),
            ("reporter-fails", 17, 0, "valid", true),
            ("reporter-fails-success", 0, 0, "valid", true),
        ] {
            let path = std::env::temp_dir()
                .join(format!("ripr-build-metrics-{}-{name}", std::process::id()));
            fs::create_dir(&path)?;
            let scratch = Scratch(path);
            let bin = scratch.0.join("bin");
            fs::create_dir(&bin)?;
            for (binary, source) in [
                ("cargo", CARGO),
                ("time", TIMER),
                ("git", GIT),
                ("free", FREE),
                // End the sampler after its first sample; no descendant outlives
                // this bounded source replay or delays its cleanup.
                ("sleep", "#!/bin/bash\nexit 1\n"),
                ("awk", AWK),
            ] {
                let file = bin.join(binary);
                fs::write(&file, source)?;
                fs::set_permissions(&file, fs::Permissions::from_mode(0o755))?;
            }
            let old_path = std::env::var_os("PATH").ok_or("PATH is unavailable")?;
            let path = std::env::join_paths(
                std::iter::once(bin.clone()).chain(std::env::split_paths(&old_path)),
            )?;
            let output = Command::new("timeout")
                .args(["--kill-after=1s", "5s", "bash", "-e", "-c", &script])
                .current_dir(&scratch.0)
                .env_remove("BASH_ENV")
                .env_remove("ENV")
                .env_remove("SHELLOPTS")
                .env_remove("BASHOPTS")
                .env("PATH", path)
                .env(
                    "TEST_TIMER",
                    bin.join(if mode == "absent" { "missing" } else { "time" }),
                )
                .env("TRACE", scratch.0.join("trace"))
                .env("CASE", name)
                .env("FIRST_EXIT", first.to_string())
                .env("SECOND_EXIT", second.to_string())
                .env("TIMER_MODE", mode)
                .env("BROKEN_REPORTER", if reporter_fails { "1" } else { "0" })
                .env("CHECKOUT_SHA", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .env("GITHUB_REPOSITORY", "fixture/repo")
                .env("GITHUB_RUN_ID", "123")
                .env("GITHUB_RUN_ATTEMPT", "4")
                .output()?;
            let expected_exit = if first != 0 { first } else { second };
            assert_eq!(
                output.status.code(),
                Some(expected_exit),
                "{name}: {output:?}"
            );
            let trace = fs::read_to_string(scratch.0.join("trace"))?;
            let expected = if first != 0 {
                "test --no-run --locked --workspace --exclude xtask\n"
            } else {
                "test --no-run --locked --workspace --exclude xtask\ntest --no-run --locked --workspace\n"
            };
            assert_eq!(trace, expected, "{name}: Cargo count/order/argv changed");
            let metrics = scratch.0.join("target/nextest/ci/build-metrics");
            let context = fs::read_to_string(metrics.join("context.txt"))?;
            for field in [
                "repository=fixture/repo",
                "checkout_sha=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "run_id=123",
                "run_attempt=4",
                "shard=2/4",
                "sample_period_seconds=5",
            ] {
                assert!(context.contains(field), "{name}: missing {field}");
            }
            let samples = fs::read_to_string(metrics.join("system-memory.tsv"))?;
            assert_eq!(samples.is_empty(), name == "no-samples", "{name}");
            if reporter_fails {
                assert!(String::from_utf8_lossy(&output.stdout).contains("::warning::"));
                assert_eq!(
                    fs::read_to_string(metrics.join("system-summary.txt"))?,
                    "measurement=unavailable\n"
                );
                assert_eq!(
                    fs::read_to_string(metrics.join("without-xtask.txt"))?,
                    format!("native_exit={first}\nmeasurement=unavailable\n")
                );
                continue;
            }
            let system = fs::read_to_string(metrics.join("system-summary.txt"))?;
            let fields: &[&str] = match name {
                "multiple-samples" => &[
                    "sample_count=3",
                    "sampled_min_available_mb=3000",
                    "sampled_max_swap_used_mb=500",
                ],
                "no-samples" => &["sample_count=0", "measurement=unavailable"],
                _ => &[
                    "measurement=sampled",
                    "sampled_min_available_mb=4200",
                    "sampled_max_swap_used_mb=375",
                ],
            };
            for &field in fields {
                assert!(system.contains(field), "{name}: {system}");
            }
            if name == "no-samples" {
                assert!(!system.contains("sampled_min_available_mb="), "{system}");
                assert!(!system.contains("sampled_max_swap_used_mb="), "{system}");
            }
            for (phase, status) in [("without-xtask", first), ("workspace", second)] {
                if phase == "workspace" && first != 0 {
                    assert!(!metrics.join("workspace.txt").exists(), "{name}");
                    continue;
                }
                let result = fs::read_to_string(metrics.join(format!("{phase}.txt")))?;
                assert!(
                    result.contains(&format!("native_exit={status}\n")),
                    "{name}: {result}"
                );
                if mode == "valid" {
                    for field in [
                        "measurement=available",
                        "elapsed_seconds=1.25",
                        "largest_process_max_rss_kib=12345",
                    ] {
                        assert!(result.contains(field), "{name}: {result}");
                    }
                } else {
                    assert!(
                        result.contains("measurement=unavailable"),
                        "{name}: {result}"
                    );
                    assert!(
                        !result.contains("largest_process_max_rss_kib="),
                        "{name}: {result}"
                    );
                }
            }
        }
        Ok(())
    }
}
