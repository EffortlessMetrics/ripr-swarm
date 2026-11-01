//! Execute the production Actions shell against real Git DAGs, not a copied classifier.
//! The workflow runs on Ubuntu; these subprocess fixtures require Bash and Git on Unix.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const WORKFLOW: &str = include_str!("../../../.github/workflows/routed-rust.yml");
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn job(name: &str) -> TestResult<&str> {
    let marker = format!("\n  {name}:\n");
    let (_, tail) = WORKFLOW
        .split_once(&marker)
        .ok_or("production job exists")?;
    let end = tail
        .match_indices('\n')
        .find_map(|(offset, _)| {
            let next = &tail[offset + 1..];
            (next.starts_with("  ") && !next.starts_with("   ") && !next.starts_with("  #"))
                .then_some(offset)
        })
        .unwrap_or(tail.len());
    Ok(&tail[..end])
}

fn result_step() -> TestResult<&'static str> {
    let (_, step) = job("result")?
        .split_once("      - name: Summarize routed result\n")
        .ok_or("production summary step exists")?;
    Ok(step)
}

fn shell(name: &str) -> TestResult<String> {
    let source = if name == "result" {
        result_step()?
    } else {
        job(name)?
    };
    assert_eq!(source.matches("        run: |\n").count(), 1);
    let (_, body) = source.split_once("        run: |\n").ok_or("shell block")?;
    let script = body
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !script.trim().is_empty(),
        "production script must be nonempty"
    );
    Ok(script)
}

