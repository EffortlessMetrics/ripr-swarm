//! Final queue source contract. Runtime admission, cache/capacity qualification
//! and the protected H/G publisher remain independent activation obligations.

use crate::workflow_lifecycle::event_declarations;

pub const GENERATION: &str = "  CI_LIFECYCLE_GENERATION: merge-queue-v1";
const CONTROL_SOURCE: &str = "abbe32369c5b0587613f331ec5bd6f22f5bef934";
const HEAVY: &str =
    "{\"group\":\"em-ci-build\",\"labels\":[\"self-hosted\",\"linux\",\"x64\",\"rust-heavy\"]}";

fn job<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let (_, jobs) = source.split_once("\njobs:\n")?;
    let marker = format!("  {name}:\n");
    let (_, tail) = jobs.split_once(&marker)?;
    let end = tail
        .match_indices('\n')
        .find_map(|(offset, _)| {
            let next = &tail[offset + 1..];
            (next.starts_with("  ") && !next.starts_with("   ") && !next.starts_with("  #"))
                .then_some(offset)
        })
        .unwrap_or(tail.len());
    Some(&tail[..end])
}

fn step<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("      - name: {name}\n");
    let (_, tail) = source.split_once(&marker)?;
    Some(tail.split("\n      - ").next().unwrap_or(tail))
}

