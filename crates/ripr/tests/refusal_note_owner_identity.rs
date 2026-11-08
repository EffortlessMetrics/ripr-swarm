//! The "Not credited" note names an assertion that calls the changed owner,
//! not one that calls a different function with the same name (#6929).
//!
//! The changed owner is the free function `parse`. `real` calls it but is
//! refused for an analyzer limit (an async test). `other` is refused as a
//! conditional path and calls `Other::parse`, an associated function that only
//! shares the name. Ranking prefers a non-limit owner-calling refusal, so the
//! name-only call used to front for the gap and the note named `other`.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn parse(input: &str) -> i32 {
-    if input.len() > 3 { 1 } else { 0 }
+    if input.len() > 2 { 1 } else { 0 }
 }
";

fn source(other_call: &str) -> String {
    format!(
        "pub fn parse(input: &str) -> i32 {{
    if input.len() > 2 {{ 1 }} else {{ 0 }}
}}

pub struct Other;

impl Other {{
    pub fn parse(input: &str) -> i32 {{
        input.len() as i32
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    async fn real() {{
        let input = \"abcd\";
        assert_eq!(parse(input), 1);
    }}

    #[test]
    fn other() {{
        let input = \"abcd\";
        if false {{
            assert_eq!({other_call}, 4);
        }}
    }}
}}
"
    )
}

/// The human "Not credited" line for the one finding on changed line 2.
fn not_credited_line(other_call: &str) -> Result<String, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-refusal-owner-identity-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let scratch = Scratch(root.clone());
    std::fs::create_dir_all(root.join("src"))
        .map_err(|error| format!("create source directory failed: {error}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"owner-identity\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
    std::fs::write(root.join("src/lib.rs"), source(other_call))
        .map_err(|error| format!("write source failed: {error}"))?;
    std::fs::write(root.join("diff.patch"), DIFF)
        .map_err(|error| format!("write diff failed: {error}"))?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Human,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let finding = output
        .findings
        .iter()
        .find(|finding| finding.probe.location.line == 2)
        .ok_or("premise: a finding on changed line 2")?;
    if finding.class != ExposureClass::InfectionUnknown {
        return Err(format!(
            "premise: infection_unknown, got {:?}",
            finding.class
        ));
    }
    let text = render_check(&output, &OutputFormat::Human)?;
    drop(scratch);
    text.lines()
        .find(|line| line.trim_start().starts_with("Not credited:"))
        .map(str::to_string)
        .ok_or_else(|| format!("premise: a Not credited line in {text}"))
}

#[test]
fn a_same_named_associated_function_does_not_front_for_the_owner() -> Result<(), String> {
    let line = not_credited_line("Other::parse(input)")?;
    assert!(line.contains(" in real "), "{line}");
    assert!(!line.contains(" in other "), "{line}");
    Ok(())
}

#[test]
fn a_receiver_call_of_the_same_name_does_not_front_for_the_owner() -> Result<(), String> {
    let line = not_credited_line("helper.parse(input)")?;
    assert!(line.contains(" in real "), "{line}");
    Ok(())
}

#[test]
fn a_real_conditional_owner_call_still_outranks_the_limit_refusal() -> Result<(), String> {
    // Discriminating control: when the conditional refusal does call the
    // owner, #6903's ranking still puts it first.
    let line = not_credited_line("parse(input)")?;
    assert!(line.contains(" in other "), "{line}");
    Ok(())
}

#[test]
fn a_spaced_or_module_qualified_owner_call_still_counts_as_the_owner() -> Result<(), String> {
    // Controls for the identity check: rustfmt-unusual spacing and a
    // module-qualified path are still calls to the free function.
    for call in [
        "parse (input)",
        "self::parse(input)",
        "Other::parse(input) + parse(input)",
    ] {
        let line = not_credited_line(call)?;
        assert!(line.contains(" in other "), "{call}: {line}");
    }
    Ok(())
}
