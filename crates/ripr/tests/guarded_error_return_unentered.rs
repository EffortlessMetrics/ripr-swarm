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
    fn create(test_source: &str, line: usize) -> Result<Self, String> {
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
        write("src/lib.rs", edited)?;
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

/// Classes and guard-naming flags of every finding on the changed line.
fn readings(body: &str, line: usize) -> Result<Vec<(ExposureClass, bool)>, String> {
    let repo = TempRepo::create(&format!("{TEST_HEADER}{body}"), line)?;
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
        .iter()
        .filter(|finding| finding.probe.location.line == line)
        .map(|finding| {
            let named = finding
                .activation
                .missing_discriminators
                .iter()
                .any(|fact| fact.reason.starts_with(UNENTERED_PREFIX))
                || finding.ripr.infect.summary.contains(UNENTERED_PREFIX);
            (finding.class.clone(), named)
        })
        .collect::<Vec<_>>();
    if found.is_empty() {
        return Err("no finding on the changed return".to_string());
    }
    Ok(found)
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
fn a_call_that_enters_the_guard_keeps_the_exposed_reading() -> Result<(), String> {
    let tests = format!(
        "{MISSING_TESTS}\n#[test]\nfn too_large() {{\n    assert_eq!(discount(2_000_000, 1), Err(PriceError::TooLarge));\n}}\n"
    );
    let found = readings(&tests, 12)?;
    if found
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
