//! End-to-end witness for the RIPR-SPEC-0263 error-return amendment (#11147).
//!
//! A changed `return Err(X)` inside `if price > 1_000_000` runs only for
//! prices above the bound. An exact `Err(X)` assertion from a test that takes
//! another branch cannot reveal the change, so while no related test call
//! enters the guard the finding must not read `exposed`. A call that enters
//! the guard, or a changed line in a guard the tests do enter, keeps today's
//! verdict.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const UNENTERED_PREFIX: &str = "No related test call enters guard";

const SOURCE: &str = "#[derive(Debug, PartialEq)]\npub enum PriceError {\n    Negative,\n    TooLarge,\n}\n\npub fn discount(price: i64, qty: u32) -> Result<i64, PriceError> {\n    if price < 0 {\n        return Err(PriceError::Negative);\n    }\n    if price > 1_000_000 {\n        return Err(PriceError::TooLarge);\n    }\n    let rate = if qty >= 10 { 20 } else { 0 };\n    Ok(price - price * rate / 100)\n}\n";

/// The one-line edit at `line` (9 or 12), swapping the returned variant.
fn diff(line: usize) -> String {
    let (guard_line, before, after) = if line == 9 {
        ("    if price < 0 {", "Negative", "TooLarge")
    } else {
        ("    if price > 1_000_000 {", "TooLarge", "Negative")
    };
    let start = line - 1;
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{start},3 +{start},3 @@\n{guard_line}\n-        return Err(PriceError::{before});\n+        return Err(PriceError::{after});\n     }}\n"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(
        test_source: &str,
        line: usize,
        tweak: &dyn Fn(&str) -> String,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-guarded-error-return-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .and_then(|()| std::fs::create_dir_all(root.join("tests")))
            .map_err(|error| format!("create directories failed: {error}"))?;
        let write = |path: &str, text: String| {
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"guarded-error-return\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n".to_string(),
        )?;
        // The post-edit source: the changed line already holds the new variant.
        let edited = if line == 9 {
            SOURCE.replacen("Err(PriceError::Negative)", "Err(PriceError::TooLarge)", 1)
        } else {
            SOURCE.replacen("Err(PriceError::TooLarge)", "Err(PriceError::Negative)", 1)
        };
        write("src/lib.rs", tweak(&edited))?;
        write("tests/discount.rs", test_source.to_string())?;
        write("diff.patch", diff(line))?;
        Ok(Self { root })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const TEST_HEADER: &str = "use guarded_error_return::*;\n\n";

fn findings_with(
    body: &str,
    line: usize,
    tweak: &dyn Fn(&str) -> String,
) -> Result<(PathBuf, Vec<ripr::Finding>, TempRepo), String> {
    let repo = TempRepo::create(&format!("{TEST_HEADER}{body}"), line, tweak)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let found = output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect::<Vec<_>>();
    if found.is_empty() {
        return Err("no finding on the changed return".to_string());
    }
    Ok((repo.root.clone(), found, repo))
}

fn names_guard(finding: &ripr::Finding) -> bool {
    finding
        .activation
        .missing_discriminators
        .iter()
        .any(|fact| fact.reason.starts_with(UNENTERED_PREFIX))
        || finding.ripr.infect.summary.contains(UNENTERED_PREFIX)
}

/// Classes and guard-naming flags of every finding on the changed line.
fn readings(body: &str, line: usize) -> Result<Vec<(ExposureClass, bool)>, String> {
    readings_with(body, line, &|source| source.to_string())
}

fn readings_with(
    body: &str,
    line: usize,
    tweak: &dyn Fn(&str) -> String,
) -> Result<Vec<(ExposureClass, bool)>, String> {
    let (_, found, _repo) = findings_with(body, line, tweak)?;
    Ok(found
        .iter()
        .map(|finding| (finding.class.clone(), names_guard(finding)))
        .collect())
}

const MISSING_TESTS: &str = "#[test]\nfn ok_small() {\n    assert!(discount(100, 1).is_ok());\n}\n\n#[test]\nfn negative() {\n    assert_eq!(discount(-1, 1), Err(PriceError::Negative));\n}\n\n#[test]\nfn bulk() {\n    assert_eq!(discount(100, 20), Ok(80));\n}\n";

#[test]
fn an_exact_variant_assertion_on_another_branch_does_not_read_exposed() -> Result<(), String> {
    let found = readings(MISSING_TESTS, 12)?;
    for (class, named) in &found {
        if *class != ExposureClass::WeaklyExposed || !*named {
            return Err(format!(
                "unentered error return must name the guard and not read exposed: {class:?} named={named}"
            ));
        }
    }
    Ok(())
}

