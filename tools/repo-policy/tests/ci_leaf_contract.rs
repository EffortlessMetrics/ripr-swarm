//! Bounded source qualification; no product build or hosted admission is established.
#![cfg(target_os = "linux")]
use std::process::{Command, Output};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const WORKFLOW: &str = include_str!("../../../.github/workflows/ci.yml");
const ORIGINAL: &str = include_str!("../../../fixtures/convergence/ci-exact-subject/original.yml");
const JOBS: &[(&str, &str)] = &[
    ("rust-only-features", "run_rust_only"),
    ("release-proof", "run_release_proof"),
    ("msrv", "run_msrv"),
    ("vscode", "run_vscode"),
];
fn job<'a>(source: &'a str, name: &str) -> &'a str {
    let marker = format!("\n  {name}:\n");
    let tail = source.split_once(&marker).unwrap_or_default().1;
    assert!(!tail.is_empty(), "missing job: {name}");
    let end = tail
        .lines()
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            Some((start, line))
        })
        .find(|(_, line)| {
            line.starts_with("  ") && !line.starts_with("   ") && !line.starts_with("  #")
        })
        .map_or(tail.len(), |(offset, _)| offset);
    &tail[..end]
}
fn body(step: &str) -> String {
    step.split_once("        run: |\n")
        .unwrap_or_default()
        .1
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}
fn run(script: &str, overrides: &[(&str, &str)]) -> TestResult<Output> {
    let mut cmd = Command::new("timeout");
    cmd.args([
        "--kill-after=1s",
        "5s",
        "bash",
        "--noprofile",
        "--norc",
        "-eo",
        "pipefail",
        "-c",
        script,
    ])
    .env_remove("BASH_ENV")
    .env_remove("ENV");
    for (key, value) in [
        ("PROFILE", "qualification"),
        ("REQUEST", "123-1"),
        ("HEAD_SHA", "2222222222222222222222222222222222222222"),
        ("BASE_SHA", "3333333333333333333333333333333333333333"),
        ("SUBJECT", "1111111111111111111111111111111111111111"),
        ("ACTUAL", "1111111111111111111111111111111111111111"),
        ("TREE", "4444444444444444444444444444444444444444"),
        ("ACTUAL_TREE", "4444444444444444444444444444444444444444"),
        ("GENERATION", "5555555555555555555555555555555555555555"),
        (
            "GITHUB_WORKFLOW_SHA",
            "5555555555555555555555555555555555555555",
        ),
        (
            "PARENTS",
            "3333333333333333333333333333333333333333 2222222222222222222222222222222222222222",
        ),
        ("SELECT_RUST", "true"),
        ("SELECT_RELEASE", "true"),
        ("SELECT_MSRV", "true"),
        ("SELECT_VSCODE", "true"),
        ("CANCELLED", "false"),
        ("IDENTITY_RESULT", "success"),
        ("RUST_RESULT", "success"),
        ("RELEASE_RESULT", "success"),
        ("MSRV_RESULT", "success"),
        ("VSCODE_RESULT", "success"),
        ("NATIVE_STATUS", "0"),
        ("GITHUB_OUTPUT", "/dev/null"),
        ("GITHUB_STEP_SUMMARY", "/dev/null"),
    ] {
        cmd.env(key, value);
    }
    for (key, value) in overrides {
        cmd.env(key, value);
    }
    let output = cmd.output()?;
    assert!(
        !matches!(output.status.code(), Some(124)),
        "timeout: {output:?}"
    );
    Ok(output)
}
fn guard(source: &str) -> String {
    format!(
        "git() {{ case \"$*\" in 'rev-parse HEAD') echo \"$ACTUAL\";; 'rev-parse HEAD^{{tree}}') echo \"$ACTUAL_TREE\";; 'show -s --format=%P HEAD') echo \"$PARENTS\";; *) return 91;; esac; }}\n{}",
        body(job(source, "source-identity"))
    )
}
#[test]
fn exact_subject_guard_rejects_missing_stale_malformed_and_wrong_parent_inputs() -> TestResult {
    let script = guard(WORKFLOW);
    assert!(run(&script, &[])?.status.success());
    for key in [
        "PROFILE",
        "REQUEST",
        "HEAD_SHA",
        "BASE_SHA",
        "SUBJECT",
        "TREE",
        "GENERATION",
        "GITHUB_WORKFLOW_SHA",
        "ACTUAL",
        "ACTUAL_TREE",
        "PARENTS",
        "SELECT_RUST",
    ] {
        for value in ["", "main", "0000000000000000000000000000000000000000"] {
            assert!(
                !run(&script, &[(key, value)])?.status.success(),
                "accepted {key}={value}"
            );
        }
    }
    assert!(
        !run(
            &script,
            &[
                ("SELECT_RUST", "false"),
                ("SELECT_RELEASE", "false"),
                ("SELECT_MSRV", "false"),
                ("SELECT_VSCODE", "false")
            ]
        )?
        .status
        .success()
    );
    let mutant = script.replace(
        "[[ \"$(git rev-parse HEAD)\" == \"$SUBJECT\" ]] || fail 'Wrong source checkout'",
        ":",
    );
    assert_ne!(mutant, script);
    assert!(
        run(
            &mutant,
            &[("ACTUAL", "0000000000000000000000000000000000000000")]
        )?
        .status
        .success(),
        "removal control must expose a false success"
    );
    Ok(())
}
#[test]
fn selected_child_missing_failed_cancelled_or_unexpected_cannot_pass() -> TestResult {
    let script = body(job(WORKFLOW, "ci-result"));
    assert!(run(&script, &[])?.status.success());
    for key in [
        "IDENTITY_RESULT",
        "RUST_RESULT",
        "RELEASE_RESULT",
        "MSRV_RESULT",
        "VSCODE_RESULT",
    ] {
        for value in ["", "skipped", "failure", "cancelled", "queued"] {
            assert!(
                !run(&script, &[(key, value)])?.status.success(),
                "accepted {key}={value}"
            );
        }
    }
    assert!(
        job(WORKFLOW, "ci-result").contains("if: always() && cancelled()\n        run: exit 1")
    );
    assert!(!run("exit 1", &[])?.status.success());
    for (select, result) in [
        ("SELECT_RUST", "RUST_RESULT"),
        ("SELECT_RELEASE", "RELEASE_RESULT"),
        ("SELECT_MSRV", "MSRV_RESULT"),
        ("SELECT_VSCODE", "VSCODE_RESULT"),
    ] {
        assert!(
            run(&script, &[(select, "false"), (result, "skipped")])?
                .status
                .success()
        );
        assert!(!run(&script, &[(select, "false")])?.status.success());
        let line = format!("require \"${select}\" \"${result}\"");
        assert!(script.contains(&line));
        assert!(
            run(&script.replace(&line, ":"), &[(result, "")])?
                .status
                .success(),
            "missing-child removal must expose false success"
        );
    }
    Ok(())
}
fn condition(source: &str) -> String {
    let tail = source.split_once("    if: >-\n").unwrap_or_default().1;
    tail.lines()
        .take_while(|line| line.starts_with("      "))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
}
fn predicate(
    expression: &str,
    called: bool,
    event: &str,
    action: &str,
    label: &str,
    selected: bool,
    identity: &str,
) -> TestResult<bool> {
    let flag = |v| if v { "true" } else { "false" };
    let mut expr=expression.replace("contains(toJSON(inputs), '\"subject_sha\"')",flag(called))
        .replace("contains(github.event.pull_request.labels.*.name, 'full-ci')",flag(label=="full-ci"))
        .replace("contains(github.event.pull_request.labels.*.name, 'release-check')",flag(label=="release-check"))
        .replace("contains(fromJSON('[\"opened\", \"synchronize\", \"reopened\"]'), github.event.action)",flag(["opened","synchronize","reopened"].contains(&action)))
        .replace("github.event_name",&format!("'{event}'"))
        .replace("github.ref", "'refs/heads/main'")
        .replace("needs.source-identity.result",&format!("'{identity}'"))
        .replace("always()","true").replace("!cancelled()","true");
    for (_, input) in JOBS {
        expr = expr.replace(&format!("inputs.{input}"), flag(selected));
    }
    assert!(
        !expr.contains("inputs.") && !expr.contains("github.") && !expr.contains("needs."),
        "untranslated {expr}"
    );
    // Translate Actions booleans to Bash comparisons, preserving the production operators.
    expr = expr
        .replace("!true", "false")
        .replace("!false", "true")
        .replace("true", "1 == 1")
        .replace("false", "1 == 0");
    let output = run(
        &format!("if [[ {expr} ]]; then exit 0; else exit 1; fi"),
        &[],
    )?;
    assert!(
        matches!(output.status.code(), Some(0 | 1)),
        "predicate syntax: {output:?}"
    );
    Ok(output.status.success())
}
#[test]
fn actual_predicates_preserve_legacy_events_and_use_only_frozen_callable_selection() -> TestResult {
    for (name, _) in JOBS {
        let expression = condition(job(WORKFLOW, name));
        // Draft is deliberately not a legacy filter; open/push/Ready remain staged, not activated.
        for (event, action, label) in [
            ("pull_request", "opened", ""),
            ("pull_request", "synchronize", ""),
            ("pull_request", "ready_for_review", ""),
            ("pull_request", "labeled", "unrelated"),
            ("pull_request", "labeled", "full-ci"),
            ("pull_request", "labeled", "release-check"),
            ("push", "", ""),
            ("workflow_dispatch", "", ""),
        ] {
            let expected = match *name {
                "release-proof" => {
                    event != "pull_request" || ["full-ci", "release-check"].contains(&label)
                }
                "rust-only-features" => {
                    event != "pull_request"
                        || ["opened", "synchronize", "reopened"].contains(&action)
                }
                "msrv" => {
                    event == "workflow_dispatch" || (event == "pull_request" && label == "full-ci")
                }
                "vscode" => event != "pull_request" || label == "full-ci",
                _ => return Err(format!("Unknown test job {name}").into()),
            };
            assert_eq!(
                predicate(&expression, false, event, action, label, false, "skipped")?,
                expected,
                "{name} {event}/{action}/{label}"
            );
            for selected in [true, false] {
                assert_eq!(
                    predicate(&expression, true, event, action, label, selected, "success")?,
                    selected,
                    "called {name} {event}/{action}/{label}"
                );
                assert!(!predicate(
                    &expression,
                    true,
                    event,
                    action,
                    label,
                    selected,
                    "failure"
                )?);
            }
        }
    }
    Ok(())
}
fn run_blocks(source: &str) -> Vec<String> {
    source
        .split("\n      - ")
        .skip(1)
        .filter_map(|step| {
            if step.contains("        run: |\n") {
                Some(body(step))
            } else if let Some(command) = step.strip_prefix("run: ") {
                Some(command.lines().next().unwrap_or_default().to_owned())
            } else {
                step.lines()
                    .find_map(|line| line.strip_prefix("        run: ").map(str::to_owned))
            }
        })
        .collect()
}
#[test]
fn feature_command_action_and_manual_denominators_are_unchanged() -> TestResult {
    for (name, _) in JOBS {
        assert_eq!(
            run_blocks(job(WORKFLOW, name)).iter().map(|script| script.replace("  [ \"${CALLED:-false}\" != true ] || return 0\n", "")).collect::<Vec<_>>(),
            run_blocks(job(ORIGINAL, name)).iter().map(|script| script.replace(
                "cargo package -p ripr --no-verify && tar xzf \"target/package/ripr-$(cargo pkgid -p ripr | sed 's/.*#//').crate\" -C \"$RUNNER_TEMP\"\ncd \"$RUNNER_TEMP/ripr-$(cargo pkgid -p ripr | sed 's/.*#//')\" && rm Cargo.lock && cargo generate-lockfile",
                "cargo package -p ripr --no-verify\npackage_version=\"$(cargo pkgid -p ripr | sed 's/.*#//')\"\ntar xzf \"target/package/ripr-$package_version.crate\" -C \"$RUNNER_TEMP\"\ncd \"$RUNNER_TEMP/ripr-$package_version\"\nrm Cargo.lock\ncargo generate-lockfile"
            )).collect::<Vec<_>>(),
            "payload {name}"
        );
        let actions = |s: &str| {
            s.lines()
                .filter(|l| l.trim_start().starts_with("- uses:"))
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(actions(job(WORKFLOW, name)), actions(job(ORIGINAL, name)));
        let child = job(WORKFLOW, name);
        assert!(child.contains("    needs: source-identity"));
        assert!(child.contains("ref: ${{ inputs.subject_sha || github.sha }}"));
        assert!(child.contains("persist-credentials: false"));
        assert!(child.contains("&& 20 ||"));
    }
    assert!(WORKFLOW.contains("permissions:\n  contents: read\n"));
    assert!(!WORKFLOW.contains("continue-on-error:") && !WORKFLOW.contains("secrets:"));
    assert!(WORKFLOW.contains("types: [opened, synchronize, reopened, labeled, unlabeled]"));
    assert!(WORKFLOW.contains("branches: [main, master]"));
    assert!(WORKFLOW.contains("  workflow_dispatch:"));
    assert!(!WORKFLOW.contains("  schedule:") && !WORKFLOW.contains("merge_group:"));
    assert!(
        WORKFLOW
            .contains("needs: [source-identity, rust-only-features, release-proof, msrv, vscode]")
    );
    // The callable Rust lane must bypass event-diff pruning after the caller selected it.
    assert!(
        job(WORKFLOW, "rust-only-features")
            .contains("CALLED: ${{ contains(toJSON(inputs), '\"subject_sha\"') }}")
    );
    for step in job(WORKFLOW, "release-proof").split("\n      - ").skip(3) {
        if step.contains("release surface")
            || step.contains("release surface)")
            || step.contains("release-readiness receipts")
        {
            assert!(step.contains("contains(toJSON(inputs), '\"subject_sha\"') ||"));
        }
    }
    Ok(())
}
#[test]
fn literal_runtime_commands_propagate_native_failure() -> TestResult {
    let stub = "cargo() { return \"$NATIVE_STATUS\"; }; npm() { return \"$NATIVE_STATUS\"; }; xvfb-run() { shift; \"$@\"; };\n";
    let mut count = 0;
    for (name, _) in JOBS {
        for script in run_blocks(job(WORKFLOW, name)) {
            if script.starts_with("cargo ") && !script.contains('\n')
                || script.starts_with("npm ")
                || script.starts_with("xvfb-run ")
            {
                count += 1;
                assert!(run(&format!("{stub}{script}"), &[])?.status.success());
                for status in ["1", "101", "137"] {
                    assert!(
                        !run(&format!("{stub}{script}"), &[("NATIVE_STATUS", status)])?
                            .status
                            .success(),
                        "masked {script}"
                    );
                }
            }
        }
    }
    assert_eq!(count, 11, "nonempty payload denominator");
    Ok(())
}

fn binding_contract(source: &str) -> bool {
    let identity = job(source, "source-identity");
    let result = job(source, "ci-result");
    [
        "PROFILE: ${{ inputs.proof_profile }}",
        "REQUEST: ${{ inputs.request_id }}",
        "HEAD_SHA: ${{ inputs.head_sha }}",
        "BASE_SHA: ${{ inputs.base_sha }}",
        "SUBJECT: ${{ inputs.subject_sha }}",
        "TREE: ${{ inputs.subject_tree }}",
        "GENERATION: ${{ inputs.workflow_generation }}",
        "ref: ${{ inputs.subject_sha }}",
        "persist-credentials: false",
    ].iter().all(|binding| identity.contains(binding))
        && !identity.contains("continue-on-error:")
        && !identity.contains("        if:")
        && identity.contains("if: contains(toJSON(inputs), '\"subject_sha\"')")
        && source.contains("group: ${{ github.workflow }}-${{ inputs.request_id || github.event.pull_request.number || github.ref }}-${{ inputs.subject_sha }}")
        && source.contains("cancel-in-progress: ${{ !contains(toJSON(inputs), '\"subject_sha\"') && github.event_name == 'pull_request' && github.event.action == 'synchronize' }}")
        && result.contains("if: always() && contains(toJSON(inputs), '\"subject_sha\"')")
        && result.contains("needs: [source-identity, rust-only-features, release-proof, msrv, vscode]")
        && result.contains("IDENTITY_RESULT: ${{ needs.source-identity.result }}")
        && [("RUST", "rust-only-features", "run_rust_only"), ("RELEASE", "release-proof", "run_release_proof"), ("MSRV", "msrv", "run_msrv"), ("VSCODE", "vscode", "run_vscode")].iter().all(|(variable, child, flag)| {
            let binding=format!("SELECT_{variable}: ${{{{ inputs.{flag} }}}}");
            identity.contains(&binding) && result.contains(&binding)
                && result.contains(&format!("{variable}_RESULT: ${{{{ needs.{child}.result }}}}"))
                && job(source, child).contains(&format!("needs.source-identity.result == 'success' && inputs.{flag}"))
        })
}

#[test]
fn wiring_rejects_mutable_subjects_fabricated_results_and_wrong_selections() -> TestResult {
    assert!(binding_contract(WORKFLOW));
    for (old, new) in [
        ("SUBJECT: ${{ inputs.subject_sha }}", "SUBJECT: main"),
        (
            "GENERATION: ${{ inputs.workflow_generation }}",
            "GENERATION: main",
        ),
        ("ref: ${{ inputs.subject_sha }}", "ref: main"),
        ("persist-credentials: false", "persist-credentials: true"),
        (
            "RUST_RESULT: ${{ needs.rust-only-features.result }}",
            "RUST_RESULT: success",
        ),
        (
            "SELECT_RELEASE: ${{ inputs.run_release_proof }}",
            "SELECT_RELEASE: false",
        ),
        (
            "needs.source-identity.result == 'success' && inputs.run_msrv",
            "inputs.run_msrv",
        ),
        (
            "needs: [source-identity, rust-only-features, release-proof, msrv, vscode]",
            "needs: [source-identity, rust-only-features, release-proof, msrv]",
        ),
        (
            "Verify exact admitted source\n",
            "Verify exact admitted source\n        if: false\n",
        ),
    ] {
        assert!(WORKFLOW.contains(old), "control must reach source");
        assert!(
            !binding_contract(&WORKFLOW.replace(old, new)),
            "accepted {old}"
        );
    }
    Ok(())
}

#[test]
fn selected_rust_runtime_cannot_be_pruned_by_event_diff_or_missing_history() -> TestResult {
    let detector = job(WORKFLOW, "rust-only-features")
        .split_once("      - uses: dtolnay")
        .unwrap_or_default()
        .0;
    let script = body(detector);
    for event in ["pull_request", "workflow_dispatch", "push"] {
        // If CALLED were ignored, git failure would widen too: require Git never be read.
        let source = format!("git() {{ echo called-git >&2; return 93; }};\n{script}");
        let output = run(
            &source,
            &[
                ("CALLED", "true"),
                ("EVENT_NAME", event),
                ("GITHUB_OUTPUT", "/dev/stdout"),
            ],
        )?;
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("rust=true"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("called-git"));
    }
    Ok(())
}

#[test]
fn unlocked_install_and_readiness_preserve_each_native_failure() -> TestResult {
    let release = job(WORKFLOW, "release-proof");
    let unlocked = release
        .split_once("      - name: Unlocked install resolution")
        .unwrap_or_default()
        .1;
    let unlocked = body(unlocked);
    let readiness = release
        .split_once("      - name: Release readiness")
        .unwrap_or_default()
        .1;
    let readiness = body(readiness);
    let stub = r#"
        cargo() {
          local stage="$1"
          printf 'reached:cargo-%s\n' "$stage" >&2
          [[ "$FAIL_COMMAND" != "cargo-$stage" ]] || return 47
          if [[ "$stage" == pkgid ]]; then printf 'ripr#0.11.0\n'; fi
        }
        tar() { echo reached:tar >&2; [[ "$FAIL_COMMAND" != tar ]]; }
        cd() { echo reached:cd >&2; [[ "$FAIL_COMMAND" != cd ]]; }
        rm() { echo reached:rm >&2; [[ "$FAIL_COMMAND" != rm ]]; }
    "#;
    assert!(
        run(&format!("{stub}\n{unlocked}"), &[("FAIL_COMMAND", "")])?
            .status
            .success()
    );
    for command in [
        "cargo-package",
        "cargo-pkgid",
        "tar",
        "cd",
        "rm",
        "cargo-generate-lockfile",
        "cargo-check",
    ] {
        let output = run(&format!("{stub}\n{unlocked}"), &[("FAIL_COMMAND", command)])?;
        assert!(!output.status.success(), "masked {command}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("reached:{command}")),
            "did not reach {command}"
        );
    }
    for command in ["cargo-pkgid", "cargo-xtask"] {
        assert!(
            !run(
                &format!("{stub}\n{readiness}"),
                &[("FAIL_COMMAND", command)]
            )?
            .status
            .success()
        );
    }
    // Pin the known wrong predecessor's masked package and directory failures.
    let prior = ORIGINAL
        .split_once("      - name: Unlocked install resolution")
        .unwrap_or_default()
        .1;
    let prior = body(prior);
    for command in ["cargo-package", "cd"] {
        assert!(
            run(&format!("{stub}\n{prior}"), &[("FAIL_COMMAND", command)])?
                .status
                .success(),
            "negative control should expose the old false success"
        );
    }
    Ok(())
}

#[test]
fn release_steps_preserve_manual_semantics_and_ignore_labels_on_calls() -> TestResult {
    let release = job(WORKFLOW, "release-proof");
    let mut count = 0;
    for step in release.split("\n      - ") {
        if let Some(expr) = step
            .lines()
            .find_map(|line| line.strip_prefix("        if: "))
        {
            count += 1;
            for event in ["pull_request", "push", "workflow_dispatch"] {
                for label in ["", "full-ci", "release-check", "unrelated"] {
                    let expected = event == "push" || ["full-ci", "release-check"].contains(&label);
                    assert_eq!(
                        predicate(expr, false, event, "labeled", label, false, "skipped")?,
                        expected,
                        "{event}/{label}"
                    );
                    assert!(predicate(
                        expr, true, event, "labeled", label, true, "success"
                    )?);
                }
            }
        }
    }
    assert_eq!(count, 5, "four release steps and receipt upload");
    Ok(())
}

#[test]
fn staged_and_explicit_activation_event_matrix() -> TestResult {
    const LEGACY: &str = "  pull_request:\n    types: [opened, synchronize, reopened, labeled, unlabeled]\n  push:\n    branches: [main, master]\n";
    assert!(WORKFLOW.contains(LEGACY));
    let active = WORKFLOW.replace(LEGACY, "");
    let subscribed = |source: &str, event: &str| {
        source
            .split_once("on:\n")
            .unwrap_or_default()
            .1
            .split_once("\npermissions:")
            .unwrap_or_default()
            .0
            .lines()
            .any(|line| line == format!("  {event}:"))
    };
    for (_state, event) in [
        ("Draft open", "pull_request"),
        ("open", "pull_request"),
        ("push", "push"),
        ("Ready", "pull_request"),
        ("unrelated label", "pull_request"),
    ] {
        assert!(
            !subscribed(&active, event),
            "activation must remove automatic allocations"
        );
    }
    for source in [WORKFLOW, active.as_str()] {
        assert!(subscribed(source, "workflow_call"));
        assert!(subscribed(source, "workflow_dispatch"));
        assert!(!subscribed(source, "schedule"));
        assert!(!subscribed(source, "merge_group"));
        assert!(!subscribed(source, "pull_request_target"));
    }
    assert!(
        !WORKFLOW.contains("ready_for_review"),
        "Ready was not an original trigger"
    );
    Ok(())
}
