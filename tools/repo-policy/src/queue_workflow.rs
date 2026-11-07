//! Final queue source contract. Runtime admission, cache/capacity qualification
//! and the protected H/G publisher remain independent activation obligations.

use crate::workflow_lifecycle::event_declarations;

pub mod reconciliation;

pub const GENERATION: &str = "  CI_LIFECYCLE_GENERATION: merge-queue-v1";
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

fn budget_survives_checkouts(body: &str) -> bool {
    let mut retained = false;
    for block in body.split("\n      - ").skip(1) {
        if block.starts_with("name: Download native-run deadline verifier\n")
            || block.starts_with("name: Issue native-run qualification deadline\n")
        {
            retained = true;
        }
        let checkout = block.starts_with("uses: actions/checkout@")
            || block
                .lines()
                .any(|line| line.starts_with("        uses: actions/checkout@"));
        // The retained trusted-source checkout is confined to its own sibling
        // directory. A root/default or other unreviewed checkout can clean the
        // ignored target/ verifier and must precede retention instead.
        if retained
            && checkout
            && !block
                .lines()
                .any(|line| line == "          path: .admission-trusted")
        {
            return false;
        }
    }
    retained
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
            events.len() == 1
                && events
                    .get("merge_group")
                    .is_some_and(|config| config == &["types: [checks_requested]"])
        }),
        "ordinary root must accept only native merge_group checks_requested",
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
        require(header.contains("    runs-on:\n      group: em-ci-build\n      labels: [self-hosted, linux, x64, rust-heavy]"), &format!("{name} candidate composition requires selected-source heavy capacity"));
        require(
            header.contains("    timeout-minutes:"),
            &format!("{name} requires a deadline"),
        );
    }
    let admission = job(source, "admission").unwrap_or("");
    require(
        admission
            .split("    steps:\n")
            .next()
            .unwrap_or("")
            .contains("    if: needs.detect-docs-only.result == 'success'"),
        "admission must not allocate work after issuer failure or omission",
    );

    for binding in [
        "uses: ./.github/actions/verify-merge-group-subject",
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
        "    needs: [detect-docs-only, admission, rust-owned, windows-subset, python-wheel, ci-feature, printed-command]",
        "    if: always()",
        "    timeout-minutes: 5",
        "    runs-on:\n      group: em-ci-build\n      labels: [self-hosted, linux, x64, ci-control]",
    ] {
        require(
            header.contains(binding),
            &format!("mandatory result binding is absent: {binding}"),
        );
    }
    for binding in [
        "ref: ${{ needs.admission.outputs.trusted_sha }}",
        "sparse-checkout: .github/actions/pr-candidate-admission",
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
    let aggregate = step(result, "Require complete native group portfolio").unwrap_or("");
    require(
        aggregate.contains("ISSUER_RESULT: ${{ needs.detect-docs-only.result }}")
            && aggregate.contains("[ \"$ISSUER_RESULT\" = success ]"),
        "native portfolio must require the issuer's actual successful result",
    );
    let summary = step(result, "Summarize routed result").unwrap_or("");
    require(
        summary.contains("ISSUER_RESULT: ${{ needs.detect-docs-only.result }}")
            && summary.contains("\"$ISSUER_RESULT\" == success"),
        "terminal summary must refuse missing or failed issuer proof",
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
    findings.extend(budget_violations(
        ".github/workflows/routed-rust.yml",
        source,
    ));
    findings
}

/// Acceptance conserves the caller's native run deadline through every selected
/// child. This does not claim Actions jobs are cancelled at that deadline.
pub fn budget_violations(path: &str, source: &str) -> Vec<String> {
    let mut findings = Vec::new();
    let mut require = |condition: bool, detail: &str| {
        if !condition {
            findings.push(format!("{path}: qualification budget: {detail}"));
        }
    };
    let keys = [
        "qualification_budget",
        "budget_artifact_id",
        "budget_verifier_sha256",
    ];
    let caller = path == ".github/workflows/routed-rust.yml";
    let selected: &[&str] = match path {
        ".github/workflows/routed-rust.yml" => &["admission", "result"],
        ".github/workflows/rust-gates.yml" => &["rust-gates"],
        ".github/workflows/ci.yml" => &[
            "source-identity",
            "release-proof",
            "rust-only-features",
            "msrv",
            "ci-result",
        ],
        ".github/workflows/python-wheel-qualification.yml" => {
            &["applicability", "source-contract", "build-wheel", "consume"]
        }
        ".github/workflows/windows-subset-qualification.yml" => &["windows-advisory-subset"],
        ".github/workflows/printed-command-paste.yml" => &["paste"],
        ".github/workflows/vscode-e2e.yml" => &["vscode"],
        _ => return findings,
    };
    let line = |text: &str, expected: &str| text.lines().any(|line| line == expected);
    if caller {
        let detection = job(source, "detect-docs-only").unwrap_or("");
        require(
            step(detection, "Issue native-run qualification deadline").is_some_and(|body| {
                line(body, "        uses: ./.github/actions/qualification-budget")
                    && line(body, "          issue: 'true'")
                    && line(body, "          subject: ${{ github.sha }}")
            }),
            "only the initial caller observes the native run start",
        );
        for binding in [
            "      qualification_budget: ${{ steps.budget.outputs.receipt }}",
            "      budget_artifact_id: ${{ steps.budget_artifact.outputs.artifact-id }}",
            "      budget_verifier_sha256: ${{ steps.budget.outputs.verifier-sha256 }}",
            "          name: qualification-budget-${{ github.run_id }}-${{ github.run_attempt }}",
            "          path: ${{ runner.temp }}/qualification-budget/",
            "          if-no-files-found: error",
        ] {
            require(line(detection, binding), &format!("issuer lost {binding}"));
        }
        require(
            step(detection, "Complete within native qualification deadline").is_some_and(|guard| {
                line(guard, "        shell: python {0}")
                    && line(guard, "          BUDGET_ISSUE: 'false'")
                    && line(guard, "          BUDGET_RECEIPT: ${{ steps.budget.outputs.receipt }}")
                    && line(guard, "          BUDGET_SOURCE: ${{ runner.temp }}/qualification-budget/budget.py")
                    && line(guard, "          BUDGET_VERIFIER_SHA256: ${{ steps.budget.outputs.verifier-sha256 }}")
                    && line(guard, "          BUDGET_DESCRIPTOR_SHA256: ${{ steps.budget.outputs.descriptor-sha256 }}")
                    && !guard.contains("continue-on-error")
            }),
            "issuer must authenticate its complete frozen verifier before deadline acceptance",
        );
        require(
            budget_survives_checkouts(detection),
            "detection checkout can erase or replace its retained verifier source",
        );
        require(
            !detection.contains("uses: actions/download-artifact@")
                && !detection.contains("name: Reuse issued native-run deadline verifier locally"),
            "issuer must directly consume its frozen verifier without a copy or artifact round trip",
        );
        let admission = job(source, "admission").unwrap_or("");
        for key in keys {
            require(
                line(
                    admission,
                    &format!("      {key}: ${{{{ needs.detect-docs-only.outputs.{key} }}}}"),
                ),
                "admission must forward the original custody without renewal",
            );
        }
        for child in [
            "rust-owned",
            "windows-subset",
            "python-wheel",
            "ci-feature",
            "printed-command",
        ] {
            let body = job(source, child).unwrap_or("");
            for key in keys {
                require(
                    line(
                        body,
                        &format!("      {key}: ${{{{ needs.admission.outputs.{key} }}}}"),
                    ),
                    &format!("{child} must preserve {key}"),
                );
            }
        }
    } else {
        let inputs = source.split("\njobs:\n").next().unwrap_or("");
        for key in keys {
            require(
                line(inputs, &format!("      {key}:")),
                &format!("callable leaf lacks {key}"),
            );
        }
        require(
            !source.contains("          issue: 'true'"),
            "children must not issue a new budget",
        );
        if path == ".github/workflows/ci.yml" {
            let nested = job(source, "vscode").unwrap_or("");
            for key in keys {
                require(
                    line(nested, &format!("      {key}: ${{{{ inputs.{key} }}}}")),
                    &format!("nested VS Code must preserve {key}"),
                );
            }
        }
    }
    for name in selected {
        let body = job(source, name).unwrap_or("");
        require(
            budget_survives_checkouts(body),
            &format!("{name} checkout can erase its downloaded deadline verifier"),
        );
        let origin = if caller {
            if *name == "admission" {
                "needs.detect-docs-only.outputs"
            } else {
                "needs.admission.outputs"
            }
        } else {
            "inputs"
        };
        let receipt = format!("          receipt: ${{{{ {origin}.qualification_budget }}}}");
        let digest =
            format!("          verifier-sha256: ${{{{ {origin}.budget_verifier_sha256 }}}}");
        require(
            step(body, "Download native-run deadline verifier").is_some_and(|download| {
                line(
                    download,
                    &format!("          artifact-ids: ${{{{ {origin}.budget_artifact_id }}}}"),
                ) && line(download, "          path: target/qualification-budget")
            }),
            &format!("{name} must consume the exact issuer artifact"),
        );
        for (label, condition) in [
            ("Enter", "        if: github.event_name == 'merge_group'"),
            (
                "Complete",
                "        if: ${{ always() && !cancelled() && github.event_name == 'merge_group' }}",
            ),
        ] {
            let marker = format!("{label} within native qualification deadline");
            require(
                step(body, &marker).is_some_and(|guard| {
                    line(guard, condition)
                        && line(guard, "        uses: ./target/qualification-budget")
                        && line(guard, &receipt)
                        && line(guard, &digest)
                        && !guard.contains("continue-on-error")
                }),
                &format!("{name} lost mandatory {label} deadline check"),
            );
        }
        let last = body.rsplit("      - ").next().unwrap_or("");
        require(
            last.starts_with("name: Complete within native qualification deadline\n"),
            &format!("{name} must finish with deadline acceptance"),
        );
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    const CALLER: &str = include_str!("../../../.github/workflows/routed-rust.yml");
    const RUST: &str = include_str!("../../../.github/workflows/rust-gates.yml");

    #[test]
    fn checkout_cannot_clean_a_verifier_needed_at_completion() -> Result<(), String> {
        let source = include_str!("../../../.github/workflows/windows-subset-qualification.yml");
        let start = source
            .find("      - uses: actions/checkout@")
            .ok_or("checkout missing from Windows fixture")?;
        let end = start
            + source[start + 1..]
                .find("\n      - ")
                .ok_or("next Windows step missing")?
            + 1;
        let checkout = &source[start..end];
        let without = source.replacen(checkout, "", 1);
        let marker = "      - name: Verify admitted integration source before product execution\n";
        let wrong = without.replacen(marker, &format!("{checkout}\n{marker}"), 1);
        assert_ne!(wrong, source);
        let path = ".github/workflows/windows-subset-qualification.yml";
        assert!(budget_violations(path, source).is_empty());
        assert!(
            budget_violations(path, &wrong)
                .iter()
                .any(|finding| finding.contains("checkout can erase"))
        );

        let rebound = CALLER.replace(
            "          path: .admission-trusted",
            "          path: target",
        );
        assert_ne!(rebound, CALLER);
        assert!(
            budget_violations(".github/workflows/routed-rust.yml", &rebound)
                .iter()
                .any(|finding| finding.contains("checkout can erase"))
        );
        Ok(())
    }

    #[test]
    fn deadline_custody_survives_every_callable_and_nested_leaf() {
        for (path, source) in [
            ("routed-rust.yml", CALLER),
            ("rust-gates.yml", RUST),
            ("ci.yml", include_str!("../../../.github/workflows/ci.yml")),
            (
                "python-wheel-qualification.yml",
                include_str!("../../../.github/workflows/python-wheel-qualification.yml"),
            ),
            (
                "windows-subset-qualification.yml",
                include_str!("../../../.github/workflows/windows-subset-qualification.yml"),
            ),
            (
                "printed-command-paste.yml",
                include_str!("../../../.github/workflows/printed-command-paste.yml"),
            ),
            (
                "vscode-e2e.yml",
                include_str!("../../../.github/workflows/vscode-e2e.yml"),
            ),
        ] {
            let path = format!(".github/workflows/{path}");
            assert!(budget_violations(&path, source).is_empty(), "{path}");
            for wrong in [
                source.replace("          receipt:", "          ignored-receipt:"),
                source.replace("          artifact-ids:", "          ignored-artifact-ids:"),
                source.replace(
                    "name: Complete within native qualification deadline",
                    "name: Skip deadline acceptance",
                ),
                source.replace(
                    "        uses: ./target/qualification-budget",
                    "        continue-on-error: true\n        uses: ./target/qualification-budget",
                ),
            ] {
                assert_ne!(wrong, source);
                assert!(!budget_violations(&path, &wrong).is_empty(), "{path}");
            }
        }
        let ci = include_str!("../../../.github/workflows/ci.yml");
        let wrong = ci.replace(
            "      qualification_budget: ${{ inputs.qualification_budget }}",
            "      qualification_budget: renewed",
        );
        assert_ne!(wrong, ci);
        assert!(!budget_violations(".github/workflows/ci.yml", &wrong).is_empty());
    }

    #[test]
    fn current_queue_generation_is_complete() {
        assert_eq!(violations(CALLER, Some(RUST)), Vec::<String>::new());
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
    fn skipped_missing_or_rebound_proof_cannot_weaken_the_result() {
        for wrong in [
            CALLER.replace(
                "ISSUER_RESULT: ${{ needs.detect-docs-only.result }}",
                "ISSUER_RESULT: success",
            ),
            CALLER.replace(
                "    if: needs.detect-docs-only.result == 'success'",
                "    if: always()",
            ),
            CALLER.replace("needs: [detect-docs-only, admission,", "needs: [admission,"),
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