pub fn violations(source: &str, reusable: Option<&str>) -> Vec<String> {
    let mut findings = Vec::new();
    let mut require = |condition: bool, detail: &str| {
        if !condition {
            findings.push(format!("queue-only workflow: {detail}"));
        }
    };
    require(
        source.lines().any(|line| line == GENERATION),
        "missing reviewed source generation",
    );
    let events = event_declarations(source);
    require(
        events.is_ok_and(|events| {
            events.len() == 1 && events.get("workflow_call").is_some_and(Vec::is_empty)
        }),
        "trusted orchestration must accept only input-free workflow_call",
    );
    for forbidden in [
        "EM_RUNNER_READ_TOKEN",
        "orgs/EffortlessMetrics/actions/runners",
        "continue-on-error:",
        "rust-cx43:",
        "rust-cpx42:",
        "rust-cx53:",
        "rust-github:",
        "workflow_dispatch:",
        "ubuntu-latest",
    ] {
        require(
            !source.contains(forbidden),
            &format!("retired or unqualified route remains: {forbidden}"),
        );
    }
    for name in ["detect-docs-only", "admission"] {
        let body = job(source, name).unwrap_or("");
        let header = body.split("    steps:\n").next().unwrap_or("");
        let runner = if name == "admission" {
            "    runs-on:\n      group: em-ci-queue-control\n      labels: [self-hosted, linux, x64, ci-control]"
        } else {
            "    runs-on:\n      group: em-ci-build\n      labels: [self-hosted, linux, x64, rust-heavy]"
        };
        require(
            header.contains(runner),
            &format!("{name} must preserve control/build isolation"),
        );
        require(
            header.contains("    timeout-minutes:"),
            &format!("{name} requires a deadline"),
        );
        let condition = if name == "admission" {
            "    if: needs.intake.result == 'success' && needs.detect-docs-only.result == 'success'"
        } else {
            "    if: needs.intake.result == 'success'"
        };
        require(
            header.contains("    needs: [intake") && header.lines().any(|line| line == condition),
            &format!("{name} must not allocate candidate capacity before authenticated intake"),
        );
    }
    let intake = job(source, "intake").unwrap_or("");
    for binding in [
        "labels: [self-hosted, linux, x64, ci-control]",
        "ref: abbe32369c5b0587613f331ec5bd6f22f5bef934",
        "checks: read",
        "queue_intake.py",
        "policy/queue-publisher.toml",
        "intake_json: ${{ steps.verify.outputs.intake_json }}",
    ] {
        require(
            intake.contains(binding),
            &format!("protected intake control lacks {binding}"),
        );
    }
    require(
        !intake.contains("rust-heavy") && !intake.contains("ref: ${{ github.sha }}"),
        "intake must not execute or allocate candidate capacity",
    );
    let admission = job(source, "admission").unwrap_or("");
    for binding in [
        "uses: ./.admission-trusted/.github/actions/verify-merge-group-subject",
        "subject-sha: ${{ github.event.merge_group.head_sha }}",
        "base-sha: ${{ github.event.merge_group.base_sha }}",
        "subject-tree: ${{ steps.entry.outputs.tree }}",
        "constituent-head-sha: ${{ steps.entry.outputs.head }}",
        "subject_json: ${{ steps.group.outputs.subject-json }}",
        "subject_identity: ${{ steps.group.outputs.subject-identity }}",
        "name: ripr-queue-subject-${{ github.run_id }}-${{ github.run_attempt }}",
        "if-no-files-found: error",
        "uses: ./.admission-trusted/.github/actions/pr-candidate-admission",
    ] {
        require(
            admission.contains(binding),
            &format!("admission lacks frozen identity/cut binding: {binding}"),
        );
    }
    let rust = job(source, "rust-owned").unwrap_or("");
    for binding in [
        "    needs: [admission]",
        "    uses: ./.github/workflows/rust-gates.yml",
        "      proof-profile: merge-group",
        "      execution-profile: owned-jit",
        "      use-scratch: false",
        "      test-shards: 1",
        "      job-timeout-minutes: 20",
        "      subject-sha: ${{ needs.admission.outputs.merge }}",
        "      subject-json: ${{ needs.admission.outputs.subject_json }}",
        "      subject-identity: ${{ needs.admission.outputs.subject_identity }}",
        "      constituent-head-sha: ${{ needs.admission.outputs.head }}",
    ] {
        require(
            rust.lines().any(|line| line == binding),
            &format!("owned Rust lacks binding: {binding}"),
        );
    }
    require(
        rust.contains(&format!("      runner-config: '{HEAVY}'")),
        "owned Rust must select the governed heavy capability",
    );
    for name in [
        "windows-subset",
        "python-wheel",
        "ci-feature",
        "printed-command",
    ] {
        let body = job(source, name).unwrap_or("");
        for binding in [
            "    needs: [admission]",
            "      proof_profile: merge-group",
            "      head_sha: ${{ needs.admission.outputs.head }}",
            "      base_sha: ${{ needs.admission.outputs.base }}",
            "      subject_sha: ${{ needs.admission.outputs.merge }}",
            "      subject_tree: ${{ needs.admission.outputs.tree }}",
            "      request_id: ${{ needs.admission.outputs.request }}",
            "      workflow_generation: ${{ github.workflow_sha }}",
        ] {
            require(
                body.lines().any(|line| line == binding),
                &format!("{name} lacks complete G binding: {binding}"),
            );
        }
        require(
            !body.contains("docs_only"),
            &format!("{name} may not prune the frozen queue portfolio"),
        );
    }
    let ci = job(source, "ci-feature").unwrap_or("");
    for input in [
        "run_rust_only",
        "run_release_proof",
        "run_msrv",
        "run_vscode",
    ] {
        require(
            ci.lines()
                .any(|line| line == format!("      {input}: true")),
            &format!("full {input} portfolio must remain selected"),
        );
    }
    require(
        ci.contains(&format!("      runner_config: '{HEAVY}'")),
        "generic Linux CI proof must use owned heavy capacity",
    );
    let result = job(source, "result").unwrap_or("");
    let header = result.split("    steps:\n").next().unwrap_or("");
    for binding in [
        "    name: Ripr Rust Small Result",
        "    needs: [intake, admission, rust-owned, windows-subset, python-wheel, ci-feature, printed-command]",
        "    if: always()",
        "    timeout-minutes: 5",
        "    runs-on:\n      group: em-ci-queue-control\n      labels: [self-hosted, linux, x64, ci-control]",
    ] {
        require(
            header.contains(binding),
            &format!("mandatory result binding is absent: {binding}"),
        );
    }
    for binding in [
        "ref: abbe32369c5b0587613f331ec5bd6f22f5bef934",
        "sparse-checkout: |\n            .github/actions/pr-candidate-admission\n            policy/queue-publisher.toml",
        "--head \"$EXPECTED_HEAD\"",
        ".head.sha == $head",
        ".mergeQueueEntry.id == $entry",
        "all(.isResolved == true)",
        "why=same_head_succession",
        "GROUP_PROOF_RESULT: ${{ steps.group_proof.outcome }}",
        "ELIGIBILITY_RESULT: ${{ steps.eligibility.outcome }}",
        "\"$ADMISSION_STATUS\" == admitted",
        "\"$CUT_STATUS\" != refused",
        "\"$ELIGIBILITY_RESULT\" == success",
        "\"$GROUP_PROOF_RESULT\" == success",
    ] {
        require(
            result.contains(binding),
            &format!("terminal currentness/cut/receipt check is absent: {binding}"),
        );
    }
    for (name, body) in [
        ("intake", intake),
        ("admission", admission),
        ("result", result),
    ] {
        require(
            body.contains("group: em-ci-queue-control"),
            &format!("{name} needs restricted control capacity"),
        );
        for forbidden in [
            "ref: ${{ github.event.merge_group.base_sha }}",
            "TRUSTED_SHA: ${{ github.event.merge_group.base_sha }}",
            "merge_group) sha=\"$GROUP_BASE_SHA\"",
            "uses: ./.github/actions/",
        ] {
            require(
                !body.contains(forbidden),
                &format!("{name} executes candidate-selected control code: {forbidden}"),
            );
        }
    }
    require(
        intake.contains(&format!("TRUSTED_SHA: {CONTROL_SOURCE}"))
            && result.contains(&format!("TRUSTED_SHA: {CONTROL_SOURCE}")),
        "initial and final intake must use the same immutable evaluator",
    );
    require(
        admission.contains(&format!("sha={CONTROL_SOURCE}")),
        "admission source cannot come from the event or caller",
    );
    require(
        result.contains(&format!("TRUSTED_ADMISSION_SHA: {CONTROL_SOURCE}")),
        "final review must use the immutable admission source",
    );
    require(
        !ci.contains("ci-control"),
        "candidate CI leaf must not receive control capacity",
    );
    let aggregate = step(result, "Require complete native group portfolio").unwrap_or("");
    require(
        result.contains("--expected-env EXPECTED_INTAKE")
            && result.contains("EXPECTED_INTAKE: ${{ needs.intake.outputs.intake_json }}")
            && result.contains("INTAKE_RECHECK: ${{ steps.intake.outcome }}")
            && result.contains("\"$INTAKE_RECHECK\" == success"),
        "terminal result must recheck the identical authenticated intake",
    );
    for (env, name) in [
        ("OWNED_RESULT", "rust-owned"),
        ("WINDOWS_RESULT", "windows-subset"),
        ("WHEEL_RESULT", "python-wheel"),
        ("CI_RESULT", "ci-feature"),
        ("PRINTED_RESULT", "printed-command"),
    ] {
        require(
            aggregate.contains(&format!("{env}: ${{{{ needs.{name}.result }}}}")),
            &format!("aggregate must read its own {name} result"),
        );
    }
    require(aggregate.contains("for result in \"$OWNED_RESULT\" \"$WINDOWS_RESULT\" \"$WHEEL_RESULT\" \"$CI_RESULT\" \"$PRINTED_RESULT\"; do") && aggregate.contains("[ \"$result\" = success ]"), "every mandatory outcome must equal success");
    require(
        aggregate.contains(".request_id == $request")
            && aggregate.contains(".merge == $group")
            && aggregate.contains("sha256sum"),
        "aggregate must verify the complete frozen subject digest/run",
    );
    let reusable = reusable.unwrap_or("");
    require(
        reusable.contains("cargo nextest run --workspace --profile ci"),
        "full nextest command is missing",
    );
    for name in [
        "Acquire scratch lease",
        "Reclaim terminal scratch",
        "Configure scratch environment",
        "Prepare scratch",
        "Clean scratch",
        "Release scratch lease",
    ] {
        require(
            step(reusable, name).is_some_and(|body| {
                body.contains("inputs.execution-profile == 'legacy' && inputs.use-scratch")
            }),
            &format!("owned execution must not enter legacy {name}"),
        );
    }
    require(
        step(reusable, "Restore hosted Rust cache").is_some_and(|body| {
            body.contains("inputs.execution-profile == 'legacy' && !inputs.use-scratch")
        }),
        "owned execution must not restore/save an Actions cache",
    );
    findings
}

