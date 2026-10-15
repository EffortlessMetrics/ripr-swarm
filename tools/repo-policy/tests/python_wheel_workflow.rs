//! Real workflow shell and immutable Git histories; no wheel build or consumers.
#![cfg(target_os = "linux")]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Output;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const WORKFLOW: &str = include_str!("../../../.github/workflows/python-wheel-qualification.yml");
const LEDGER: &str = "policy/workflow_allowlist.txt";
const WHEEL: &str = ".github/workflows/python-wheel-qualification.yml";

fn job(source: &str, name: &str) -> TestResult<String> {
    let marker = format!("\n  {name}:\n");
    let (_, tail) = source.split_once(&marker).ok_or("production job exists")?;
    let lines = tail.lines().take_while(|line| {
        line.is_empty()
            || !line.starts_with("  ")
            || line.starts_with("   ")
            || line.starts_with("  #")
    });
    Ok(lines.collect::<Vec<_>>().join("\n"))
}

fn policy_step_reachable_before_selection(source: &str) -> TestResult<bool> {
    let applicability = job(source, "applicability")?;
    let policy_name = "      - name: Validate workflow policy independently\n";
    let selector_name = "      - name: Select wheel applicability from immutable event history\n";
    let policy_offset = applicability
        .find(policy_name)
        .ok_or("policy step exists")?;
    let selector_offset = applicability
        .find(selector_name)
        .ok_or("selector step exists")?;
    let (_, tail) = applicability
        .split_once(policy_name)
        .ok_or("policy step body")?;
    let policy = tail.split("      - ").next().ok_or("policy metadata")?;
    Ok(policy_offset < selector_offset
        && policy.contains("run: cargo policy check-workflows")
        && !policy.lines().any(|line| {
            let line = line.trim();
            line.starts_with("if:") || line.starts_with("continue-on-error:")
        }))
}

#[test]
fn workflow_graph_retains_policy_and_dispatch_independence() -> TestResult {
    assert!(policy_step_reachable_before_selection(WORKFLOW)?);
    let conditional_policy = WORKFLOW.replace(
        "      - name: Validate workflow policy independently\n",
        "      - name: Validate workflow policy independently\n        if: ${{ false }}\n",
    );
    assert!(!policy_step_reachable_before_selection(
        &conditional_policy
    )?);
    let ignored_failure = WORKFLOW.replace(
        "      - name: Validate workflow policy independently\n",
        "      - name: Validate workflow policy independently\n        continue-on-error: true\n",
    );
    assert!(!policy_step_reachable_before_selection(&ignored_failure)?);
    let reversed = WORKFLOW
        .replace("Validate workflow policy independently", "TEMP_STEP")
        .replace(
            "Select wheel applicability from immutable event history",
            "Validate workflow policy independently",
        )
        .replace(
            "TEMP_STEP",
            "Select wheel applicability from immutable event history",
        );
    assert!(!policy_step_reachable_before_selection(&reversed)?);
    let applicability = job(WORKFLOW, "applicability")?;
    assert!(applicability.contains("cargo policy check-workflows"));
    assert!(applicability.contains("cargo policy python-wheel-applicability"));
    assert!(!applicability.contains("needs:"));
    let pair = job(WORKFLOW, "mcp-evidence-pair")?;
    assert!(!pair.contains("needs:"));
    let build = job(WORKFLOW, "build-wheel")?;
    assert!(build.contains("needs: applicability"));
    assert!(build.contains("always()"));
    assert!(build.contains("needs.applicability.outputs.qualify != 'false'"));
    Ok(())
}

fn shell(name: &str) -> TestResult<String> {
    shell_from(WORKFLOW, name)
}

fn shell_from(workflow: &str, name: &str) -> TestResult<String> {
    shell_from_job(workflow, "applicability", name)
}

