//! #14181 (rustls/rustls `7ef3a80`): `const { assert!(N <= 249) };` inside a
//! function is checked when the function is instantiated. A violating input
//! fails to build, so no test can pass a value on the other side of its
//! comparison. The line must not select a `predicate` family, mint a
//! `predicate_boundary` gap id or advise boundary inputs. It keeps one
//! `static_unknown` finding, so an empty result cannot pass these tests. The
//! same comparison in a const block that yields a value keeps its predicate
//! reading.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-inline-const-assertion-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"inline-const-assertion\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The issue's repro crate with `statement` as the first line of `fixed`
/// (line 5), or no statement there when it is empty.
fn lib(statement: &str) -> String {
    let line = if statement.is_empty() {
        String::new()
    } else {
        format!("        {statement}\n")
    };
    format!(
        "pub struct Label<'a>(&'a [u8]);

impl Label<'static> {{
    pub const fn fixed<const N: usize>(label: &'static [u8; N]) -> Self {{
{line}        Self(label)
    }}
}}

impl Label<'_> {{
    pub fn len(&self) -> usize {{
        self.0.len()
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn fixed_keeps_label() {{
        assert_eq!(Label::fixed(b\"abc\").len(), 3);
    }}
}}
"
    )
}

const HEADER: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
";

fn added_diff(statement: &str) -> String {
    format!(
        "{HEADER}@@ -4,2 +4,3 @@
     pub const fn fixed<const N: usize>(label: &'static [u8; N]) -> Self {{
+        {statement}
         Self(label)
"
    )
}

fn edited_diff(old: &str, new: &str) -> String {
    format!(
        "{HEADER}@@ -4,3 +4,3 @@
     pub const fn fixed<const N: usize>(label: &'static [u8; N]) -> Self {{
-        {old}
+        {new}
         Self(label)
"
    )
}

fn describe(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|finding| {
            format!(
                "{}:{} {:?}",
                finding.probe.family.as_str(),
                finding.probe.location.line,
                finding.probe.expression
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn on_line_five(findings: &[Finding]) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|finding| finding.probe.location.line == 5)
        .collect()
}

/// Line 5 has exactly one finding, a `static_unknown` one, and no
/// boundary-input advice.
fn assert_one_static_unknown(findings: &[Finding]) -> Result<(), String> {
    let line = on_line_five(findings);
    let families = line
        .iter()
        .map(|finding| finding.probe.family.clone())
        .collect::<Vec<_>>();
    if families != [ProbeFamily::StaticUnknown] {
        return Err(format!(
            "the inline const assertion must keep exactly one static_unknown finding, got [{}]",
            describe(findings)
        ));
    }
    let debug = format!("{:?}", line[0]);
    if debug.contains("predicate_boundary") || debug.contains("boundary input") {
        return Err(format!(
            "a build-time check must not carry boundary evidence: {debug}"
        ));
    }
    Ok(())
}

#[test]
fn added_inline_const_assertion_is_not_a_runtime_predicate() -> Result<(), String> {
    let statement = "const { assert!(N <= 249) };";
    let findings = TempRepo::create(&lib(statement), &added_diff(statement))?.findings()?;
    assert_one_static_unknown(&findings)
}

#[test]
fn edited_inline_const_assertion_is_not_a_runtime_predicate() -> Result<(), String> {
    let (old, new) = (
        "const { assert!(N <= 249) };",
        "const { assert!(N <= 250) };",
    );
    let findings = TempRepo::create(&lib(new), &edited_diff(old, new))?.findings()?;
    assert_one_static_unknown(&findings)
}

/// Control: the same comparison in a const block that yields a value is
/// still read as a predicate, so the tests above discriminate the assertion.
#[test]
fn value_const_block_keeps_its_predicate_reading() -> Result<(), String> {
    let (old, new) = (
        "let _fits = const { N <= 249 };",
        "let _fits = const { N <= 250 };",
    );
    let findings = TempRepo::create(&lib(new), &edited_diff(old, new))?.findings()?;
    if on_line_five(&findings)
        .iter()
        .any(|finding| finding.probe.family == ProbeFamily::Predicate)
    {
        Ok(())
    } else {
        Err(format!(
            "premise: a value const block comparison reads as a predicate, got [{}]",
            describe(&findings)
        ))
    }
}
