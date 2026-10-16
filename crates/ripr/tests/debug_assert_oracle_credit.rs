//! Public-API controls for `debug_assert*!` oracles (#8051).
//!
//! `cargo test` builds the debug profile, so a test whose only check is
//! `debug_assert_eq!` observes the changed behavior exactly as `assert_eq!`
//! does for a changed field write. Each case runs the same test body under both
//! spellings: the field verdicts must be identical, and the positive case must
//! actually be credited so the parity is not two equal gaps. The owner-return
//! pin keeps refusing `debug_assert_eq!` (RIPR-SPEC-0197), so the returned
//! value is never `exposed` through it.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNERS: &str = "pub struct Counter {
    pub count: u32,
}
pub fn one() -> u32 {
    2 - 1
}
impl Counter {
    pub fn bump(&mut self) {
        self.count = 2 - 1;
    }
}
";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,11 +1,11 @@
 pub struct Counter {
     pub count: u32,
 }
 pub fn one() -> u32 {
-    1
+    2 - 1
 }
 impl Counter {
     pub fn bump(&mut self) {
-        self.count = 1;
+        self.count = 2 - 1;
     }
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(tests: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-debug-assert-oracle-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            this.root.join("Cargo.toml"),
            "[package]\nname = \"debug-assert-oracle\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(this.root.join("src/lib.rs"), format!("{OWNERS}{tests}"))
            .map_err(|error| format!("write lib.rs failed: {error}"))?;
        std::fs::write(this.root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(this)
    }

    /// The class of the returned-value finding and of the field-write
    /// finding, in that order.
    fn classes(&self) -> Result<(ExposureClass, ExposureClass), String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let class_of = |family: ProbeFamily, line: usize| {
            let mut found = output.findings.iter().filter(|finding| {
                finding.probe.family == family && finding.probe.location.line == line
            });
            match (found.next(), found.next()) {
                (Some(finding), None) => Ok(finding.class.clone()),
                other => Err(format!(
                    "premise: exactly one {family:?} finding on line {line}, got {other:?}"
                )),
            }
        };
        Ok((
            class_of(ProbeFamily::ReturnValue, 5)?,
            class_of(ProbeFamily::FieldConstruction, 9)?,
        ))
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn tests_with(assertion_macro: &str, body: &str) -> String {
    format!(
        "
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn checks() {{
{}
    }}
}}
",
        body.replace("ASSERT", assertion_macro)
    )
}

fn classes_under(
    assertion_macro: &str,
    body: &str,
) -> Result<(ExposureClass, ExposureClass), String> {
    TempRepo::create(&tests_with(assertion_macro, body))?.classes()
}

#[test]
fn debug_assert_eq_pins_the_changed_field_like_assert_eq() -> Result<(), String> {
    let body = "        let mut c = Counter { count: 0 };
        c.bump();
        ASSERT!(c.count, 1);
        ASSERT!(one(), 1);";
    let plain = classes_under("assert_eq", body)?;
    assert_eq!(
        plain,
        (ExposureClass::Exposed, ExposureClass::Exposed),
        "premise: the plain spelling must be credited"
    );
    let (value, field) = classes_under("debug_assert_eq", body)?;
    assert_eq!(field, plain.1);
    // RIPR-SPEC-0197: the owner-return pin never admits `debug_assert_eq!`.
    assert_ne!(value, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn debug_assert_without_an_observing_assertion_keeps_the_gap() -> Result<(), String> {
    // Self-computed and unrelated checks must not become credit merely
    // because the macro is a debug spelling.
    for body in [
        "        let mut c = Counter { count: 0 };
        c.bump();
        ASSERT!(1, 1);",
        "        let mut c = Counter { count: 0 };
        c.bump();
        let _ = one();
        ASSERT!(c.count, c.count);",
    ] {
        let plain = classes_under("assert_eq", body)?;
        assert_ne!(
            plain.1,
            ExposureClass::Exposed,
            "premise: a vacuous check must not credit the field: {body}"
        );
        assert_eq!(classes_under("debug_assert_eq", body)?.1, plain.1, "{body}");
    }
    Ok(())
}

#[test]
fn lookalike_macro_names_are_not_assertions() -> Result<(), String> {
    // `my_debug_assert_eq!` / `debug_prop_assert_eq!` are unknown macros and
    // must read like no assertion at all.
    let body = "        let mut c = Counter { count: 0 };
        c.bump();
        ASSERT!(c.count, 1);
        ASSERT!(one(), 1);";
    for name in ["my_debug_assert_eq", "debug_prop_assert_eq"] {
        let (value, field) = classes_under(name, body)?;
        assert_ne!(value, ExposureClass::Exposed, "{name}");
        assert_ne!(field, ExposureClass::Exposed, "{name}");
    }
    Ok(())
}