fn shell_from_job(workflow: &str, job_name: &str, name: &str) -> TestResult<String> {
    let source = job(workflow, job_name)?;
    let (_, tail) = source
        .split_once(&format!("      - name: {name}\n"))
        .ok_or("step exists")?;
    let step = tail.split("      - name:").next().ok_or("step body")?;
    if let Some((_, tail)) = step.split_once("        run: |\n") {
        return Ok(tail
            .lines()
            .take_while(|line| line.is_empty() || line.starts_with("          "))
            .map(|line| line.strip_prefix("          ").unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    Ok(step
        .lines()
        .find_map(|line| line.strip_prefix("        run: "))
        .ok_or("run step")?
        .to_owned())
}

fn copy(from: &Path, to: &Path) -> TestResult {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        fs::create_dir_all(to.parent().ok_or("parent")?)?;
        fs::copy(from, to)?;
    }
    Ok(())
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> TestResult<Self> {
        let root = std::env::temp_dir().join(format!(
            "wheel-routing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(std::env::temp_dir())?;
        fs::create_dir(&root)?;
        let fixture = Self(root);
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let inputs = [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "tools/repo-policy",
            ".github",
            ".agents",
            ".claude",
            "AGENTS.md",
            "AGENTS.override.md",
            "CLAUDE.md",
            "docs/ARCHITECTURE.md",
            "policy",
            "fixtures/boundary_gap/expected",
        ];
        for path in inputs {
            copy(&source.join(path), &fixture.0.join(path))?;
        }
        fixture.git(&["init", "--object-format=sha1", "-b", "main"])?;
        fixture.git(&["config", "user.name", "Wheel Fixture"])?;
        fixture.git(&["config", "user.email", "fixture@example.invalid"])?;
        fixture.git(&["config", "commit.gpgsign", "false"])?;
        let mut add = vec!["add", "--"];
        add.extend(inputs);
        fixture.git(&add)?;
        fixture.commit()?;
        fixture.write_shim("cargo", "exec \"$POLICY_BINARY\" \"${@:2}\"\n")?;
        Ok(fixture)
    }
    fn command(&self, program: &str) -> TestResult<Command> {
        let mut command = Command::new("timeout");
        let path = std::env::join_paths(std::iter::once(self.0.join(".git/bin")).chain(
            std::env::split_paths(&std::env::var_os("PATH").ok_or("PATH")?),
        ))?;
        command
            .args(["--kill-after=1s", "20s", program])
            .current_dir(&self.0)
            .env_clear()
            .env("PATH", path)
            .env("HOME", &self.0)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env(
                "RUSTUP_HOME",
                std::env::var_os("RUSTUP_HOME").ok_or("RUSTUP_HOME")?,
            )
            .env("POLICY_BINARY", env!("CARGO_BIN_EXE_repo-policy"));
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        Ok(command)
    }
    fn git(&self, args: &[&str]) -> TestResult<String> {
        let output = self.command("git")?.args(args).output()?;
        assert!(output.status.success(), "git {args:?}: {output:?}");
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn commit(&self) -> TestResult<String> {
        self.git(&["add", "--update"])?;
        self.git(&["commit", "-m", "fixture", "--allow-empty"])?;
        self.git(&["rev-parse", "HEAD"])
    }
    fn write_shim(&self, name: &str, body: &str) -> TestResult {
        let path = self.0.join(".git/bin").join(name);
        fs::create_dir_all(path.parent().ok_or("parent")?)?;
        fs::write(&path, format!("#!/bin/bash\n{body}"))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }
    fn ledger(&self) -> TestResult<String> {
        Ok(fs::read_to_string(self.0.join(LEDGER))?)
    }
    fn change(&self, path: &str, text: &str) -> TestResult<String> {
        fs::create_dir_all(self.0.join(path).parent().ok_or("parent")?)?;
        fs::write(self.0.join(path), text)?;
        self.git(&["add", "--", path])?;
        self.commit()
    }
    fn select(&self, event: &str, base: &str, head: &str) -> TestResult<(Output, String)> {
        self.select_from(WORKFLOW, event, base, head)
    }
    fn select_from(
        &self,
        workflow: &str,
        event: &str,
        base: &str,
        head: &str,
    ) -> TestResult<(Output, String)> {
        let output_path = self.0.join(".git/actions-output");
        fs::write(&output_path, "")?;
        let output = self
            .command("bash")?
            .args([
                "-e",
                "-o",
                "pipefail",
                "-c",
                &shell_from(
                    workflow,
                    "Select wheel applicability from immutable event history",
                )?,
            ])
            .env("GITHUB_EVENT_NAME", event)
            .env("EVENT_BASE_SHA", base)
            .env("CANDIDATE_SHA", head)
            .env("GITHUB_OUTPUT", &output_path)
            .output()?;
        Ok((output, fs::read_to_string(output_path)?))
    }
    fn assert_selection(&self, event: &str, base: &str, head: &str, qualify: bool) -> TestResult {
        let (output, selected) = self.select(event, base, head)?;
        assert!(output.status.success(), "{output:?}");
        assert_eq!(selected, format!("qualify={qualify}\n"), "{output:?}");
        Ok(())
    }
    fn policy(&self) -> TestResult<Output> {
        Ok(self
            .command("bash")?
            .args([
                "-e",
                "-o",
                "pipefail",
                "-c",
                &shell("Validate workflow policy independently")?,
            ])
            .output()?)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("retain {}: {error}", self.0.display());
        }
    }
}

#[test]
fn unrelated_record_skip_still_executes_real_workflow_policy() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let initial = fixture.ledger()?;
    let unrelated = initial
        .lines()
        .find(|line| line.starts_with(".github/workflows/ci.yml|"))
        .ok_or("unrelated record")?;
    let ledger = initial.replace(unrelated, &format!("{unrelated} Updated unrelated reason."));
    let head = fixture.change(LEDGER, &ledger)?;
    let policy = fixture.policy()?;
    assert!(policy.status.success(), "{policy:?}");
    assert!(String::from_utf8(policy.stdout)?.contains("check-workflows: pass"));
    fixture.assert_selection("pull_request", &base, &head, false)
}

#[test]
fn wheel_records_and_ambiguous_inputs_qualify() -> TestResult {
    let fixture = Fixture::new()?;
    let initial = fixture.ledger()?;
    let wheel = initial
        .lines()
        .find(|line| line.starts_with(&format!("{WHEEL}|")))
        .ok_or("wheel record")?;
    let mutations = [
        initial.replace(wheel, ""),
        initial.replace(
            wheel,
            &wheel.replace(WHEEL, ".github/workflows/renamed-wheel.yml"),
        ),
        initial.replace(wheel, &wheel.replace("|194|", "|195|")),
        initial.replace(wheel, &format!("{wheel} changed")),
        format!("{initial}\n{wheel}\n"),
        format!("{initial}\n{}\n", wheel.replace('/', "\\")),
        format!("{initial}\nother.yml|1|one\nother.yml|1|two\n"),
        format!("{initial}\nbad|row\n"),
        format!("{initial}\nbad|NaN|reason\n"),
        format!("{initial}\nbad|1| \n"),
    ];
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    for changed in mutations {
        assert_ne!(changed, initial, "every mutation must alter the ledger");
        fixture.git(&["reset", "--hard", &base])?;
        let head = fixture.change(LEDGER, &changed)?;
        fixture.assert_selection("pull_request", &base, &head, true)?;
        // Bad/absent record at merge base remains conservative after repair/addition.
        let repaired = fixture.change(LEDGER, &initial)?;
        fixture.assert_selection("pull_request", &head, &repaired, true)?;
    }
    fixture.git(&["reset", "--hard", &base])?;
    let mut bytes = initial.into_bytes();
    bytes.extend([b'\n', 0xff]);
    fs::write(fixture.0.join(LEDGER), bytes)?;
    let head = fixture.commit()?;
    fixture.assert_selection("pull_request", &base, &head, true)
}

#[test]
fn mixed_other_ledgers_and_selector_surfaces_qualify() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let ledger = format!("{}\n# unrelated\n", fixture.ledger()?);
    for path in [
        "policy/distribution.toml",
        "policy/dependency_allowlist.txt",
        "policy/non-rust-allowlist.toml",
        "xtask/changed.rs",
        "crates/ripr/changed.rs",
        "packaging/python/changed.toml",
        "Cargo.lock",
        WHEEL,
        "tools/repo-policy/src/python_wheel.rs",
    ] {
        fixture.git(&["reset", "--hard", &base])?;
        fixture.change(LEDGER, &ledger)?;
        // Append instead of damaging identity-bound files; identity failure is also conservative.
        let old = fs::read_to_string(fixture.0.join(path)).unwrap_or_default();
        let head = fixture.change(path, &format!("{old}\n# changed\n"))?;
        let (output, selected) = fixture.select("pull_request", &base, &head)?;
        if path == "Cargo.lock" || path == "tools/repo-policy/src/python_wheel.rs" {
            assert!(
                !output.status.success(),
                "identity must reject stale executable"
            );
            assert!(selected.is_empty());
        } else {
            assert!(output.status.success(), "{output:?}");
            assert_eq!(selected, "qualify=true\n");
        }
    }
    Ok(())
}

#[test]
fn fixture_commits_preserve_untracked_profiles_and_track_intended_inputs() -> TestResult {
    let fixture = Fixture::new()?;
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        let output = fixture
            .command("bash")?
            .args(["-c", "printf '%s' \"$LLVM_PROFILE_FILE\""])
            .output()?;
        assert!(output.status.success());
        assert_eq!(output.stdout, profile.as_encoded_bytes());
    }
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let profile = fixture.0.join("default_regression.profraw");
    fs::write(&profile, b"coverage bytes must remain available")?;
    fs::write(fixture.0.join("unrequested.txt"), "untracked control")?;
    let ledger = format!("{}\n# intentional ledger change\n", fixture.ledger()?);
    fixture.change(LEDGER, &ledger)?;
    let head = fixture.change("intentional.rs", "// explicit fixture input\n")?;
    assert_eq!(
        fixture.git(&["diff", "--name-only", &base, &head])?,
        format!("intentional.rs\n{LEDGER}")
    );
    assert_eq!(fs::read(&profile)?, b"coverage bytes must remain available");
    assert!(fixture.0.join("unrequested.txt").is_file());
    Ok(())
}

#[test]
fn main_drift_and_independent_matching_edits_use_the_merge_base() -> TestResult {
    let fixture = Fixture::new()?;
    let original = fixture.ledger()?;
    let fork = fixture.git(&["rev-parse", "HEAD"])?;
    let main_ledger = original.replace("|194|", "|195|");
    assert_ne!(main_ledger, original, "wheel budget fixture value changed");
    let main = fixture.change(LEDGER, &main_ledger)?;
    fixture.git(&["checkout", "-b", "candidate", &fork])?;
    let candidate = fixture.change(
        LEDGER,
        &format!("{original}\n# unrelated candidate record\n"),
    )?;
    fixture.assert_selection("pull_request", &main, &candidate, false)?;
    fixture.git(&["reset", "--hard", &fork])?;
    fixture.change(LEDGER, &main_ledger)?;
    fixture.git(&[
        "commit",
        "--amend",
        "-m",
        "independent candidate wheel edit",
    ])?;
    let matching = fixture.git(&["rev-parse", "HEAD"])?;
    assert_ne!(
        main, matching,
        "independent identical edits need distinct commits"
    );
    assert_eq!(
        fixture.git(&["merge-base", "--all", &main, &matching])?,
        fork
    );
    assert_eq!(
        fixture.git(&["diff", "--name-only", &fork, &matching])?,
        LEDGER
    );
    assert!(
        fixture
            .git(&["diff", "--name-only", &main, &matching])?
            .is_empty()
    );
    fixture.assert_selection("pull_request", &main, &matching, true)?;
    fixture.git(&["reset", "--hard", &fork])?;
    fixture.assert_selection("pull_request", &main, &fork, true)
}

#[test]
fn missing_history_partial_diff_and_manual_dispatch_are_conservative() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let head = fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
    fixture.assert_selection("pull_request", &"0".repeat(40), &head, true)?;
    fixture.assert_selection("pull_request", "bad-base", &head, true)?;
    let (mismatch, output) = fixture.select("pull_request", &base, &base)?;
    assert!(
        !mismatch.status.success(),
        "checkout/head mismatch stays visible"
    );
    assert!(output.is_empty());
    fixture.assert_selection("workflow_dispatch", "", &head, true)?;
    let git_path = fixture
        .command("bash")?
        .args(["-c", "command -v git"])
        .output()?;
    assert!(git_path.status.success());
    let git_path = String::from_utf8(git_path.stdout)?.trim().to_owned();
    // A resolved fixture-independent executable keeps the failure at diff, not setup.
    fixture.write_shim("git", "if test \"$1\" = diff; then printf 'policy/workflow_allowlist.txt\\0'; printf 'partial-diff-control' >&2; exit 9; fi\nexec \"$REAL_GIT\" \"$@\"\n")?;
    let output_path = fixture.0.join(".git/actions-output");
    fs::write(&output_path, "")?;
    let failed_diff = fixture
        .command("bash")?
        .args([
            "-e",
            "-o",
            "pipefail",
            "-c",
            &shell("Select wheel applicability from immutable event history")?,
        ])
        .env("GITHUB_EVENT_NAME", "pull_request")
        .env("EVENT_BASE_SHA", &base)
        .env("CANDIDATE_SHA", &head)
        .env("GITHUB_OUTPUT", &output_path)
        .env("REAL_GIT", git_path)
        .output()?;
    assert!(failed_diff.status.success(), "{failed_diff:?}");
    assert_eq!(fs::read_to_string(output_path)?, "qualify=true\n");
    assert!(
        String::from_utf8(failed_diff.stderr)?.contains("partial-diff-control"),
        "the partial diff branch must be reached"
    );
    Ok(())
}

