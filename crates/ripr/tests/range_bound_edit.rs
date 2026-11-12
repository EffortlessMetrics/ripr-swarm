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

const UPTO: &str = "/// The values below the bound.\npub fn upto(n: u32) -> Vec<u32> {\n    (0..=n).collect()\n}\n";

#[test]
fn a_collected_range_keeps_the_ordinary_reading() -> Result<(), String> {
    // `.collect()` preserves length, so one element more shows at every
    // non-empty input; the gate leaves this shape to the ordinary reading.
    let findings = findings_on_changed_line(
        UPTO,
        "    (0..=n).collect()",
        "    (0..n).collect()",
        &test_calling("assert_eq!(upto(2), vec![0, 1, 2]);"),
    )?;
    assert_eq!(classes(&findings), vec![ExposureClass::Exposed]);
    Ok(())
}

#[test]
fn a_wrapper_around_the_owner_call_does_not_credit_a_range_bound() -> Result<(), String> {
    // 0..3 sums to 3 and 0..=3 to 6: both are positive, so the wrapper
    // folds the separating input back into one outcome.
    let test = "use ranges::*;\n\nfn is_positive(value: u32) -> bool {\n    value > 0\n}\n\n#[test]\nfn pins() {\n    assert!(is_positive(sum_to(3)));\n}\n";
    let findings = findings_on_changed_line(
        &sum_to("(0..=n).sum()"),
        "    (0..=n).sum()",
        "    (0..n).sum()",
        test,
    )?;
    assert!(
        classes(&findings)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{findings:?}"
    );
    assert_ne!(infection_states(&findings), vec!["Yes".to_string()]);
    Ok(())
}

#[test]
fn a_self_comparison_does_not_credit_a_range_bound() -> Result<(), String> {
    let findings = findings_on_changed_line(
        &sum_to("(0..=n).sum()"),
        "    (0..=n).sum()",
        "    (0..n).sum()",
        &test_calling("assert_eq!(sum_to(5), sum_to(5));"),
    )?;
    assert_ne!(infection_states(&findings), vec!["Yes".to_string()]);
    Ok(())
}

#[test]
fn a_moved_match_range_start_needs_an_input_between_the_bounds() -> Result<(), String> {
    // `1..5` and `0..5` differ only at 0; `tier(3)` reads 1 on both.
    let between = findings_on_changed_line(
        TIER,
        "        0..5 => 1,",
        "        1..5 => 1,",
        &test_calling("assert_eq!(tier(3), 1);"),
    )?;
    assert!(
        classes(&between)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{between:?}"
    );
    // The arm head carries the change; the removed old arm keeps its own
    // deleted-behavior reading, unchanged from before this gate.
    let head_infection = |findings: &[Finding]| {
        findings
            .iter()
            .filter(|finding| finding.probe.expression == "0..5 =>")
            .map(|finding| format!("{:?}", finding.ripr.infect.state))
            .collect::<Vec<_>>()
    };
    assert_eq!(head_infection(&between), vec!["Weak".to_string()]);
    let moved = findings_on_changed_line(
        TIER,
        "        0..5 => 1,",
        "        1..5 => 1,",
        &test_calling("assert_eq!(tier(0), 1);"),
    )?;
    assert_eq!(head_infection(&moved), vec!["Yes".to_string()]);
    Ok(())
}

