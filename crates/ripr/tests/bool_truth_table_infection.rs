//! End-to-end witness for RIPR-SPEC-0353 (#6488): a pure bool `&&`/`||`
//! owner has no literal boundary, so its infection is read from the truth
//! table the directly-called literal rows give. Rows that separate the chain
//! from its one-step mutations read `yes`; anything else keeps its reading.

use ripr::domain::StageState;
use ripr::{CheckInput, CheckOutput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
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
            "ripr-bool-truth-table-{}-{sequence}",
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
            "[package]\nname = \"truth_table\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The findings on `line` of `src/lib.rs` after applying `old` -> `new`.
fn findings_on_changed_line(
    lib: &str,
    new: &str,
    old: &str,
    test: &str,
) -> Result<Vec<Finding>, String> {
    let (diff, line) = diff_at("src/lib.rs", lib, new, old)?;
    let repo = TempRepo::create(lib, &[], test, &diff)?;
    let output = repo.check()?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect())
}

fn lib_with(owner_body: &str, extra: &str) -> String {
    format!(
        "/// Ship gate.\n///\n/// Paid and in stock.\npub fn can_ship(paid: bool, in_stock: bool) -> bool {{\n{owner_body}}}\n{extra}\npub fn tail() {{}}\n"
    )
}

fn test(calls: &[(&str, bool)]) -> String {
    let body = calls
        .iter()
        .map(|(args, want)| format!("    assert_eq!(can_ship({args}), {want});\n"))
        .collect::<String>();
    format!("use truth_table::can_ship;\n\n#[test]\nfn ships() {{\n{body}}}\n")
}

/// The predicate findings on the reordered chain.
fn predicate_findings(lib: &str, test: &str) -> Result<Vec<Finding>, String> {
    let findings =
        findings_on_changed_line(lib, "    in_stock && paid", "    paid && in_stock", test)?;
    let predicates = findings
        .into_iter()
        .filter(|finding| finding.probe.family.as_str() == "predicate")
        .collect::<Vec<_>>();
    assert!(!predicates.is_empty(), "no predicate finding");
    Ok(predicates)
}

fn infection_credited(findings: &[Finding]) -> bool {
    findings
        .iter()
        .any(|finding| finding.ripr.infect.state == StageState::Yes)
}

const THREE_ROWS: &[(&str, bool)] = &[
    ("true, true", true),
    ("true, false", false),
    ("false, true", false),
];

