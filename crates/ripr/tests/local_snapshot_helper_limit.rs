//! A sibling oracle cannot turn an opaque inline-snapshot helper into a gap.

#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = r#"mod snap {
    pub fn check(actual: &str, expected: &str) { assert_eq!(actual, expected); }
}
macro_rules! assert_data_eq {
    ($a:expr, $e:expr) => { $crate::snap::check(&$a, $e) };
}
macro_rules! snap_str {
    [[$expected:literal]] => { $expected };
}
pub fn render_float(value: f64) -> String {
    if value == 0.0 && value.is_sign_negative() {
        "-0.0".to_string()
    } else {
        value.to_string()
    }
}
pub fn separate_owner() -> i32 { 7 }
pub fn separate_float(_: f64) -> String { "-0.0".to_string() }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_zero_snapshot() {
        assert_data_eq!(render_float(-0.0), snap_str![["-0.0"]]);
    }
    #[test]
    fn separate_exact_oracle() { assert_eq!(separate_owner(), 7); }
}
"#;

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-local-snapshot-helper-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname = \"snapshot-helper-control\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
        let lines: Vec<_> = source.lines().collect();
        let positions: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.trim() == "\"-0.0\".to_string()")
            .collect();
        assert_eq!(positions.len(), 1, "one changed production return");
        let (offset, after) = positions[0];
        let line = offset + 1;
        let before = after.replace("\"-0.0\"", "\"-0\"");
        std::fs::write(
            fixture.0.join("diff.patch"),
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{before}\n+{after}\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut findings = output.findings;
        assert_eq!(findings.len(), 1, "nonempty changed-owner subject");
        let finding = findings.remove(0);
        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| { test.name == "negative_zero_snapshot" && test.oracle.is_none() })
        );
        assert!(
            finding
                .related_tests
                .iter()
                .any(|test| { test.name == "separate_exact_oracle" && test.oracle.is_some() })
        );
        Ok(finding)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn limited(source: &str) -> Result<(), String> {
    let finding = Fixture::new(source)?.finding()?;
    assert_eq!(
        finding.static_limit_kind.map(|kind| kind.as_str()),
        Some("rust_macro_wrapped_assertion_unresolved")
    );
    Ok(())
}

fn keeps_gap(body: &str) -> Result<(), String> {
    let source = SOURCE.replace(
        "assert_data_eq!(render_float(-0.0), snap_str![[\"-0.0\"]]);",
        body,
    );
    let finding = Fixture::new(&source)?.finding()?;
    assert_eq!(finding.static_limit_kind, None);
    Ok(())
}

#[test]
fn nested_expected_macro_keeps_the_local_helper_limit() -> Result<(), String> {
    limited(SOURCE)
}

#[test]
fn helper_and_expected_names_cannot_supply_exposed_credit() -> Result<(), String> {
    for source in [
        SOURCE.replace(
            "pub fn check(actual: &str, expected: &str) { assert_eq!(actual, expected); }",
            "pub fn check(_actual: &str, _expected: &str) {}",
        ),
        SOURCE.replace(
            "pub fn check(actual: &str, expected: &str) { assert_eq!(actual, expected); }",
            "pub fn check(_actual: &str, _expected: &str) {}",
        ).replace(
            "    use super::*;",
            "    use super::*;\n    mod snap { pub fn check(a: &str, e: &str) { assert_eq!(a, e); } }",
        ),
        SOURCE.replace("snap_str![[\"-0.0\"]]", "snap_str![[\"0.0\"]]"),
        SOURCE.replace(
            "assert_eq!(actual, expected);",
            "macro_rules! assert_eq { ($a:expr, $e:expr) => {}; } assert_eq!(actual, expected);",
        ),
    ] {
        limited(&source)?;
    }
    Ok(())
}

#[test]
fn only_owner_calls_outside_nested_macro_inputs_supply_the_limit() -> Result<(), String> {
    for body in [
        "let _ = render_float(-0.0); assert_data_eq!(separate_float(-0.0), snap_str![[\"-0.0\"]]);",
        "let _ = render_float(-0.0); assert_data_eq!(discard!(render_float(-0.0)), snap_str![[\"-0.0\"]]);",
        "let value = render_float(-0.0); assert_data_eq!(value, snap_str![[\"-0.0\"]]);",
    ] {
        keeps_gap(body)?;
    }
    Ok(())
}

#[test]
fn a_literal_expected_operand_retains_its_existing_limit() -> Result<(), String> {
    limited(&SOURCE.replace("snap_str![[\"-0.0\"]]", "\"-0.0\""))
}