fn vscode_step_shell() -> TestResult<String> {
    let (_, tail) = job("result")?
        .split_once("      - name: Read the extension e2e check for the admitted head\n")
        .ok_or("production extension e2e step exists")?;
    let (step, _) = tail
        .split_once("      - name: Summarize routed result\n")
        .ok_or("summary step follows the extension e2e step")?;
    let (_, body) = step.split_once("        run: |\n").ok_or("shell block")?;
    Ok(body
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn capability_shell() -> TestResult<String> {
    let source = job("docs-gate")?;
    let marker = "      - name: Capability registry checks (SUPPORT_TIERS.md, preview guide or spec change)\n";
    assert_eq!(source.matches(marker).count(), 1);
    let (_, tail) = source.split_once(marker).ok_or("capability step")?;
    let step = tail.split("\n      - ").next().ok_or("step body")?;
    for binding in [
        "          BASE_SHA: ${{ github.event.pull_request.base.sha || github.event.merge_group.base_sha }}",
        "          HEAD_SHA: ${{ github.event.pull_request.head.sha || github.event.merge_group.head_sha }}",
    ] {
        assert_eq!(step.lines().filter(|line| *line == binding).count(), 1);
    }
    let (_, body) = step.split_once("        run: |\n").ok_or("step shell")?;
    let script = body
        .lines()
        .map(|line| line.strip_prefix("          ").ok_or("shell indentation"))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    assert!(!script.trim().is_empty());
    Ok(script)
}

const CAPABILITY_CALLS: &str = "xtask capability-docs --check\nxtask check-capability-producers\n";

fn capability_calls(
    fixture: &Fixture,
    script: &str,
    base: &str,
    head: &str,
    cargo_exit: &str,
) -> TestResult<(Output, String)> {
    let calls = fixture.root.join(".git/capability-calls");
    fs::write(&calls, "")?;
    let temporary = fixture.root.join(".git/selector-temp");
    fs::create_dir_all(&temporary)?;
    // Retain the production selector and command order, but record Cargo argv:
    // this is applicability evidence, not execution of either Rust gate.
    let script = format!(
        "cargo() {{ printf '%s\\n' \"$*\" >> .git/capability-calls; return \"$CARGO_EXIT\"; }}\n{script}"
    );
    let output = fixture.run(
        &script,
        &[
            ("BASE_SHA", base),
            ("HEAD_SHA", head),
            ("CARGO_EXIT", cargo_exit),
            ("TMPDIR", ".git/selector-temp"),
        ],
        None,
    )?;
    let calls = fs::read_to_string(calls)?;
    assert_eq!(fs::read_dir(temporary)?.count(), 0, "selector temp cleanup");
    Ok((output, calls))
}

#[test]
fn capability_selector_runs_both_checks_for_docs_only_add_edit_delete() -> TestResult {
    let script = capability_shell()?;
    let detector = shell("detect-docs-only")?;
    for path in [
        "docs/status/SUPPORT_TIERS.md",
        "docs/LANGUAGE_ADAPTER_PREVIEW.md",
        "docs/specs/RIPR-SPEC-9999-selector.md",
        "docs/specs/nested/café with spaces.md",
    ] {
        let fixture = Fixture::new()?;
        let mut base = fixture.git(&["rev-parse", "HEAD"])?;
        for content in [Some("added\n"), Some("edited\n"), None] {
            match content {
                Some(text) => fixture.write(path, text)?,
                None => fs::remove_file(fixture.root.join(path))?,
            }
            let head = fixture.commit()?;
            assert!(fixture.detect(&detector, "pull_request", &base, &head, None)?);
            let (output, calls) = capability_calls(&fixture, &script, &base, &head, "0")?;
            assert!(output.status.success(), "{path:?}: {output:?}");
            assert_eq!(calls, CAPABILITY_CALLS, "{path:?}, {content:?}");
            base = head;
        }
    }
    Ok(())
}

#[test]
fn capability_selector_rejects_unrelated_and_hostile_path_records() -> TestResult {
    let script = capability_shell()?;
    for path in [
        "README.md",
        "docs/other.md",
        "docs/status/SUPPORT_TIERS.md.bak",
        "docs/LANGUAGE_ADAPTER_PREVIEW.md.backup",
        "docs/specs-neighbor/ordinary.md",
        "other/docs/specs/ordinary.md",
        "docs/status/support_tiers.md",
        "other/café.md",
        "other/quote\".md",
        "other/tab\tname.md",
        "other/slash\\name.md",
        "other/line\ndocs/specs/forged.md",
    ] {
        let fixture = Fixture::new()?;
        let base = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.write(path, "unrelated\n")?;
        let head = fixture.commit()?;
        let (output, calls) = capability_calls(&fixture, &script, &base, &head, "0")?;
        assert!(output.status.success(), "{path:?}: {output:?}");
        assert!(calls.is_empty(), "unrelated {path:?} selected: {calls}");
    }
    Ok(())
}

#[test]
fn capability_selector_uses_nul_records_independent_of_quote_path() -> TestResult {
    let fixture = Fixture::new()?;
    fixture.git(&["config", "core.quotePath", "true"])?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    fixture.write("docs/specs/café.md", "unicode\n")?;
    let head = fixture.commit()?;
    let script = capability_shell()?;
    let (output, calls) = capability_calls(&fixture, &script, &base, &head, "0")?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(calls, CAPABILITY_CALLS);
    assert_eq!(script.matches("core.quotePath=false").count(), 1);
    let quoted = script.replace("core.quotePath=false", "core.quotePath=true");
    let (output, calls) = capability_calls(&fixture, &quoted, &base, &head, "0")?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(calls, CAPABILITY_CALLS, "NUL output ignores quotePath");
    assert_eq!(script.matches("--name-only -z").count(), 1);
    let mutant = script.replace("--name-only -z", "--name-only");
    let (output, calls) = capability_calls(&fixture, &mutant, &base, &head, "0")?;
    assert!(output.status.success(), "{output:?}");
    assert!(
        calls.is_empty(),
        "newline output cannot satisfy the NUL record reader"
    );
    Ok(())
}

#[test]
fn capability_selector_preserves_diff_and_selected_command_failures() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    fixture.write("docs/status/SUPPORT_TIERS.md", "selected\n")?;
    let head = fixture.commit()?;
    let script = capability_shell()?;
    let (output, calls) = capability_calls(&fixture, &script, "missing", &head, "0")?;
    assert!(
        !output.status.success(),
        "invalid diff cannot silently skip"
    );
    assert!(calls.is_empty());
    let (output, calls) = capability_calls(&fixture, &script, &base, &head, "23")?;
    assert_eq!(output.status.code(), Some(23), "{output:?}");
    assert_eq!(calls, "xtask capability-docs --check\n");
    let wrapper = fixture.wrapper(
        "if [ \"$3\" = diff ]; then printf 'docs/status/SUPPORT_TIERS.md\\0'; exit 17; fi",
    )?;
    let output = fixture.run(
        &format!("cargo() {{ return 99; }}\n{script}"),
        &[
            ("BASE_SHA", &base),
            ("HEAD_SHA", &head),
            ("TMPDIR", ".git/selector-temp"),
        ],
        Some(&wrapper),
    )?;
    assert_eq!(output.status.code(), Some(17), "partial diff: {output:?}");
    assert_eq!(
        fs::read_dir(fixture.root.join(".git/selector-temp"))?.count(),
        0
    );
    Ok(())
}

#[test]
fn capability_selector_selects_quoted_spec_paths() -> TestResult {
    let script = capability_shell()?;
    let mut missed = Vec::new();
    for path in [
        "docs/specs/quoted\".md",
        "docs/specs/line\nbreak.md",
        "docs/specs/tab\tname.md",
        "docs/specs/slash\\name.md",
    ] {
        let fixture = Fixture::new()?;
        let base = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.write(path, "relevant spec path\n")?;
        let head = fixture.commit()?;
        assert!(fixture.detect(
            &shell("detect-docs-only")?,
            "pull_request",
            &base,
            &head,
            None
        )?);
        let (output, calls) = capability_calls(&fixture, &script, &base, &head, "0")?;
        assert!(output.status.success(), "{output:?}");
        if calls != CAPABILITY_CALLS {
            missed.push(format!("{path:?}: calls={calls:?}"));
        }
    }
    // Literal expectations retain the quote/newline regression from #10065.
    assert!(
        missed.is_empty(),
        "quoted relevant paths skipped: {missed:?}"
    );
    Ok(())
}

#[test]
fn capability_selector_keeps_empty_diff_and_both_rename_sides() -> TestResult {
    let script = capability_shell()?;
    let fixture = Fixture::new()?;
    let head = fixture.git(&["rev-parse", "HEAD"])?;
    let (output, calls) = capability_calls(&fixture, &script, &head, &head, "0")?;
    assert!(output.status.success(), "{output:?}");
    assert!(calls.is_empty(), "empty diff must not select a build");
    for (from, to, expected) in [
        ("docs/status/SUPPORT_TIERS.md", "docs/other.md", true),
        ("docs/other.md", "docs/LANGUAGE_ADAPTER_PREVIEW.md", true),
        ("docs/specs/quoted\".md", "docs/other.md", true),
        ("docs/other.md", "docs/specs/line\nbreak.md", true),
        ("docs/old.md", "docs/new.md", false),
    ] {
        let fixture = Fixture::new()?;
        fixture.write(from, "identical rename content\n")?;
        let base = fixture.commit()?;
        fs::create_dir_all(fixture.root.join(to).parent().ok_or("rename parent")?)?;
        fixture.git(&["mv", from, to])?;
        let head = fixture.commit()?;
        let (output, calls) = capability_calls(&fixture, &script, &base, &head, "0")?;
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            calls,
            if expected { CAPABILITY_CALLS } else { "" },
            "rename {from:?} -> {to:?}"
        );
    }
    Ok(())
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> TestResult<Self> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-docs-routing-{}-{nonce}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(std::env::temp_dir())?;
        fs::create_dir(&root)?;
        let fixture = Self { root };
        fixture.git(&["init", "--object-format=sha1", "-b", "main"])?;
        fixture.git(&["config", "user.name", "Routing Fixture"])?;
        fixture.git(&["config", "user.email", "fixture@example.invalid"])?;
        fixture.git(&["config", "commit.gpgsign", "false"])?;
        fixture.write("seed.txt", "seed\n")?;
        fixture.commit()?;
        Ok(fixture)
    }

    fn command(&self, executable: &str) -> TestResult<Command> {
        let mut command = Command::new(executable);
        command
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").ok_or("PATH is available")?)
            .env("HOME", &self.root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("LC_ALL", "C");
        Ok(command)
    }

    fn output(&self, command: &mut Command) -> TestResult<Output> {
        // Files avoid pipe-buffer deadlocks while polling the child deadline.
        // Siblings cannot be accidentally staged, including during git init.
        let stdout = self.root.with_extension("stdout");
        let stderr = self.root.with_extension("stderr");
        let mut child = command
            .stdout(fs::File::create(&stdout)?)
            .stderr(fs::File::create(&stderr)?)
            .spawn()?;
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < std::time::Duration::from_secs(15) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "fixture subprocess timed out or could not be polled: {result:?}"
                    )
                    .into());
                }
            }
        };
        let output = Output {
            status,
            stdout: fs::read(&stdout)?,
            stderr: fs::read(&stderr)?,
        };
        fs::remove_file(stdout)?;
        fs::remove_file(stderr)?;
        Ok(output)
    }

    fn git(&self, args: &[&str]) -> TestResult<String> {
        let output = self.output(self.command("git")?.args(args))?;
        assert!(output.status.success(), "git {args:?}: {output:?}");
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    fn write(&self, path: &str, text: &str) -> TestResult {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().ok_or("fixture parent")?)?;
        fs::write(path, text)?;
        Ok(())
    }

    fn commit(&self) -> TestResult<String> {
        self.git(&["add", "--all"])?;
        self.git(&["commit", "-m", "fixture", "--allow-empty"])?;
        self.git(&["rev-parse", "HEAD"])
    }

    fn run(
        &self,
        script: &str,
        env: &[(&str, &str)],
        wrapper: Option<&Path>,
    ) -> TestResult<Output> {
        let output_file = self.root.join(".git/actions-output");
        let summary_file = self.root.join(".git/actions-summary");
        fs::write(&output_file, "")?;
        fs::write(&summary_file, "")?;
        let mut command = self.command("bash")?;
        command
            .args([
                "--noprofile",
                "--norc",
                "-e",
                "-o",
                "pipefail",
                "-c",
                script,
            ])
            .env("GITHUB_OUTPUT", output_file)
            .env("GITHUB_STEP_SUMMARY", summary_file)
            .envs(env.iter().copied());
        if let Some(wrapper) = wrapper {
            let mut paths = vec![wrapper.to_path_buf()];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").ok_or("PATH")?,
            ));
            command.env("PATH", std::env::join_paths(paths)?);
        }
        self.output(&mut command)
    }

    fn detect(
        &self,
        script: &str,
        event: &str,
        base: &str,
        head: &str,
        wrapper: Option<&Path>,
    ) -> TestResult<bool> {
        let output = self.run(
            script,
            &[
                ("EVENT_NAME", event),
                ("BASE_SHA", base),
                ("HEAD_SHA", head),
            ],
            wrapper,
        )?;
        assert!(
            output.status.success(),
            "detector must emit conservative output: {output:?}"
        );
        let outputs = fs::read_to_string(self.root.join(".git/actions-output"))?;
        let values = outputs
            .lines()
            .filter_map(|line| line.strip_prefix("docs_only="))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 1, "exactly one routing decision: {outputs}");
        assert!(
            matches!(values[0], "true" | "false"),
            "boolean routing output"
        );
        let summary = fs::read_to_string(self.root.join(".git/actions-summary"))?;
        assert!(
            !summary.trim().is_empty(),
            "detector must retain diagnostics"
        );
        Ok(values[0] == "true")
    }

    fn wrapper(&self, intervention: &str) -> TestResult<PathBuf> {
        let real_git = self.output(self.command("bash")?.args(["-c", "command -v git"]))?;
        assert!(real_git.status.success());
        let real_git = String::from_utf8(real_git.stdout)?;
        assert!(Path::new(real_git.trim()).is_absolute());
        let dir = self.root.join(".git/wrapper");
        fs::create_dir_all(&dir)?;
        let path = dir.join("git");
        let quoted_git = real_git.trim().replace('\'', "'\\''");
        fs::write(
            &path,
            format!("#!/bin/bash\n{intervention}\nexec '{quoted_git}' \"$@\"\n"),
        )?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(dir)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_file(self.root.with_extension("stdout"));
        let _ = fs::remove_file(self.root.with_extension("stderr"));
    }
}

