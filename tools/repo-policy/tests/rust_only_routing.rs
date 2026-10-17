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
    retained: bool,
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
        let fixture = Self {
            root,
            retained: false,
        };
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
        if !self.retained {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn verify_guarded_rust_only_steps(source: &str) -> TestResult {
    let steps = source.split("\n      - ").collect::<Vec<_>>();
    for (header, command) in [
        ("uses: dtolnay/rust-toolchain@1.95.0", None),
        ("uses: Swatinem/rust-cache@v2", None),
        (
            "name: Prepare locked owner-return fixture dependencies",
            Some(
                "cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml",
            ),
        ),
        (
            "name: Test --no-default-features --features lang-rust",
            Some(
                "cargo test -p ripr --locked --no-default-features --features lang-rust --no-fail-fast",
            ),
        ),
    ] {
        let matching = steps
            .iter()
            .filter(|step| step.lines().next() == Some(header))
            .collect::<Vec<_>>();
        let [step] = matching.as_slice() else {
            return Err(format!("expected one guarded Rust step: {header}").into());
        };
        let guards = step
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("if:"))
            .collect::<Vec<_>>();
        if guards != ["if: steps.surface.outputs.rust == 'true'"] {
            return Err(format!("Rust surface guard must belong to step: {header}").into());
        }
        if let Some(command) = command {
            let runs = step
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("run:"))
                .collect::<Vec<_>>();
            if runs != [format!("run: {command}")] {
                return Err(format!("exact command must belong to guarded step: {header}").into());
            }
        }
    }
    Ok(())
}

#[test]
fn rust_step_contract_refuses_missing_guards_and_misplaced_commands() -> TestResult {
    let source = job("rust-only-features")?;
    verify_guarded_rust_only_steps(source)?;
    for header in [
        "uses: dtolnay/rust-toolchain@1.95.0",
        "uses: Swatinem/rust-cache@v2",
        "name: Prepare locked owner-return fixture dependencies",
        "name: Test --no-default-features --features lang-rust",
    ] {
        let prefix =
            format!("\n      - {header}\n        if: steps.surface.outputs.rust == 'true'\n");
        assert!(
            source.contains(&prefix),
            "production guarded step setup: {header}"
        );
        let missing = source.replacen(&prefix, &format!("\n      - {header}\n"), 1);
        assert!(
            verify_guarded_rust_only_steps(&missing).is_err(),
            "{header}"
        );
        let weakened = source.replacen(
            &prefix,
            &format!("\n      - {header}\n        if: always()\n"),
            1,
        );
        assert!(
            verify_guarded_rust_only_steps(&weakened).is_err(),
            "{header}"
        );
    }
    for command in [
        "cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml",
        "cargo test -p ripr --locked --no-default-features --features lang-rust --no-fail-fast",
    ] {
        let moved = source.replacen(
            &format!("        run: {command}"),
            "        run: echo misplaced-command",
            1,
        ) + &format!("\n      - name: Unrelated step\n        run: {command}\n");
        assert!(verify_guarded_rust_only_steps(&moved).is_err(), "{command}");
    }
    Ok(())
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
    assert!(detector.lines().any(|line| line == "    if: (github.event_name != 'pull_request' || (contains(github.event.pull_request.labels.*.name, 'merge-ready') && (github.event.action == 'labeled' && github.event.label.name == 'merge-ready')))"));
    assert!(
        !detector
            .lines()
            .any(|line| line.trim_start().starts_with("ref:")),
        "checkout retains the synthetic integration commit"
    );
    verify_guarded_rust_only_steps(detector)?;
    Ok(())
}

