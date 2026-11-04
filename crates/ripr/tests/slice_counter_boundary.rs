//! Public-API controls for per-byte slice counter boundaries (#5346).
//!
//! A boundary on a counter that grows once per input byte is pinned only when
//! the tests assert the outcome of both literal-input calls that straddle it:
//! the longer call must assert the value the owner returns when the predicate
//! fires and the shorter call something else. One asserted side, or equal
//! outcomes (an earlier `?` exit that never reaches the counter), is not a pin.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "#[derive(Debug, PartialEq)]
pub enum ChunkError {
    TooLong,
    BadDigit,
}

pub fn chunk_size(hex: &[u8]) -> Result<u64, ChunkError> {
    let mut size: u64 = 0;
    let mut count = 0;
    for &b in hex {
        count += 1;
        if CONDITION {
            return Err(ChunkError::TooLong);
        }
        let digit = (b as char).to_digit(16).ok_or(ChunkError::BadDigit)?;
        size = (size << 4) | u64::from(digit);
    }
    Ok(size)
}
";

fn source(condition: &str, assertions: &str) -> String {
    format!(
        "{}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn chunk_sizes() {{\n{assertions}\n    }}\n}}\n",
        OWNER.replace("CONDITION", condition)
    )
}

/// A diff rewriting `count > 16` to `16 < count` on the line after
/// `count += 1;`.
fn diff() -> String {
    let line = OWNER
        .lines()
        .position(|text| text.contains("CONDITION"))
        .map_or(0, |index| index + 1);
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{start},3 +{start},3 @@
         count += 1;
-        if count > 16 {{
+        if 16 < count {{
             return Err(ChunkError::TooLong);
",
        start = line - 1
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-slice-counter-boundary-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"slice-counter\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

fn class_for(condition: &str, assertions: &str) -> Result<ExposureClass, String> {
    let findings = TempRepo::create(&source(condition, assertions), &diff())?.findings()?;
    let mut predicate = findings
        .iter()
        .filter(|finding| finding.probe.family == ProbeFamily::Predicate);
    match (predicate.next(), predicate.next()) {
        (Some(finding), None) => Ok(finding.class.clone()),
        other => Err(format!(
            "premise: exactly one predicate finding, got {other:?} in {findings:?}"
        )),
    }
}

const FF16: &str = "        assert_eq!(chunk_size(&[b'f'; 16]), Ok(u64::MAX));";
const FF17: &str = "        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));";

#[test]
fn asserting_both_adjacent_lengths_exposes_the_counter_boundary() -> Result<(), String> {
    let assertions = format!("{FF16}\n{FF17}");
    assert_eq!(
        class_for("16 < count", &assertions)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn one_asserted_side_or_equal_outcomes_are_not_a_pin() -> Result<(), String> {
    for (label, assertions) in [
        (
            "only the 17-byte call asserted",
            format!("        assert!(chunk_size(&[b'f'; 16]).is_ok());\n{FF17}"),
        ),
        (
            "only the 16-byte call asserted",
            format!("{FF16}\n        let _ = chunk_size(&[b'f'; 17]);"),
        ),
        (
            "BadDigit at both lengths (the loop exits at the first element)",
            "        assert_eq!(chunk_size(&[b'z'; 16]), Err(ChunkError::BadDigit));
        assert_eq!(chunk_size(&[b'z'; 17]), Err(ChunkError::BadDigit));"
                .to_string(),
        ),
        (
            "different input content at 16 and 17 (count >= 16 passes both)",
            "        assert_eq!(chunk_size(&[b'z'; 16]), Err(ChunkError::BadDigit));
        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));"
                .to_string(),
        ),
        (
            "off-boundary lengths 15 and 17",
            "        assert_eq!(chunk_size(&[b'f'; 15]), Ok(u64::MAX >> 4));
        assert_eq!(chunk_size(&[b'f'; 17]), Err(ChunkError::TooLong));"
                .to_string(),
        ),
    ] {
        assert_ne!(
            class_for("16 < count", &assertions)?,
            ExposureClass::Exposed,
            "{label}"
        );
    }
    Ok(())
}
