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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkflowShell {
    ImplicitBash,
    Bash,
}

// Linux Actions shell templates, including the implicit/explicit Bash distinction:
// https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepsshell
fn run(script: &str, overrides: &[(&str, &str)]) -> TestResult<Output> {
    run_with_shell(WorkflowShell::ImplicitBash, script, overrides)
}

fn run_with_shell(
    shell: WorkflowShell,
    script: &str,
    overrides: &[(&str, &str)],
) -> TestResult<Output> {
    let mut cmd = Command::new("timeout");
    cmd.args(["--kill-after=1s", "5s", "bash"]);
    match shell {
        WorkflowShell::ImplicitBash => {
            cmd.arg("-e");
        }
        WorkflowShell::Bash => {
            cmd.args(["--noprofile", "--norc", "-eo", "pipefail"]);
        }
    }
    cmd.args(["-c", script])
        .env_remove("BASH_ENV")
        .env_remove("ENV")
        .env_remove("SHELLOPTS")
        .env_remove("BASHOPTS");
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
fn mapping<'a>(scope: &'a str, indent: usize, key: &str) -> Option<&'a str> {
    let marker = format!("{}{key}:\n", " ".repeat(indent));
    let offset = scope
        .match_indices(&marker)
        .find(|(offset, _)| *offset == 0 || scope.as_bytes()[offset - 1] == b'\n')?
        .0
        + marker.len();
    let tail = &scope[offset..];
    let end = tail
        .lines()
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            Some((start, line))
        })
        .find(|(_, line)| {
            !line.trim().is_empty()
                && !line.trim_start().starts_with('#')
                && line.len() - line.trim_start().len() <= indent
        })
        .map_or(tail.len(), |(offset, _)| offset);
    Some(&tail[..end])
}

fn default_shell(scope: &str, indent: usize) -> Option<&str> {
    let defaults = mapping(scope, indent, "defaults")?;
    let run = mapping(defaults, indent + 2, "run")?;
    let prefix = format!("{}shell: ", " ".repeat(indent + 4));
    run.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
}

fn effective_shell(source: &str, name: &str, step: &str) -> TestResult<WorkflowShell> {
    let declared = step
        .lines()
        .find_map(|line| line.strip_prefix("        shell: "))
        .map(str::trim)
        .or_else(|| default_shell(job(source, name), 4))
        .or_else(|| default_shell(source, 0));
    match declared {
        None => Ok(WorkflowShell::ImplicitBash),
        Some("bash") => Ok(WorkflowShell::Bash),
        Some(other) => Err(format!("Unmodelled workflow shell: {other}").into()),
    }
}