#[test]
fn rows_that_separate_the_chain_from_every_one_step_mutant_credit_infection() -> Result<(), String>
{
    let findings = predicate_findings(&lib_with("    in_stock && paid\n", ""), &test(THREE_ROWS))?;
    assert!(infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn an_or_chain_pinned_on_its_own_table_credits_infection() -> Result<(), String> {
    let lib = "/// Review gate.\n///\n/// Flagged or large.\npub fn can_ship(paid: bool, in_stock: bool) -> bool {\n    in_stock || paid\n}\n\npub fn tail() {}\n";
    let findings = findings_on_changed_line(
        lib,
        "    in_stock || paid",
        "    paid || in_stock",
        &test(&[
            ("false, false", false),
            ("true, false", true),
            ("false, true", true),
        ]),
    )?;
    let predicates = findings
        .into_iter()
        .filter(|finding| finding.probe.family.as_str() == "predicate")
        .collect::<Vec<_>>();
    assert!(infection_credited(&predicates), "{predicates:?}");
    Ok(())
}

#[test]
fn rows_that_cannot_tell_the_connectives_apart_stay_unknown() -> Result<(), String> {
    let findings = predicate_findings(
        &lib_with("    in_stock && paid\n", ""),
        &test(&[("true, true", true), ("false, false", false)]),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_single_row_stays_unknown() -> Result<(), String> {
    let findings = predicate_findings(
        &lib_with("    in_stock && paid\n", ""),
        &test(&[("true, false", false)]),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn another_caller_of_the_owner_stays_unknown() -> Result<(), String> {
    let wrapper = "\npub fn wrapped(paid: bool) -> bool {\n    can_ship(paid, true)\n}\n";
    let findings = predicate_findings(
        &lib_with("    in_stock && paid\n", wrapper),
        &test(THREE_ROWS),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn rows_fed_through_a_table_stay_unknown() -> Result<(), String> {
    let table = "use truth_table::can_ship;\n\n#[test]\nfn ships() {\n    let rows = [(true, true, true), (true, false, false), (false, true, false)];\n    for (paid, in_stock, want) in rows {\n        assert_eq!(can_ship(paid, in_stock), want);\n    }\n}\n";
    let findings = predicate_findings(&lib_with("    in_stock && paid\n", ""), table)?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_body_with_a_leading_statement_stays_unknown() -> Result<(), String> {
    let findings = predicate_findings(
        &lib_with("    let paid = !paid;\n    in_stock && paid\n", ""),
        &test(THREE_ROWS),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

/// The `return_value` findings on the reordered chain.
fn return_value_findings(lib: &str, test: &str) -> Result<Vec<Finding>, String> {
    let findings =
        findings_on_changed_line(lib, "    in_stock && paid", "    paid && in_stock", test)?;
    let returned = findings
        .into_iter()
        .filter(|finding| finding.probe.family.as_str() == "return_value")
        .collect::<Vec<_>>();
    assert!(!returned.is_empty(), "no return_value finding");
    Ok(returned)
}

#[test]
fn exercised_rows_complete_the_returned_tail_and_read_exposed() -> Result<(), String> {
    let findings =
        return_value_findings(&lib_with("    in_stock && paid\n", ""), &test(THREE_ROWS))?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed
                && finding.ripr.propagate.state == StageState::Yes),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_loop_fed_argument_never_completes_the_returned_tail() -> Result<(), String> {
    // The surviving mutant `banned && has_ticket` shape: the loop varies one
    // argument and the other is pinned, so no literal row pair separates the
    // chain; the returned tail must not read exposed (grid-bool-property).
    let looped = "use truth_table::can_ship;\n\n#[test]\nfn ships() {\n    for in_stock in [true, false] {\n        assert!(!can_ship(false, in_stock));\n    }\n}\n";
    let findings = return_value_findings(&lib_with("    in_stock && paid\n", ""), looped)?;
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_macro_that_names_the_owner_stays_unknown() -> Result<(), String> {
    let makes =
        "\nmacro_rules! ship {\n    ($a:expr, $b:expr) => {\n        can_ship($a, $b)\n    };\n}\n";
    let findings = predicate_findings(
        &lib_with("    in_stock && paid\n", makes),
        &test(THREE_ROWS),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

/// A `macro_rules! assert_eq` that compares nothing (#10386).
const SHADOW_ASSERT_EQ: &str = "macro_rules! assert_eq {\n    ($actual:expr, $want:expr) => {{\n        let _ = $actual;\n    }};\n}\n\n";

#[test]
fn a_shadowed_assert_eq_in_the_test_file_credits_nothing() -> Result<(), String> {
    let shadowed = format!(
        "use truth_table::can_ship;\n\n{SHADOW_ASSERT_EQ}#[test]\nfn ships() {{\n    assert_eq!(can_ship(true, true), true);\n    assert_eq!(can_ship(true, false), false);\n    assert_eq!(can_ship(false, true), false);\n}}\n"
    );
    let lib = lib_with("    in_stock && paid\n", "");
    let predicates = predicate_findings(&lib, &shadowed)?;
    assert!(!infection_credited(&predicates), "{predicates:?}");
    let returned = return_value_findings(&lib, &shadowed)?;
    assert!(
        returned
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{returned:?}"
    );
    // The same rows with the std macro still credit, so the decline comes
    // from the rebinding and not from the fixture.
    assert!(infection_credited(&predicate_findings(
        &lib,
        &test(THREE_ROWS)
    )?));
    Ok(())
}

#[test]
fn a_shadowed_assert_eq_elsewhere_in_the_workspace_credits_nothing() -> Result<(), String> {
    let lib = lib_with("    in_stock && paid\n", &format!("\n{SHADOW_ASSERT_EQ}"));
    let predicates = predicate_findings(&lib, &test(THREE_ROWS))?;
    assert!(!infection_credited(&predicates), "{predicates:?}");
    Ok(())
}

/// One live strong row plus a second test that spends the other rows
/// somewhere they do not run or cannot fail.
fn split_rows(second: &str) -> String {
    format!(
        "use truth_table::can_ship;\n\n#[test]\nfn one() {{\n    assert!(can_ship(true, true));\n}}\n\n{second}\n"
    )
}

#[test]
fn rows_that_do_not_run_or_cannot_fail_are_not_counted() -> Result<(), String> {
    for second in [
        "#[test]\n#[ignore]\nfn two() {\n    assert!(!can_ship(true, false));\n    assert!(!can_ship(false, true));\n}",
        "#[test]\nfn two() {\n    if false {\n        assert!(!can_ship(true, false));\n        assert!(!can_ship(false, true));\n    }\n}",
        "#[test]\nfn two() {\n    let _c = || assert!(!can_ship(true, false));\n}",
        "#[test]\nfn two() {\n    let r = can_ship(true, false);\n    assert!(r == r);\n}",
        "#[test]\nfn two() {\n    assert!(!can_ship(true, false) || true);\n    assert!(!can_ship(false, true) || true);\n}",
        "#[test]\nfn two() {\n    assert!(can_ship(true, false) || true);\n}",
        "#[test]\nfn two() {\n    assert!(can_ship(false, true) || (true));\n    assert!(can_ship(true, false) || (true));\n}",
    ] {
        let lib = lib_with("    in_stock && paid\n", "");
        let predicates = predicate_findings(&lib, &split_rows(second))?;
        assert!(!infection_credited(&predicates), "{second}: {predicates:?}");
        let returned = return_value_findings(&lib, &split_rows(second))?;
        assert!(
            returned
                .iter()
                .all(|finding| finding.class != ExposureClass::Exposed),
            "{second}: {returned:?}"
        );
    }
    Ok(())
}

#[test]
fn rows_in_separate_test_files_on_the_same_line_are_both_read() -> Result<(), String> {
    // Both files put their owner calls on the same lines, so a line-only key
    // would merge their arguments and refuse the table.
    let (diff, line) = diff_at(
        "src/lib.rs",
        &lib_with("    in_stock && paid\n", ""),
        "    in_stock && paid",
        "    paid && in_stock",
    )?;
    let lib = lib_with("    in_stock && paid\n", "");
    let one = "use truth_table::can_ship;\n\n#[test]\nfn one() {\n    assert!(can_ship(true, true));\n    assert!(!can_ship(true, false));\n}\n";
    let two = "use truth_table::can_ship;\n\n#[test]\nfn two() {\n    assert!(!can_ship(false, true));\n    assert!(!can_ship(false, false));\n}\n";
    let repo = TempRepo::create(&lib, &[("tests/two.rs", two)], one, &diff)?;
    let findings = repo
        .check()?
        .findings
        .into_iter()
        .filter(|finding| {
            finding.probe.location.line == line && finding.probe.family.as_str() == "predicate"
        })
        .collect::<Vec<_>>();
    assert!(infection_credited(&findings), "{findings:?}");
    Ok(())
}