#[test]
fn event_commit_bindings_are_exact_and_history_is_available() -> TestResult {
    let detector = job("detect-docs-only")?;
    let (step, _) = detector
        .split_once("        run: |\n")
        .ok_or("detector step")?;
    let (_, env) = step
        .split_once("        env:\n")
        .ok_or("detector step env")?;
    for binding in [
        "          EVENT_NAME: ${{ github.event_name }}",
        "          BASE_SHA: ${{ github.event.pull_request.base.sha || github.event.merge_group.base_sha }}",
        "          HEAD_SHA: ${{ github.event.pull_request.head.sha || github.event.merge_group.head_sha }}",
    ] {
        assert_eq!(
            env.lines().filter(|line| *line == binding).count(),
            1,
            "{binding}"
        );
    }
    assert_eq!(
        env.lines()
            .filter(|line| line.trim_start().starts_with("HEAD_SHA:"))
            .count(),
        1
    );
    assert_eq!(
        env.lines()
            .filter(|line| line.trim_start().starts_with("BASE_SHA:"))
            .count(),
        1
    );
    assert!(
        detector
            .lines()
            .any(|line| line == "          fetch-depth: 0")
    );
    Ok(())
}

#[test]
fn stale_event_base_and_synthetic_merge_checkout_use_pr_head() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?; // B
    fixture.git(&["checkout", "-b", "docs"])?;
    fixture.write("docs/change.md", "documentation\n")?;
    let head = fixture.commit()?; // D
    fixture.git(&["checkout", "main"])?;
    fixture.write("src/unrelated.rs", "fn later_main() {}\n")?;
    let later_main = fixture.commit()?; // C
    fixture.git(&["merge", "--no-ff", "docs", "-m", "synthetic PR checkout"])?; // M
    let merge = fixture.git(&["rev-parse", "HEAD"])?;
    assert_ne!(merge, head);
    assert_ne!(later_main, base);
    assert_eq!(fixture.git(&["merge-base", &base, &head])?, base);
    assert_eq!(
        fixture.git(&["diff", "--name-only", &format!("{base}...{head}")])?,
        "docs/change.md"
    );
    assert!(
        fixture
            .git(&["diff", "--name-only", &format!("{base}...HEAD")])?
            .contains("src/unrelated.rs")
    );
    let production = shell("detect-docs-only")?;
    assert!(fixture.detect(&production, "pull_request", &base, &head, None)?);
    // The merge queue classifies the queued commit the same way.
    assert!(fixture.detect(&production, "merge_group", &base, &head, None)?);
    let range = "\"$BASE_SHA\"...\"$HEAD_SHA\"";
    assert_eq!(
        production.matches(range).count(),
        1,
        "mutant targets actual diff range"
    );
    let mutant = production.replace(range, "\"$BASE_SHA\"...HEAD");
    // The same drift oracle rejects the historical checkout-based detector.
    assert!(!fixture.detect(&mutant, "pull_request", &base, &head, None)?);
    assert_eq!(
        fixture.git(&["rev-parse", "HEAD"])?,
        merge,
        "classification preserves integration checkout"
    );
    Ok(())
}

