//! #14883: the note that says an assertion's expected value is an unfilled
//! placeholder must use the same read test as the #14810 lowering it explains.
//! A custom helper whose diagnostic message only names the placeholder local
//! does not read it, so it gets no note; the same helper reading the local,
//! directly or through a `format!` interpolation, keeps the note.

use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-placeholder-note-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"placeholder-note\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The edited crate; its only test binds `expected` to `todo!()` and then
/// runs `call`, which goes through an opaque custom assertion helper.
fn lib(call: &str) -> String {
    format!(
        "pub fn price(amount: i32) -> i32 {{
    amount - 10
}}

#[cfg(test)]
mod tests {{
    use super::*;

    fn assert_positive(value: i32, message: &str) {{
        assert!(value > 0, \"{{message}}\");
    }}

    #[test]
    fn discount() {{
        let expected: i32 = todo!(\"write the value\");
        {call}
    }}
}}
"
    )
}

const PLACEHOLDER_NOTE: &str = "the expected value is an unfilled placeholder macro";

fn placeholder_notes(finding: &Finding) -> Vec<&String> {
    finding
        .evidence
        .iter()
        .filter(|line| line.contains(PLACEHOLDER_NOTE))
        .collect()
}

#[test]
fn a_message_word_naming_the_placeholder_earns_no_note() -> Result<(), String> {
    let finding = TempRepo::create(&lib("assert_positive(price(100), \"expected total\");"))?
        .changed_return()?;
    if !finding
        .related_tests
        .iter()
        .any(|test| test.name == "discount")
    {
        return Err(format!(
            "premise: discount is a related test, got {finding:?}"
        ));
    }
    let notes = placeholder_notes(&finding);
    if notes.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "a message word is not a read of the placeholder, but the note fired: {notes:?}"
        ))
    }
}

/// Control: the same helper reading the placeholder keeps the note, so the
/// test above discriminates the read rather than the helper shape.
#[test]
fn a_helper_reading_the_placeholder_keeps_the_note() -> Result<(), String> {
    let finding = TempRepo::create(&lib("assert_positive(expected, \"expected total\");"))?
        .changed_return()?;
    if placeholder_notes(&finding).len() == 1 {
        Ok(())
    } else {
        Err(format!(
            "a helper reading the unfilled placeholder carries one note, got {:?}",
            finding.evidence
        ))
    }
}

/// A `format!` interpolation of the placeholder is a read (#14810 round 3).
#[test]
fn an_interpolated_placeholder_keeps_the_note() -> Result<(), String> {
    let finding = TempRepo::create(&lib(
        "assert_positive(price(100), &format!(\"{expected}\"));",
    ))?
    .changed_return()?;
    if placeholder_notes(&finding).len() == 1 {
        Ok(())
    } else {
        Err(format!(
            "an interpolated placeholder is a read and carries one note, got {:?}",
            finding.evidence
        ))
    }
}
