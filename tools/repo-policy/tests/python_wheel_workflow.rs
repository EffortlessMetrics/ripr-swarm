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
    let source = job(WORKFLOW, "applicability")?;
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
        for path in [
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
        ] {
            copy(&source.join(path), &fixture.0.join(path))?;
        }
        fixture.git(&["init", "--object-format=sha1", "-b", "main"])?;
        fixture.git(&["config", "user.name", "Wheel Fixture"])?;
        fixture.git(&["config", "user.email", "fixture@example.invalid"])?;
        fixture.git(&["config", "commit.gpgsign", "false"])?;
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
        Ok(command)
    }
    fn git(&self, args: &[&str]) -> TestResult<String> {
        let output = self.command("git")?.args(args).output()?;
        assert!(output.status.success(), "git {args:?}: {output:?}");
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    fn commit(&self) -> TestResult<String> {
        self.git(&["add", "--all"])?;
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
        self.commit()
    }
    fn select(&self, event: &str, base: &str, head: &str) -> TestResult<(Output, String)> {
        let output_path = self.0.join(".git/actions-output");
        fs::write(&output_path, "")?;
        let output = self
            .command("bash")?
            .args([
                "-e",
                "-o",
                "pipefail",
                "-c",
                &shell("Select wheel applicability from immutable event history")?,
            ])
            .env("GITHUB_EVENT_NAME", event)
            .env("EVENT_BASE_SHA", base)
            .env("CANDIDATE_SHA", head)
            .env("GITHUB_OUTPUT", &output_path)
            .output()?;
        Ok((output, fs::read_to_string(output_path)?))
    }
    fn expect(&self, event: &str, base: &str, head: &str, qualify: bool) -> TestResult {
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
    fixture.expect("pull_request", &base, &head, false)
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
        fixture.git(&["reset", "--hard", &base])?;
        let head = fixture.change(LEDGER, &changed)?;
        fixture.expect("pull_request", &base, &head, true)?;
        // Bad/absent record at merge base remains conservative after repair/addition.
        let repaired = fixture.change(LEDGER, &initial)?;
        fixture.expect("pull_request", &head, &repaired, true)?;
    }
    fixture.git(&["reset", "--hard", &base])?;
    let mut bytes = initial.into_bytes();
    bytes.extend([b'\n', 0xff]);
    fs::write(fixture.0.join(LEDGER), bytes)?;
    let head = fixture.commit()?;
    fixture.expect("pull_request", &base, &head, true)
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
fn main_drift_and_independent_matching_edits_use_the_merge_base() -> TestResult {
    let fixture = Fixture::new()?;
    let original = fixture.ledger()?;
    let fork = fixture.git(&["rev-parse", "HEAD"])?;
    let main_ledger = original.replace("|194|", "|195|");
    let main = fixture.change(LEDGER, &main_ledger)?;
    fixture.git(&["checkout", "-b", "candidate", &fork])?;
    let candidate = fixture.change(
        LEDGER,
        &format!("{original}\n# unrelated candidate record\n"),
    )?;
    fixture.expect("pull_request", &main, &candidate, false)?;
    fixture.git(&["reset", "--hard", &fork])?;
    let matching = fixture.change(LEDGER, &main_ledger)?;
    assert!(
        fixture
            .git(&["diff", "--name-only", &main, &matching])?
            .is_empty()
    );
    fixture.expect("pull_request", &main, &matching, true)?;
    fixture.git(&["reset", "--hard", &fork])?;
    fixture.expect("pull_request", &main, &fork, true)
}

#[test]
fn missing_history_partial_diff_and_manual_dispatch_are_conservative() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    let head = fixture.change(LEDGER, &format!("{}\n# unrelated\n", fixture.ledger()?))?;
    fixture.expect("pull_request", &"0".repeat(40), &head, true)?;
    fixture.expect("pull_request", "bad-base", &head, true)?;
    fixture.expect("pull_request", &base, &base, true)?; // checkout/head mismatch
    fixture.expect("workflow_dispatch", "", &head, true)?;
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
    assert!(consume.contains("    needs: build-wheel"));
    assert!(
        !consume.contains("    if:"),
        "default success gate keeps both consumers downstream of build"
    );
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
        job(WORKFLOW, "applicability")?.contains("        if: ${{ always() && !cancelled() }}")
    );
    Ok(())
}
