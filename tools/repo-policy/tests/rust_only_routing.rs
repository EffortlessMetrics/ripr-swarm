//! Execute the production Actions shell against real Git DAGs, not a copied classifier.
//! The Ubuntu subprocess fixtures require Bash, Git, and GNU timeout on Linux.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const WORKFLOW: &str = include_str!("../../../.github/workflows/ci.yml");
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
            "ripr-rust-routing-{}-{nonce}-{}",
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
        self.command_with_deadline(executable, "15s")
    }

    fn command_with_deadline(&self, executable: &str, deadline: &str) -> TestResult<Command> {
        // GNU timeout owns the whole subprocess group, including Bash's Git
        // children. Killing only the direct shell can leave a fixture in use.
        let mut command = Command::new("timeout");
        command
            .args(["--kill-after=1s", deadline, executable])
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
        Ok(command.output()?)
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
            .filter_map(|line| line.strip_prefix("rust="))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 1, "exactly one routing decision: {outputs}");
        assert!(
            matches!(values[0], "true" | "false"),
            "boolean routing output"
        );
        let summary = fs::read_to_string(self.root.join(".git/actions-summary"))?;
        assert!(summary.contains(&format!("- rust: `{}`", values[0])));
        assert!(
            summary.contains("- reason: `"),
            "visible routing reason: {summary}"
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
    }
}

#[test]
fn event_commit_bindings_are_exact_and_history_is_available() -> TestResult {
    let detector = job("rust-only-features")?;
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
    assert!(detector.lines().any(|line| line == "    if: github.event_name != 'pull_request' || contains(fromJSON('[\"opened\", \"synchronize\", \"reopened\"]'), github.event.action)"));
    assert!(
        !detector
            .lines()
            .any(|line| line.trim_start().starts_with("ref:")),
        "checkout retains the synthetic integration commit"
    );
    assert_eq!(
        detector
            .matches("if: steps.surface.outputs.rust == 'true'")
            .count(),
        3
    );
    assert!(detector.contains(
        "run: cargo test -p ripr --locked --no-default-features --features lang-rust --no-fail-fast"
    ));
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
    fixture.write("crates/ripr/src/unrelated.rs", "fn later_main() {}\n")?;
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
            .contains("crates/ripr/src/unrelated.rs")
    );
    let production = shell("rust-only-features")?;
    assert!(!fixture.detect(&production, "pull_request", &base, &head, None)?);
    let range = "\"$BASE_SHA\"...\"$HEAD_SHA\"";
    assert_eq!(
        production.matches(range).count(),
        1,
        "mutant targets actual diff range"
    );
    let mutant = production.replace(range, "\"$BASE_SHA\"...HEAD");
    // The same drift oracle rejects the historical checkout-based detector.
    assert!(fixture.detect(&mutant, "pull_request", &base, &head, None)?);
    assert_eq!(
        fixture.git(&["rev-parse", "HEAD"])?,
        merge,
        "classification preserves integration checkout"
    );
    Ok(())
}