#[test]
fn filename_records_deletions_and_both_sides_of_renames_are_classified() -> TestResult {
    let production = shell("detect-docs-only")?;
    for (path, expected) in [
        ("docs/ordinary.md", true),
        ("docs/specs/data.json", true),
        ("docs/handoffs/data.txt", true),
        ("docs/a\nfile.md", true),
        ("docs/tab\tquote\"slash\\.md", true),
        ("docs/looks.md\nsource.rs", false),
        ("src/main.rs", false),
        ("CHANGELOG.md", false),
        ("docs/OUTPUT_SCHEMA.md", false),
        ("Cargo.toml", false),
        ("Cargo.lock", false),
        ("crates/ripr/Cargo.toml", false),
    ] {
        let fixture = Fixture::new()?;
        let base = fixture.git(&["rev-parse", "HEAD"])?;
        fixture.write(path, "content\n")?;
        let head = fixture.commit()?;
        assert_eq!(
            fixture.detect(&production, "pull_request", &base, &head, None)?,
            expected,
            "added {path:?}"
        );
        fs::remove_file(fixture.root.join(path))?;
        let deleted = fixture.commit()?;
        assert_eq!(
            fixture.detect(&production, "pull_request", &head, &deleted, None)?,
            expected,
            "deleted {path:?}"
        );
    }
    for (from, to, expected) in [
        ("old.md", "new.md", true),
        ("old.md", "src/new.rs", false),
        ("src/old.rs", "new.md", false),
        ("old.md", "CHANGELOG.md", false),
        ("CHANGELOG.md", "new.md", false),
        ("old.md", "docs/OUTPUT_SCHEMA.md", false),
        ("docs/OUTPUT_SCHEMA.md", "new.md", false),
    ] {
        let fixture = Fixture::new()?;
        fixture.write(from, "unchanged rename payload\n")?;
        let base = fixture.commit()?;
        fs::create_dir_all(fixture.root.join(to).parent().ok_or("rename parent")?)?;
        fixture.git(&["mv", from, to])?;
        let head = fixture.commit()?;
        assert_eq!(
            fixture.detect(&production, "pull_request", &base, &head, None)?,
            expected,
            "rename {from} -> {to}"
        );
    }
    Ok(())
}

#[test]
fn invalid_identities_empty_diff_and_non_pr_events_use_full_proof() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    fixture.write("change.md", "docs\n")?;
    let head = fixture.commit()?;
    let production = shell("detect-docs-only")?;
    let missing = "ffffffffffffffffffffffffffffffffffffffff";
    let blob = fixture.git(&["rev-parse", "HEAD:change.md"])?;
    for invalid in [
        "",
        "HEAD",
        "--all",
        "not-a-sha",
        &head[..12],
        missing,
        &blob,
    ] {
        assert!(
            !fixture.detect(&production, "pull_request", invalid, &head, None)?,
            "invalid base {invalid}"
        );
        assert!(
            !fixture.detect(&production, "pull_request", &base, invalid, None)?,
            "invalid head {invalid}"
        );
    }
    assert!(!fixture.detect(&production, "pull_request", &head, &head, None)?);
    fixture.write("src/mixed.rs", "fn mixed() {}\n")?;
    let mixed = fixture.commit()?;
    assert!(!fixture.detect(&production, "pull_request", &base, &mixed, None)?);
    for event in ["push", "workflow_dispatch"] {
        assert!(
            !fixture.detect(&production, event, &base, &head, None)?,
            "{event}"
        );
    }
    Ok(())
}

