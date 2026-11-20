//! Public-API controls for `#[test_case(..)]` row inputs at a changed
//! predicate boundary (#6641).
//!
//! `test_case` generates one test per row that calls the function with the
//! row's values, so a row value is the owner's input exactly as an rstest
//! `#[case]` value is. The credited twins put a row at the changed boundary;
//! every negative changes one thing and must not read `exposed`.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-test-case-rows-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"rows\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
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

/// The owner's predicate sits on line 2; the edit moves its boundary at 10.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,1 +2,1 @@
-    if q >= 10 { q * 9 } else { q * 10 }
+    if q > 10 { q * 9 } else { q * 10 }
";

const IMPORT: &str = "    use test_case::test_case;\n";
const ARGS_BODY: &str =
    "    fn prices(q: u32, expected: u32) {\n        assert_eq!(price(q), expected);\n    }\n";
const ARROW_BODY: &str = "    fn prices(q: u32) -> u32 {\n        price(q)\n    }\n";

fn lib(tests: &str) -> String {
    format!(
        "pub fn price(q: u32) -> u32 {{\n    if q > 10 {{ q * 9 }} else {{ q * 10 }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{IMPORT}\n{tests}}}\n"
    )
}

fn classes(tests: &str) -> Result<Vec<ExposureClass>, String> {
    let found: Vec<ExposureClass> = TempRepo::create(&lib(tests))?
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .map(|finding| finding.class)
        .collect();
    if found.is_empty() {
        return Err("premise: a finding on the changed predicate".to_string());
    }
    Ok(found)
}

fn require_exposed(label: &str, tests: &str) -> Result<(), String> {
    let found = classes(tests)?;
    if !found.contains(&ExposureClass::Exposed) {
        return Err(format!("{label} must be exposed: {found:?}"));
    }
    Ok(())
}

fn require_withheld(label: &str, tests: &str) -> Result<(), String> {
    let found = classes(tests)?;
    if found.contains(&ExposureClass::Exposed) {
        return Err(format!("{label} must not be credited: {found:?}"));
    }
    Ok(())
}

#[test]
fn a_row_at_the_changed_boundary_is_credited() -> Result<(), String> {
    require_exposed(
        "an argument row at the boundary",
        &format!("    #[test_case(10, 90)]\n    #[test_case(3, 30)]\n{ARGS_BODY}"),
    )?;
    require_exposed(
        "a named argument row at the boundary",
        &format!("    #[test_case(10, 90 ; \"ten\")]\n{ARGS_BODY}"),
    )?;
    require_exposed(
        "an arrow row at the boundary",
        &format!("    #[test_case(10 => 90)]\n    #[test_case(3 => 30)]\n{ARROW_BODY}"),
    )
}

#[test]
fn rows_off_the_boundary_are_not_credited() -> Result<(), String> {
    require_withheld(
        "argument rows off the boundary",
        &format!("    #[test_case(3, 30)]\n    #[test_case(20, 180)]\n{ARGS_BODY}"),
    )?;
    require_withheld(
        "arrow rows off the boundary",
        &format!("    #[test_case(3 => 30)]\n    #[test_case(20 => 180)]\n{ARROW_BODY}"),
    )
}

#[test]
fn a_boundary_row_that_never_reaches_the_owner_is_not_credited() -> Result<(), String> {
    // Ignored: the row's inputs never run.
    require_withheld(
        "an ignored arrow row",
        &format!("    #[test_case(10 => ignore 90)]\n{ARROW_BODY}"),
    )?;
    require_withheld(
        "a row named as ignored",
        &format!("    #[test_case(10, 90 ; \"ignore ten\")]\n{ARGS_BODY}"),
    )?;
    // `=> panics` passes exactly when the body's check fails, and a
    // `#[should_panic]` function inverts every row the same way.
    require_withheld(
        "a panics row",
        &format!("    #[test_case(10, 0 => panics)]\n{ARGS_BODY}"),
    )?;
    require_withheld(
        "a should_panic function",
        &format!("    #[test_case(10, 0)]\n    #[should_panic]\n{ARGS_BODY}"),
    )?;
    // One cell for two parameters cannot be aligned.
    require_withheld(
        "a row with too few cells",
        &format!("    #[test_case(10)]\n{ARGS_BODY}"),
    )?;
    // The body rebinds the parameter before the owner call.
    require_withheld(
        "a shadowed parameter",
        "    #[test_case(10, 90)]\n    fn prices(q: u32, expected: u32) {\n        let q = 3;\n        assert_eq!(price(q), expected);\n    }\n",
    )
}
