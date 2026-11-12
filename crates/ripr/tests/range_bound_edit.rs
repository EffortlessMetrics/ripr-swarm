//! End-to-end witness for changed range bounds (#14419).
//!
//! A changed range bound (`0..n` to `0..=n`, `1..n` to `0..n`, a match range
//! pattern `0..=5` to `0..5`) changes the owner's result only at inputs where
//! the old and new ranges give different values: `(0..n).sum()` and
//! `(0..=n).sum()` are both 0 at `n = 0`. A test that pins an exact value at
//! such an input passes on both versions, so it is not credited as exposing
//! the change. A test whose asserted input does separate the ranges is.
//! When ripr cannot evaluate the owner at the test's inputs, the change is not
//! credited either.

use ripr::{CheckInput, CheckOutput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, test: &str, diff: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests share one
        // `SystemTime` on some platforms.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-range-bound-edit-{}-{sequence}",
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
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"ranges\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", lib)?;
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

/// A one-line diff of `src/lib.rs` from `old` to `new`, and the changed line.
fn diff_at(text: &str, new: &str, old: &str) -> Result<(String, usize), String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut matches = lines.iter().enumerate().filter(|(_, line)| **line == new);
    let (index, _) = matches
        .next()
        .ok_or_else(|| format!("src/lib.rs has no line {new:?}"))?;
    if matches.next().is_some() || index < 1 || index + 2 > lines.len() {
        return Err(format!(
            "src/lib.rs needs one {new:?} with a line before and one after"
        ));
    }
    let start = index;
    Ok((
        format!(
            "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{start},3 +{start},3 @@\n {}\n-{old}\n+{new}\n {}\n",
            lines[index - 1],
            lines[index + 1]
        ),
        index + 1,
    ))
}

/// The findings on the changed line of `src/lib.rs` after `old` -> `new`.
fn findings_on_changed_line(
    lib: &str,
    new: &str,
    old: &str,
    test: &str,
) -> Result<Vec<Finding>, String> {
    let (diff, line) = diff_at(lib, new, old)?;
    let repo = TempRepo::create(lib, test, &diff)?;
    let output = repo.check()?;
    let findings: Vec<Finding> = output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect();
    if findings.is_empty() {
        return Err(format!("no finding on line {line}"));
    }
    Ok(findings)
}

fn classes(findings: &[Finding]) -> Vec<ExposureClass> {
    findings
        .iter()
        .map(|finding| finding.class.clone())
        .collect()
}

fn infection_states(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| format!("{:?}", finding.ripr.infect.state))
        .collect()
}

fn test_calling(body: &str) -> String {
    format!("use ranges::*;\n\n#[test]\nfn pins() {{\n    {body}\n}}\n")
}

fn sum_to(expression: &str) -> String {
    format!("/// Sum below the bound.\npub fn sum_to(n: u32) -> u32 {{\n    {expression}\n}}\n")
}

#[test]
fn an_inclusive_sum_bound_is_not_credited_by_an_input_where_both_ranges_agree() -> Result<(), String>
{
    // The #14419 repro: both ranges sum to 0 at n = 0.
    let findings = findings_on_changed_line(
        &sum_to("(0..=n).sum()"),
        "    (0..=n).sum()",
        "    (0..n).sum()",
        &test_calling("assert_eq!(sum_to(0), 0);"),
    )?;
    assert_eq!(classes(&findings), vec![ExposureClass::WeaklyExposed]);
    assert_eq!(infection_states(&findings), vec!["Weak".to_string()]);
    Ok(())
}

#[test]
fn an_inclusive_sum_bound_is_credited_by_an_input_that_separates_the_ranges() -> Result<(), String>
{
    // n = 4: 0..4 sums to 6 and 0..=4 to 10.
    let findings = findings_on_changed_line(
        &sum_to("(0..=n).sum()"),
        "    (0..=n).sum()",
        "    (0..n).sum()",
        &test_calling("assert_eq!(sum_to(4), 10);"),
    )?;
    assert_eq!(classes(&findings), vec![ExposureClass::Exposed]);
    Ok(())
}

#[test]
fn a_relational_check_at_a_separating_input_stays_weak() -> Result<(), String> {
    let findings = findings_on_changed_line(
        &sum_to("(0..=n).sum()"),
        "    (0..=n).sum()",
        "    (0..n).sum()",
        &test_calling("assert!(sum_to(4) > 0);"),
    )?;
    assert_eq!(classes(&findings), vec![ExposureClass::WeaklyExposed]);
    Ok(())
}