#[test]
fn missing_disjoint_ambiguous_merge_bases_and_partial_diff_fail_closed() -> TestResult {
    let fixture = Fixture::new()?;
    let base = fixture.git(&["rev-parse", "HEAD"])?;
    fixture.write("change.md", "docs\n")?;
    let head = fixture.commit()?;
    let production = shell("detect-docs-only")?;
    assert!(fixture.detect(&production, "pull_request", &base, &head, None)?);
    let shallow = fixture.wrapper("if [ \"$1\" = rev-parse ] && [ \"$2\" = --is-shallow-repository ]; then echo true; exit 0; fi")?;
    assert!(!fixture.detect(&production, "pull_request", &base, &head, Some(&shallow))?);
    let wrapper = fixture.wrapper("if [ \"$1\" = merge-base ]; then exit 2; fi")?;
    assert!(!fixture.detect(&production, "pull_request", &base, &head, Some(&wrapper))?);
    let wrapper =
        fixture.wrapper("if [ \"$1\" = diff ]; then printf 'partial.md\\0'; exit 23; fi")?;
    assert!(!fixture.detect(&production, "pull_request", &base, &head, Some(&wrapper))?);
    let tree = fixture.git(&["rev-parse", "HEAD^{tree}"])?;
    let disjoint = fixture.git(&["commit-tree", &tree, "-m", "unrelated root"])?;
    assert!(!fixture.detect(&production, "pull_request", &disjoint, &head, None)?);
    let left = fixture.git(&["commit-tree", &tree, "-p", &base, "-m", "left"])?;
    let right = fixture.git(&["commit-tree", &tree, "-p", &base, "-m", "right"])?;
    let merge_left = fixture.git(&[
        "commit-tree",
        &tree,
        "-p",
        &left,
        "-p",
        &right,
        "-m",
        "left merge",
    ])?;
    // Whichever best base Git chooses, a docs diff would otherwise pass.
    fixture.write("ambiguous.md", "requires a unique comparison base\n")?;
    fixture.commit()?;
    let changed_tree = fixture.git(&["rev-parse", "HEAD^{tree}"])?;
    let merge_right = fixture.git(&[
        "commit-tree",
        &changed_tree,
        "-p",
        &right,
        "-p",
        &left,
        "-m",
        "right merge",
    ])?;
    let bases = fixture.git(&["merge-base", "--all", &merge_left, &merge_right])?;
    assert_eq!(bases.lines().count(), 2, "real criss-cross fixture");
    assert_eq!(
        fixture.git(&[
            "diff",
            "--name-only",
            &format!("{merge_left}...{merge_right}")
        ])?,
        "ambiguous.md",
        "without unique-base validation this would look docs-only"
    );
    assert!(!fixture.detect(&production, "pull_request", &merge_left, &merge_right, None)?);
    Ok(())
}

fn routed_result(
    fixture: &Fixture,
    event: &str,
    detector: &str,
    docs: &str,
    docs_gate: &str,
    heavy: &str,
) -> TestResult<bool> {
    routed_result_admitted(
        fixture,
        event,
        detector,
        docs,
        docs_gate,
        heavy,
        ("success", "admitted"),
    )
}

fn routed_result_admitted(
    fixture: &Fixture,
    event: &str,
    detector: &str,
    docs: &str,
    docs_gate: &str,
    heavy: &str,
    admission: (&str, &str),
) -> TestResult<bool> {
    // The extension e2e step (#12122) runs only for an admitted merge-ready
    // request; elsewhere it is skipped and the summary does not read it.
    let vscode = if admission == ("success", "admitted") {
        "success"
    } else {
        "skipped"
    };
    routed_result_with_vscode(
        fixture,
        event,
        detector,
        docs,
        docs_gate,
        heavy,
        (admission.0, admission.1, vscode),
    )
}

fn routed_result_with_vscode(
    fixture: &Fixture,
    event: &str,
    detector: &str,
    docs: &str,
    docs_gate: &str,
    heavy: &str,
    (admission_result, admission_status, vscode): (&str, &str, &str),
) -> TestResult<bool> {
    let source = result_step()?;
    let (setup, _) = source.split_once("        run: |\n").ok_or("result step")?;
    let (_, bindings) = setup.split_once("        env:\n").ok_or("result env")?;
    let mut env = bindings
        .lines()
        .filter_map(|line| {
            line.strip_prefix("          ")
                .and_then(|line| line.split_once(':'))
                .map(|(key, _)| (key, ""))
        })
        .collect::<Vec<_>>();
    assert!(env.len() >= 20, "actual result env is initialized");
    env.extend([
        ("EVENT_NAME", event),
        // A pull_request result is a qualification only for an explicit
        // merge-ready request; other labels are exercised separately.
        (
            "LABEL_NAME",
            if event == "pull_request" {
                "merge-ready"
            } else {
                ""
            },
        ),
        // Manual/push positives represent the independent main authority;
        // diagnostic branch/tag refusal is exercised by routed_dispatch_result.
        (
            "WORKFLOW_REF",
            if event == "pull_request" {
                "refs/pull/10/merge"
            } else {
                "refs/heads/main"
            },
        ),
        ("DOCS_DETECT_RESULT", detector),
        ("DOCS_ONLY", docs),
        ("DOCS_GATE_RESULT", docs_gate),
        ("TARGET", "github"),
        ("ROUTE_RESULT", "success"),
        ("GITHUB_RESULT", heavy),
        // The hosted route runs its nextest step in the sharded job (#10507); a
        // green hosted gates job stands for a green sharded job in these fixtures.
        ("GITHUB_TESTS_RESULT", heavy),
        // This fixture isolates aggregate selection. The actual late shell and
        // its changing API inputs run in routed_dispatch_result.rs.
        (
            "ELIGIBILITY_RESULT",
            if admission_result == "success" && admission_status == "admitted" {
                "success"
            } else {
                "skipped"
            },
        ),
        ("VSCODE_CHECK_RESULT", vscode),
        ("ADMISSION_RESULT", admission_result),
        ("ADMISSION_STATUS", admission_status),
    ]);
    let output = fixture.run(&shell("result")?, &env, None)?;
    let summary = fs::read_to_string(fixture.root.join(".git/actions-summary"))?;
    assert!(
        summary.contains("- status:"),
        "shell reached result decision, not a setup error: {output:?}"
    );
    assert_eq!(
        output.status.success(),
        summary.contains("- status: `pass`"),
        "{summary}"
    );
    Ok(output.status.success())
}

