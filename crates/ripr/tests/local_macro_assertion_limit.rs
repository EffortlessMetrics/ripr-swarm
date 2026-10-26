//! Public-API controls for a same-file `macro_rules!` assertion wrapper that
//! is not spelled `assert_*` (RIPR-SPEC-0120, #7089).
//!
//! `check_eq!(surcharge(100), 110)` expands to `assert_eq!`, but ripr only
//! credits it at unknown grade. Beside a strong assertion on another function
//! in the same file, that used to read as a confident `weakly_exposed` gap;
//! it now names `rust_macro_wrapped_assertion_unresolved` (verdict `limited`). Each control
//! changes one thing and keeps the old verdict.

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIMIT: &str = "rust_macro_wrapped_assertion_unresolved";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-local-macro-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"local-macro\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The owner sits on line 2, so the rewrite is a one-line edit there.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,1 +2,1 @@
-    base + base / 5
+    base + base / 10
";

const MACRO: &str = "    macro_rules! check_eq {\n        ($actual:expr, $expected:expr) => {\n            assert_eq!($actual, $expected)\n        };\n    }\n\n";
const OTHER_PIN: &str = "    #[test]\n    fn other_is_pinned() {\n        assert_eq!(discount(200, 10), 180);\n    }\n\n";
const MACRO_PIN: &str = "    #[test]\n    fn macro_pins_surcharge() {\n        check_eq!(surcharge(100), 110);\n    }\n";

fn lib(tests: &str) -> String {
    format!(
        "pub fn surcharge(base: u32) -> u32 {{\n    base + base / 10\n}}\n\npub fn discount(price: u32, percent: u32) -> u32 {{\n    price - price * percent / 100\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{tests}}}\n"
    )
}

fn findings(tests: &str) -> Result<Vec<Finding>, String> {
    let found: Vec<Finding> = TempRepo::create(&lib(tests), DIFF)?
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .collect();
    if found.is_empty() {
        return Err("premise: a finding on the rewritten line".to_string());
    }
    Ok(found)
}

fn limited(finding: &Finding) -> bool {
    finding.static_limit_kind.map(|kind| kind.as_str()) == Some(LIMIT)
}

fn summary(found: &[Finding]) -> Vec<(&'static str, Option<&'static str>)> {
    found
        .iter()
        .map(|f| {
            (
                f.class.as_str(),
                f.static_limit_kind.map(|kind| kind.as_str()),
            )
        })
        .collect()
}

#[test]
fn a_local_wrapper_beside_a_proximity_oracle_reads_as_the_macro_limit() -> Result<(), String> {
    let found = findings(&format!("{MACRO}{OTHER_PIN}{MACRO_PIN}"))?;
    // The named limit rides on the finding; its verdict is `limited`.
    if !found.iter().any(limited) {
        return Err(format!(
            "expected the macro limit, got {:?}",
            summary(&found)
        ));
    }
    Ok(())
}

#[test]
fn a_wrapper_with_no_local_definition_is_not_credited_or_limited() -> Result<(), String> {
    // Without a same-file `macro_rules!` ripr has no assertion to credit, so
    // the limit rule does not apply.
    let found = findings(&format!("{OTHER_PIN}{MACRO_PIN}"))?;
    if found.iter().any(limited) {
        return Err(format!(
            "undefined macro got the limit: {:?}",
            summary(&found)
        ));
    }
    Ok(())
}

#[test]
fn a_recognized_assertion_on_the_owner_keeps_its_verdict() -> Result<(), String> {
    let direct =
        "    #[test]\n    fn direct_pin() {\n        assert_eq!(surcharge(100), 110);\n    }\n";
    let found = findings(&format!("{MACRO}{OTHER_PIN}{MACRO_PIN}\n{direct}"))?;
    if found.iter().any(limited) {
        return Err(format!(
            "a direct assertion got the limit: {:?}",
            summary(&found)
        ));
    }
    Ok(())
}

