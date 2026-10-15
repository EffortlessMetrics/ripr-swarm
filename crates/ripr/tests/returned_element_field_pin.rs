//! End-to-end witness for #7858 (RIPR-SPEC-0225 decision 8).
//!
//! A field constructed inside an array the owner returns reads `exposed`
//! when a test pins that element alone: by its literal index, or through an
//! `.any(|r| r.field == literal)` membership that no other element can
//! satisfy. The `total: 33` decoy, where another element already holds the
//! literal, passes whatever the changed element holds and keeps the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with the returned array on line 6, built from `first` and
/// the changed `total: a + b + 0`, then one test with `body` after `items`
/// declared in the test module.
fn source_with(first: &str, items: &str, body: &str) -> String {
    format!(
        "pub struct Row {{
    pub total: u32,
}}

pub fn rows(a: u32, b: u32) -> [Row; 2] {{
    [Row {{ total: {first} }}, Row {{ total: a + b + 0 }}]
}}

#[cfg(test)]
mod tests {{
    use super::*;
{items}
    #[test]
    fn rows_carry_the_sum() {{
{body}
    }}
}}
"
    )
}

/// Line 6 changed from `a + b` to `a + b + 0` in the second element.
fn diff(first: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5,3 +5,3 @@
 pub fn rows(a: u32, b: u32) -> [Row; 2] {{
-    [Row {{ total: {first} }}, Row {{ total: a + b }}]
+    [Row {{ total: {first} }}, Row {{ total: a + b + 0 }}]
 }}
"
    )
}

const ANY: &str = "        let rows = rows(30, 3);
        assert!(rows.iter().any(|r| r.total == 33));";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-returned-element-field-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"element-field\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The class of the `field_construction` finding on line 6. The premise is
/// that the finding exists and the related test reaches the owner.
fn field_class(first: &str, body: &str) -> Result<ExposureClass, String> {
    field_class_with(first, "", body)
}

fn field_class_with(first: &str, items: &str, body: &str) -> Result<ExposureClass, String> {
    field_class_of(&source_with(first, items, body), first)
}

fn field_class_of(lib: &str, first: &str) -> Result<ExposureClass, String> {
    let findings = TempRepo::create(lib, &diff(first))?.findings()?;
    let field = findings
        .iter()
        .find(|finding| {
            finding.probe.location.line == 6
                && finding.probe.family == ProbeFamily::FieldConstruction
        })
        .ok_or_else(|| {
            format!("premise: a field_construction finding on line 6, got {findings:?}")
        })?;
    if field.ripr.reach.summary.contains("rows_carry_the_sum") {
        Ok(field.class.clone())
    } else {
        Err(format!(
            "premise: the test reaches the owner, got {:?}",
            field.ripr.reach
        ))
    }
}

#[test]
fn a_test_pinning_the_changed_element_exposes_its_field() -> Result<(), String> {
    for body in [
        ANY,
        "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33);",
    ] {
        assert_eq!(field_class("0", body)?, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_membership_or_read_another_element_may_satisfy_keeps_the_gap() -> Result<(), String> {
    for (first, body, label) in [
        ("33", ANY, "decoy: the first element already holds 33"),
        ("a", ANY, "the first element is computed"),
        (
            "0",
            "        let rows = rows(30, 3);\n        assert_eq!(rows[0].total, 0);",
            "index of the unchanged element",
        ),
        (
            "0",
            "        let rows = rows(30, 3);\n        let rows = [Row { total: 33 }];\n        assert!(rows.iter().any(|r| r.total == 33));",
            "shadowed receiver",
        ),
    ] {
        assert_eq!(
            field_class(first, body)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

/// Each of these tests passes without running a check of the changed
/// element, so none observes it (#8059 review).
#[test]
fn a_test_that_may_skip_or_fake_the_assertion_keeps_the_gap() -> Result<(), String> {
    const INDEX: &str = "        assert_eq!(rows[1].total, 33);";
    for (items, body, label) in [
        (
            "    macro_rules! fake {\n        ($n:ident) => { let $n = [Row { total: 33 }, Row { total: 33 }]; };\n    }\n",
            format!("        let rows = rows(1, 1);\n        fake!(rows);\n{INDEX}"),
            "a macro statement rebinds the receiver",
        ),
        (
            "    macro_rules! assert_eq {\n        ($a:expr, $b:expr) => { let _ = (&$a, &$b); };\n    }\n",
            format!("        let rows = rows(1, 1);\n{INDEX}"),
            "a local assert_eq! that checks nothing",
        ),
        (
            "",
            format!("        let rows = rows(1, 1);\n        std::process::exit(0);\n{INDEX}"),
            "process::exit before the assertion",
        ),
    ] {
        assert_eq!(
            field_class_with("0", items, &body)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

/// A nested module's own `rows` takes the test's `super::rows(..)` call and
/// returns 33 in both elements, so the assertion passes whatever the owner's
/// changed element holds. A name shared by two indexed functions is not read
/// (#8059 review).
#[test]
fn a_call_another_function_of_the_owner_name_may_take_keeps_the_gap() -> Result<(), String> {
    let lib = "pub struct Row {
    pub total: u32,
}

pub fn rows(a: u32, b: u32) -> [Row; 2] {
    [Row { total: 0 }, Row { total: a + b + 0 }]
}

pub mod inner {
    use super::Row;

    pub fn rows(_a: u32, _b: u32) -> [Row; 2] {
        [Row { total: 33 }, Row { total: 33 }]
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn rows_carry_the_sum() {
            let rows = super::rows(30, 3);
            assert_eq!(rows[1].total, 33);
        }
    }
}
";
    assert_eq!(field_class_of(lib, "0")?, ExposureClass::WeaklyExposed);
    Ok(())
}