#[test]
fn actual_result_block_treats_other_labels_as_a_nonrequest_that_never_qualifies() -> TestResult {
    let source = job("result")?;
    // The step keeps the label-aware nonrequest branch ahead of every
    // qualification decision; the job name (policy-pinned) then differs from
    // the required context, so this pass can never satisfy branch protection.
    let (_, run) = result_step()?
        .split_once("        run: |\n")
        .ok_or("result step")?;
    let first_decision = run
        .lines()
        .find(|line| line.trim_start().starts_with("if ["))
        .ok_or("first decision")?;
    assert!(
        first_decision.contains("\"${LABEL_NAME,,}\" != \"merge-ready\""),
        "{first_decision}"
    );
    assert!(source.contains("'Merge-ready Nonrequest'"));
    Ok(())
}

#[test]
fn actual_result_block_fails_a_pull_request_whose_selected_extension_e2e_did_not_succeed()
-> TestResult {
    let fixture = Fixture::new()?;
    assert!(routed_result_with_vscode(
        &fixture,
        "pull_request",
        "success",
        "true",
        "success",
        "skipped",
        ("success", "admitted", "success"),
    )?);
    for outcome in ["failure", "cancelled", "skipped"] {
        assert!(
            !routed_result_with_vscode(
                &fixture,
                "pull_request",
                "success",
                "true",
                "success",
                "skipped",
                ("success", "admitted", outcome),
            )?,
            "a {outcome} extension e2e step must fail the Result (#12122)"
        );
    }
    Ok(())
}

#[test]
fn actual_result_block_requires_successful_detection_and_selected_proof() -> TestResult {
    let fixture = Fixture::new()?;
    assert!(routed_result(
        &fixture,
        "pull_request",
        "success",
        "true",
        "success",
        "skipped"
    )?);
    for failed in ["failure", "cancelled", "skipped"] {
        assert!(!routed_result(
            &fixture,
            "pull_request",
            failed,
            "true",
            "success",
            "success"
        )?);
        assert!(!routed_result(
            &fixture,
            "pull_request",
            "success",
            "true",
            failed,
            "success"
        )?);
    }
    // Candidate admission is a required input: a refused, missing or failed
    // admission cannot be rescued by a green selected proof.
    for (result, status) in [
        ("success", "refused"),
        ("success", ""),
        ("failure", "admitted"),
        ("skipped", "admitted"),
    ] {
        assert!(!routed_result_admitted(
            &fixture,
            "pull_request",
            "success",
            "false",
            "skipped",
            "success",
            (result, status)
        )?);
    }
    assert!(!routed_result_admitted(
        &fixture,
        "pull_request",
        "success",
        "false",
        "skipped",
        "success",
        ("success", "not_applicable")
    )?);
    for event in ["push", "workflow_dispatch"] {
        for docs in ["false", "true"] {
            assert!(!routed_result(
                &fixture, event, "success", docs, "success", "skipped"
            )?);
            assert!(routed_result(
                &fixture, event, "success", docs, "skipped", "success"
            )?);
        }
    }
    Ok(())
}

// #12160: the extension e2e verdict step is executed with a stub `gh` and fixture
// check-run JSON, so the classification (not just the pinned text) is proved.
const VSCODE_HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const VSCODE_REQUEST_AT: &str = "2026-11-01T11:02:00Z";

fn check_json(id: u64, app: &str, status: &str, conclusion: &str, suite: u64, run: u64) -> String {
    let conclusion = if conclusion.is_empty() {
        "null".to_string()
    } else {
        format!("\"{conclusion}\"")
    };
    format!(
        "{{\"id\":{id},\"app\":{{\"slug\":\"{app}\"}},\"status\":\"{status}\",\"conclusion\":{conclusion},\"check_suite\":{{\"id\":{suite}}},\"html_url\":\"https://github.com/o/r/actions/runs/{run}/job/{id}\"}}"
    )
}

type NamedCase<'a> = (&'a str, VscodeCase<'a>, bool, &'a str);

struct VscodeCase<'a> {
    // (run id, display_title) of ci.yml runs referenced by the check rows.
    runs: &'a [(u64, &'a str)],
    surface: Vec<String>,
    vscode: Vec<String>,
    // Leading `gh` invocations that fail before the stub answers.
    api_failures: u32,
}