#[test]
fn the_repair_is_an_input_that_enters_the_guard_never_an_assertion() -> Result<(), String> {
    let (_, found, repo) = findings_with(MISSING_TESTS, 12, &|source| source.to_string())?;
    let families = found
        .iter()
        .map(|finding| finding.probe.family.as_str())
        .collect::<Vec<_>>();
    if !(families.contains(&"error_path") && families.contains(&"return_value")) {
        return Err(format!("expected both families, saw {families:?}"));
    }
    for finding in &found {
        let step = finding.recommended_next_step.as_deref().unwrap_or_default();
        if !step.starts_with("Add a related test input that enters `if price > 1_000_000`") {
            return Err(format!("next step must ask for the input: {step:?}"));
        }
        let misses = finding
            .related_tests
            .iter()
            .filter_map(|test| test.miss.as_ref())
            .collect::<Vec<_>>();
        // The smoke-only row keeps its weak-assertion miss; no row asks for
        // a stronger exact assertion, and the strong rows ask for an input.
        let missing_input = misses
            .iter()
            .filter(|miss| matches!(miss, ripr::domain::RelatedTestMiss::MissingInput))
            .count();
        if missing_input != 2
            || misses
                .iter()
                .any(|miss| matches!(miss, ripr::domain::RelatedTestMiss::MissingExactAssertion))
        {
            return Err(format!("strong rows must miss an input: {misses:?}"));
        }
        let explained = ripr::explain_finding_with_input(
            CheckInput {
                root: repo.root.clone(),
                base: None,
                diff_file: Some(repo.root.join("diff.patch")),
                mode: Mode::Ready,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            },
            &finding.id,
        )?;
        if !explained.contains("use an input that enters `if price > 1_000_000`") {
            return Err(format!("explain must name the input route:\n{explained}"));
        }
        // The error-variant assertion stays a need for after the guard is
        // entered; the guard itself is never the thing to assert.
        if explained.contains("assert the exact `if") {
            return Err(format!(
                "the guard must never be an assertion:\n{explained}"
            ));
        }
    }
    Ok(())
}

#[test]
fn a_call_that_enters_the_guard_keeps_the_exposed_reading() -> Result<(), String> {
    let tests = format!(
        "{MISSING_TESTS}\n#[test]\nfn too_large() {{\n    assert_eq!(discount(2_000_000, 1), Err(PriceError::TooLarge));\n}}\n"
    );
    let found = readings(&tests, 12)?;
    if found.is_empty()
        || found
            .iter()
            .any(|(class, named)| *named || *class != ExposureClass::Exposed)
    {
        return Err(format!("an entering call keeps `exposed`: {found:?}"));
    }
    Ok(())
}

#[test]
fn a_changed_error_return_in_a_guard_the_tests_enter_names_nothing() -> Result<(), String> {
    let found = readings(MISSING_TESTS, 9)?;
    if found.iter().any(|(_, named)| *named) {
        return Err("price = -1 enters `price < 0`; no guard may be named".to_string());
    }
    Ok(())
}

#[test]
fn shared_refusals_keep_the_exposed_reading_for_error_returns() -> Result<(), String> {
    type Tweak = fn(&str) -> String;
    let rows: [(&str, Tweak); 3] = [
        ("a mutable parameter", |source| {
            source.replace("price: i64", "mut price: i64")
        }),
        ("a compound guard", |source| {
            source.replace(
                "if price > 1_000_000 {",
                "if price > 1_000_000 && qty > 0 {",
            )
        }),
        ("a rebound parameter", |source| {
            source.replace(
                "    }\n    if price > 1_000_000 {",
                "    } let price = price + 0;\n    if price > 1_000_000 {",
            )
        }),
    ];
    for (label, tweak) in rows {
        let found = readings_with(MISSING_TESTS, 12, &tweak)?;
        if found
            .iter()
            .any(|(class, named)| *named || *class != ExposureClass::Exposed)
        {
            return Err(format!("{label} must stay exposed and unnamed: {found:?}"));
        }
    }
    Ok(())
}

#[test]
fn a_predicate_in_an_unentered_guard_with_a_later_err_names_no_guard() -> Result<(), String> {
    // Every Predicate probe gets an error-variant flow sink when the owner
    // returns `Err(` later; that must not lend it the guard reading.
    let source = "pub fn f(price: i64, qty: u32) -> Result<bool, String> {\n    if qty > 100 {\n        let bulk = price > 40;\n        let _ = bulk;\n    }\n    if price < 0 {\n        return Err(\"neg\".to_string());\n    }\n    Ok(true)\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,3 +2,3 @@\n     if qty > 100 {\n-        let bulk = price > 50;\n+        let bulk = price > 40;\n         let _ = bulk;\n";
    let root = std::env::temp_dir().join(format!("ripr-guarded-predicate-{}", std::process::id()));
    let setup = || -> std::io::Result<()> {
        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("tests"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"guarded-predicate\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        std::fs::write(root.join("src/lib.rs"), source)?;
        std::fs::write(
            root.join("tests/f.rs"),
            "use guarded_predicate::f;\n\n#[test]\nfn small() {\n    assert_eq!(f(10, 1), Ok(true));\n}\n",
        )?;
        std::fs::write(root.join("diff.patch"), diff)
    };
    setup().map_err(|error| format!("setup failed: {error}"))?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    });
    let _ = std::fs::remove_dir_all(&root);
    let output = output?;
    let on_line = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.line == 3)
        .collect::<Vec<_>>();
    if on_line.is_empty() {
        return Err("expected a finding on the changed predicate".to_string());
    }
    for finding in on_line {
        let step = finding.recommended_next_step.as_deref().unwrap_or_default();
        if names_guard(finding) || step.contains("enters `if") {
            return Err(format!("a predicate must not read the guard: {finding:?}"));
        }
    }
    Ok(())
}