#[test]
fn the_witness_comes_from_the_directly_related_test() -> Result<(), String> {
    // `a_prox` sorts before the real caller and invokes the same macro on
    // unrelated values; the limit must still point at the owner's own test.
    let prox = "    #[test]\n    fn a_prox() {\n        check_eq!(1, 1);\n        assert_eq!(discount(200, 10), 180);\n    }\n\n";
    let found = findings(&format!("{MACRO}{prox}{MACRO_PIN}"))?;
    let witness = found
        .iter()
        .filter(|f| limited(f))
        .flat_map(|f| f.evidence.iter())
        .find(|line| line.contains("reaches the changed owner, then invokes"))
        .ok_or_else(|| format!("expected a witness line, got {:?}", summary(&found)))?;
    if !witness.contains("macro_pins_surcharge") || witness.contains("a_prox") {
        return Err(format!("the witness names the wrong test: {witness}"));
    }
    Ok(())
}

#[test]
fn a_generated_test_case_comparison_beside_a_proximity_oracle_keeps_its_limit() -> Result<(), String>
{
    // `#[test_case(4 => 12)]` generates the comparison on `rows`, which calls
    // the owner behind an opaque no-op macro; the only recognized oracle sits
    // in an unrelated test. That used to read as a bare `weakly_exposed` gap
    // although the generated assertion fails when the owner changes.
    let noop = "    macro_rules! note {\n        () => {\n            ()\n        };\n    }\n\n";
    let rows = "    #[test_case(100 => 110)]\n    fn rows(base: u32) -> u32 {\n        note!();\n        surcharge(base)\n    }\n\n";
    let found = findings(&format!("{noop}{rows}{OTHER_PIN}"))?;
    if !found.iter().any(limited) {
        return Err(format!(
            "expected the generated-comparison limit, got {:?}",
            summary(&found)
        ));
    }
    // The plain-argument form generates no comparison, so nothing is limited.
    let plain = rows.replace("100 => 110", "100, 110");
    let found = findings(&format!("{noop}{plain}{OTHER_PIN}"))?;
    if found.iter().any(limited) {
        return Err(format!(
            "a row without `=>` must not be limited: {:?}",
            summary(&found)
        ));
    }
    Ok(())
}

#[test]
fn a_generated_comparison_on_a_constant_return_is_not_a_limit() -> Result<(), String> {
    // The owner is called, but `rows` returns a literal, so the generated
    // `=> 110` comparison cannot notice a changed owner. Calling that "an
    // unreadable comparison" would hide the missing discriminator (#10302).
    let constant = "    #[test_case(100 => 110)]\n    fn rows(base: u32) -> u32 {\n        let _ = surcharge(base);\n        110\n    }\n";
    let found = findings(constant)?;
    if found.is_empty() || found.iter().any(limited) {
        return Err(format!(
            "a constant-return generated comparison must stay a gap: {:?}",
            summary(&found)
        ));
    }
    if found
        .iter()
        .any(|f| f.class == ripr::ExposureClass::StaticUnknown)
    {
        return Err(format!(
            "the unlimited finding must not read static_unknown: {:?}",
            summary(&found)
        ));
    }
    // The same row returning the owner's value keeps the named limit.
    let live = constant.replace("let _ = surcharge(base);\n        110", "surcharge(base)");
    let found = findings(&live)?;
    if !found.iter().any(limited) {
        return Err(format!(
            "expected the generated-comparison limit, got {:?}",
            summary(&found)
        ));
    }
    Ok(())
}

#[test]
fn a_generated_predicate_beside_a_constant_return_keeps_the_limit() -> Result<(), String> {
    // `with` runs a predicate of its own, which can read state the owner
    // changed even though the row returns a literal.
    let with = "    #[test_case(100 => with |_: u32| assert_eq!(surcharge(100), 110))]\n    fn rows(base: u32) -> u32 {\n        let _ = surcharge(base);\n        110\n    }\n";
    let found = findings(with)?;
    if !found.iter().any(limited) {
        return Err(format!(
            "a `with` predicate must keep the generated-comparison limit: {:?}",
            summary(&found)
        ));
    }
    Ok(())
}
