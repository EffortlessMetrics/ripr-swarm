//! Source qualification only: execute the literal Bash guard/payload with bounded stubs.
const WORKFLOW: &str = include_str!("../../../.github/workflows/printed-command-paste.yml");
const ORIGINAL: &str =
    include_str!("../../../fixtures/convergence/printed-command-paste/original.yml");

fn payload(source: &str) -> String {
    // Remove only the independently checked receipt and native-deadline
    // additions, then compare the original toolchain/payload byte-for-byte.
    let clear = "      - name: Clear stale terminal paste receipt\n        if: github.event_name == 'merge_group'\n        shell: bash\n        run: python -c 'from pathlib import Path; import os; (Path(os.environ[\"RUNNER_TEMP\"]) / \"printed-command-receipt.json\").unlink(missing_ok=True)'\n\n";
    let upload = "      - name: Upload terminal paste receipt\n        if: ${{ github.event_name == 'merge_group' && steps.paste_replay.outcome == 'success' }}\n        uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2\n        with:\n          name: printed-command-receipt-${{ matrix.os }}-${{ github.run_id }}-${{ github.run_attempt }}\n          path: ${{ runner.temp }}/printed-command-receipt.json\n          if-no-files-found: error\n          retention-days: 5\n\n";
    let deadline = "      - name: Complete within native qualification deadline\n        if: ${{ always() && !cancelled() && github.event_name == 'merge_group' }}\n        uses: ./target/qualification-budget\n        with:\n          receipt: ${{ inputs.qualification_budget }}\n          verifier-sha256: ${{ inputs.budget_verifier_sha256 }}\n          subject: ${{ inputs.subject_sha || github.sha }}\n";
    let receipt_env = "          RIPR_PASTE_RECEIPT: ${{ github.event_name == 'merge_group' && format('{0}/printed-command-receipt.json', runner.temp) || '' }}\n          RIPR_PASTE_PROFILE: ${{ inputs.proof_profile }}\n          RIPR_PASTE_H: ${{ inputs.head_sha }}\n          RIPR_PASTE_B: ${{ inputs.base_sha }}\n          RIPR_PASTE_G: ${{ inputs.subject_sha }}\n          RIPR_PASTE_TREE: ${{ inputs.subject_tree }}\n          RIPR_PASTE_GENERATION: ${{ inputs.workflow_generation }}\n          RIPR_PASTE_REQUEST: ${{ inputs.request_id }}\n          RIPR_PASTE_OS: ${{ matrix.os }}\n";
    source
        .split_once("      - uses: dtolnay/rust-toolchain@035fceb3c12fb0c6a9461b52775e16473133e591 # 1.95\n")
        .or_else(|| source.split_once("      - uses: dtolnay/rust-toolchain@1.95\n"))
        .map_or("", |(_, tail)| tail)
        .replace(clear, "")
        .replace(upload, "")
        .replace(deadline, "")
        .replace("        id: paste_replay\n", "")
        .replace(receipt_env, "")
        .replace("Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2", "Swatinem/rust-cache@v2")
        .replace("actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2", "actions/upload-artifact@v7")
        .trim_end()
        .to_owned()
}

fn input_bindings(source: &str) -> bool {
    let Some((_, tail)) =
        source.split_once("      - name: Verify exact source before product execution\n")
    else {
        return false;
    };
    let Some((step, _)) = tail.split_once("\n      - ") else {
        return false;
    };
    source.contains("group: printed-command-paste-${{ inputs.request_id || github.event.pull_request.number || github.ref }}-${{ inputs.subject_sha || ((github.event_name == 'pull_request' && !(github.event.action == 'labeled' && github.event.label.name == 'merge-ready')) && github.run_id || 'run') }}")
        && !step.lines().any(|line| line.starts_with("        if:") || line.starts_with("        continue-on-error:"))
        && source.contains("cancel-in-progress: ${{ !contains(toJSON(inputs), '\"subject_sha\"') && (github.event_name == 'pull_request' && github.event.action == 'labeled' && github.event.label.name == 'merge-ready') }}")
        && [
            "CALLED: ${{ contains(toJSON(inputs), '\"subject_sha\"') }}",
            "PROFILE: ${{ inputs.proof_profile }}", "REQUEST: ${{ inputs.request_id }}",
            "HEAD_SHA: ${{ inputs.head_sha }}", "BASE_SHA: ${{ inputs.base_sha }}",
            "SUBJECT: ${{ inputs.subject_sha || github.sha }}", "TREE: ${{ inputs.subject_tree }}",
            "GENERATION: ${{ inputs.workflow_generation }}",
        ].iter().all(|binding| step.contains(binding))
}