#[test]
fn an_unread_input_does_not_credit_a_changed_range_bound() -> Result<(), String> {
    let findings = findings_on_changed_line(
        &sum_to("(0..n).sum()"),
        "    (0..n).sum()",
        "    (0..=n).sum()",
        &test_calling("let k = std::env::args().count() as u32;\n    assert_eq!(sum_to(k), 0);"),
    )?;
    assert!(
        classes(&findings)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{findings:?}"
    );
    assert_eq!(infection_states(&findings), vec!["Unknown".to_string()]);
    Ok(())
}

#[test]
fn a_count_bound_is_credited_where_the_empty_range_gains_an_element() -> Result<(), String> {
    // Unlike the sum, the count does separate at n = 0: 0 versus 1.
    let findings = findings_on_changed_line(
        &sum_to("(0..=n).count() as u32"),
        "    (0..=n).count() as u32",
        "    (0..n).count() as u32",
        &test_calling("assert_eq!(sum_to(0), 1);"),
    )?;
    assert_eq!(classes(&findings), vec![ExposureClass::Exposed]);
    Ok(())
}

#[test]
fn an_equivalent_range_start_edit_is_not_credited() -> Result<(), String> {
    // `1..n` and `0..n` sum alike at every n: no test can tell them apart.
    let findings = findings_on_changed_line(
        &sum_to("(0..n).sum()"),
        "    (0..n).sum()",
        "    (1..n).sum()",
        &test_calling("assert_eq!(sum_to(3), 3);"),
    )?;
    assert!(
        classes(&findings)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

fn inside(expression: &str) -> String {
    format!(
        "/// Whether `x` is in range.\npub fn inside(x: u32) -> bool {{\n    {expression}\n}}\n"
    )
}

#[test]
fn a_membership_bound_needs_the_endpoint_input() -> Result<(), String> {
    let lib = inside("(0..=10).contains(&x)");
    let off_boundary = findings_on_changed_line(
        &lib,
        "    (0..=10).contains(&x)",
        "    (0..10).contains(&x)",
        &test_calling("assert!(inside(3));"),
    )?;
    assert_eq!(classes(&off_boundary), vec![ExposureClass::WeaklyExposed]);
    let on_boundary = findings_on_changed_line(
        &lib,
        "    (0..=10).contains(&x)",
        "    (0..10).contains(&x)",
        &test_calling("assert!(inside(10));"),
    )?;
    assert_eq!(classes(&on_boundary), vec![ExposureClass::Exposed]);
    Ok(())
}

const TIER: &str = "/// The tier of `x`.\npub fn tier(x: u32) -> u32 {\n    match x {\n        0..5 => 1,\n        _ => 2,\n    }\n}\n";

#[test]
fn a_match_range_pattern_bound_needs_the_endpoint_input() -> Result<(), String> {
    let inside_both = findings_on_changed_line(
        TIER,
        "        0..5 => 1,",
        "        0..=5 => 1,",
        &test_calling("assert_eq!(tier(3), 1);"),
    )?;
    assert!(
        classes(&inside_both)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{inside_both:?}"
    );
    assert_eq!(infection_states(&inside_both), vec!["Weak".to_string()]);
    let endpoint = findings_on_changed_line(
        TIER,
        "        0..5 => 1,",
        "        0..=5 => 1,",
        &test_calling("assert_eq!(tier(5), 2);"),
    )?;
    assert_eq!(classes(&endpoint), vec![ExposureClass::Exposed]);
    Ok(())
}

#[test]
fn a_slice_range_feeding_a_sum_is_not_credited_without_a_fold() -> Result<(), String> {
    // The extra element is 0 here, so the test passes on both versions; ripr
    // cannot fold the slice and must not credit it.
    let lib = "/// Sum the first `n` values.\npub fn total(v: &[u32], n: usize) -> u32 {\n    v[..=n].iter().sum()\n}\n";
    let findings = findings_on_changed_line(
        lib,
        "    v[..=n].iter().sum()",
        "    v[..n].iter().sum()",
        &test_calling("assert_eq!(total(&[0, 0, 5], 1), 0);"),
    )?;
    assert!(
        classes(&findings)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{findings:?}"
    );
    assert_eq!(infection_states(&findings), vec!["Unknown".to_string()]);
    Ok(())
}