fn condition(source: &str) -> TestResult<&str> {
    source
        .lines()
        .find_map(|line| {
            line.strip_prefix("    if: ${{ ")
                .and_then(|line| line.strip_suffix(" }}"))
        })
        .ok_or("job condition".into())
}

fn runnable(
    fixture: &Fixture,
    expression: &str,
    event: &str,
    pair: bool,
    result: &str,
    qualify: &str,
    dependency: bool,
) -> TestResult<bool> {
    runnable_with_status(
        fixture,
        expression,
        event,
        pair,
        result,
        qualify,
        (dependency, false),
    )
}

fn runnable_with_status(
    fixture: &Fixture,
    expression: &str,
    event: &str,
    pair: bool,
    result: &str,
    qualify: &str,
    status: (bool, bool),
) -> TestResult<bool> {
    let (dependency, cancelled) = status;
    // GitHub supplies implicit success() unless the actual expression has a status function.
    if dependency
        && result != "success"
        && !expression.contains("always()")
        && !expression.contains("cancelled()")
    {
        return Ok(false);
    }
    let translated = expression
        .replace("!contains(toJSON(inputs), '\"subject_sha\"')", "1 == 1")
        .replace("contains(toJSON(inputs), '\"subject_sha\"')", "1 == 0")
        .replace("always()", "1 == 1")
        .replace("!cancelled()", if cancelled { "1 == 0" } else { "1 == 1" })
        .replace("github.event_name", &format!("'{event}'"))
        .replace(
            "inputs.mcp_evidence_pair",
            if pair { "1 == 1" } else { "1 == 0" },
        )
        .replace("needs.applicability.result", &format!("'{result}'"))
        .replace(
            "needs.applicability.outputs.qualify",
            &format!("'{qualify}'"),
        );
    assert!(
        !translated.contains("needs.")
            && !translated.contains("github.")
            && !translated.contains("inputs.")
    );
    let script = format!("if [[ {translated} ]]; then exit 0; else exit 1; fi");
    let status = fixture.command("bash")?.args(["-c", &script]).status()?;
    assert!(
        matches!(status.code(), Some(0 | 1)),
        "predicate syntax must run"
    );
    Ok(status.success())
}