#[test]
fn input_binding_and_guard_removal_mutants_are_rejected() {
    assert!(input_bindings(WORKFLOW));
    for (old, new) in [
        (
            "group: printed-command-paste-${{ inputs.request_id || github.event.pull_request.number || github.ref }}-${{ inputs.subject_sha || ((github.event_name == 'pull_request' && !(github.event.action == 'labeled' && github.event.label.name == 'merge-ready')) && github.run_id || 'run') }}",
            "group: printed-command-paste-${{ github.sha }}",
        ),
        (
            "CALLED: ${{ contains(toJSON(inputs), '\"subject_sha\"') }}",
            "CALLED: false",
        ),
        ("${{ inputs.proof_profile }}", "qualification"),
        ("${{ inputs.request_id }}", "1-1"),
        ("${{ inputs.head_sha }}", "main"),
        ("${{ inputs.base_sha }}", "main"),
        (
            "SUBJECT: ${{ inputs.subject_sha || github.sha }}",
            "SUBJECT: main",
        ),
        ("${{ inputs.subject_tree }}", "main"),
        ("${{ inputs.workflow_generation }}", "main"),
        (
            "cancel-in-progress: ${{ !contains(toJSON(inputs), '\"subject_sha\"') && (github.event_name == 'pull_request' && github.event.action == 'labeled' && github.event.label.name == 'merge-ready') }}",
            "cancel-in-progress: true",
        ),
        (
            "      - name: Verify exact source before product execution\n",
            "      - name: Verify exact source before product execution\n        if: false\n",
        ),
        (
            "Verify exact source before product execution",
            "Removed guard",
        ),
    ] {
        assert!(WORKFLOW.contains(old));
        assert!(
            !input_bindings(&WORKFLOW.replace(old, new)),
            "accepted {old}"
        );
    }
}

fn contract(source: &str) -> bool {
    let original_matrix = ORIGINAL
        .split_once("    strategy:\n")
        .and_then(|(_, tail)| tail.split_once("    steps:\n"))
        .map(|(matrix, _)| matrix);
    let matrix = source
        .split_once("    strategy:\n")
        .and_then(|(_, tail)| tail.split_once("    steps:\n"))
        .map(|(matrix, _)| matrix);
    input_bindings(source)
        && terminal_receipt_wiring(source)
        && repo_policy::queue_workflow::budget_violations(
            ".github/workflows/printed-command-paste.yml",
            source,
        )
        .is_empty()
        && matrix.is_some()
        && matrix == original_matrix
        && payload(source) == payload(ORIGINAL)
        && source.contains("timeout-minutes: 20")
        && source.contains("ref: ${{ inputs.subject_sha || github.sha }}")
        && source.contains("persist-credentials: false")
        && source.contains("permissions:\n  contents: read\n")
        && !source.contains("continue-on-error:")
}

#[test]
fn exact_checkout_and_full_platform_shell_payload_are_preserved() {
    assert!(contract(WORKFLOW));
    assert!(WORKFLOW.contains("schedule:\n    - cron: \"43 5 * * *\""));
    assert!(!WORKFLOW.contains("  pull_request:"));
    assert!(WORKFLOW.contains("  workflow_call:"));
    assert!(WORKFLOW.contains("  workflow_dispatch:"));
    assert!(!WORKFLOW.contains("  push:"));
    assert!(!WORKFLOW.contains("pull_request_target:"));
    for key in [
        "proof_profile",
        "request_id",
        "head_sha",
        "base_sha",
        "subject_sha",
        "subject_tree",
        "workflow_generation",
    ] {
        assert!(WORKFLOW.contains(&format!(
            "      {key}:\n        type: string\n        required: true"
        )));
    }
    let checkout =
        WORKFLOW.find("actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803 # v6.1.0");
    let guard = WORKFLOW.find("Verify exact source before product execution");
    let toolchain =
        WORKFLOW.find("dtolnay/rust-toolchain@035fceb3c12fb0c6a9461b52775e16473133e591");
    assert!(checkout.is_some() && guard.is_some() && toolchain.is_some());
    assert!(checkout < guard && guard < toolchain);
}

