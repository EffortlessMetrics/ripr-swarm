//! #14181 (rustls/rustls `7ef3a80`): `const { assert!(N <= 249) };` inside a
//! function is checked when the function is instantiated. A violating input
//! fails to build, so no test can pass a value on the other side of its
//! comparison. The line must not select a `predicate` family, mint a
//! `predicate_boundary` gap id or advise boundary inputs. It keeps one
//! `static_unknown` finding that names the `compile_time_invariant` stop
//! reason, so an empty result cannot pass these tests. The
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
/// (line 5), or no statement there when it is empty. Each line of a
/// multi-line `statement` takes the body indent, so its lines start at 5.
fn lib(statement: &str) -> String {
    let line = statement
        .lines()
        .map(|line| format!("        {line}\n"))
        .collect::<String>();
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
    on_line(findings, 5)
}

fn on_line(findings: &[Finding], line: usize) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect()
}

/// Line 5 has exactly one finding, a `static_unknown` one, and no
/// boundary-input advice.
fn assert_one_static_unknown(findings: &[Finding]) -> Result<(), String> {
    assert_one_static_unknown_on(findings, 5)
}

/// `line` has exactly one finding, a `static_unknown` one naming
/// `compile_time_invariant`, and no boundary-input advice.
fn assert_one_static_unknown_on(findings: &[Finding], line: usize) -> Result<(), String> {
    let line = on_line(findings, line);
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
    if let Some(gap) = &line[0].canonical_gap
        && (gap.behavior_kind == "predicate_boundary" || gap.probe_kind == "predicate")
    {
        return Err(format!(
            "a build-time check must not mint a predicate gap id: {}",
            gap.id
        ));
    }
    // #14181 part 2: the finding names the build-time check.
    if !line[0]
        .stop_reasons
        .iter()
        .any(|reason| reason.as_str() == "compile_time_invariant")
    {
        return Err(format!(
            "the inline const assertion must carry compile_time_invariant, got {:?}",
            line[0].stop_reasons
        ));
    }
    let advice = format!("{:?} {:?}", line[0].missing, line[0].recommended_next_step);
    if advice.contains("boundary input") {
        return Err(format!(
            "a build-time check must not advise boundary inputs: {advice}"
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
    let line = on_line_five(&findings);
    if line.iter().any(|finding| {
        finding
            .stop_reasons
            .iter()
            .any(|reason| reason.as_str() == "compile_time_invariant")
    }) {
        return Err(format!(
            "a value const block is not a build-time check, got [{}]",
            describe(&findings)
        ));
    }
    if line
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

/// #14181 follow-up: rustfmt splits a long inline const assertion over three
/// lines. The block is the same build-time check, so its comparison line
/// keeps one `static_unknown` finding naming `compile_time_invariant`.
const SPLIT_OLD: &str = "const {\n    assert!(N <= 249);\n};";
const SPLIT_NEW: &str = "const {\n    assert!(N <= 250);\n};";

fn edited_inner_diff(open: &str, old: &str, new: &str, close: &str) -> String {
    format!(
        "{HEADER}@@ -4,5 +4,5 @@
     pub const fn fixed<const N: usize>(label: &'static [u8; N]) -> Self {{
         {open}
-        {old}
+        {new}
         {close}
         Self(label)
"
    )
}

fn added_block_diff(block: &str) -> String {
    let added = block
        .lines()
        .map(|line| format!("+        {line}\n"))
        .collect::<String>();
    let count = block.lines().count() + 2;
    format!(
        "{HEADER}@@ -4,2 +4,{count} @@
     pub const fn fixed<const N: usize>(label: &'static [u8; N]) -> Self {{
{added}         Self(label)
"
    )
}

/// No finding on the block's lines selects a `predicate` family.
fn assert_no_predicate_in(
    findings: &[Finding],
    lines: std::ops::RangeInclusive<usize>,
) -> Result<(), String> {
    if findings.iter().any(|finding| {
        lines.contains(&finding.probe.location.line)
            && finding.probe.family == ProbeFamily::Predicate
    }) {
        return Err(format!(
            "a split inline const assertion must not select predicate, got [{}]",
            describe(findings)
        ));
    }
    Ok(())
}

#[test]
fn edited_split_inline_const_assertion_is_not_a_runtime_predicate() -> Result<(), String> {
    let diff = edited_inner_diff(
        "const {",
        "    assert!(N <= 249);",
        "    assert!(N <= 250);",
        "};",
    );
    let findings = TempRepo::create(&lib(SPLIT_NEW), &diff)?.findings()?;
    assert_no_predicate_in(&findings, 5..=7)?;
    assert_one_static_unknown_on(&findings, 6)
}

#[test]
fn added_split_inline_const_assertion_is_not_a_runtime_predicate() -> Result<(), String> {
    let findings = TempRepo::create(&lib(SPLIT_OLD), &added_block_diff(SPLIT_OLD))?.findings()?;
    assert_no_predicate_in(&findings, 5..=7)?;
    assert_one_static_unknown_on(&findings, 6)
}

/// rustfmt also splits a long argument list inside the macro.
#[test]
fn edited_split_assertion_arguments_are_not_a_runtime_predicate() -> Result<(), String> {
    let old = "const {\n    assert!(\n        N <= 249,\n        \"label too long\"\n    );\n};";
    let new = old.replace("249", "250");
    let diff = format!(
        "{HEADER}@@ -4,6 +4,6 @@
     pub const fn fixed<const N: usize>(label: &'static [u8; N]) -> Self {{
         const {{
             assert!(
-                N <= 249,
+                N <= 250,
                 \"label too long\"
             );
"
    );
    let findings = TempRepo::create(&lib(&new), &diff)?.findings()?;
    assert_no_predicate_in(&findings, 5..=10)?;
    assert_one_static_unknown_on(&findings, 7)
}

/// Control: a split const block that yields a value keeps its predicate
/// reading, so the split tests above discriminate the assertion.
#[test]
fn split_value_const_block_keeps_its_predicate_reading() -> Result<(), String> {
    let new = "let _fits = const {\n    N <= 250\n};";
    let diff = edited_inner_diff("let _fits = const {", "    N <= 249", "    N <= 250", "};");
    let findings = TempRepo::create(&lib(new), &diff)?.findings()?;
    expect_predicate_without_reason(&findings, 6, "a split value const block")
}

fn expect_predicate_without_reason(
    findings: &[Finding],
    line: usize,
    subject: &str,
) -> Result<(), String> {
    let line = on_line(findings, line);
    if line.iter().any(|finding| {
        finding
            .stop_reasons
            .iter()
            .any(|reason| reason.as_str() == "compile_time_invariant")
    }) {
        return Err(format!(
            "{subject} is not a lone build-time check, got [{}]",
            describe(findings)
        ));
    }
    if line
        .iter()
        .any(|finding| finding.probe.family == ProbeFamily::Predicate)
    {
        Ok(())
    } else {
        Err(format!(
            "premise: {subject} comparison reads as a predicate, got [{}]",
            describe(findings)
        ))
    }
}
