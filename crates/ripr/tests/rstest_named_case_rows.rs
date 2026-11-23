//! Public-API controls for rstest's named case rows (#15341).
//!
//! `#[case::description(..)]` carries the same positional row as
//! `#[case(..)]`; only the generated test's name differs. A named row at the
//! changed boundary is the owner's input, and named rows off the boundary
//! leave the boundary missing.

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
            "ripr-rstest-named-rows-{}-{}",
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

const IMPORT: &str = "    use rstest::rstest;\n";
const BODY: &str = "    #[rstest]\n{rows}    fn prices(#[case] q: u32, #[case] expected: u32) {\n        assert_eq!(price(q), expected);\n    }\n";

fn lib(rows: &str) -> String {
    let tests = BODY.replace("{rows}", rows);
    format!(
        "pub fn price(q: u32) -> u32 {{\n    if q > 10 {{ q * 9 }} else {{ q * 10 }}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{IMPORT}\n{tests}}}\n"
    )
}

fn classes(rows: &str) -> Result<Vec<ExposureClass>, String> {
    let found: Vec<ExposureClass> = TempRepo::create(&lib(rows))?
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

#[test]
fn a_named_row_at_the_changed_boundary_is_credited() -> Result<(), String> {
    for rows in [
        "    #[case::at_ten(10, 90)]\n    #[case::small(3, 30)]\n",
        "    #[case :: at_ten(10, 90)]\n    #[case(3, 30)]\n",
    ] {
        let found = classes(rows)?;
        if !found.contains(&ExposureClass::Exposed) {
            return Err(format!("{rows:?} must be exposed: {found:?}"));
        }
    }
    Ok(())
}

#[test]
fn named_rows_off_the_boundary_are_not_credited() -> Result<(), String> {
    let found = classes("    #[case::small(3, 30)]\n    #[case::large(20, 180)]\n")?;
    if found.contains(&ExposureClass::Exposed) {
        return Err(format!(
            "named rows off the boundary must not be credited: {found:?}"
        ));
    }
    // The rows are read, so the missing boundary is named, not unknown.
    if !found.contains(&ExposureClass::WeaklyExposed) {
        return Err(format!(
            "named rows off the boundary must read weakly exposed: {found:?}"
        ));
    }
    Ok(())
}