fn run_vscode_step(case: &VscodeCase) -> TestResult<(bool, String)> {
    let fixture = Fixture::new()?;
    let jq = fixture.command("jq").and_then(|mut jq| {
        jq.arg("--version");
        fixture.output(&mut jq)
    });
    if !jq.is_ok_and(|output| output.status.success()) {
        return Ok((true, "skip: jq unavailable".to_string()));
    }
    let stub = fixture.root.join(".git/stub");
    fs::create_dir_all(stub.join("bin"))?;
    fs::create_dir_all(stub.join("runs"))?;
    for (run, title) in case.runs {
        fs::write(stub.join(format!("runs/{run}")), format!("{title}\n"))?;
    }
    fs::write(stub.join("surface"), case.surface.join("\n") + "\n")?;
    fs::write(stub.join("vscode"), case.vscode.join("\n") + "\n")?;
    fs::write(stub.join("fails"), format!("{}\n", case.api_failures))?;
    let gh = stub.join("bin/gh");
    fs::write(
        &gh,
        r#"#!/bin/bash
set -e
n=$(cat "$STUB/fails")
if [ "$n" -gt 0 ]; then echo $((n - 1)) > "$STUB/fails"; exit 1; fi
case "$*" in
  *actions/runs/*) rid=$(sed -E 's#.*actions/runs/([0-9]+).*#\1#' <<<"$*"); cat "$STUB/runs/$rid" ;;
  *check_name=vscode%20surface*) cat "$STUB/surface" ;;
  *check_name=vscode*) cat "$STUB/vscode" ;;
  *) exit 2 ;;
esac
"#,
    )?;
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755))?;
    // A shell function shadows `sleep` and advances $SECONDS, so the 20 minute
    // deadline is reached without waiting. Retry sleeps run in a subshell and
    // are no-ops.
    fs::write(
        stub.join("bash_env"),
        "sleep() { SECONDS=$((SECONDS + 400)); }\n",
    )?;
    let stub_dir = stub.to_string_lossy().into_owned();
    let bash_env = stub.join("bash_env").to_string_lossy().into_owned();
    let env = [
        ("STUB", stub_dir.as_str()),
        ("BASH_ENV", bash_env.as_str()),
        ("REPOSITORY", "o/r"),
        ("REQUEST_AT", VSCODE_REQUEST_AT),
        ("EXPECTED_HEAD", VSCODE_HEAD),
    ];
    let output = fixture.run(&vscode_step_shell()?, &env, Some(&stub.join("bin")))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok((output.status.success(), text))
}

#[test]
fn extension_e2e_step_classifies_check_runs_by_the_requests_own_suite() -> TestResult {
    let ga = "github-actions";
    // Surface and vscode rows for one request: run 100, suite 100, titled with this request.
    let title = |at: &str| format!("CI merge-ready request {VSCODE_HEAD} {at}");
    let own_title = title(VSCODE_REQUEST_AT);
    let earlier_title = title("2026-11-01T11:01:58Z");
    let mine = [(100, own_title.as_str())];
    let earlier_only = [(100, earlier_title.as_str())];
    let ordinary = [(100, "CI")];
    let earlier_then_own = [(100, earlier_title.as_str()), (200, own_title.as_str())];
    let many = [
        (100, own_title.as_str()),
        (200, own_title.as_str()),
        (300, "CI"),
        (400, "CI"),
        (500, "CI"),
        (600, "CI"),
    ];
    let same_title_pair = [(100, own_title.as_str()), (200, own_title.as_str())];
    let own_then_earlier = [(100, own_title.as_str()), (200, earlier_title.as_str())];
    let surface_ok = || vec![check_json(10, ga, "completed", "success", 100, 100)];
    let cases: Vec<NamedCase> = vec![
        (
            "no surface check yet fails closed after the wait",
            VscodeCase {
                runs: &mine,
                surface: vec![],
                vscode: vec![],
                api_failures: 0,
            },
            false,
            "is vscode surface absent",
        ),
        (
            "pending surface fails closed after the wait",
            VscodeCase {
                runs: &mine,
                surface: vec![check_json(10, ga, "in_progress", "", 100, 100)],
                vscode: vec![],
                api_failures: 0,
            },
            false,
            "vscode surface in_progress",
        ),
        (
            "failed surface fails the Result",
            VscodeCase {
                runs: &mine,
                surface: vec![check_json(10, ga, "completed", "failure", 100, 100)],
                vscode: vec![],
                api_failures: 0,
            },
            false,
            "vscode surface concluded 'failure'",
        ),
        (
            "skipped surface never counts",
            VscodeCase {
                runs: &mine,
                surface: vec![check_json(10, ga, "completed", "skipped", 100, 100)],
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 0,
            },
            false,
            "vscode surface absent",
        ),
        (
            "a surface from another app never counts",
            VscodeCase {
                runs: &mine,
                surface: vec![check_json(
                    10,
                    "other-app",
                    "completed",
                    "success",
                    100,
                    100,
                )],
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 0,
            },
            false,
            "vscode surface absent",
        ),
        (
            "skipped vscode in the request's own suite means not selected",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "completed", "skipped", 100, 100)],
                api_failures: 0,
            },
            true,
            "was skipped in this request's run",
        ),
        (
            "a stale skipped vscode from another suite cannot read as not selected",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![check_json(5, ga, "completed", "skipped", 50, 50)],
                api_failures: 0,
            },
            false,
            "not yet created",
        ),
        (
            "a succeeding vscode check passes",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 0,
            },
            true,
            "vscode check succeeded",
        ),
        (
            "a failing vscode check fails",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "completed", "failure", 100, 100)],
                api_failures: 0,
            },
            false,
            "concluded 'failure'",
        ),
        (
            "a vscode check still running at the deadline fails closed",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "in_progress", "", 100, 100)],
                api_failures: 0,
            },
            false,
            "is in_progress",
        ),
        (
            "a passing re-run in the suite clears an earlier failure",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![
                    check_json(11, ga, "completed", "failure", 100, 100),
                    check_json(12, ga, "completed", "success", 100, 100),
                ],
                api_failures: 0,
            },
            true,
            "vscode check succeeded",
        ),
        (
            "a later failure in the suite overrides an earlier pass",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![
                    check_json(11, ga, "completed", "success", 100, 100),
                    check_json(12, ga, "completed", "failure", 100, 100),
                ],
                api_failures: 0,
            },
            false,
            "concluded 'failure'",
        ),
        (
            "an earlier request's surface run is not this request's and is ignored",
            VscodeCase {
                runs: &earlier_only,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 0,
            },
            false,
            "vscode surface absent",
        ),
        (
            "a run with an ordinary title (manual dispatch, other event) does not count",
            VscodeCase {
                runs: &ordinary,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 0,
            },
            false,
            "vscode surface absent",
        ),
        (
            "a second request binds its own suite, not the earlier request's failure (#12191)",
            VscodeCase {
                runs: &earlier_then_own,
                surface: vec![
                    check_json(10, ga, "completed", "success", 100, 100),
                    check_json(20, ga, "completed", "success", 200, 200),
                ],
                vscode: vec![
                    check_json(11, ga, "completed", "failure", 100, 100),
                    check_json(21, ga, "completed", "success", 200, 200),
                ],
                api_failures: 0,
            },
            true,
            "runs/200",
        ),
        (
            "a second request's own failure fails even when the earlier request passed (#12191)",
            VscodeCase {
                runs: &earlier_then_own,
                surface: vec![
                    check_json(10, ga, "completed", "success", 100, 100),
                    check_json(20, ga, "completed", "success", 200, 200),
                ],
                vscode: vec![
                    check_json(11, ga, "completed", "success", 100, 100),
                    check_json(21, ga, "completed", "failure", 200, 200),
                ],
                api_failures: 0,
            },
            false,
            "runs/200",
        ),
        (
            "requests seconds apart, second surface not yet created: wait, never read the earlier suite (#12236)",
            VscodeCase {
                runs: &earlier_only,
                surface: vec![check_json(10, ga, "completed", "success", 100, 100)],
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 0,
            },
            false,
            "vscode surface absent",
        ),
        (
            "the request's own surface wins whatever the candidate order or creation time (#12236)",
            VscodeCase {
                runs: &own_then_earlier,
                surface: vec![
                    check_json(10, ga, "completed", "success", 100, 100),
                    check_json(20, ga, "completed", "success", 200, 200),
                ],
                vscode: vec![
                    check_json(11, ga, "completed", "success", 100, 100),
                    check_json(21, ga, "completed", "failure", 200, 200),
                ],
                api_failures: 0,
            },
            true,
            "runs/100",
        ),
        (
            "two distinct runs with the identical request title fail closed whichever suite is green (#12236 review)",
            VscodeCase {
                runs: &same_title_pair,
                surface: vec![
                    check_json(10, ga, "completed", "success", 100, 100),
                    check_json(20, ga, "completed", "success", 200, 200),
                ],
                vscode: vec![
                    check_json(11, ga, "completed", "failure", 100, 100),
                    check_json(21, ga, "completed", "success", 200, 200),
                ],
                api_failures: 0,
            },
            false,
            "more than one ci.yml run",
        ),
        (
            "identical request titles with one suite skipped cannot make the other request look not selected (#12236 review)",
            VscodeCase {
                runs: &same_title_pair,
                surface: vec![
                    check_json(10, ga, "completed", "success", 100, 100),
                    check_json(20, ga, "completed", "success", 200, 200),
                ],
                vscode: vec![check_json(11, ga, "completed", "skipped", 100, 100)],
                api_failures: 0,
            },
            false,
            "more than one ci.yml run",
        ),
        (
            "a cancelled run with the identical request title and a green one fail closed",
            VscodeCase {
                runs: &same_title_pair,
                surface: vec![
                    check_json(10, ga, "completed", "cancelled", 100, 100),
                    check_json(20, ga, "completed", "success", 200, 200),
                ],
                vscode: vec![check_json(21, ga, "completed", "success", 200, 200)],
                api_failures: 0,
            },
            false,
            "more than one ci.yml run",
        ),
        (
            "an earlier same-title run left cancelled by the concurrency group cannot be accepted (#12236 review)",
            VscodeCase {
                runs: &mine,
                surface: vec![check_json(10, ga, "completed", "cancelled", 100, 100)],
                vscode: vec![check_json(11, ga, "completed", "skipped", 100, 100)],
                api_failures: 0,
            },
            false,
            "concluded 'cancelled'",
        ),
        (
            "a matching failed run older than five newer nonmatching surfaces still counts toward the ambiguity (#12236 review)",
            VscodeCase {
                runs: &many,
                surface: vec![
                    check_json(10, ga, "completed", "failure", 100, 100),
                    check_json(20, ga, "completed", "success", 200, 200),
                    check_json(30, ga, "completed", "success", 300, 300),
                    check_json(40, ga, "completed", "success", 400, 400),
                    check_json(50, ga, "completed", "success", 500, 500),
                    check_json(60, ga, "completed", "success", 600, 600),
                ],
                vscode: vec![check_json(21, ga, "completed", "success", 200, 200)],
                api_failures: 0,
            },
            false,
            "more than one ci.yml run",
        ),
        (
            "a re-run of one run keeps one run id and still passes",
            VscodeCase {
                runs: &mine,
                surface: vec![
                    check_json(10, ga, "completed", "success", 100, 100),
                    check_json(12, ga, "completed", "success", 100, 100),
                ],
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 0,
            },
            true,
            "vscode check succeeded",
        ),
        (
            "transient API failures are retried",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 2,
            },
            true,
            "vscode check succeeded",
        ),
        (
            "persistent API failure fails the Result",
            VscodeCase {
                runs: &mine,
                surface: surface_ok(),
                vscode: vec![check_json(11, ga, "completed", "success", 100, 100)],
                api_failures: 1000,
            },
            false,
            "could not read vscode surface check runs",
        ),
    ];
    for (name, case, passes, needle) in cases {
        let (ok, text) = run_vscode_step(&case)?;
        if text.starts_with("skip:") {
            return Ok(());
        }
        assert_eq!(ok, passes, "{name}: {text}");
        assert!(
            text.contains(needle),
            "{name}: expected `{needle}` in {text}"
        );
    }
    Ok(())
}
