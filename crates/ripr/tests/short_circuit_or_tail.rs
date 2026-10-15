//! End-to-end witness for the RIPR-SPEC-0197 short-circuit tail
//! propagation edge (#8037).
//!
//! An admitted `||` chain that is the owner's whole body completes its
//! return-value witness, so a constant-row table that runs every operand
//! exposes the return-value probe. Whether a pinned run evaluates every
//! operand stays the owner pin's run check: a table that never evaluates
//! the changed operand keeps the probe below `exposed` (the #7970 shape).

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const WEEKEND_SOURCE: &str =
    "pub fn is_weekend(day: u8) -> bool {\n    day % 7 == 6 || day % 7 == 0\n}\n";
const WEEKEND_OLD: &str = "    day % 7 == 0 || day % 7 == 6";
const WEEKEND_TAIL: &str = "day % 7 == 6 || day % 7 == 0";

const EITHER_SOURCE: &str = "pub fn either(x: u8, y: u8) -> bool {\n    x == 0 || y == 1\n}\n";
const EITHER_OLD: &str = "    x == 0 || y == 0";
const EITHER_TAIL: &str = "x == 0 || y == 1";

fn table_test(crate_name: &str, call: &str, rows: &str, pattern: &str) -> String {
    format!(
        "use {crate_name}::*;\n\n#[test]\nfn table() {{\n    let cases = [{rows}];\n    for ({pattern}, want) in cases {{\n        assert_eq!({call}, want);\n    }}\n}}\n"
    )
}

fn single_line_diff(source: &str, old_line: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut body = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index == 1 {
            body.push_str(&format!("-{old_line}\n+{line}\n"));
        } else {
            body.push_str(&format!(" {line}\n"));
        }
    }
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n{body}",
        count = lines.len()
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, test_source: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-short-circuit-or-tail-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"short-circuit-or-tail\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/table.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The class of the single current return-value finding on line 2, after
/// asserting the fixture produced it from the changed tail.
fn return_value_class(
    source: &str,
    old_line: &str,
    tail: &str,
    test_source: &str,
) -> Result<ExposureClass, String> {
    let repo = TempRepo::create(source, test_source, &single_line_diff(source, old_line))?;
    let output = repo.check()?;
    let mut matches = output.findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.line == 2
            && finding.probe.after.is_some()
    });
    let finding = matches
        .next()
        .ok_or_else(|| "missing return_value finding at line 2".to_string())?;
    if matches.next().is_some() {
        return Err("duplicate return_value findings at line 2".to_string());
    }
    if finding.probe.expression.trim() != tail {
        return Err(format!(
            "return_value finding is `{}`, not the changed tail",
            finding.probe.expression
        ));
    }
    Ok(finding.class.clone())
}

#[test]
fn an_or_tail_run_to_its_end_by_a_table_is_exposed() -> Result<(), String> {
    let test = table_test(
        "short_circuit_or_tail",
        "is_weekend(day)",
        "(0, true), (1, false), (5, false), (6, true), (13, true)",
        "day",
    );
    assert_eq!(
        return_value_class(WEEKEND_SOURCE, WEEKEND_OLD, WEEKEND_TAIL, &test)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_or_tail_whose_changed_operand_never_runs_stays_below_exposed() -> Result<(), String> {
    // Every row has `x == 0`, so `||` returns before the changed `y == 1`
    // runs (#7970): the witness completes, but the pin's run check refuses.
    let test = table_test(
        "short_circuit_or_tail",
        "either(x, y)",
        "(0, 3, true), (0, 0, true)",
        "x, y",
    );
    let class = return_value_class(EITHER_SOURCE, EITHER_OLD, EITHER_TAIL, &test)?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}
