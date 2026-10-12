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

fn shell(name: &str) -> TestResult<String> {
    let source = job(name)?;
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
        "          BASE_SHA: ${{ github.event.pull_request.base.sha }}",
        "          HEAD_SHA: ${{ github.event.pull_request.head.sha }}",
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
    let source = job("result")?;
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
        ("DOCS_DETECT_RESULT", detector),
        ("DOCS_ONLY", docs),
        ("DOCS_GATE_RESULT", docs_gate),
        ("TARGET", "github"),
        ("ROUTE_RESULT", "success"),
        ("GITHUB_RESULT", heavy),
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