#[test]
fn removal_and_failure_masking_controls_reject_wrong_workflows() {
    for (old, new) in [
        (
            "cargo test -p ripr --test printed_command_paste -- --nocapture",
            "echo skipped",
        ),
        ("shells: bash,sh,zsh,pwsh", "shells: bash,sh,pwsh"),
        ("shells: bash,pwsh,powershell", "shells: bash,pwsh"),
        ("RIPR_PASTE_REQUIRE", "OPTIONAL_SHELLS"),
        ("if: always()", "if: success()"),
        ("ref: ${{ inputs.subject_sha || github.sha }}", "ref: main"),
        ("timeout-minutes: 20", "timeout-minutes: 45"),
        (
            "shell: bash",
            "continue-on-error: true\n        shell: bash",
        ),
    ] {
        assert!(
            WORKFLOW.contains(old),
            "control must mutate actual source: {old}"
        );
        assert!(
            !contract(&WORKFLOW.replace(old, new)),
            "accepted mutant: {old}"
        );
    }
}

#[cfg(target_os = "linux")]
mod execution {
    use super::*;
    use std::process::Command;

    const SUBJECT: &str = "1111111111111111111111111111111111111111";
    const HEAD: &str = "2222222222222222222222222222222222222222";
    const BASE: &str = "3333333333333333333333333333333333333333";
    const TREE: &str = "4444444444444444444444444444444444444444";
    const GENERATION: &str = "5555555555555555555555555555555555555555";