#[test]
fn actual_job_conditions_cover_skip_failures_and_both_dispatch_routes() -> TestResult {
    let fixture = Fixture::new()?;
    let build = job(WORKFLOW, "build-wheel")?;
    let applicability = job(WORKFLOW, "applicability")?;
    let pair_job = job(WORKFLOW, "mcp-evidence-pair")?;
    let consume = job(WORKFLOW, "consume")?;
    assert!(consume.contains("    needs: [build-wheel, source-contract]"));
    assert!(consume.contains("client: [pip, uv]"));
    assert!(consumer_runnable(
        &fixture, WORKFLOW, "failure", "success", false
    )?);
    let build_if = condition(&build)?;
    let pair_if = condition(&pair_job)?;
    for (result, qualify, expected) in [
        ("success", "false", false),
        ("success", "true", true),
        ("success", "", true),
        ("failure", "false", true),
        ("skipped", "false", true),
    ] {
        assert_eq!(
            runnable(
                &fixture,
                build_if,
                "pull_request",
                false,
                result,
                qualify,
                true
            )?,
            expected
        );
        assert!(runnable(
            &fixture,
            build_if,
            "workflow_dispatch",
            false,
            result,
            "true",
            true
        )?);
        assert!(!runnable(
            &fixture,
            build_if,
            "workflow_dispatch",
            true,
            result,
            qualify,
            true
        )?);
        assert!(runnable(
            &fixture,
            pair_if,
            "workflow_dispatch",
            true,
            result,
            qualify,
            pair_job.contains("needs:")
        )?);
    }
    assert!(runnable(
        &fixture,
        condition(&applicability)?,
        "workflow_dispatch",
        false,
        "success",
        "",
        false
    )?);
    assert!(!runnable(
        &fixture,
        condition(&applicability)?,
        "workflow_dispatch",
        true,
        "success",
        "",
        false
    )?);
    assert!(!runnable(
        &fixture,
        pair_if,
        "workflow_dispatch",
        false,
        "success",
        "",
        false
    )?);
    assert!(!runnable(
        &fixture,
        pair_if,
        "pull_request",
        true,
        "success",
        "",
        false
    )?);
    assert!(WORKFLOW.contains("      - \"tools/repo-policy/**\""));
    for path in [
        "policy/workflow_allowlist.txt",
        "policy/distribution.toml",
        "xtask/**",
        "packaging/python/**",
        WHEEL,
    ] {
        assert!(WORKFLOW.contains(&format!("      - \"{path}\"")));
    }
    // Demonstrate that each failure-sensitive assertion rejects a wrong graph.
    assert!(!runnable(
        &fixture,
        &build_if.replace("always() && !cancelled() && ", ""),
        "pull_request",
        false,
        "failure",
        "false",
        true
    )?);
    assert!(!runnable(
        &fixture,
        &build_if.replace("!= 'false'", "== 'true'"),
        "pull_request",
        false,
        "success",
        "",
        true
    )?);
    assert!(!runnable(
        &fixture,
        pair_if,
        "workflow_dispatch",
        true,
        "skipped",
        "",
        true
    )?);
    assert!(!runnable_with_status(
        &fixture,
        build_if,
        "pull_request",
        false,
        "success",
        "true",
        (true, true)
    )?);
    assert!(runnable_with_status(
        &fixture,
        &build_if.replace("!cancelled() && ", ""),
        "pull_request",
        false,
        "success",
        "true",
        (true, true)
    )?);
    // Actual policy/selector failures, rather than only injected classifier booleans.
    fs::write(fixture.0.join(LEDGER), "malformed\n")?;
    assert!(!fixture.policy()?.status.success());
    fixture.write_shim("cargo", "exit 7\n")?;
    let (failed, output) = fixture.select("pull_request", "", "")?;
    assert!(!failed.status.success());
    assert!(output.is_empty());
    assert!(
        job(WORKFLOW, "applicability")?.contains("        if: ${{ always() && !cancelled() && !contains(toJSON(inputs), '\"subject_sha\"') }}")
    );
    Ok(())
}

/// GitHub's implicit success includes failed ancestors, not only the direct needs job.
fn consumer_runnable(
    fixture: &Fixture,
    workflow: &str,
    applicability: &str,
    build: &str,
    cancelled: bool,
) -> TestResult<bool> {
    consumer_runnable_with_contract(
        fixture,
        workflow,
        applicability,
        build,
        "success",
        cancelled,
    )
}

fn consumer_runnable_with_contract(
    fixture: &Fixture,
    workflow: &str,
    applicability: &str,
    build: &str,
    contract: &str,
    cancelled: bool,
) -> TestResult<bool> {
    let consume = job(workflow, "consume")?;
    let expression = condition(&consume).unwrap_or("1 == 1");
    let status_function = expression.contains("always()") || expression.contains("cancelled()");
    if !status_function
        && (applicability != "success" || build != "success" || contract != "success")
    {
        return Ok(false);
    }
    let expression = expression
        .replace("needs.build-wheel.result", &format!("'{build}'"))
        .replace("needs.source-contract.result", &format!("'{contract}'"));
    runnable_with_status(
        fixture,
        &expression,
        "pull_request",
        false,
        build,
        "",
        (true, cancelled),
    )
}

