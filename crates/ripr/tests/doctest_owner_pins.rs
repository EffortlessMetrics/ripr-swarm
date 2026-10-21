//! Public-API controls for doc examples as owner pins (#6648,
//! RIPR-SPEC-0279).
//!
//! `cargo test` runs each runnable fenced example in a library's `///` docs
//! as its own crate that links the library. An exact `assert_eq!` there on
//! the owner's call through the library's import name pins the owner's
//! return like a unit test does. Every alternate below keeps the same
//! comparison but changes one fact rustdoc or name resolution depends on,
//! and must leave the return unpinned.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OLD: &str = "u16::from_be_bytes([bytes[0], bytes[1]])";
const NEW: &str = "(u16::from(bytes[0]) << 8) | u16::from(bytes[1])";

/// `src/lib.rs` with `example` as the doc example of `read_u16` behind the
/// fence `fence`, then `extra`; `read_u16`'s body is on line
/// `example lines + 6`.
fn library(fence: &str, example: &[&str], tail: &str, extra: &str) -> String {
    let mut text = String::from("/// Reads a big-endian `u16`.\n///\n");
    text.push_str(&format!("/// ```{fence}\n"));
    for line in example {
        text.push_str(&format!("/// {line}\n"));
    }
    text.push_str("/// ```\n");
    text.push_str(&format!(
        "pub fn read_u16(bytes: &[u8]) -> u16 {{\n    {tail}\n}}\n"
    ));
    text.push_str(extra);
    text
}

/// The diff from the `OLD` body to the `NEW` one of `library(..)`.
fn patch(fence: &str, example: &[&str]) -> String {
    let body_line = example.len() + 6;
    let context = library(fence, example, NEW, "");
    let declaration = context
        .lines()
        .nth(body_line - 2)
        .unwrap_or_default()
        .to_string();
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{start},3 +{start},3 @@
 {declaration}
-    {OLD}
+    {NEW}
 }}
",
        start = body_line - 1,
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-doctest-owner-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"wire\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
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

/// The one return-value finding of `read_u16`'s changed body.
fn return_value(fence: &str, example: &[&str], extra: &str) -> Result<Finding, String> {
    let lib = library(fence, example, NEW, extra);
    the_return_value(&lib, &patch(fence, example), example.len() + 6)
}

/// The one return-value finding on `line` when `patch` produced `lib`.
fn the_return_value(lib: &str, patch: &str, line: usize) -> Result<Finding, String> {
    let findings = TempRepo::create(lib, patch)?.findings()?;
    let mut returns = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == line
    });
    match (returns.next(), returns.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one return_value finding on the body line, got {other:?} in {findings:?}"
        )),
    }
}

const PINNED: &str = "assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);";

fn assert_pinned(fence: &str, example: &[&str]) -> Result<(), String> {
    let finding = return_value(fence, example, "")?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{example:?}: {finding:?}"
    );
    Ok(())
}

fn assert_unpinned(fence: &str, example: &[&str]) -> Result<(), String> {
    assert_unpinned_beside(fence, example, "")
}

fn assert_unpinned_beside(fence: &str, example: &[&str], extra: &str) -> Result<(), String> {
    let finding = return_value(fence, example, extra)?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "{example:?}: {finding:?}"
    );
    Ok(())
}

#[test]
fn a_doc_example_exact_pin_through_the_crate_name_exposes_the_return() -> Result<(), String> {
    assert_pinned("", &[PINNED])?;
    assert_pinned("rust", &[PINNED])
}

