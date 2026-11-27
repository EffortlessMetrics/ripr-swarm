//! #14882: a terminal `if price(..) != expected { return Err(..) }` guard
//! stands for `assert_eq!(price(..), expected)`. When `expected` is still the
//! unfilled `todo!()` a stub writes, the guard pins no value and must not
//! close the gap (#14810 did this for `assert_eq!`). The same guard on the
//! changed value's literal is credited, so the harness can see a guard pin.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-terminal-guard-unfilled-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"terminal-guard\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn changed_return(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut returns = output
            .findings
            .into_iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect::<Vec<_>>();
        if returns.len() != 1 {
            return Err(format!(
                "premise: one return_value finding on the changed line, got {returns:?}"
            ));
        }
        returns
            .pop()
            .ok_or_else(|| "premise: one return_value finding".to_string())
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn price(amount: i32) -> i32 {
-    amount - 9
+    amount - 10
 }
";

/// The edited crate; its only test is the terminal guard `guard`.
fn lib(guard: &str) -> String {
    format!(
        "pub fn price(amount: i32) -> i32 {{
    amount - 10
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn discount() -> Result<(), String> {{
        {guard}
        Ok(())
    }}
}}
"
    )
}

#[test]
fn a_guard_on_an_unfilled_expected_value_does_not_close_the_gap() -> Result<(), String> {
    let guard = "let expected: i32 = todo!(\"write the value\");
        if price(100) != expected {
            return Err(\"wrong\".into());
        }";
    let finding = TempRepo::create(&lib(guard))?.changed_return()?;
    if finding.class == ExposureClass::Exposed {
        return Err(format!(
            "a guard on an unfilled todo!() pins nothing and must not read exposed: {finding:?}"
        ));
    }
    Ok(())
}

/// Control: the same guard on the changed value's literal is credited, so the
/// test above discriminates the placeholder rather than the guard shape.
#[test]
fn a_guard_on_the_literal_value_is_credited() -> Result<(), String> {
    let guard = "if price(100) != 90 {
            return Err(\"wrong\".into());
        }";
    let finding = TempRepo::create(&lib(guard))?.changed_return()?;
    if finding.class == ExposureClass::Exposed {
        Ok(())
    } else {
        Err(format!(
            "premise: a guard pinning the changed literal reads exposed, got {:?}",
            finding.class
        ))
    }
}

/// The weak `assert!(price(..) == expected)` reading is raised to strong by
/// the owner return pin, so an unfilled `expected` must not close it either.
#[test]
fn an_equality_assert_on_an_unfilled_expected_value_does_not_close_the_gap() -> Result<(), String> {
    let body = "let expected: i32 = todo!(\"write the value\");
        assert!(price(100) == expected);";
    let finding = TempRepo::create(&lib(body))?.changed_return()?;
    if finding.class == ExposureClass::Exposed {
        return Err(format!(
            "an equality on an unfilled todo!() pins nothing and must not read exposed: {finding:?}"
        ));
    }
    Ok(())
}
