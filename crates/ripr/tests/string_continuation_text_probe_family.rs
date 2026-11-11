//! #14094 (sharkdp/fd `2381562`): text on a continuation line of a multi-line
//! string literal is data. When it reads like code (`if x > 3 { return
//! Err(e) }` in a help message) it must not select a predicate, error-path,
//! return-value, call or field probe, on either side of the diff. The real
//! code edit in the same diff keeps its finding, so an empty result cannot
//! pass these tests.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const CODE_LOOKING_TEXT: [&str; 3] = [
    "  --exact  match literally (instead of a regular expression)",
    "  Note: quoting { matters }",
    "  if x > 3 { return Err(e) }",
];

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-string-continuation-text-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"string-continuation-text\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The fixture crate. `help_text` is the middle of the `HELP` literal and
/// `increment` the amount `bump` adds.
fn lib(help_text: &[&str], increment: u32) -> String {
    let mut help = String::new();
    for line in help_text {
        help.push_str(line);
        help.push('\n');
    }
    format!(
        "pub const HELP: &str = \"usage: tool [options]
  first line of help
{help}  last line\";

pub fn bump(x: u32) -> u32 {{
    x + {increment}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn bumps() {{
        assert_eq!(bump(1), 2);
        assert!(HELP.starts_with(\"usage\"));
    }}
}}
"
    )
}

/// A diff that adds (`+`) or removes (`-`) the three help lines after line 2
/// and, in a second hunk, edits the `bump` body.
fn diff(sign: char, old_increment: u32, new_increment: u32) -> String {
    let (old_len, new_len) = if sign == '+' { (2, 5) } else { (5, 2) };
    let (old_bump, new_bump) = if sign == '+' { (5, 8) } else { (8, 5) };
    let text = CODE_LOOKING_TEXT
        .iter()
        .map(|line| format!("{sign}{line}\n"))
        .collect::<String>();
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,{old_len} +2,{new_len} @@
   first line of help
{text}   last line\";
@@ -{old_bump},3 +{new_bump},3 @@
 pub fn bump(x: u32) -> u32 {{
-    x + {old_increment}
+    x + {new_increment}
 }}
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

/// Every finding must be about the `bump` edit; at least one must exist.
fn assert_only_the_code_edit_is_probed(findings: &[Finding]) -> Result<(), String> {
    let on_code = findings
        .iter()
        .filter(|finding| finding.probe.expression.contains("x + "))
        .count();
    if on_code == 0 {
        return Err(format!(
            "premise: the `bump` edit keeps a finding, got [{}]",
            describe(findings)
        ));
    }
    let on_text = findings
        .iter()
        .filter(|finding| {
            CODE_LOOKING_TEXT.iter().any(|line| {
                finding.probe.expression.contains(line.trim())
                    || finding
                        .probe
                        .before
                        .as_deref()
                        .is_some_and(|before| before.contains(line.trim()))
            })
        })
        .collect::<Vec<_>>();
    if !on_text.is_empty() {
        return Err(format!(
            "string literal text must seed no probe, got [{}]",
            describe(findings)
        ));
    }
    Ok(())
}

#[test]
fn added_code_looking_string_text_seeds_no_probe() -> Result<(), String> {
    let source = lib(&CODE_LOOKING_TEXT, 2);
    let findings = TempRepo::create(&source, &diff('+', 1, 2))?.findings()?;
    assert_only_the_code_edit_is_probed(&findings)
}

#[test]
fn deleted_code_looking_string_text_seeds_no_probe() -> Result<(), String> {
    let source = lib(&[], 2);
    let findings = TempRepo::create(&source, &diff('-', 1, 2))?.findings()?;
    assert_only_the_code_edit_is_probed(&findings)
}

/// An edited continuation line is changed output text. It keeps the one
/// `static_unknown` probe string text has always kept (#9174), whatever the
/// text looks like.
#[test]
fn an_edited_code_looking_string_line_stays_static_unknown() -> Result<(), String> {
    let source = lib(&["  if x > 4 { return Err(e) }"], 2);
    let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,3 +2,3 @@
   first line of help
-  if x > 3 { return Err(e) }
+  if x > 4 { return Err(e) }
   last line\";
";
    let findings = TempRepo::create(&source, diff)?.findings()?;
    let families = findings
        .iter()
        .map(|finding| finding.probe.family.as_str())
        .collect::<Vec<_>>();
    if families != ["static_unknown"] {
        return Err(format!(
            "an edited string line is one static_unknown, got [{}]",
            describe(&findings)
        ));
    }
    Ok(())
}

/// The same three lines as statements are code: the guard above must not
/// hide them. This is the fixture's own negative control.
#[test]
fn the_same_text_outside_a_literal_still_selects_behavioral_families() -> Result<(), String> {
    let source = "pub fn check(x: u32, e: String) -> Result<u32, String> {
    if x > 3 { return Err(e) }
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks() {
        assert_eq!(check(1, String::new()), Ok(1));
    }
}
";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 pub fn check(x: u32, e: String) -> Result<u32, String> {
+    if x > 3 { return Err(e) }
     Ok(x)
 }
";
    let findings = TempRepo::create(source, diff)?.findings()?;
    if !findings
        .iter()
        .any(|finding| finding.probe.family == ProbeFamily::Predicate)
    {
        return Err(format!(
            "a real `if x > 3` keeps its predicate finding, got [{}]",
            describe(&findings)
        ));
    }
    Ok(())
}
