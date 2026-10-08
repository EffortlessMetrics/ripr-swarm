//! Public-API controls for bitwise-OR return pins (#6675).
//!
//! An exact `assert_eq!` on the owner's own call pins a returned bitwise-OR
//! tail (`u16::from(lo) | (u16::from(hi) << 8)`) as it does arithmetic, and
//! an operand-swap rewrite adds no second `static_unknown` finding for the
//! removed line. A closure tail or a non-pinning assertion gets no credit.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SWAPPED: &str = "(u16::from(hi) << 8) | u16::from(lo)";
const PACKED: &str = "u16::from(lo) | (u16::from(hi) << 8)";

/// `src/lib.rs` with `tail` as the body of `pack` on line 2.
fn source(tail: &str, assertion: &str) -> String {
    format!(
        "pub fn pack(lo: u8, hi: u8) -> u16 {{
    {tail}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn packs_little_endian() {{
        {assertion}
    }}
}}
"
    )
}

fn diff(removed: &str, added: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn pack(lo: u8, hi: u8) -> u16 {{
-    {removed}
+    {added}
 }}
"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-bitwise-or-return-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"bitwise-or\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The one finding on the changed line 2 whose expression is `expression`.
fn line_two(findings: &[Finding], expression: &str) -> Result<Finding, String> {
    let mut on_line = findings.iter().filter(|finding| {
        finding.probe.location.line == 2 && finding.probe.expression.contains(expression)
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one finding on line 2 for {expression}, got {other:?} in {findings:?}"
        )),
    }
}

#[test]
fn an_exact_pin_on_the_owner_call_exposes_a_returned_bitwise_or() -> Result<(), String> {
    let findings = TempRepo::create(
        &source(PACKED, "assert_eq!(pack(0x34, 0x12), 0x1234);"),
        &diff(SWAPPED, PACKED),
    )?
    .findings()?;
    let finding = line_two(&findings, PACKED)?;
    assert_eq!(
        finding.probe.family,
        ProbeFamily::ReturnValue,
        "{finding:?}"
    );
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    // The removed operand-swap line adds no second static_unknown finding.
    assert!(
        findings.iter().all(
            |finding| !(finding.probe.family == ProbeFamily::StaticUnknown
                && finding.probe.expression.trim() == SWAPPED)
        ),
        "the reordered removed line must not get its own finding: {findings:?}"
    );
    Ok(())
}

#[test]
fn a_closure_tail_or_a_non_pinning_assertion_is_not_exposed() -> Result<(), String> {
    let closure_old = "Some(lo).map_or(0, |v| (u16::from(hi) << 8) | u16::from(v))";
    let closure_new = "Some(lo).map_or(0, |v| u16::from(v) | (u16::from(hi) << 8))";
    for (old, new, assertion) in [
        (
            closure_old,
            closure_new,
            "assert_eq!(pack(0x34, 0x12), 0x1234);",
        ),
        (SWAPPED, PACKED, "assert!(pack(0x34, 0x12) > 0);"),
    ] {
        let findings = TempRepo::create(&source(new, assertion), &diff(old, new))?.findings()?;
        let changed = findings
            .iter()
            .filter(|finding| finding.probe.location.line == 2)
            .collect::<Vec<_>>();
        assert!(
            !changed.is_empty(),
            "premise: a finding on line 2: {findings:?}"
        );
        assert!(
            changed
                .iter()
                .all(|finding| finding.class != ExposureClass::Exposed),
            "{new} / {assertion}: {changed:?}"
        );
    }
    Ok(())
}