#[test]
fn legacy_dispatcher_is_recognized_without_swallowing_modern_failures() -> TestResult {
    let fixture = Fixture::new()?;
    let legacy = include_str!("fixtures/legacy_policy_main.rs");
    let legacy_path = fixture
        .0
        .join("tools/repo-policy/tests/fixtures/legacy_policy_main.rs");
    assert_eq!(
        fixture.git(&["hash-object", legacy_path.to_str().ok_or("legacy path")?])?,
        "248dd1bded04f3c650bbd31d03e7107dfeb5cc5a",
        "fixture must be the actual incident dispatcher"
    );
    // Only the unknown-command branch is exercised; known commands remain outside this fixture's claim.
    let wrapper = fixture.0.join(".git/legacy-wrapper.rs");
    fs::write(
        &wrapper,
        format!(
            r#"
mod repo_policy {{
    pub fn verify_executable_identity() -> Result<(), String> {{ Ok(()) }}
    pub fn check_workflows_impl() -> Result<(), String> {{ Ok(()) }}
    pub fn enter_workspace_root() -> Result<(), String> {{ Ok(()) }}
    pub fn preflight() -> Result<(), String> {{ Ok(()) }}
    pub fn verify_preflight(_: &std::path::Path) -> Result<(), String> {{ Ok(()) }}
    pub mod agent_skills {{ pub fn check() -> Result<(), String> {{ Ok(()) }} }}
    pub mod executable_files {{ pub fn check() -> Result<(), String> {{ Ok(()) }} }}
}}
include!({:?});
"#,
            legacy_path
        ),
    )?;
    let binary = fixture.0.join(".git/legacy-policy");
    let compiled = fixture
        .command("rustc")?
        .args(["--edition=2024", "--crate-name", "legacy_policy"])
        .arg(&wrapper)
        .arg("-o")
        .arg(&binary)
        .output()?;
    assert!(
        compiled.status.success(),
        "legacy dispatcher compilation: {compiled:?}"
    );
    let old = fixture
        .command(binary.to_str().ok_or("legacy binary")?)?
        .arg("python-wheel-applicability")
        .output()?;
    assert_eq!(old.status.code(), Some(1));
    assert!(old.stdout.is_empty());
    assert_eq!(
        String::from_utf8(old.stderr)?,
        "xtask: usage: cargo policy check-workflows | check-agent-skills | check-executable-files | preflight | verify-preflight\n"
    );
    assert!(
        fixture.policy()?.status.success(),
        "independent policy shell is exercised separately"
    );
    let head = fixture.change("tools/repo-policy/src/main.rs", legacy)?;
    fixture.write_shim("cargo", &format!("if test \"$2\" = python-wheel-applicability; then exec {:?} \"${{@:2}}\"; fi\nexec \"$POLICY_BINARY\" \"${{@:2}}\"\n", binary))?;
    let (selected, output) = fixture.select("pull_request", &head, &head)?;
    assert!(selected.status.success(), "{selected:?}");
    assert_eq!(output, "qualify=true\n");
    assert!(String::from_utf8(selected.stderr)?.contains("legacy policy dispatcher"));
    let removed = WORKFLOW.replace(
        "= 248dd1bded04f3c650bbd31d03e7107dfeb5cc5a;",
        "= disabled-legacy-control;",
    );
    assert_ne!(removed, WORKFLOW);
    let (rejected, output) = fixture.select_from(&removed, "pull_request", &head, &head)?;
    assert_eq!(rejected.status.code(), Some(1));
    assert!(output.is_empty());
    let (mismatch, output) = fixture.select("pull_request", &head, &"0".repeat(40))?;
    assert!(
        !mismatch.status.success(),
        "legacy admission must bind candidate identity"
    );
    assert!(output.is_empty());
    let real_git = fixture
        .command("bash")?
        .args(["-c", "command -v git"])
        .output()?;
    assert!(real_git.status.success());
    let real_git = String::from_utf8(real_git.stdout)?.trim().to_owned();
    fixture.write_shim("git", &format!(
        "if test \"$1\" = rev-parse && test \"$2\" = HEAD; then printf '%s\\n' {head:?}; echo 'failed-head-probe-control' >&2; exit 9; fi\nexec {real_git:?} \"$@\"\n"))?;
    let (failed_probe, output) = fixture.select("pull_request", &head, &head)?;
    assert_eq!(
        failed_probe.status.code(),
        Some(9),
        "matching stdout from failed HEAD inspection is not admission evidence: {failed_probe:?}"
    );
    assert!(output.is_empty());
    assert!(String::from_utf8(failed_probe.stderr)?.contains("failed-head-probe-control"));
    Ok(())
}

#[test]
fn consumers_survive_failed_ancestor_but_require_successful_build() -> TestResult {
    let fixture = Fixture::new()?;
    for (app, build, cancelled, expected) in [
        ("failure", "success", false, true),
        ("skipped", "success", false, true),
        ("success", "success", false, true),
        ("failure", "failure", false, false),
        ("failure", "skipped", false, false),
        ("success", "cancelled", false, false),
        ("failure", "success", true, false),
    ] {
        assert_eq!(
            consumer_runnable(&fixture, WORKFLOW, app, build, cancelled)?,
            expected,
            "app={app}, build={build}, cancelled={cancelled}"
        );
    }
    let guard = "    if: ${{ always() && !cancelled() && needs.build-wheel.result == 'success' && needs.source-contract.result == 'success' }}\n";
    let removed = WORKFLOW.replace(guard, "");
    assert_ne!(removed, WORKFLOW);
    assert!(!consumer_runnable(
        &fixture, &removed, "failure", "success", false
    )?);
    let no_cancel = WORKFLOW.replace(
        "always() && !cancelled() && needs.build-wheel.result",
        "always() && needs.build-wheel.result",
    );
    assert!(consumer_runnable(
        &fixture, &no_cancel, "failure", "success", true
    )?);
    let ignore_build = WORKFLOW.replace("needs.build-wheel.result == 'success'", "1 == 1");
    assert!(consumer_runnable(
        &fixture,
        &ignore_build,
        "failure",
        "failure",
        false
    )?);
    Ok(())
}