#[test]
fn filename_records_deletions_and_both_sides_of_renames_are_classified() -> TestResult {
    let production = shell("rust-only-features")?;
    for (path, expected) in [
        ("README.md", false),
        (".github/workflows/example.yml", false),
        ("docs/ordinary.md", false),
        ("docs/a\ncrates/not-rust.md", false),
        ("docs/tab\tquote\"slash\\.md", false),
        ("crates/ripr/src/main.rs", true),
        ("crates/a\nfile.rs", true),
        ("fixtures/rust/example.rs", true),
        ("Cargo.toml", true),
        ("Cargo.lock", true),
        ("crates/ripr/Cargo.toml", true),
        ("rust-toolchain.toml", true),
        ("rust-toolchain", true),
        (".cargo/config.toml", true),
        ("Cargo.toml.backup", false),
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
        ("old.md", "new.md", false),
        ("old.md", "crates/ripr/new.rs", true),
        ("crates/ripr/old.rs", "new.md", true),
        ("old.md", "Cargo.toml", true),
        ("Cargo.lock", "new.md", true),
        (".cargo/config.toml", "docs/config.md", true),
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
    let production = shell("rust-only-features")?;
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
            fixture.detect(&production, "pull_request", invalid, &head, None)?,
            "invalid base {invalid}"
        );
        assert!(
            fixture.detect(&production, "pull_request", &base, invalid, None)?,
            "invalid head {invalid}"
        );
    }
    assert!(fixture.detect(&production, "pull_request", &head, &head, None)?);
    fixture.write("crates/ripr/mixed.rs", "fn mixed() {}\n")?;
    let mixed = fixture.commit()?;
    assert!(fixture.detect(&production, "pull_request", &base, &mixed, None)?);
    for event in ["push", "workflow_dispatch"] {
        assert!(
            fixture.detect(&production, event, &base, &head, None)?,
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
    let production = shell("rust-only-features")?;
    assert!(!fixture.detect(&production, "pull_request", &base, &head, None)?);
    let shallow = fixture.wrapper("if [ \"$1\" = rev-parse ] && [ \"$2\" = --is-shallow-repository ]; then echo true; exit 0; fi")?;
    assert!(fixture.detect(&production, "pull_request", &base, &head, Some(&shallow))?);
    let wrapper = fixture.wrapper("if [ \"$1\" = merge-base ]; then exit 2; fi")?;
    assert!(fixture.detect(&production, "pull_request", &base, &head, Some(&wrapper))?);
    let wrapper =
        fixture.wrapper("if [ \"$1\" = diff ]; then printf 'partial.md\\0'; exit 23; fi")?;
    assert!(fixture.detect(&production, "pull_request", &base, &head, Some(&wrapper))?);
    let tree = fixture.git(&["rev-parse", "HEAD^{tree}"])?;
    let disjoint = fixture.git(&["commit-tree", &tree, "-m", "unrelated root"])?;
    assert!(fixture.detect(&production, "pull_request", &disjoint, &head, None)?);
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
        "without unique-base validation this would skip the Rust lane"
    );
    assert!(fixture.detect(&production, "pull_request", &merge_left, &merge_right, None)?);
    Ok(())
}

#[test]
fn missing_event_objects_can_be_recovered_without_changing_checkout() -> TestResult {
    let remote = Fixture::new()?;
    remote.write("remote-seed.txt", "distinct remote history\n")?;
    let base = remote.commit()?;
    remote.write("docs/change.md", "docs-only remote candidate\n")?;
    let head = remote.commit()?;
    let local = Fixture::new()?;
    let checkout = local.git(&["rev-parse", "HEAD"])?;
    local.git(&[
        "remote",
        "add",
        "origin",
        remote.root.to_str().ok_or("fixture path")?,
    ])?;
    let missing = local.output(local.command("git")?.args(["cat-file", "-e", &head]))?;
    assert!(
        !missing.status.success(),
        "the event head starts unavailable"
    );
    assert!(!local.detect(
        &shell("rust-only-features")?,
        "pull_request",
        &base,
        &head,
        None
    )?);
    assert_eq!(local.git(&["cat-file", "-t", &head])?, "commit");
    assert_eq!(local.git(&["rev-parse", "HEAD"])?, checkout);
    Ok(())
}

#[test]
fn timeout_terminates_and_reaps_shell_descendants() -> TestResult {
    let fixture = Fixture::new()?;
    for (script, stubborn) in [
        (
            "sleep 30 & child=$!; echo \"$child\" > .git/child-pid; trap 'wait \"$child\"; exit 0' TERM; wait \"$child\"",
            false,
        ),
        (
            "trap '' TERM; bash -c \"trap '' TERM; exec sleep 30\" & child=$!; echo \"$child\" > .git/child-pid; wait \"$child\"",
            true,
        ),
    ] {
        let output = fixture.output(
            fixture
                .command_with_deadline("bash", "0.2s")?
                .args(["-c", script]),
        )?;
        if stubborn {
            assert_eq!(
                output.status.signal(),
                Some(9),
                "timeout escalated to group SIGKILL: {output:?}"
            );
        } else {
            assert_eq!(
                output.status.code(),
                Some(124),
                "actual timeout: {output:?}"
            );
        }
        let pid = fs::read_to_string(fixture.root.join(".git/child-pid"))?;
        let pid = pid.trim();
        assert!(!pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit()));
        let process = Path::new("/proc").join(pid);
        let started = std::time::Instant::now();
        while process.exists() && started.elapsed() < std::time::Duration::from_secs(1) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let alive = process.exists();
        if alive {
            // Clean up the deliberately spawned witness before reporting a failure.
            fixture.output(fixture.command("kill")?.args(["-KILL", pid]))?;
        }
        assert!(
            !alive,
            "timed-out descendant {pid} must be gone before fixture cleanup (stubborn={stubborn})"
        );
    }
    Ok(())
}