#[test]
fn another_caller_keeps_agreeing_inputs_unknown() -> Result<(), String> {
    // A helper may feed the owner inputs no test names, so "every asserted
    // input agrees" is not claimed.
    let lib = "/// Sum below the bound.\npub fn sum_to(n: u32) -> u32 {\n    (0..=n).sum()\n}\n\n/// Twice the sum.\npub fn twice(n: u32) -> u32 {\n    sum_to(n) * 2\n}\n";
    let findings = findings_on_changed_line(
        lib,
        "    (0..=n).sum()",
        "    (0..n).sum()",
        &test_calling("assert_eq!(sum_to(0), 0);"),
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
fn a_scoped_import_of_the_owner_name_does_not_supply_a_separating_input() -> Result<(), String> {
    let lib = "/// Sum below the bound.\npub fn sum_to(n: u32) -> u32 {\n    (0..=n).sum()\n}\n\npub mod rival {\n    pub fn sum_to(_n: u32) -> u32 {\n        6\n    }\n}\n";
    let test = "use ranges::*;\n\n#[test]\nfn pins() {\n    assert_eq!(sum_to(0), 0);\n    {\n        use ranges::rival::sum_to;\n        assert_eq!(sum_to(3), 6);\n    }\n}\n";
    let findings = findings_on_changed_line(lib, "    (0..=n).sum()", "    (0..n).sum()", test)?;
    assert!(
        classes(&findings)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{findings:?}"
    );
    assert_ne!(infection_states(&findings), vec!["Yes".to_string()]);
    Ok(())
}

#[test]
fn a_match_arm_with_intermediate_arithmetic_is_not_folded_or_contradicted() -> Result<(), String> {
    // At n = 255 `n + 1` wraps to 0 in unchecked u8 arithmetic, so the
    // passing assertion must not be read against an untyped 128.
    let lib = "/// Bucket a byte.\npub fn bucket(n: u8) -> u8 {\n    match n {\n        200..=255 => (n + 1) / 2,\n        _ => 0,\n    }\n}\n";
    let findings = findings_on_changed_line(
        lib,
        "        200..=255 => (n + 1) / 2,",
        "        201..=255 => (n + 1) / 2,",
        &test_calling("assert_eq!(bucket(255), 0);"),
    )?;
    assert!(
        classes(&findings)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{findings:?}"
    );
    let contradicted = findings
        .iter()
        .any(|finding| format!("{finding:?}").contains("contradicts static evaluation"));
    assert!(!contradicted, "{findings:?}");
    Ok(())
}

#[test]
fn a_range_in_a_match_arm_body_is_gated() -> Result<(), String> {
    // The arrow on this arm line is not a bare arm head, so the line is
    // compared whole and its range is found in the arm body. An unread
    // input keeps an earlier fold from settling it.
    let lib = "/// Sum or zero.\npub fn pick(k: u32, n: u32) -> u32 {\n    match k {\n        1 => 0,\n        _ => (0..=n).sum(),\n    }\n}\n";
    let findings = findings_on_changed_line(
        lib,
        "        _ => (0..=n).sum(),",
        "        _ => (0..n).sum(),",
        &test_calling("let k = std::env::args().count() as u32;\n    assert_eq!(pick(2, k), 0);"),
    )?;
    assert!(
        classes(&findings)
            .iter()
            .all(|class| *class != ExposureClass::Exposed),
        "{findings:?}"
    );
    assert!(
        !infection_states(&findings).contains(&"Yes".to_string()),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_separating_input_on_a_match_arm_body_range_is_unknown_not_weak() -> Result<(), String> {
    // Known limit: the fold reads only plain arm values, so an arm-body range
    // reduction cannot be evaluated. A separating input then stays Unknown,
    // never the "every input agrees" Weak reading and never Yes.
    let lib = "/// Sum or zero.\npub fn pick(k: u32, n: u32) -> u32 {\n    match k {\n        1 => 0,\n        _ => (0..=n).sum(),\n    }\n}\n";
    let findings = findings_on_changed_line(
        lib,
        "        _ => (0..=n).sum(),",
        "        _ => (0..n).sum(),",
        &test_calling("assert_eq!(pick(2, 3), 6);"),
    )?;
    assert_eq!(infection_states(&findings), vec!["Unknown".to_string()]);
    Ok(())
}

#[test]
fn a_glob_import_in_the_test_body_does_not_supply_a_separating_input() -> Result<(), String> {
    let lib = "/// Sum below the bound.\npub fn sum_to(n: u32) -> u32 {\n    (0..=n).sum()\n}\n\npub mod rival {\n    pub fn sum_to(_n: u32) -> u32 {\n        6\n    }\n}\n";
    let test = "use ranges::*;\n\n#[test]\nfn pins() {\n    assert_eq!(sum_to(0), 0);\n    {\n        use ranges::rival::*;\n        assert_eq!(sum_to(3), 6);\n    }\n}\n";
    let findings = findings_on_changed_line(lib, "    (0..=n).sum()", "    (0..n).sum()", test)?;
    assert_ne!(infection_states(&findings), vec!["Yes".to_string()]);
    Ok(())
}

#[test]
fn an_arrow_inside_a_comment_does_not_hide_a_range_bound() -> Result<(), String> {
    // An unread input keeps an earlier fold from settling it: the gate itself
    // must still read the change as a range-bound change.
    let findings = findings_on_changed_line(
        &sum_to("(/* => */ 0..=n).sum()"),
        "    (/* => */ 0..=n).sum()",
        "    (/* => */ 0..n).sum()",
        &test_calling("let k = std::env::args().count() as u32;\n    assert_eq!(sum_to(k), 0);"),
    )?;
    assert_eq!(infection_states(&findings), vec!["Unknown".to_string()]);
    Ok(())
}