#[test]
fn a_doc_example_bare_call_imported_from_the_crate_exposes_the_return() -> Result<(), String> {
    assert_pinned(
        "",
        &[
            "use wire::read_u16;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
    )?;
    assert_pinned(
        "",
        &[
            "use wire::*;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
    )?;
    // The hidden-line form rustdoc compiles but does not render.
    assert_pinned(
        "",
        &[
            "# use wire::read_u16;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
    )
}

#[test]
fn a_doc_example_rustdoc_does_not_run_pins_nothing() -> Result<(), String> {
    // Corpus twin `mined-doctest-ignored-read-u16-le`: the same comparison
    // under a fence rustdoc compiles but does not run, or does not test.
    for fence in ["ignore", "no_run", "should_panic", "compile_fail", "text"] {
        assert_unpinned(fence, &[PINNED])?;
    }
    Ok(())
}

#[test]
fn crate_self_and_super_in_a_doc_example_name_the_example_not_the_library() -> Result<(), String> {
    for example in [
        ["assert_eq!(crate::read_u16(&[0x12, 0x34]), 0x1234);", ""],
        ["assert_eq!(self::read_u16(&[0x12, 0x34]), 0x1234);", ""],
        [
            "use crate::read_u16;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
        [
            "use self::read_u16;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
        ["assert_eq!(super::read_u16(&[0x12, 0x34]), 0x1234);", ""],
        [
            "use super::read_u16;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
    ] {
        assert_unpinned("", &example)?;
    }
    Ok(())
}

#[test]
fn a_bare_call_the_example_does_not_import_pins_nothing() -> Result<(), String> {
    // In the example's crate the bare name is not the library's item.
    assert_unpinned("", &["assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);"])
}

#[test]
fn an_example_that_binds_the_crate_name_or_the_owner_pins_nothing() -> Result<(), String> {
    for example in [
        // A local module takes the path from the extern crate.
        vec![
            "mod wire { pub fn read_u16(_: &[u8]) -> u16 { 0x1234 } }",
            PINNED,
        ],
        // A local twin takes the bare call from the import.
        vec![
            "use wire::*;",
            "fn read_u16(_: &[u8]) -> u16 { 0x1234 }",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
        // A local module takes the import's root from the extern crate.
        vec![
            "mod wire { pub fn read_u16(_: &[u8]) -> u16 { 0x1234 } }",
            "use wire::read_u16;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
        // A glob may bring in a `wire` that shadows the extern crate.
        vec!["use other::*;", PINNED],
        // An import from elsewhere takes the bare call.
        vec![
            "use std::convert::identity as read_u16;",
            "assert_eq!(read_u16(&[0x12, 0x34]), 0x1234);",
        ],
    ] {
        assert_unpinned("", &example)?;
    }
    Ok(())
}

#[test]
fn an_example_that_may_rebind_assert_eq_pins_nothing() -> Result<(), String> {
    assert_unpinned(
        "",
        &["macro_rules! assert_eq { ($($t:tt)*) => {}; }", PINNED],
    )?;
    assert_unpinned(
        "",
        &["std::assert_eq!(wire::read_u16(&[0x12, 0x34]), 0x1234);"],
    )?;
    // An imported `assert_eq` from elsewhere is not std's.
    assert_unpinned("", &["use pretty::assert_eq;", PINNED])?;
    // A std-rooted call elsewhere in the example makes the plain one
    // ambiguous too.
    assert_unpinned("", &["std::assert_eq!(1, 1);", PINNED])
}

#[test]
fn a_rival_owner_of_the_same_name_leaves_a_doc_example_unpinned() -> Result<(), String> {
    // Telling rivals apart reads the scopes of the file holding the comment,
    // which are not the example's, so the example is refused.
    assert_unpinned_beside(
        "",
        &[PINNED],
        "\npub mod le {\n    pub fn read_u16(bytes: &[u8]) -> u16 {\n        u16::from_le_bytes([bytes[0], bytes[1]])\n    }\n}\n",
    )
}

/// `Reader::read_u16` documented by `example`, beside `tests`; its body is
/// on line 11.
fn method_library(example: &str, tests: &str) -> String {
    format!(
        "pub struct Reader;

impl Reader {{
    /// Reads a big-endian `u16`.
    ///
    /// ```
    /// let reader = wire::Reader;
    /// {example}
    /// ```
    pub fn read_u16(&self, bytes: &[u8]) -> u16 {{
        {NEW}
    }}
}}
{tests}"
    )
}

const METHOD_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,3 +10,3 @@
     pub fn read_u16(&self, bytes: &[u8]) -> u16 {
-        u16::from_be_bytes([bytes[0], bytes[1]])
+        (u16::from(bytes[0]) << 8) | u16::from(bytes[1])
     }
";

#[test]
fn a_method_call_in_a_doc_example_pins_nothing() -> Result<(), String> {
    const METHOD_PIN: &str = "assert_eq!(reader.read_u16(&[0x12, 0x34]), 0x1234);";
    // Control: the same comparison in a unit test beside the owner pins it.
    let unit = format!(
        "
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn reads_big_endian() {{
        let reader = Reader;
        {METHOD_PIN}
    }}
}}
"
    );
    let finding = the_return_value(&method_library("", &unit), METHOD_PATCH, 11)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // In the example the receiver's type would be read from scopes that are
    // not the example's, so it is refused.
    let finding = the_return_value(&method_library(METHOD_PIN, ""), METHOD_PATCH, 11)?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
