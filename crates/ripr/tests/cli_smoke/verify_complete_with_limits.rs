//! #14821: when exact tests leave only analyzer limits, the agent record ends
//! `verify=complete_with_limits|...|limits=...`, so the documented loop can
//! finish in DONE with those limits disclosed. The same diff under tests that
//! only smoke-check (`is_ok()`) or assert nothing keeps `verify=refused`.
use super::{ignore_remove_dir_all, run_command, run_git, unique_temp_workspace};
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const COMMITTER: [&str; 4] = [
    "-c",
    "user.name=RIPR test",
    "-c",
    "user.email=ripr@example.invalid",
];

const LIB_BASE: &str = r#"#[derive(Debug, PartialEq, Eq)]
pub enum TakeError {
    Insufficient { available: u64 },
}

pub const DEFAULT_RATE: u64 = 10;

pub struct Bucket {
    tokens: u64,
    capacity: u64,
    rate: u64,
}

impl Bucket {
    pub fn new(capacity: u64) -> Self {
        Bucket { tokens: capacity, capacity, rate: DEFAULT_RATE }
    }

    pub fn refill(&mut self, elapsed_ms: u64) {
        let added = elapsed_ms * self.rate / 100;
        self.tokens = (self.tokens + added).min(self.capacity);
    }

    pub fn try_take(&mut self, cost: u64) -> Result<(), TakeError> {
        if cost == 0 {
            return Ok(());
        }
        if self.tokens <= cost {
            return Err(TakeError::Insufficient { available: self.tokens });
        }
        self.tokens -= cost;
        Ok(())
    }

    pub fn available(&self) -> u64 {
        self.tokens
    }
}
"#;

const REFERENCE_TESTS: &str = r#"use ratelimit::{Bucket, TakeError};

#[test]
fn drains_exactly_to_zero() {
    let mut b = Bucket::new(5);
    assert_eq!(b.try_take(5), Ok(()));
    assert_eq!(b.try_take(1), Err(TakeError::Insufficient { available: 0 }));
}

#[test]
fn refill_precision_and_cap() {
    let mut b = Bucket::new(100);
    assert_eq!(b.try_take(100), Ok(()));
    b.refill(1500);
    assert_eq!(b.available(), 15);
}
"#;

const SMOKE_TESTS: &str = r#"use ratelimit::Bucket;

#[test]
fn smoke() {
    let mut b = Bucket::new(5);
    assert!(b.try_take(1).is_ok());
    b.refill(10);
}
"#;

const CALL_ONLY_TESTS: &str = r#"use ratelimit::Bucket;

#[test]
fn calls_only() {
    let mut b = Bucket::new(5);
    let _ = b.try_take(1);
    b.refill(10);
}
"#;

/// A committed `main` with `LIB_BASE` and `tests`, then a `feature` commit
/// that fixes the refill precision (`/ 100` -> `/ 1000`) and the drain
/// boundary (`<=` -> `<`): the two changed lines read `static_unknown` and
/// `infection_unknown`.
fn fixture(label: &str, tests: &str) -> TestResult<PathBuf> {
    let root = unique_temp_workspace(label);
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("tests"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"ratelimit\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    std::fs::write(root.join(".gitignore"), "/target/\n")?;
    std::fs::write(root.join("src/lib.rs"), LIB_BASE)?;
    std::fs::write(root.join("tests/reference.rs"), tests)?;
    run_git(&root, &["init", "-q", "-b", "main"])?;
    run_git(&root, &["config", "core.autocrlf", "false"])?;
    run_git(&root, &["add", "-A"])?;
    run_git(
        &root,
        &[&COMMITTER[..], &["commit", "-qm", "base"]].concat(),
    )?;
    run_git(&root, &["checkout", "-q", "-b", "feature"])?;
    let head = LIB_BASE
        .replace(
            "elapsed_ms * self.rate / 100;",
            "elapsed_ms * self.rate / 1000;",
        )
        .replace("self.tokens <= cost", "self.tokens < cost");
    std::fs::write(root.join("src/lib.rs"), head)?;
    run_git(&root, &["add", "-A"])?;
    run_git(
        &root,
        &[&COMMITTER[..], &["commit", "-qm", "change"]].concat(),
    )?;
    Ok(root)
}

fn agent_report(root: &Path) -> TestResult<(Option<i32>, String)> {
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
        &["check", "--base", "main", "--quiet", "--format", "agent"],
    )?;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    ))
}

fn last_line(report: &str) -> &str {
    report.lines().last().unwrap_or("")
}

#[test]
fn exact_tests_leaving_only_analyzer_limits_complete_with_limits() -> TestResult {
    let root = fixture("verify-limits-reference", REFERENCE_TESTS)?;
    let result = agent_report(&root);
    ignore_remove_dir_all(&root);
    let (code, report) = result?;
    assert_eq!(code, Some(0), "{report}");
    // Fixture premise: no test-side gap, and both changed lines are limits.
    assert!(
        report.starts_with("#ripr agent static-advisory base=main findings=2 gaps=0 "),
        "{report}"
    );
    assert!(
        report.contains("\nlimits=infection_unknown:1,static_unknown:1\n"),
        "{report}"
    );
    assert_eq!(
        last_line(&report),
        "verify=complete_with_limits|outcome=complete_with_findings|findings=2|exposed=0|limits=infection_unknown:1,static_unknown:1",
        "{report}"
    );
    Ok(())
}

#[test]
fn smoke_or_assertion_free_tests_over_the_same_limits_still_refuse() -> TestResult {
    for (label, tests) in [
        ("verify-limits-smoke", SMOKE_TESTS),
        ("verify-limits-calls-only", CALL_ONLY_TESTS),
    ] {
        let root = fixture(label, tests)?;
        let result = agent_report(&root);
        ignore_remove_dir_all(&root);
        let (code, report) = result?;
        assert_eq!(code, Some(0), "{label}: {report}");
        // Same premise as the reference tree: gaps=0 and the same limits.
        assert!(
            report.contains(" findings=2 gaps=0 ")
                && report.contains("\nlimits=infection_unknown:1,static_unknown:1\n"),
            "{label}: {report}"
        );
        assert_eq!(
            last_line(&report),
            "verify=refused|outcome=complete_with_findings|findings=2|exposed=0|reason=unexposed_findings_remain",
            "{label}: {report}"
        );
    }
    Ok(())
}
