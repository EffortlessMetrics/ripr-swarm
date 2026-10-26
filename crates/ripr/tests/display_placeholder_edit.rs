//! Public-API controls for a changed format placeholder (RIPR-SPEC-0297, #9836).
//!
//! An exact `to_string()` pin sees a changed literal fragment of a `write!`,
//! but a changed placeholder (`{}` to `{:?}` or `{0}`) is an argument edit in
//! disguise: the one value the test chose may print the same under both. The
//! literal-only edit keeps the pin's credit; the placeholder edits do not.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB: &str = "use std::fmt;

pub struct Temp(pub i32);

impl fmt::Display for Temp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, AFTER, self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_degrees() {
        assert_eq!(Temp(21).to_string(), EXPECTED);
    }
}
";

/// The `write!` on line 6 changed from `before` to `after`.
fn diff(before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/display.rs b/src/display.rs
index 1111111..2222222 100644
--- a/src/display.rs
+++ b/src/display.rs
@@ -6,3 +6,3 @@
     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
-        write!(f, {before}, self.0)
+        write!(f, {after}, self.0)
     }}
"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(after: &str, patch: &str, expected: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-display-placeholder-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"display-placeholder\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), "pub mod display;\n")
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(
            root.join("src/display.rs"),
            LIB.replace("AFTER", after).replace("EXPECTED", expected),
        )
        .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
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

/// The class of the one finding on line 7 after `before` became `after`.
fn class_after(before: &str, after: &str) -> Result<ExposureClass, String> {
    class_after_template(
        &format!("\"{before}\""),
        &format!("\"{after}\""),
        r#""21°C""#,
    )
}

fn class_after_template(
    before: &str,
    after: &str,
    expected: &str,
) -> Result<ExposureClass, String> {
    let findings = TempRepo::create(after, &diff(before, after), expected)?.findings()?;
    let mut on_line = findings
        .iter()
        .filter(|finding| finding.probe.location.line == 7);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.class.clone()),
        other => Err(format!(
            "premise: exactly one finding on line 7, got {other:?} in {findings:?}"
        )),
    }
}

#[test]
fn a_literal_only_edit_keeps_the_display_pin_credit() -> Result<(), String> {
    assert_eq!(
        class_after("{}\u{b0}F", "{}\\u{b0}C")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_changed_placeholder_is_not_a_literal_only_edit() -> Result<(), String> {
    for after in ["{:?}\\u{b0}C", "{0}\\u{b0}C"] {
        let class = class_after("{}\u{b0}C", after)?;
        assert_ne!(class, ExposureClass::Exposed, "{after}");
    }
    Ok(())
}

#[test]
fn decoded_placeholders_and_spelling_only_edits_withhold_credit() -> Result<(), String> {
    for (before, after) in [
        (r"\u{7b}\u{7d}°C", r"\u{7b}:?\u{7d}°C"),
        (r"\x7b\x7d°C", r"\x7b0\x7d°C"),
        ("{}°C", r"{}\u{b0}C"),
    ] {
        assert_ne!(
            class_after(before, after)?,
            ExposureClass::Exposed,
            "{before} -> {after}"
        );
    }
    // Independent runtime witnesses: these edits do not change the pinned value.
    assert_eq!(
        format!("\u{7b}\u{7d}°C", 21),
        format!("\u{7b}:?\u{7d}°C", 21)
    );
    assert_eq!(format!("{}°C", 21), format!("{}\u{b0}C", 21));
    Ok(())
}

#[test]
fn raw_template_quotes_cannot_hide_a_placeholder_change() -> Result<(), String> {
    assert_eq!(
        format!(r#"quoted"marker"{}°C"#, 21),
        format!(r#"quoted"marker"{:?}°C"#, 21)
    );
    assert_ne!(
        class_after_template(
            r##"r#"quoted"marker"{}°C"#"##,
            r##"r#"quoted"marker"{:?}°C"#"##,
            r#""quoted\"marker\"21°C""#
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn cancelling_fixed_fragments_withhold_credit() -> Result<(), String> {
    assert_eq!(format!("21{}", 21), format!("{}21", 21));
    assert_ne!(
        class_after_template(r#""21{}""#, r#""{}21""#, r#""2121""#)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_raw_expected_string_cannot_bypass_the_display_edit_gate() -> Result<(), String> {
    assert_ne!(
        class_after_template(
            r##"r#"quoted"marker"{}°C"#"##,
            r##"r#"quoted"marker"{:?}°C"#"##,
            r##"r#"quoted"marker"21°C"#"##,
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}