fn qualification_chain(
    fixture: &Fixture,
    applicability: &str,
    qualify: &str,
    context: (&str, bool, bool),
) -> TestResult<(bool, bool)> {
    let (event, pair, cancelled) = context;
    let build = job(WORKFLOW, "build-wheel")?;
    let scheduled = runnable_with_status(
        fixture,
        condition(&build)?,
        event,
        pair,
        applicability,
        qualify,
        (true, cancelled),
    )?;
    let result = if scheduled { "success" } else { "skipped" };
    Ok((
        scheduled,
        consumer_runnable(fixture, WORKFLOW, applicability, result, cancelled)?,
    ))
}

#[test]
fn actual_selector_errors_preserve_red_applicability_and_full_qualification() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let head = fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
    assert!(fixture.policy()?.status.success());
    fixture.assert_selection("pull_request", &base, &head, false)?;
    assert_eq!(
        qualification_chain(&fixture, "success", "false", ("pull_request", false, false))?,
        (false, false)
    );
    for body in [
        "echo 'modern selector error' >&2; exit 7",
        "echo 'compiler error control' >&2; exit 101",
        "echo 'xtask: usage: cargo policy check-workflows | check-agent-skills | check-executable-files | preflight | verify-preflight' >&2; exit 1",
        "printf 'qualify=false\\n'; exit 9",
        "printf 'qualify=false'; exit 0",
        "printf 'qualify=false\\npartial\\n'; exit 0",
        "printf 'qualify=false\\n\\n'; exit 0",
        "printf 'qualify=unknown\\n'; exit 0",
        "exit 0",
    ] {
        fixture.write_shim("cargo", &format!(
            "if test \"$2\" = python-wheel-applicability; then {body}; fi\nexec \"$POLICY_BINARY\" \"${{@:2}}\"\n"))?;
        assert!(fixture.policy()?.status.success());
        let (selected, output) = fixture.select("pull_request", &base, &head)?;
        assert!(!selected.status.success(), "{body}: {selected:?}");
        assert!(
            output.is_empty(),
            "partial output cannot escape failed selection: {body}"
        );
        for event in ["pull_request", "workflow_dispatch"] {
            assert_eq!(
                qualification_chain(&fixture, "failure", "", (event, false, false))?,
                (true, true)
            );
        }
        assert_eq!(
            qualification_chain(&fixture, "failure", "", ("pull_request", false, true))?,
            (false, false)
        );
        assert_eq!(
            qualification_chain(&fixture, "failure", "", ("workflow_dispatch", true, false))?,
            (false, false)
        );
        assert!(runnable(
            &fixture,
            condition(&job(WORKFLOW, "mcp-evidence-pair")?)?,
            "workflow_dispatch",
            true,
            "failure",
            "",
            false
        )?);
    }
    Ok(())
}

#[test]
fn inspection_and_policy_failures_do_not_become_legacy_admission() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let head = fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
    let real_git = fixture
        .command("bash")?
        .args(["-c", "command -v git"])
        .output()?;
    assert!(real_git.status.success());
    let real_git = String::from_utf8(real_git.stdout)?.trim().to_owned();
    for response in [
        "printf '248dd1bded04f3c650bbd31d03e7107dfeb5cc5a'; exit 9",
        "printf '248dd1bded04f3c650bbd31d03e7107dfeb5cc5a partial'; exit 0",
        "exit 0",
    ] {
        fixture.write_shim(
            "git",
            &format!(
                "if test \"$1\" = hash-object; then {response}; fi\nexec {real_git:?} \"$@\"\n"
            ),
        )?;
        let (selected, output) = fixture.select("pull_request", &base, &head)?;
        assert!(!selected.status.success(), "{response}: {selected:?}");
        assert!(output.is_empty());
    }
    fs::remove_file(fixture.0.join(".git/bin/git"))?;
    fs::rename(
        fixture.0.join("tools/repo-policy/src/main.rs"),
        fixture.0.join(".git/saved-main.rs"),
    )?;
    let (missing, output) = fixture.select("pull_request", &base, &head)?;
    assert!(!missing.status.success());
    assert!(output.is_empty());
    fs::rename(
        fixture.0.join(".git/saved-main.rs"),
        fixture.0.join("tools/repo-policy/src/main.rs"),
    )?;
    let source = fs::read_to_string(fixture.0.join("tools/repo-policy/src/main.rs"))?;
    fs::write(
        fixture.0.join("tools/repo-policy/src/main.rs"),
        format!("{source}\n"),
    )?;
    let (identity, output) = fixture.select("pull_request", &base, &head)?;
    assert!(!identity.status.success());
    assert!(output.is_empty());
    fs::write(fixture.0.join("tools/repo-policy/src/main.rs"), source)?;
    fixture.write_shim("cargo", "if test \"$2\" = check-workflows; then echo 'policy failure control' >&2; exit 42; fi\nexec \"$POLICY_BINARY\" \"${@:2}\"\n")?;
    assert_eq!(fixture.policy()?.status.code(), Some(42));
    fixture.assert_selection("pull_request", &base, &head, false)?;
    assert_eq!(
        qualification_chain(&fixture, "failure", "false", ("pull_request", false, false))?,
        (true, true)
    );
    fixture.write_shim("cargo", "exec \"$POLICY_BINARY\" \"${@:2}\"\n")?;
    let malformed_head = fixture.change(LEDGER, "malformed\n")?;
    assert!(!fixture.policy()?.status.success());
    fixture.assert_selection("pull_request", &base, &malformed_head, true)?;
    assert_eq!(
        qualification_chain(&fixture, "failure", "true", ("pull_request", false, false))?,
        (true, true)
    );
    Ok(())
}

/// Replay the complete early production guard, with actual Git objects and payload fields.
fn exact_subject(fixture: &Fixture, inputs: &serde_json::Value) -> TestResult<(Output, String)> {
    exact_subject_from(fixture, inputs, WORKFLOW)
}