/// The only event subscriber is a fixed, input-free call to an immutable workflow.
/// Runner-group selected_workflows is the independent runtime enforcement: a
/// malicious candidate can edit this checker, but cannot grant its job access.
pub fn entrypoint_violations(source: &str) -> Vec<String> {
    const PREFIX: &str =
        "    uses: EffortlessMetrics/ripr-swarm/.github/workflows/routed-rust.yml@";
    let Some(pin) = source.lines().find_map(|line| line.strip_prefix(PREFIX)) else {
        return vec!["queue entrypoint must call the immutable trusted workflow".into()];
    };
    if pin.len() != 40
        || !pin
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || pin.bytes().all(|b| b == b'0')
    {
        return vec!["queue entrypoint requires a nonzero full commit SHA".into()];
    }
    let expected = format!(
        "name: Routed Rust Small\nrun-name: Merge queue ${{{{ github.event.merge_group.head_sha }}}}\non:\n  merge_group:\n    types: [checks_requested]\npermissions:\n  actions: read\n  contents: read\n  pull-requests: read\n  checks: read\njobs:\n  queue:\n{PREFIX}{pin}\n"
    );
    if source == expected {
        Vec::new()
    } else {
        vec!["queue entrypoint must contain only the fixed immutable call; no candidate steps, inputs, secrets or runner selectors".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CALLER: &str = include_str!("../../../.github/workflows/routed-rust.yml");
    const ENTRY: &str = include_str!("../../../.github/workflows/merge-queue.yml");
    const RUST: &str = include_str!("../../../.github/workflows/rust-gates.yml");

    #[test]
    fn control_sources_are_immutable_not_speculative_event_base() {
        for name in ["intake", "admission", "result"] {
            let body = job(CALLER, name).unwrap_or("");
            assert!(
                body.contains("group: em-ci-queue-control"),
                "{name} needs isolated control group"
            );
            assert!(!body.contains("ref: ${{ github.event.merge_group.base_sha }}"));
            assert!(!body.contains("TRUSTED_SHA: ${{ github.event.merge_group.base_sha }}"));
            assert!(!body.contains("merge_group) sha=\"$GROUP_BASE_SHA\""));
        }
    }

    #[test]
    fn current_queue_generation_is_complete() {
        assert_eq!(violations(CALLER, Some(RUST)), Vec::<String>::new());
        assert!(entrypoint_violations(ENTRY).is_empty());
    }
    #[test]
    fn candidate_workflow_cannot_define_control_steps_or_select_mutable_authority() {
        for wrong in [
            ENTRY.replace(
                ENTRY
                    .lines()
                    .last()
                    .unwrap_or("")
                    .rsplit('@')
                    .next()
                    .unwrap_or(""),
                "main",
            ),
            ENTRY.replace(
                ENTRY
                    .lines()
                    .last()
                    .unwrap_or("")
                    .rsplit('@')
                    .next()
                    .unwrap_or(""),
                "${{ github.event.merge_group.base_sha }}",
            ),
            format!(
                "{ENTRY}  injected:\n    runs-on:\n      group: em-ci-queue-control\n    steps:\n      - run: echo candidate\n"
            ),
            format!("{ENTRY}    secrets: inherit\n"),
            format!("{ENTRY}    with:\n      trusted-sha: ${{{{ github.sha }}}}\n"),
        ] {
            assert_ne!(wrong, ENTRY);
            assert!(!entrypoint_violations(&wrong).is_empty());
        }
    }

    #[test]
    fn development_roots_and_router_fallback_cannot_return() {
        for extra in [
            "  pull_request:\n    types: [labeled]\n",
            "  push:\n    branches: [main]\n",
            "  workflow_dispatch:\n",
        ] {
            assert!(
                !violations(
                    &CALLER.replace("on:\n", &format!("on:\n{extra}")),
                    Some(RUST)
                )
                .is_empty()
            );
        }
        let hosted = CALLER.replace(HEAVY, "\"ubuntu-latest\"");
        assert_ne!(hosted, CALLER);
        assert!(!violations(&hosted, Some(RUST)).is_empty());
        for event in [
            "pull_request",
            "pull_request_target",
            "push",
            "workflow_dispatch",
        ] {
            assert_eq!(
                crate::routed_rust_event_route(CALLER, event, Some("labeled"), Some("merge-ready")),
                crate::RoutedRustEventRoute::WorkflowNotTriggered
            );
        }
    }
    #[test]
    fn heavy_allocation_and_terminal_success_require_authenticated_intake() {
        for (before, after) in [
            ("    needs: [intake]\n", "    needs: []\n"),
            (
                "    needs: [intake, detect-docs-only]\n",
                "    needs: [detect-docs-only]\n",
            ),
            (
                "    if: needs.intake.result == 'success'",
                "    if: always()",
            ),
            ("--expected-env EXPECTED_INTAKE", ""),
            ("\"$INTAKE_RECHECK\" == success", "true"),
            (
                "ref: abbe32369c5b0587613f331ec5bd6f22f5bef934",
                "ref: ${{ github.event.merge_group.base_sha }}",
            ),
        ] {
            let wrong = CALLER.replace(before, after);
            assert_ne!(wrong, CALLER);
            assert!(
                !violations(&wrong, Some(RUST)).is_empty(),
                "accepted {before}"
            );
        }
    }
    #[test]
    fn skipped_missing_or_rebound_proof_cannot_weaken_the_result() {
        for wrong in [
            CALLER.replace(
                "admission, rust-owned, windows-subset, python-wheel, ci-feature, printed-command]",
                "admission, rust-owned]",
            ),
            CALLER.replace(
                "subject_sha: ${{ needs.admission.outputs.merge }}",
                "subject_sha: ${{ needs.admission.outputs.head }}",
            ),
            CALLER.replace("run_rust_only: true", "run_rust_only: false"),
            CALLER.replace("[ \"$result\" = success ]", "[ \"$result\" != failure ]"),
            CALLER.replace("if: always()", "if: success()"),
            CALLER.replace("why=same_head_succession", "why=skipped"),
            CALLER.replace("\"$CUT_STATUS\" != refused", "true"),
        ] {
            assert_ne!(wrong, CALLER);
            assert!(!violations(&wrong, Some(RUST)).is_empty());
        }
    }
    #[test]
    fn legacy_cleanup_and_cache_writes_cannot_enter_owned_profile() {
        let wrong = RUST.replace("inputs.execution-profile == 'legacy' && ", "");
        assert_ne!(wrong, RUST);
        assert!(!violations(CALLER, Some(&wrong)).is_empty());
        assert!(!violations(CALLER, None).is_empty());
    }

    #[test]
    fn generic_rust_leaves_use_owned_tools_and_specialized_platforms_keep_their_contracts() {
        let ci = include_str!("../../../.github/workflows/ci.yml");
        let printed = include_str!("../../../.github/workflows/printed-command-paste.yml");
        assert!(
            ci.contains("runs-on: ${{ fromJSON(inputs.runner_config || '\"ubuntu-latest\"') }}")
        );
        assert!(ci.contains(
            "runs-on: ${{ fromJSON(inputs.control_runner_config || '\"ubuntu-latest\"') }}"
        ));
        for cache in ci.split("      - uses: Swatinem/rust-cache@").skip(1) {
            let step = cache.split("\n      - ").next().unwrap_or("");
            assert!(step.contains("        if: inputs.proof_profile != 'merge-group'"));
            assert_eq!(step.matches("        if:").count(), 1);
        }
        assert_eq!(
            ci.matches("uses: ./.github/actions/select-owned-rust")
                .count(),
            3
        );
        assert!(RUST.contains("uses: ./.github/actions/select-owned-rust\n        if: inputs.execution-profile == 'owned-jit'\n        with:\n          require-llvm: 'true'"));
        for toolchain in ci.split("      - uses: dtolnay/rust-toolchain@").skip(1) {
            assert!(
                toolchain
                    .split("\n      - ")
                    .next()
                    .unwrap_or("")
                    .contains("if: inputs.proof_profile != 'merge-group'")
            );
        }
        assert!(RUST.contains("# v1\n        if: inputs.execution-profile == 'legacy'\n        with:\n          toolchain: 1.95.0"));
        assert!(printed.contains("runs-on: ${{ matrix.os }}"));
        assert!(printed.contains("shells: bash,sh,zsh,pwsh"));
        assert!(!CALLER.contains("linux_runner_config"));
        // Windows/macOS, Ubuntu 22.04 wheel ABI and extension/Xvfb hosts remain
        // explicitly qualified platform obligations, not an automatic fallback.
        assert!(
            include_str!("../../../.github/workflows/python-wheel-qualification.yml")
                .contains("runs-on: ubuntu-22.04")
        );
        assert!(
            include_str!("../../../.github/workflows/windows-subset-qualification.yml")
                .contains("runs-on: windows-latest")
        );
    }
}