/// Every merge-ready-gated lane admits a pull request only through an explicit
/// `labeled` event for `merge-ready`; no gated `if:` re-admits opened,
/// synchronize or reopened events while the label is still attached.
#[test]
fn gated_lanes_admit_pull_requests_only_on_the_merge_ready_labeled_event() {
    const GATED: [(&str, &str); 12] = [
        ("ci.yml", include_str!("../../../.github/workflows/ci.yml")),
        (
            "coverage.yml",
            include_str!("../../../.github/workflows/coverage.yml"),
        ),
        (
            "future-clippy.yml",
            include_str!("../../../.github/workflows/future-clippy.yml"),
        ),
        (
            "macos-advisory.yml",
            include_str!("../../../.github/workflows/macos-advisory.yml"),
        ),
        (
            "npm-launcher.yml",
            include_str!("../../../.github/workflows/npm-launcher.yml"),
        ),
        (
            "pilot-ranking.yml",
            include_str!("../../../.github/workflows/pilot-ranking.yml"),
        ),
        (
            "printed-command-paste.yml",
            include_str!("../../../.github/workflows/printed-command-paste.yml"),
        ),
        (
            "public-proof.yml",
            include_str!("../../../.github/workflows/public-proof.yml"),
        ),
        (
            "security.yml",
            include_str!("../../../.github/workflows/security.yml"),
        ),
        (
            "test-analytics.yml",
            include_str!("../../../.github/workflows/test-analytics.yml"),
        ),
        (
            "windows-advisory.yml",
            include_str!("../../../.github/workflows/windows-advisory.yml"),
        ),
        (
            "zed-extension.yml",
            include_str!("../../../.github/workflows/zed-extension.yml"),
        ),
    ];
    for (name, source) in GATED {
        if source.contains("\nconcurrency:\n") {
            const REQUEST: &str = "github.event_name == 'pull_request' && github.event.action == 'labeled' && github.event.label.name == 'merge-ready'";
            let prefix = match name {
                "pilot-ranking.yml" => "pilot-ranking",
                "public-proof.yml" => "public-proof",
                _ => "${{ github.workflow }}",
            };
            let cancels_non_pr = matches!(name, "pilot-ranking.yml" | "public-proof.yml");
            let expected_group = format!(
                "  group: {prefix}-${{{{ github.event.pull_request.number || github.ref }}}}-${{{{ (github.event_name == 'pull_request' && !(github.event.action == 'labeled' && github.event.label.name == 'merge-ready')) && github.run_id || 'run' }}}}"
            );
            let expected_cancel = if cancels_non_pr {
                format!(
                    "  cancel-in-progress: ${{{{ ({REQUEST}) || github.event_name != 'pull_request' }}}}"
                )
            } else {
                format!("  cancel-in-progress: ${{{{ ({REQUEST}) }}}}")
            };
            assert!(
                source.lines().any(|line| line == expected_group),
                "{name}: group must give non-request pull_request events a unique group"
            );
            assert!(
                source.lines().any(|line| line == expected_cancel),
                "{name}: only a merge-ready request may cancel pull_request runs"
            );
        }
        let violations = job_guard_violations(source);
        assert!(violations.is_empty(), "{name}: {violations:?}");
    }
}

/// Every job under `jobs:` must carry a single-line `if:` admitting only the
/// explicit merge-ready `labeled` event. Returns one message per violation.
fn job_guard_violations(source: &str) -> Vec<String> {
    const EXPLICIT: &str =
        "github.event.action == 'labeled' && github.event.label.name == 'merge-ready'";
    let mut violations = Vec::new();
    let mut guarded: Vec<(&str, bool)> = Vec::new();
    let mut in_jobs = false;
    for line in source.lines() {
        if line == "jobs:" {
            in_jobs = true;
        } else if in_jobs
            && !line.starts_with(' ')
            && !line.starts_with('#')
            && !line.trim().is_empty()
        {
            in_jobs = false;
        }
        if !in_jobs {
            continue;
        }
        if let Some(key) = line
            .strip_prefix("  ")
            .filter(|rest| !rest.starts_with(' '))
            .and_then(|rest| rest.strip_suffix(':'))
            .filter(|key| {
                key.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            })
        {
            guarded.push((key, false));
        } else if line.starts_with("    if:") && !guarded.is_empty() {
            if !(line.contains("'merge-ready'") && line.contains(EXPLICIT)) {
                violations.push(format!(
                    "guard must admit only the explicit merge-ready event: {line}"
                ));
            }
            for event in ["opened", "synchronize", "reopened", "ready_for_review"] {
                if line.contains(&format!("'{event}'")) {
                    violations.push(format!("re-admits {event}: {line}"));
                }
            }
            if let Some(entry) = guarded.last_mut() {
                entry.1 = true;
            }
        }
    }
    if guarded.is_empty() {
        violations.push("declares no jobs".to_string());
    }
    for (job, has_guard) in guarded {
        if !has_guard {
            violations.push(format!("job `{job}` has no single-line merge-ready guard"));
        }
    }
    violations
}

#[test]
fn job_guard_scanner_rejects_unguarded_jobs_even_after_column_zero_comments() {
    const GUARD: &str = "    if: github.event_name != 'pull_request' || (github.event.action == 'labeled' && github.event.label.name == 'merge-ready')";
    let guarded = format!("jobs:\n  a:\n{GUARD}\n    runs-on: x\n");
    assert!(job_guard_violations(&guarded).is_empty());

    let unguarded = "jobs:\n  a:\n    runs-on: x\n";
    assert_eq!(job_guard_violations(unguarded).len(), 1);

    let after_comment =
        format!("jobs:\n  a:\n{GUARD}\n# a column-zero comment\n  b:\n    runs-on: x\n");
    let violations = job_guard_violations(&after_comment);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("`b`"), "{violations:?}");

    let readmits = "jobs:\n  a:\n    if: github.event.action == 'opened' || (github.event.action == 'labeled' && github.event.label.name == 'merge-ready')\n";
    assert_eq!(job_guard_violations(readmits).len(), 1);
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

// Follow xtask's /proc observer semantics: split after the final ')' and keep
// unreadable/malformed observations distinct from a terminated process.
fn witness_identity(stat: &str) -> TestResult<(u32, u64, char)> {
    let (prefix, rest) = stat.rsplit_once(')').ok_or("process stat delimiter")?;
    let pid = prefix
        .split_ascii_whitespace()
        .next()
        .ok_or("process PID")?
        .parse()?;
    let fields = rest.split_ascii_whitespace().collect::<Vec<_>>();
    let state = fields.first().ok_or("process state")?;
    if state.len() != 1 {
        return Err("invalid process state".into());
    }
    let state = state.chars().next().ok_or("process state")?;
    let start = fields.get(19).ok_or("process start time")?.parse()?;
    Ok((pid, start, state))
}

