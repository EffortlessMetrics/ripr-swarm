//! End-to-end witness for #15343 and #15344: a predicate boundary change
//! whose related test feeds the owner through an input ripr does not read
//! (an rstest `#[fixture]` parameter, a test-local macro that calls the
//! owner) reads infection unknown, never a false "no boundary input" gap.
//! Readable inputs keep their reading.

use ripr::domain::StageState;
use ripr::{CheckInput, CheckOutput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// `lib` is the new `src/lib.rs` and `files` the other sources by path;
    /// `diff` changes one line of one of them.
    fn create(lib: &str, files: &[(&str, &str)], test: &str, diff: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests share one
        // `SystemTime` on some platforms.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-unread-boundary-input-{}-{sequence}",
            std::process::id()
        ));
        // Built before any write so `Drop` removes a partly written root.
        let repo = Self { root };
        let root = &repo.root;
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        let write = |path: &str, text: &str| {
            if let Some(parent) = root.join(path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"pricing\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", lib)?;
        for (path, text) in files {
            write(path, text)?;
        }
        write("tests/it.rs", test)?;
        write("diff.patch", diff)?;
        Ok(repo)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn diff_at(path: &str, text: &str, new: &str, old: &str) -> Result<(String, usize), String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut matches = lines.iter().enumerate().filter(|(_, line)| **line == new);
    let (index, _) = matches
        .next()
        .ok_or_else(|| format!("{path} has no line {new:?}"))?;
    if matches.next().is_some() || index < 3 || index + 3 > lines.len() {
        return Err(format!(
            "{path} needs one {new:?} with three lines before and two after"
        ));
    }
    let context = |range: std::ops::Range<usize>| {
        lines[range]
            .iter()
            .map(|line| format!(" {line}\n"))
            .collect::<String>()
    };
    let start = index - 2;
    Ok((
        format!(
            "diff --git a/{path} b/{path}\nindex 0000000..1111111 100644\n--- a/{path}\n+++ b/{path}\n@@ -{start},6 +{start},6 @@\n{}-{old}\n+{new}\n{}",
            context(index - 3..index),
            context(index + 1..index + 3)
        ),
        index + 1,
    ))
}

const LIB: &str = "/// Discount for an order.\n///\n/// Ten off at or above the threshold.\npub fn discount(amount: u32) -> u32 {\n    if amount >= 100 { 10 } else { 0 }\n}\n\npub fn tail() {}\n";

/// The predicate findings on the changed boundary for `test` as `tests/it.rs`.
fn boundary_findings(test: &str) -> Result<Vec<Finding>, String> {
    let (diff, line) = diff_at(
        "src/lib.rs",
        LIB,
        "    if amount >= 100 { 10 } else { 0 }",
        "    if amount > 100 { 10 } else { 0 }",
    )?;
    let repo = TempRepo::create(LIB, &[], test, &diff)?;
    let output = repo.check()?;
    let findings = output
        .findings
        .into_iter()
        .filter(|finding| {
            finding.probe.location.line == line && finding.probe.family.as_str() == "predicate"
        })
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return Err(format!("no predicate finding on line {line}"));
    }
    Ok(findings)
}

const ABSENT_INPUT: &str = "No detected boundary input for the changed predicate";

fn infection_states(findings: &[Finding]) -> Vec<StageState> {
    findings
        .iter()
        .map(|finding| finding.ripr.infect.state.clone())
        .collect()
}

#[test]
fn an_rstest_fixture_feeding_the_owner_reads_infection_unknown() -> Result<(), String> {
    let findings = boundary_findings(
        "use pricing::discount;\nuse rstest::{fixture, rstest};\n\n#[fixture]\nfn amount() -> u32 {\n    100\n}\n\n#[rstest]\nfn at_threshold(amount: u32) {\n    assert_eq!(discount(amount), 10);\n}\n",
    )?;
    assert_eq!(
        infection_states(&findings),
        vec![StageState::Unknown],
        "{findings:?}"
    );
    assert_eq!(
        findings[0].missing,
        vec![findings[0].ripr.infect.summary.clone()]
    );
    let unresolved = boundary_findings(
        "use pricing::discount;\nuse rstest::{fixture, rstest};\n#[fixture]\nfn amount() -> u32 { std::env::var(\"AMOUNT\").unwrap().parse().unwrap() }\n#[rstest]\nfn at_threshold(amount: u32) { assert_eq!(discount(amount), 0); }\n",
    )?;
    assert_eq!(infection_states(&unresolved), vec![StageState::Unknown]);
    assert_eq!(
        unresolved[0].missing,
        vec![unresolved[0].ripr.infect.summary.clone()]
    );
    let literal = boundary_findings(
        "use pricing::discount;\n#[test]\nfn at_threshold() { assert_eq!(discount(100), 10); }\n",
    )?;
    assert_eq!(infection_states(&literal), vec![StageState::Yes]);
    Ok(())
}

#[test]
fn a_test_local_macro_calling_the_owner_reads_infection_unknown() -> Result<(), String> {
    let findings = boundary_findings(
        "use pricing::discount;\n\n#[test]\nfn at_threshold() {\n    macro_rules! d {\n        ($x:expr) => {\n            discount($x)\n        };\n    }\n    assert_eq!(d!(100), 10);\n}\n",
    )?;
    assert_eq!(
        infection_states(&findings),
        vec![StageState::Unknown],
        "{findings:?}"
    );
    // The unknown reading names its cause; no absent-input line beside it.
    assert!(
        !findings[0].missing.iter().any(|line| line == ABSENT_INPUT),
        "{:?}",
        findings[0].missing
    );
    Ok(())
}

/// Controls: a readable off-boundary input, and a fixture that does not
/// feed the owner, keep the gap.
#[test]
fn readable_inputs_keep_the_weak_boundary_gap() -> Result<(), String> {
    for test in [
        "use pricing::discount;\n\n#[test]\nfn below() {\n    assert_eq!(discount(50), 0);\n}\n",
        "use pricing::discount;\nuse rstest::{fixture, rstest};\n\n#[fixture]\nfn scale() -> u32 {\n    1\n}\n\n#[rstest]\nfn below(scale: u32) {\n    assert_eq!(discount(50) * scale, 0);\n}\n",
    ] {
        let findings = boundary_findings(test)?;
        assert_eq!(
            infection_states(&findings),
            vec![StageState::Weak],
            "{test}\n{findings:?}"
        );
    }
    Ok(())
}