fn named_step<'a>(source: &'a str, name: &str, step: &str) -> &'a str {
    let prefix = format!("      - name: {step}");
    let tail = job(source, name).split_once(&prefix).unwrap_or_default().1;
    assert!(!tail.is_empty(), "missing step: {step}");
    tail.split_once("\n      - ").map_or(tail, |(step, _)| step)
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
        .replace("contains(github.event.pull_request.labels.*.name, 'merge-ready')",flag(label=="merge-ready"))
        .replace("github.event.action == 'labeled'",flag(action=="labeled"))
        .replace("github.event.label.name == 'merge-ready'",flag(label=="merge-ready"))
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
        // Callable selection is frozen; direct legacy events follow main's merge-ready gate.
        for (event, action, label) in [
            ("pull_request", "opened", ""),
            ("pull_request", "synchronize", ""),
            ("pull_request", "ready_for_review", ""),
            ("pull_request", "labeled", "unrelated"),
            ("pull_request", "labeled", "full-ci"),
            ("pull_request", "labeled", "release-check"),
            ("pull_request", "labeled", "merge-ready"),
            ("push", "", ""),
            ("workflow_dispatch", "", ""),
        ] {
            // Current main gates direct pull_request runs on the explicit
            // merge-ready labeled event; callable runs use frozen selection.
            let merge_ready = action == "labeled" && label == "merge-ready";
            let expected = match *name {
                "release-proof" => {
                    (event != "pull_request" || merge_ready)
                        && (event != "pull_request"
                            || ["full-ci", "release-check"].contains(&label))
                }
                "rust-only-features" => event != "pull_request" || merge_ready,
                "msrv" => {
                    (event != "pull_request" || merge_ready)
                        && (event == "workflow_dispatch"
                            || (event == "pull_request" && label == "full-ci"))
                }
                "vscode" => {
                    (event != "pull_request" || merge_ready)
                        && (event != "pull_request" || label == "full-ci")
                }
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
            run_blocks(job(WORKFLOW, name)).iter().map(|script| script.replace("  [ \"${CALLED:-false}\" != true ] || return 0\n", "").replace("cargo package -p ripr --no-verify\npackage_id=\"$(cargo pkgid -p ripr)\"\npackage_version=\"${package_id##*#}\"\ntest -n \"$package_version\"\n", "cargo package -p ripr --no-verify\npackage_version=\"$(cargo pkgid -p ripr | sed 's/.*#//')\"\n")).collect::<Vec<_>>(),
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
    assert!(WORKFLOW.contains("types: [opened, synchronize, reopened, labeled]"));
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
        && source.contains("group: ${{ github.workflow }}-${{ inputs.request_id || github.event.pull_request.number || github.ref }}-${{ inputs.subject_sha || ((github.event_name == 'pull_request' && !(github.event.action == 'labeled' && github.event.label.name == 'merge-ready')) && github.run_id || 'run') }}")
        && source.contains("cancel-in-progress: ${{ !contains(toJSON(inputs), '\"subject_sha\"') && (github.event_name == 'pull_request' && github.event.action == 'labeled' && github.event.label.name == 'merge-ready') }}")
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
    let stub = r#"
        cargo() {
          local stage="$1"
          printf 'reached:cargo-%s\n' "$stage" >&2
          if [[ "$stage" == pkgid && "$PKGID_STDOUT" != empty ]]; then printf 'ripr#0.11.0\n'; fi
          [[ "$FAIL_COMMAND" != "cargo-$stage" ]] || return 47
        }
        tar() { echo reached:tar >&2; [[ "$FAIL_COMMAND" != tar ]]; }
        cd() { echo reached:cd >&2; [[ "$FAIL_COMMAND" != cd ]]; }
        rm() { echo reached:rm >&2; [[ "$FAIL_COMMAND" != rm ]]; }
    "#;
    for (name, commands) in [
        (
            "Unlocked install resolution",
            &[
                "cargo-package",
                "cargo-pkgid",
                "tar",
                "cd",
                "rm",
                "cargo-generate-lockfile",
                "cargo-check",
            ][..],
        ),
        ("Release readiness", &["cargo-pkgid", "cargo-xtask"][..]),
    ] {
        let step = named_step(WORKFLOW, "release-proof", name);
        let shell = effective_shell(WORKFLOW, "release-proof", step)?;
        let script = format!("{stub}\n{}", body(step));
        assert!(
            run_with_shell(shell, &script, &[("FAIL_COMMAND", "")])?
                .status
                .success(),
            "positive setup: {name}"
        );
        for command in commands {
            let output = run_with_shell(shell, &script, &[("FAIL_COMMAND", command)])?;
            assert!(!output.status.success(), "masked {name}/{command}");
            let trace = String::from_utf8(output.stderr)?;
            assert!(
                trace.contains(&format!("reached:{command}")),
                "did not reach {name}/{command}"
            );
        }
        for stdout in ["valid", "empty"] {
            let output = run_with_shell(
                shell,
                &script,
                &[("FAIL_COMMAND", "cargo-pkgid"), ("PKGID_STDOUT", stdout)],
            )?;
            assert_eq!(
                output.status.code(),
                Some(47),
                "pkgid native status: {name}/{stdout}"
            );
            let trace = String::from_utf8(output.stderr)?;
            assert!(trace.contains("reached:cargo-pkgid"));
            for downstream in [
                "reached:tar",
                "reached:cd",
                "reached:rm",
                "reached:cargo-generate-lockfile",
                "reached:cargo-check",
                "reached:cargo-xtask",
            ] {
                assert!(
                    !trace.contains(downstream),
                    "ran {downstream} after pkgid failed: {name}"
                );
            }
        }
    }
    // Original payload's package/cd masking remains a separate known-wrong control.
    let prior = named_step(ORIGINAL, "release-proof", "Unlocked install resolution");
    for command in ["cargo-package", "cd"] {
        assert!(
            run_with_shell(
                effective_shell(ORIGINAL, "release-proof", prior)?,
                &format!("{stub}\n{}", body(prior)),
                &[("FAIL_COMMAND", command)]
            )?
            .status
            .success()
        );
    }
    Ok(())
}

#[test]
fn effective_shell_precedence_matches_the_actual_workflow_declarations() -> TestResult {
    assert!(
        run_with_shell(WorkflowShell::ImplicitBash, "false | true", &[])?
            .status
            .success()
    );
    assert!(
        !run_with_shell(WorkflowShell::Bash, "false | true", &[])?
            .status
            .success()
    );
    let scope = "jobs:\n  release-proof:\n    steps:\n";
    assert_eq!(
        effective_shell(scope, "release-proof", "")?,
        WorkflowShell::ImplicitBash
    );
    let workflow_default = format!("defaults:\n  run:\n    shell: bash\n{scope}");
    assert_eq!(
        effective_shell(&workflow_default, "release-proof", "")?,
        WorkflowShell::Bash
    );
    let job_default =
        "jobs:\n  release-proof:\n    defaults:\n      run:\n        shell: bash\n    steps:\n";
    assert_eq!(
        effective_shell(job_default, "release-proof", "")?,
        WorkflowShell::Bash
    );
    let conflicting_job = format!("defaults:\n  run:\n    shell: unsupported\n{job_default}");
    assert_eq!(
        effective_shell(&conflicting_job, "release-proof", "")?,
        WorkflowShell::Bash
    );
    let unsupported_job = job_default.replace("shell: bash", "shell: unsupported");
    let error = effective_shell(&unsupported_job, "release-proof", "")
        .err()
        .ok_or("unsupported shell must refuse")?;
    assert!(error.to_string().contains("Unmodelled workflow shell"));
    assert_eq!(
        effective_shell(&unsupported_job, "release-proof", "        shell: bash\n")?,
        WorkflowShell::Bash
    );
    Ok(())
}

#[test]
fn removing_the_declared_shell_reproduces_masked_pkgid_failures() -> TestResult {
    let implicit = WORKFLOW.replace("        shell: bash\n", "");
    let stub = r#"
        cargo() {
          echo "reached:$1" >&2
          if [[ "$1" == pkgid ]]; then printf 'ripr#0.11.0\n'; return 47; fi
        }
        tar() { return 0; }
        cd() { return 0; }
        rm() { return 0; }
    "#;
    // Main's unlocked-install form assigns `cargo pkgid` directly, so even
    // the implicit shell fails closed there; the readiness pipeline through
    // `sed` still masks a pkgid failure without pipefail.
    for (name, downstream, masked) in [
        ("Unlocked install resolution", "reached:check", false),
        ("Release readiness", "reached:xtask", true),
    ] {
        let step = named_step(&implicit, "release-proof", name);
        let shell = effective_shell(&implicit, "release-proof", step)?;
        assert_eq!(shell, WorkflowShell::ImplicitBash);
        let output = run_with_shell(shell, &format!("{stub}\n{}", body(step)), &[])?;
        assert_eq!(
            output.status.success(),
            masked,
            "missing-shell control for {name}"
        );
        assert_eq!(
            String::from_utf8(output.stderr)?.contains(downstream),
            masked,
            "downstream for {name}"
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
    const LEGACY: &str = "  pull_request:\n    types: [opened, synchronize, reopened, labeled]\n  push:\n    branches: [main, master]\n";
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
