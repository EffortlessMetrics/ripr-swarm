//! CLI witness for the #7062 non-ASCII call-path panic.
//!
//! `ripr check --diff` aborted with "byte index is not a char boundary"
//! when a test called through a non-ASCII module re-export
//! (`crate::módulo::render`). This drives the public CLI, not
//! `called_paths` alone.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "pub mod módulo { pub use crate::a::render; }\n\
pub mod a {\n    pub fn render(x: i32) -> i32 {\n        x + 1\n    }\n}\n\
pub mod b {\n    pub fn render(x: i32) -> i32 {\n        x * 2\n    }\n}\n\
#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(crate::módulo::render(2), 3);\n    }\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,7 +1,7 @@
 pub mod módulo { pub use crate::a::render; }
 pub mod a {
     pub fn render(x: i32) -> i32 {
-        1 + x
+        x + 1
     }
 }
 pub mod b {
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-non-ascii-called-path-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"non-ascii-called-path-check\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), SOURCE)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn check(&self) -> Result<Output, String> {
        let root = self.root.display().to_string();
        let diff = self.root.join("diff.patch").display().to_string();
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args([
                "check",
                "--root",
                root.as_str(),
                "--diff",
                diff.as_str(),
                "--format",
                "json",
                "--include-unchanged-tests",
            ])
            .output()
            .map_err(|error| format!("run ripr check --diff: {error}"))
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn check_diff_on_non_ascii_module_reexport_finishes_without_internal_error() -> Result<(), String> {
    let repo = TempRepo::create()?;
    let output = repo.check()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("internal error"),
        "ripr check aborted: {stderr}"
    );
    assert!(
        !stderr.contains("char boundary"),
        "ripr check aborted: {stderr}"
    );

    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "ripr check --diff JSON did not parse ({error}); stdout={} stderr={stderr}",
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    let findings = parsed["findings"]
        .as_array()
        .ok_or_else(|| format!("missing findings array: {parsed}"))?;
    let finding = findings
        .iter()
        .find(|finding| {
            finding
                .pointer("/probe/expression")
                .and_then(Value::as_str)
                .is_some_and(|expression| expression.contains("x + 1"))
        })
        .ok_or_else(|| format!("missing changed render probe; observed {parsed}"))?;
    let related = finding["related_tests"]
        .as_array()
        .ok_or_else(|| format!("missing related_tests: {finding}"))?;
    assert!(
        related
            .iter()
            .any(|test| test.get("name").and_then(Value::as_str) == Some("t")),
        "related tests: {related:?}"
    );
    Ok(())
}
