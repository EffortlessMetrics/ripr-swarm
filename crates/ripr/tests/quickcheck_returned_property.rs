//! Public-API controls for a `#[quickcheck]` property that reports failure
//! through the `bool` or `TestResult` it returns (#6643).
//!
//! The property has no assertion macro: the harness checks its returned
//! value. ripr does not read that returned check, so it earns no credit, but
//! the row must not claim the test "has no assertion" and the finding must not
//! tell the reader to add one. Every negative below keeps the absence claim.

use ripr::{CheckInput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn bump(x: u32) -> u32 {\n    x.wrapping_add(2)\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn bump(x: u32) -> u32 {
-    x.wrapping_add(1)
+    x.wrapping_add(2)
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(tests: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-quickcheck-returned-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self { root };
        std::fs::create_dir_all(this.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            this.root.join("Cargo.toml"),
            "[package]\nname = \"quickcheck-returned\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let source = format!(
            "{OWNER}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    use quickcheck::TestResult;\n    use quickcheck_macros::quickcheck;\n\n{tests}}}\n"
        );
        std::fs::write(this.root.join("src/lib.rs"), source)
            .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
        std::fs::write(this.root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(this)
    }

    /// The one finding on the changed line 2.
    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut on_line = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == 2);
        match (on_line.next(), on_line.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const ADD_ASSERTION: &str = "Add a meaningful assertion";
const PROPERTY_LINE: &str = "QuickCheck property returns its check: `holds`";

/// The `holds` row's miss token, whether the finding names the property,
/// whether its next step asks for an assertion, and whether it is exposed.
type Reading = (Option<&'static str>, bool, bool, bool);

fn read(tests: &str) -> Result<Reading, String> {
    read_related(tests)?.ok_or_else(|| format!("premise: `holds` is related\n{tests}"))
}

/// [`read`], or `None` when `holds` is not related at all.
fn read_related(tests: &str) -> Result<Option<Reading>, String> {
    let finding = TempRepo::create(tests)?.finding()?;
    let Some(row) = finding
        .related_tests
        .iter()
        .find(|test| test.name == "holds")
    else {
        return Ok(None);
    };
    if row.oracle.is_some() {
        return Err(format!("no oracle may be granted: {row:?}"));
    }
    let named = finding
        .evidence
        .iter()
        .any(|line| line.starts_with(PROPERTY_LINE));
    let asks = finding
        .recommended_next_step
        .as_deref()
        .is_some_and(|step| step.starts_with(ADD_ASSERTION));
    let exposed = finding.class == ripr::ExposureClass::Exposed;
    Ok(Some((
        row.miss.map(|miss| miss.as_str()),
        named,
        asks,
        exposed,
    )))
}

#[test]
fn a_returned_property_is_not_read_as_missing_an_assertion() -> Result<(), String> {
    for tests in [
        "    #[quickcheck]\n    fn holds(x: u32) -> bool {\n        bump(x) == x.wrapping_add(2)\n    }\n",
        "    #[quickcheck_macros::quickcheck]\n    fn holds(x: u32) -> bool {\n        bump(x) == x.wrapping_add(2)\n    }\n",
        "    #[quickcheck]\n    fn holds(x: u32) -> TestResult {\n        TestResult::from_bool(bump(x) == x.wrapping_add(2))\n    }\n",
    ] {
        let (miss, named, asks, exposed) = read(tests)?;
        if miss != Some("assertion_admission_unproven") || !named || asks || exposed {
            return Err(format!(
                "a returned property must name the limit without credit: miss={miss:?} named={named} asks={asks} exposed={exposed}\n{tests}"
            ));
        }
    }
    Ok(())
}

#[test]
fn other_shapes_keep_the_absence_claim() -> Result<(), String> {
    for tests in [
        // Returns nothing: no check reaches the harness.
        "    #[quickcheck]\n    fn holds(x: u32) {\n        let _ = bump(x);\n    }\n",
        // A plain test returning a value is not a property.
        "    #[test]\n    fn holds() {\n        let _ = bump(1) == 3;\n    }\n",
    ] {
        let (miss, named, _, exposed) = read(tests)?;
        if named || miss == Some("assertion_admission_unproven") || exposed {
            return Err(format!(
                "this shape must keep its previous reading: miss={miss:?} named={named} exposed={exposed}\n{tests}"
            ));
        }
    }
    Ok(())
}

#[test]
fn another_attribute_ending_in_quickcheck_is_not_a_property() -> Result<(), String> {
    // `#[my::quickcheck]` is not a test attribute ripr knows: either the
    // function is not related at all, or it keeps its previous reading.
    let tests = "    #[my::quickcheck]\n    fn holds(x: u32) -> bool {\n        bump(x) == x.wrapping_add(2)\n    }\n";
    if let Some((miss, named, _, exposed)) = read_related(tests)?
        && (named || miss == Some("assertion_admission_unproven") || exposed)
    {
        return Err(format!(
            "an unknown attribute must not be read as a property: miss={miss:?} named={named} exposed={exposed}"
        ));
    }
    Ok(())
}
