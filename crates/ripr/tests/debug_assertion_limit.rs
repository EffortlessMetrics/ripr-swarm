//! Public-API controls for `debug_` assertions (#8051, RIPR-SPEC-0240).
//!
//! A test that calls the owner and checks it with a strong
//! `debug_assert_eq!` or `debug_assert!` discriminates under the default
//! `cargo test` profile and never under `--release`, so the finding reads
//! `static_unknown` with `rust_debug_assertion_profile_unresolved`: neither
//! a missing assertion nor credit. A standard `assert_eq!` keeps its credit;
//! `debug_assert_ne!`, a relational `debug_assert!` and an ignored test keep
//! the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with `one` on lines 1-3 and a test with `attrs` and `body`.
fn source(attrs: &str, body: &str) -> String {
    format!(
        "pub fn one() -> u32 {{
    2 - 1
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    {attrs}
    fn one_is_one() {{
        {body}
    }}
}}
"
    )
}

/// The returned expression on line 2 changed.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn one() -> u32 {
-    1
+    2 - 1
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-debug-assertion-limit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"debug-assertion\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), DIFF)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The `return_value` finding on line 2, which must be the only one.
fn return_finding(attrs: &str, body: &str) -> Result<Finding, String> {
    let repo = TempRepo::create(&source(attrs, body))?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut on_line = output.findings.iter().filter(|finding| {
        finding.probe.location.line == 2 && finding.probe.family == ripr::ProbeFamily::ReturnValue
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one return_value finding on line 2, got {other:?} in {:?}",
            output.findings
        )),
    }
}

#[test]
fn a_strong_debug_assertion_reads_as_a_profile_limit() -> Result<(), String> {
    for body in [
        "debug_assert_eq!(one(), 1);",
        "debug_assert_eq!(1, one());",
        "std::debug_assert_eq!(one(), 1);",
    ] {
        let finding = return_finding("", body)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{body}");
        assert_eq!(
            finding.static_limit_kind.map(|kind| kind.as_str()),
            Some("rust_debug_assertion_profile_unresolved"),
            "{body}"
        );
        assert!(
            finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("`one_is_one`") && step.contains("assert_eq!")),
            "{body}: {:?}",
            finding.recommended_next_step
        );
    }
    Ok(())
}

#[test]
fn standard_assertions_and_weak_debug_checks_keep_their_class() -> Result<(), String> {
    // The standard macro is credited.
    let finding = return_finding("", "assert_eq!(one(), 1);")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None);
    for (attrs, body) in [
        // Passes for every value other than the one named.
        ("", "debug_assert_ne!(one(), 2);"),
        // A relational condition would not discriminate as `assert!` either.
        ("", "debug_assert!(one() > 0);"),
        // An ignored test never runs by default.
        ("#[ignore]", "debug_assert_eq!(one(), 1);"),
    ] {
        let finding = return_finding(attrs, body)?;
        assert!(
            matches!(
                finding.class,
                ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
            ),
            "{attrs} {body}: {finding:?}"
        );
        assert_eq!(finding.static_limit_kind, None, "{attrs} {body}");
    }
    Ok(())
}