fn exact_subject_from(
    fixture: &Fixture,
    inputs: &serde_json::Value,
    workflow: &str,
) -> TestResult<(Output, String)> {
    let path = fixture.0.join(".git/exact-output");
    fs::write(&path, "")?;
    let mut command = fixture.command("bash")?;
    command
        .args([
            "-e",
            "-o",
            "pipefail",
            "-c",
            &shell_from(
                workflow,
                "Verify callable exact subject before policy or product execution",
            )?,
        ])
        .env("CALL_INPUTS", inputs.to_string())
        .env("GITHUB_REPOSITORY", "EffortlessMetrics/ripr-swarm")
        .env("GITHUB_WORKFLOW_SHA", "a".repeat(40))
        .env("GITHUB_RUN_ID", "123")
        .env("GITHUB_RUN_ATTEMPT", "2")
        .env("GITHUB_OUTPUT", &path);
    for (field, variable) in [
        ("proof_profile", "PROOF_PROFILE"),
        ("request_id", "ADMITTED_REQUEST"),
        ("head_sha", "ADMITTED_HEAD"),
        ("base_sha", "ADMITTED_BASE"),
        ("subject_sha", "ADMITTED_SUBJECT"),
        ("subject_tree", "ADMITTED_TREE"),
        ("workflow_generation", "ADMITTED_GENERATION"),
    ] {
        command.env(variable, inputs[field].as_str().unwrap_or(""));
    }
    let output = command.output()?;
    Ok((output, fs::read_to_string(path)?))
}

fn integration_inputs(fixture: &Fixture) -> TestResult<serde_json::Value> {
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let head = fixture.change("Cargo.toml", "# integration payload control\n")?;
    let tree = fixture.git(&["rev-parse", "HEAD^{tree}"])?;
    let subject = fixture.git(&[
        "commit-tree",
        &tree,
        "-p",
        &base,
        "-p",
        &head,
        "-m",
        "integration",
    ])?;
    fixture.git(&["checkout", "--detach", &subject])?;
    Ok(
        serde_json::json!({"proof_profile":"qualification", "request_id":"123-2",
        "base_sha":base, "head_sha":head, "subject_sha":subject, "subject_tree":tree,
        "workflow_generation":"a".repeat(40)}),
    )
}

#[test]
fn callable_guard_rejects_wrong_empty_and_stale_payloads_before_expensive_work() -> TestResult {
    let fixture = Fixture::new()?;
    let valid = integration_inputs(&fixture)?;
    let (result, output) = exact_subject(&fixture, &valid)?;
    assert!(result.status.success(), "{result:?}");
    assert_eq!(output, "qualify=true\n");
    for field in [
        "proof_profile",
        "request_id",
        "base_sha",
        "head_sha",
        "subject_sha",
        "subject_tree",
        "workflow_generation",
    ] {
        for bad in ["", "main", &"0".repeat(40), "$(touch injected)"] {
            let mut payload = valid.clone();
            payload[field] = bad.into();
            let (result, output) = exact_subject(&fixture, &payload)?;
            assert!(
                !result.status.success(),
                "accepted {field}={bad}: {result:?}"
            );
            assert!(output.is_empty());
        }
    }
    let empty = serde_json::json!({"subject_sha":""});
    assert!(!exact_subject(&fixture, &empty)?.0.status.success());
    let mut stale_attempt = valid.clone();
    stale_attempt["request_id"] = "123-1".into();
    assert!(!exact_subject(&fixture, &stale_attempt)?.0.status.success());
    let (legacy, output) = exact_subject(&fixture, &serde_json::json!({}))?;
    assert!(legacy.status.success());
    assert!(output.is_empty());
    assert!(
        exact_subject(&fixture, &serde_json::Value::Null)?
            .0
            .status
            .success()
    );
    let removed = WORKFLOW.replace(
        "              test \"${tree}\" = \"${ADMITTED_TREE}\"\n",
        "",
    );
    assert_ne!(removed, WORKFLOW);
    let mut wrong_tree = valid.clone();
    wrong_tree["subject_tree"] = "0".repeat(40).into();
    assert!(
        exact_subject_from(&fixture, &wrong_tree, &removed)?
            .0
            .status
            .success(),
        "tree-check removal must expose the wrong-tree false success"
    );
    let mut diagnostic = valid.clone();
    diagnostic["proof_profile"] = "diagnostic".into();
    diagnostic["subject_sha"] = valid["head_sha"].clone();
    fixture.git(&[
        "checkout",
        "--detach",
        diagnostic["head_sha"].as_str().ok_or("head")?,
    ])?;
    assert_eq!(exact_subject(&fixture, &diagnostic)?.1, "qualify=true\n");
    assert!(
        !exact_subject(&fixture, &valid)?.0.status.success(),
        "stale checkout must fail"
    );
    // The separate build checkout must not credit matching stdout from failed Git.
    let expected = diagnostic["head_sha"].as_str().ok_or("diagnostic head")?;
    fixture.write_shim("git", &format!("printf '%s\\n' {expected:?}; exit 9\n"))?;
    fixture.write_shim(
        "cargo",
        "printf '%s\\n' '{\"packages\":[{\"name\":\"ripr\",\"version\":\"0.11.0\"}]}'\n",
    )?;
    let run_binding = |workflow: &str| -> TestResult<(Output, String)> {
        let output = fixture.0.join(".git/bind-output");
        let environment = fixture.0.join(".git/bind-env");
        fs::write(&output, "")?;
        fs::write(&environment, "")?;
        let result = fixture
            .command("bash")?
            .args([
                "-e",
                "-o",
                "pipefail",
                "-c",
                &shell_from_job(
                    workflow,
                    "build-wheel",
                    "Bind exact source candidate and native version",
                )?,
            ])
            .env("CANDIDATE_SHA", expected)
            .env("GITHUB_REPOSITORY", "EffortlessMetrics/ripr-swarm")
            .env("GITHUB_OUTPUT", &output)
            .env("GITHUB_ENV", &environment)
            .output()?;
        Ok((result, fs::read_to_string(output)?))
    };
    let (failed_probe, output) = run_binding(WORKFLOW)?;
    assert_eq!(failed_probe.status.code(), Some(9));
    assert!(output.is_empty());
    let masked = WORKFLOW.replace(
        "          checkout_head=\"$(git rev-parse HEAD)\"\n          test \"${checkout_head}\" = \"${CANDIDATE_SHA,,}\"",
        "          test \"$(git rev-parse HEAD)\" = \"${CANDIDATE_SHA,,}\"",
    );
    assert_ne!(masked, WORKFLOW);
    let (false_success, output) = run_binding(&masked)?;
    assert!(false_success.status.success(), "{false_success:?}");
    assert_eq!(output, "native_version=0.11.0\n");
    Ok(())
}