    fn guard() -> Result<String, String> {
        let (_, tail) = WORKFLOW
            .split_once("        run: |\n")
            .ok_or("missing guard")?;
        let (body, _) = tail.split_once("\n      - ").ok_or("missing guard end")?;
        Ok(body
            .lines()
            .map(|line| line.strip_prefix("          ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    // Stub only Git's immutable-object reads. The executed decision is the real workflow body.
    fn run(body: &str, overrides: &[(&str, &str)]) -> Result<bool, String> {
        let stub = "git() { case \"$*\" in 'rev-parse HEAD') printf '%s\\n' \"$ACTUAL\";; 'rev-parse HEAD^{tree}') printf '%s\\n' \"$ACTUAL_TREE\";; 'show -s --format=%P HEAD') printf '%s\\n' \"$PARENTS\";; *) return 91;; esac; }\ncargo() { return \"$CARGO_STATUS\"; }\n";
        let output = Command::new("timeout")
            .args([
                "10s",
                "bash",
                "--noprofile",
                "--norc",
                "-eo",
                "pipefail",
                "-c",
                &format!("{stub}{body}"),
            ])
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .env("CALLED", "true")
            .env("PROFILE", "qualification")
            .env("REQUEST", "123-1")
            .env("HEAD_SHA", HEAD)
            .env("BASE_SHA", BASE)
            .env("SUBJECT", SUBJECT)
            .env("TREE", TREE)
            .env("GENERATION", GENERATION)
            .env("GITHUB_WORKFLOW_SHA", GENERATION)
            .env("GITHUB_EVENT_NAME", "pull_request")
            .env("ACTUAL", SUBJECT)
            .env("ACTUAL_TREE", TREE)
            .env("PARENTS", format!("{BASE} {HEAD}"))
            .env("CARGO_STATUS", "0")
            .envs(overrides.iter().copied())
            .output()
            .map_err(|e| e.to_string())?;
        assert_ne!(output.status.code(), Some(124), "fixture exceeded deadline");
        Ok(output.status.success())
    }

    fn job_admission(source: &str) -> Result<&str, String> {
        let (_, paste) = source
            .split_once("\njobs:\n  paste:\n")
            .ok_or("missing paste job")?;
        let (header, _) = paste
            .split_once("    steps:\n")
            .ok_or("missing paste steps")?;
        let mut conditions = header
            .lines()
            .filter_map(|line| line.strip_prefix("    if: "));
        let condition = conditions.next().ok_or("missing paste job admission")?;
        if conditions.next().is_some() {
            return Err("duplicate paste job admission".into());
        }
        Ok(condition)
    }

    fn admitted(
        expression: &str,
        called: bool,
        event: &str,
        action: &str,
        label: &str,
        labels: &[&str],
    ) -> Result<bool, String> {
        let flag = |value| if value { "true" } else { "false" };
        let expression = expression
            .replace("contains(toJSON(inputs), '\"subject_sha\"')", flag(called))
            .replace(
                "github.event_name != 'pull_request'",
                flag(event != "pull_request"),
            )
            .replace(
                "contains(github.event.pull_request.labels.*.name, 'merge-ready')",
                flag(labels.contains(&"merge-ready")),
            )
            .replace(
                "github.event.action == 'labeled'",
                flag(action == "labeled"),
            )
            .replace(
                "github.event.label.name == 'merge-ready'",
                flag(label == "merge-ready"),
            );
        assert!(!expression.contains("github.") && !expression.contains("inputs."));
        let expression = expression
            .replace("true", "1 == 1")
            .replace("false", "1 == 0");
        run(
            &format!("if [[ {expression} ]]; then exit 0; else exit 1; fi"),
            &[],
        )
    }

    #[test]
    fn callable_job_admission_preserves_inherited_events_and_refuses_sticky_labels()
    -> Result<(), String> {
        let expression = job_admission(WORKFLOW)?;
        for (event, action, label, labels, expected_legacy) in [
            (
                "pull_request",
                "synchronize",
                "",
                vec!["merge-ready"],
                false,
            ),
            ("pull_request", "reopened", "", vec!["merge-ready"], false),
            (
                "pull_request",
                "labeled",
                "unrelated",
                vec!["merge-ready"],
                false,
            ),
            (
                "pull_request",
                "labeled",
                "merge-ready",
                vec!["merge-ready", "full-ci"],
                true,
            ),
            ("schedule", "", "", vec![], true),
            ("workflow_dispatch", "", "", vec![], true),
        ] {
            assert_eq!(
                admitted(expression, false, event, action, label, &labels)?,
                expected_legacy
            );
            assert!(admitted(expression, true, event, action, label, &labels)?);
        }
        Ok(())
    }

    #[test]
    fn callable_admission_removal_is_rejected() -> Result<(), String> {
        const BYPASS: &str = "contains(toJSON(inputs), '\"subject_sha\"') || ";
        let expression = job_admission(WORKFLOW)?;
        assert_eq!(expression.matches(BYPASS).count(), 1);
        let mutant = expression.replacen(BYPASS, "", 1);
        assert!(admitted(
            expression,
            true,
            "pull_request",
            "synchronize",
            "",
            &[]
        )?);
        assert!(
            !admitted(&mutant, true, "pull_request", "synchronize", "", &[])?,
            "removing only the callable bypass must reject the valid called request"
        );
        Ok(())
    }

    #[test]
    fn production_guard_event_matrix() -> Result<(), String> {
        let body = guard()?;
        for event in ["pull_request", "schedule", "workflow_dispatch"] {
            assert!(
                run(&body, &[("CALLED", "false"), ("GITHUB_EVENT_NAME", event)])?,
                "{event}"
            );
        }
        for event in [
            "push",
            "merge_group",
            "pull_request_target",
            "issue_comment",
            "workflow_run",
            "",
        ] {
            assert!(
                !run(&body, &[("CALLED", "false"), ("GITHUB_EVENT_NAME", event)])?,
                "{event}"
            );
        }
        // Called workflows inherit the caller event, not a fictitious workflow_call event.
        assert!(run(&body, &[("GITHUB_EVENT_NAME", "pull_request_target")])?);
        Ok(())
    }

    #[test]
    fn malformed_wrong_stale_and_injected_subjects_fail_closed() -> Result<(), String> {
        let body = guard()?;
        assert!(run(&body, &[])?);
        for (key, value) in [
            ("SUBJECT", "main"),
            ("SUBJECT", ""),
            ("SUBJECT", "$(exit 0); echo injected"),
            ("ACTUAL", HEAD),
            ("TREE", ""),
            ("ACTUAL_TREE", HEAD),
            ("GENERATION", HEAD),
            ("GITHUB_WORKFLOW_SHA", HEAD),
            ("HEAD_SHA", ""),
            ("BASE_SHA", "refs/heads/main"),
            ("REQUEST", "123; exit 0"),
            ("REQUEST", ""),
            ("PROFILE", "diagnostic"),
            ("PROFILE", ""),
            ("PARENTS", HEAD),
        ] {
            assert!(!run(&body, &[(key, value)])?, "accepted {key}={value}");
        }
        assert!(!run(&body, &[("PARENTS", &format!("{HEAD} {BASE}"))])?);
        // Control: deleting the parent predicate admits a wrong integration commit.
        let mutant = body
            .lines()
            .filter(|line| !line.contains("Wrong integration parents"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(run(&mutant, &[("PARENTS", HEAD)])?);
        Ok(())
    }

    #[test]
    fn actual_payload_native_failure_is_not_hidden() -> Result<(), String> {
        let source = payload(WORKFLOW);
        let command = source
            .lines()
            .find_map(|line| line.strip_prefix("        run: cargo test"))
            .ok_or("missing test payload")?;
        let body = format!("cargo test{command}");
        assert!(run(&body, &[])?);
        assert!(!run(&body, &[("CARGO_STATUS", "37")])?);
        Ok(())
    }
}

/// Terminal evidence is separate from the pre-replay printed-command inventory.
/// This source contract does not authenticate the artifact or its producer.
fn terminal_receipt_wiring(source: &str) -> bool {
    let Some((prefix, tail)) =
        source.split_once("      - name: Paste every printed command into every shell\n")
    else {
        return false;
    };
    let Some((replay, rest)) = tail.split_once("\n      - ") else {
        return false;
    };
    let Some((_, upload_tail)) = rest.split_once("name: Upload terminal paste receipt\n") else {
        return false;
    };
    let upload = upload_tail
        .split("\n      - ")
        .next()
        .unwrap_or(upload_tail);
    let Some((_, clear_tail)) =
        prefix.rsplit_once("      - name: Clear stale terminal paste receipt\n")
    else {
        return false;
    };
    let clear = clear_tail.split("\n      - ").next().unwrap_or(clear_tail);
    let clear_expected = [
        "        if: github.event_name == 'merge_group'",
        "        shell: bash",
        "        run: python -c 'from pathlib import Path; import os; (Path(os.environ[\"RUNNER_TEMP\"]) / \"printed-command-receipt.json\").unlink(missing_ok=True)'",
    ];
    let replay_expected = [
        "        id: paste_replay",
        "          RIPR_PASTE_REQUIRE: ${{ matrix.shells }}",
        "          RIPR_PASTE_REPORT: ${{ runner.temp }}/printed-commands.jsonl",
        "          RIPR_PASTE_RECEIPT: ${{ github.event_name == 'merge_group' && format('{0}/printed-command-receipt.json', runner.temp) || '' }}",
        "          RIPR_PASTE_PROFILE: ${{ inputs.proof_profile }}",
        "          RIPR_PASTE_H: ${{ inputs.head_sha }}",
        "          RIPR_PASTE_B: ${{ inputs.base_sha }}",
        "          RIPR_PASTE_G: ${{ inputs.subject_sha }}",
        "          RIPR_PASTE_TREE: ${{ inputs.subject_tree }}",
        "          RIPR_PASTE_GENERATION: ${{ inputs.workflow_generation }}",
        "          RIPR_PASTE_REQUEST: ${{ inputs.request_id }}",
        "          RIPR_PASTE_OS: ${{ matrix.os }}",
        "        run: cargo test -p ripr --test printed_command_paste -- --nocapture",
    ];
    let upload_expected = [
        "        if: ${{ github.event_name == 'merge_group' && steps.paste_replay.outcome == 'success' }}",
        "        uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2",
        "          name: printed-command-receipt-${{ matrix.os }}-${{ github.run_id }}-${{ github.run_attempt }}",
        "          path: ${{ runner.temp }}/printed-command-receipt.json",
        "          if-no-files-found: error",
    ];
    clear_expected
        .iter()
        .all(|line| clear.lines().any(|actual| actual == *line))
        && !clear.contains("continue-on-error:")
        && replay_expected
            .iter()
            .all(|line| replay.lines().any(|actual| actual == *line))
        && upload_expected
            .iter()
            .all(|line| upload.lines().any(|actual| actual == *line))
        && !replay.contains("continue-on-error:")
        && !upload.contains("continue-on-error:")
}

#[test]
fn terminal_receipt_wiring_rejects_missing_or_weakened_bindings() {
    assert!(terminal_receipt_wiring(WORKFLOW));
    for (old, new) in [
        ("        id: paste_replay", "        id: absent"),
        (
            "Clear stale terminal paste receipt",
            "Removed stale cleanup",
        ),
        ("unlink(missing_ok=True)", "exists()"),
        (
            "RIPR_PASTE_TREE: ${{ inputs.subject_tree }}",
            "RIPR_PASTE_TREE: main",
        ),
        (
            "RIPR_PASTE_REQUEST: ${{ inputs.request_id }}",
            "RIPR_PASTE_REQUEST: 1-1",
        ),
        ("steps.paste_replay.outcome == 'success'", "true"),
        (
            "          path: ${{ runner.temp }}/printed-command-receipt.json\n          if-no-files-found: error",
            "          path: ${{ runner.temp }}/printed-command-receipt.json\n          if-no-files-found: ignore",
        ),
        (
            "printed-command-receipt-${{ matrix.os }}-${{ github.run_id }}-${{ github.run_attempt }}",
            "receipt",
        ),
    ] {
        assert!(WORKFLOW.contains(old), "missing mutation target {old}");
        assert!(
            !terminal_receipt_wiring(&WORKFLOW.replacen(old, new, 1)),
            "accepted {old}"
        );
    }
}

/// Source-order evidence only. Runtime results and authenticated artifact
/// custody remain separate obligations of the eventual finalizer.
fn terminal_source_contract(source: &str) -> bool {
    let Some((_, body)) = source.split_once(
        "fn printed_commands_paste_unchanged_in_every_shell_from_a_foreign_directory() -> Result<(), String>",
    ) else {
        return false;
    };
    let anchors = [
        "let receipt_target = receipt::prepare_from_environment()?;",
        "let fixture = build_fixture()?;",
        "let outcomes: Vec<Outcome> =",
        "if outcomes.len() != cases.len()",
        "receipt::classify_known_gap(",
        "for hit in canary_hits(&fixture)",
        "for (gap, hits) in KNOWN_GAPS.iter().zip(&gap_hits)",
        "receipt::known_gap_unreproduced(*hits)",
        "if problems.is_empty() {",
        "target.write(inventory, &gap_audit, commands.len(), &shell_replays)?;",
    ];
    let mut cursor = 0;
    for anchor in anchors {
        let Some(relative) = body[cursor..].find(anchor) else {
            return false;
        };
        cursor += relative + anchor.len();
    }
    true
}

#[test]
fn terminal_source_rejects_preexecution_or_incomplete_replay_mutants() {
    const SOURCE: &str = include_str!("../../../crates/ripr/tests/printed_command_paste.rs");
    assert!(terminal_source_contract(SOURCE));
    for (old, new) in [
        (
            "let receipt_target = receipt::prepare_from_environment()?;",
            "let receipt_target = None;",
        ),
        ("if outcomes.len() != cases.len()", "if false"),
        (
            "receipt::classify_known_gap(",
            "removed_gap_classification(",
        ),
        ("receipt::known_gap_unreproduced(*hits)", "false"),
        ("for hit in canary_hits(&fixture)", "for hit in []"),
        (
            "for (gap, hits) in KNOWN_GAPS.iter().zip(&gap_hits)",
            "for (gap, hits) in []",
        ),
        (
            "if problems.is_empty() {\n        if let Some(target) = receipt_target",
            "if true {\n        if let Some(target) = receipt_target",
        ),
    ] {
        assert!(SOURCE.contains(old), "missing mutation target {old}");
        assert!(
            !terminal_source_contract(&SOURCE.replacen(old, new, 1)),
            "accepted {old}"
        );
    }
}