fn witness_is_live(stat: Result<&str, &std::io::Error>, pid: u32, start: u64) -> TestResult<bool> {
    match stat {
        Ok(stat) => {
            let (observed_pid, observed_start, state) = witness_identity(stat)?;
            Ok(observed_pid == pid && observed_start == start && !matches!(state, 'Z' | 'X'))
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(3) =>
        {
            Ok(false)
        }
        Err(error) => Err(format!("cannot observe witness: {error}").into()),
    }
}

#[test]
fn witness_observer_distinguishes_identity_state_and_incomplete_evidence() -> TestResult {
    let stat = |pid, state, start| {
        format!(
            "{pid} (name with ) parentheses) {state} {} {start}",
            vec!["0"; 18].join(" ")
        )
    };
    assert!(witness_is_live(Ok(&stat(42, 'S', 123)), 42, 123)?);
    for (pid, state, start) in [
        (42, 'Z', 123),
        (42, 'X', 123),
        (42, 'S', 124),
        (43, 'S', 123),
    ] {
        assert!(!witness_is_live(Ok(&stat(pid, state, start)), 42, 123)?);
    }
    for malformed in ["", "42 (broken", "42 (broken) S", "42 (broken) invalid"] {
        if let Ok(observed) = witness_is_live(Ok(malformed), 42, 123) {
            return Err(format!("malformed observation became live={observed}").into());
        }
    }
    for error in [
        std::io::Error::from(std::io::ErrorKind::NotFound),
        std::io::Error::from_raw_os_error(3),
    ] {
        assert!(!witness_is_live(Err(&error), 42, 123)?);
    }
    let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    if let Ok(observed) = witness_is_live(Err(&denied), 42, 123) {
        return Err(format!("unreadable observation became live={observed}").into());
    }
    Ok(())
}

#[test]
fn timeout_terminates_shell_descendants() -> TestResult {
    let mut fixture = Fixture::new()?;
    for stubborn in [false, true] {
        // Record the shell's birth identity before exec, which preserves it.
        // Null streams prevent a leaked child from holding Command::output open
        // until its natural exit and falsely passing the termination observation.
        let trap = if stubborn { "trap \"\" TERM; " } else { "" };
        let script = format!(
            "{trap}bash -c '{trap}IFS= read -r stat < /proc/$BASHPID/stat; printf \"%s\\n\" \"$stat\" > .git/child-stat; exec sleep 8' </dev/null >/dev/null 2>&1 & child=$!; {}wait \"$child\"",
            if stubborn {
                ""
            } else {
                "trap 'wait \"$child\"; exit 0' TERM; "
            }
        );
        let started = std::time::Instant::now();
        let output = fixture.output(
            fixture
                .command_with_deadline("bash", "0.2s")?
                .args(["-c", &script]),
        );
        let elapsed = started.elapsed();
        let observation = (|| -> TestResult<bool> {
            let original = fs::read_to_string(fixture.root.join(".git/child-stat"))?;
            let (pid, start, _) = witness_identity(&original)?;
            let process = Path::new("/proc").join(pid.to_string()).join("stat");
            let live = || -> TestResult<bool> {
                let stat = fs::read_to_string(&process);
                witness_is_live(stat.as_deref(), pid, start)
            };
            let observed = std::time::Instant::now();
            while live()? && observed.elapsed() < std::time::Duration::from_secs(1) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let failed_termination = live()?;
            // No numeric-PID signal: even checking start time before kill races
            // PID reuse. Let this finite witness expire, preserving the failure.
            while live()? && observed.elapsed() < std::time::Duration::from_secs(10) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if live()? {
                return Err("witness still live after finite-expiration bound".into());
            }
            Ok(failed_termination)
        })();
        let failed_termination = match observation {
            Ok(failed) => failed,
            Err(error) => {
                let retained = fixture.root.clone();
                // Uncertain ownership/liveness is not permission to delete an
                // in-use fixture. Preserve it for diagnosis instead of signaling.
                fixture.retained = true;
                return Err(format!("{error}; fixture retained at {}", retained.display()).into());
            }
        };
        // Assert only after the witness is no longer live. An orphan zombie may
        // await PID 1; this test promises termination, not that we reaped it.
        let output = output?;
        assert!(
            !failed_termination,
            "timeout left a live owned descendant (stubborn={stubborn})"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(4),
            "timeout observation waited for natural witness expiry: {elapsed:?}"
        );
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
    }
    Ok(())
}

#[test]
fn fixture_retention_preserves_only_explicitly_retained_evidence() -> TestResult {
    for retained in [false, true] {
        let mut fixture = Fixture::new()?;
        let root = fixture.root.clone();
        fixture.retained = retained;
        drop(fixture);
        assert_eq!(root.exists(), retained);
        if retained {
            fs::remove_dir_all(root)?;
        }
    }
    Ok(())
}