#[test]
fn callable_graph_uses_exact_subject_and_cannot_turn_guard_failure_into_skip() -> TestResult {
    let fixture = Fixture::new()?;
    let build = job(WORKFLOW, "build-wheel")?;
    let expression = condition(&build)?
        .replace("!contains(toJSON(inputs), '\"subject_sha\"')", "1 == 0")
        .replace("contains(toJSON(inputs), '\"subject_sha\"')", "1 == 1");
    for event in ["pull_request", "workflow_dispatch", "push", "schedule"] {
        for result in ["failure", "skipped", "cancelled"] {
            assert!(!runnable(
                &fixture,
                &expression,
                event,
                false,
                result,
                "",
                true
            )?);
        }
        assert!(runnable(
            &fixture,
            &expression,
            event,
            false,
            "success",
            "true",
            true
        )?);
    }
    let app = job(WORKFLOW, "applicability")?;
    assert!(
        app.find("Verify callable exact subject").ok_or("guard")?
            < app.find("dtolnay/rust-toolchain").ok_or("toolchain")?
    );
    assert!(app.contains("!contains(toJSON(inputs), '\"subject_sha\"')"));
    assert!(WORKFLOW.contains("CANDIDATE_SHA: ${{ inputs.subject_sha || github.event.pull_request.head.sha || github.sha }}"));
    assert!(app.contains("fetch-depth: 0"));
    assert!(WORKFLOW.contains("  workflow_call:\n"));
    assert!(WORKFLOW.contains("  contents: read\n"));
    assert!(!WORKFLOW.contains("secrets: inherit"));
    for (job_name, minutes) in [("applicability", 3), ("build-wheel", 12), ("consume", 5)] {
        assert!(job(WORKFLOW, job_name)?.contains(&format!("&& {minutes} ||")));
    }
    let removed = expression.replace(
        "(1 == 0 || needs.applicability.result == 'success') && ",
        "",
    );
    assert_ne!(removed, expression);
    assert!(
        runnable(
            &fixture,
            &removed,
            "pull_request",
            false,
            "failure",
            "",
            true
        )?,
        "removed guard must demonstrate expensive fallback"
    );
    Ok(())
}

#[test]
fn package_contract_runs_in_parallel_but_remains_required_for_consumers() -> TestResult {
    let fixture = Fixture::new()?;
    let contract = job(WORKFLOW, "source-contract")?;
    let build = job(WORKFLOW, "build-wheel")?;
    let consume = job(WORKFLOW, "consume")?;
    // Same selector/guard authority, independent producers: neither awaits the other.
    assert!(contract.contains("    needs: applicability"));
    assert!(build.contains("    needs: applicability"));
    assert_eq!(condition(&contract)?, condition(&build)?);
    assert!(!build.contains("needs.source-contract"));
    assert!(!contract.contains("needs.build-wheel"));
    assert!(consume.contains("needs: [build-wheel, source-contract]"));
    assert!(contract.contains("&& 8 || 20"));
    assert!(build.contains("&& 12 || 20"));
    let shell = shell_from_job(
        WORKFLOW,
        "source-contract",
        "Validate package source contract",
    )?;
    assert_eq!(
        shell.trim(),
        "set -euo pipefail\ncargo xtask check-release-targets\ncargo test -p xtask --locked policy::distribution::tests -- --nocapture"
    );
    assert!(!build.contains("cargo xtask check-release-targets"));
    assert!(!build.contains("policy::distribution::tests"));
    assert!(contract.contains("ref: ${{ env.CANDIDATE_SHA }}"));
    assert!(contract.contains("persist-credentials: false"));
    assert!(!contract.contains("continue-on-error:"));
    for app in ["success", "failure", "skipped"] {
        for status in ["success", "failure", "cancelled", "skipped"] {
            assert_eq!(
                consumer_runnable_with_contract(&fixture, WORKFLOW, app, "success", status, false)?,
                status == "success",
                "package contract {status} after applicability {app}"
            );
        }
    }
    let removed = WORKFLOW.replace(" && needs.source-contract.result == 'success'", "");
    assert_ne!(removed, WORKFLOW);
    assert!(
        consumer_runnable_with_contract(
            &fixture, &removed, "success", "success", "failure", false
        )?,
        "removing the contract-result guard must expose false consumer admission"
    );
    // Replay the retained production shell. A failed first command cannot be masked
    // by a successful test command, and both commands must run in the positive case.
    let calls = fixture.0.join("package-calls");
    fixture.write_shim(
        "cargo",
        &format!(
            r#"printf '%s\n' "$*" >> '{}'
if test "$*" = 'xtask check-release-targets'; then exit 7; fi
"#,
            calls.display()
        ),
    )?;
    let failed = fixture.command("bash")?.args(["-c", &shell]).output()?;
    assert_eq!(failed.status.code(), Some(7));
    assert_eq!(fs::read_to_string(&calls)?, "xtask check-release-targets\n");
    fs::remove_file(&calls)?;
    fixture.write_shim(
        "cargo",
        &format!(
            r#"printf '%s\n' "$*" >> '{}'
"#,
            calls.display()
        ),
    )?;
    assert!(
        fixture
            .command("bash")?
            .args(["-c", &shell])
            .status()?
            .success()
    );
    assert_eq!(
        fs::read_to_string(&calls)?,
        "xtask check-release-targets\ntest -p xtask --locked policy::distribution::tests -- --nocapture\n"
    );
    Ok(())
}

#[test]
fn parallel_contract_binds_its_checkout_and_propagates_failed_git() -> TestResult {
    let fixture = Fixture::new()?;
    let head = fixture.git(&["rev-parse", "HEAD"])?;
    let shell = shell_from_job(
        WORKFLOW,
        "source-contract",
        "Bind exact package source candidate",
    )?;
    let run = |candidate: &str| -> TestResult<Output> {
        Ok(fixture
            .command("bash")?
            .args(["-c", &shell])
            .env("GITHUB_REPOSITORY", "EffortlessMetrics/ripr-swarm")
            .env("CANDIDATE_SHA", candidate)
            .output()?)
    };
    assert!(run(head.trim())?.status.success());
    assert!(!run(&"0".repeat(40))?.status.success());
    fixture.write_shim(
        "git",
        &format!("printf '%s\\n' {:?}; exit 9\n", head.trim()),
    )?;
    assert_eq!(run(head.trim())?.status.code(), Some(9));
    Ok(())
}
