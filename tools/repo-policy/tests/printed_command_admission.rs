//! Source qualification only: execute the literal Bash guard/payload with bounded stubs.
const WORKFLOW: &str = include_str!("../../../.github/workflows/printed-command-paste.yml");
const ORIGINAL: &str =
    include_str!("../../../fixtures/convergence/printed-command-paste/original.yml");

fn payload(source: &str) -> &str {
    source
        .split_once("      - uses: dtolnay/rust-toolchain@1.95\n")
        .map_or("", |(_, tail)| tail)
}

fn input_bindings(source: &str) -> bool {
    let Some((_, tail)) =
        source.split_once("      - name: Verify exact source before product execution\n")
    else {
        return false;
    };
    let Some((step, _)) = tail.split_once("      - uses: dtolnay") else {
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
    assert!(
        WORKFLOW.contains("  pull_request:\n    types: [opened, synchronize, reopened, labeled]")
    );
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
    let checkout = WORKFLOW.find("actions/checkout@v6");
    let guard = WORKFLOW.find("Verify exact source before product execution");
    let toolchain = WORKFLOW.find("dtolnay/rust-toolchain@1.95");
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
        let (body, _) = tail
            .split_once("\n      - uses: dtolnay")
            .ok_or("missing guard end")?;
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
        let command = payload(WORKFLOW)
            .lines()
            .find_map(|line| line.strip_prefix("        run: cargo test"))
            .ok_or("missing test payload")?;
        let body = format!("cargo test{command}");
        assert!(run(&body, &[])?);
        assert!(!run(&body, &[("CARGO_STATUS", "37")])?);
        Ok(())
    }
}
