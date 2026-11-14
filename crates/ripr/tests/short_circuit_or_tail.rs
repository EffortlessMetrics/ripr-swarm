//! End-to-end witness for the RIPR-SPEC-0197 short-circuit tail
//! propagation edge (#8037).
//!
//! An admitted `||` chain that is the owner's whole body completes its
//! return-value witness, so a constant-row table that runs every operand
//! exposes the return-value probe. Whether a pinned run evaluates every
//! operand stays the owner pin's run check: with the same owner and diff, a
//! table or direct calls that never evaluate the changed operand keep the
//! probe below `exposed` (the #7970 shape) while its propagation stays
//! complete, so the refusal comes from observation, not from the edge.

use ripr::domain::StageState;
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

fn direct_test(crate_name: &str, asserts: &[&str]) -> String {
    let body: String = asserts
        .iter()
        .map(|assert| format!("    {assert}\n"))
        .collect();
    format!("use {crate_name}::*;\n\n#[test]\nfn direct() {{\n{body}}}\n")
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

/// The class and propagation state of the single current return-value
/// finding on line 2, after asserting the fixture produced it from the
/// changed tail.
fn return_value_class(
    source: &str,
    old_line: &str,
    tail: &str,
    test_source: &str,
) -> Result<(ExposureClass, StageState), String> {
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
    Ok((finding.class.clone(), finding.ripr.propagate.state.clone()))
}

#[test]
fn an_or_tail_run_to_its_end_by_a_table_is_exposed() -> Result<(), String> {
    let test = table_test(
        "short_circuit_or_tail",
        "is_weekend(day)",
        "(0, true), (1, false), (5, false), (6, true), (13, true)",
        "day",
    );
    let (class, propagate) = return_value_class(WEEKEND_SOURCE, WEEKEND_OLD, WEEKEND_TAIL, &test)?;
    assert_eq!(class, ExposureClass::Exposed);
    assert_eq!(propagate, StageState::Yes);
    // The same owner and diff the refusals below use: `x == 1` lets the
    // changed `y == 1` run in every row.
    let test = table_test(
        "short_circuit_or_tail",
        "either(x, y)",
        "(1, 1, true), (1, 0, false)",
        "x, y",
    );
    let (class, _) = return_value_class(EITHER_SOURCE, EITHER_OLD, EITHER_TAIL, &test)?;
    assert_eq!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_or_tail_whose_changed_operand_never_runs_stays_below_exposed() -> Result<(), String> {
    // Every row has `x == 0`, so `||` returns before the changed `y == 1`
    // runs (#7970), though the rows hit its boundary values: the witness
    // completes, and the pin's run check refuses.
    let table = table_test(
        "short_circuit_or_tail",
        "either(x, y)",
        "(0, 1, true), (0, 0, true), (0, 2, true)",
        "x, y",
    );
    let direct = direct_test(
        "short_circuit_or_tail",
        &[
            "assert_eq!(either(0, 1), true);",
            "assert_eq!(either(0, 2), true);",
        ],
    );
    for test in [table, direct] {
        let (class, propagate) = return_value_class(EITHER_SOURCE, EITHER_OLD, EITHER_TAIL, &test)?;
        assert_ne!(class, ExposureClass::Exposed, "{test}");
        assert_eq!(propagate, StageState::Yes, "{test}");
    }
    Ok(())
}

// #8361 (`benchmarks/stub_quality/cases/D_unsafe_const`): the edit drops or
// adds one whole guard operand. On a pinned call whose literal argument
// leaves that operand at its chain's identity value (`true` for `&&`), both
// versions return the same value, so the pin cannot notice the edit.
const POW2_SOURCE: &str = "pub fn is_pow2(n: u32) -> bool {\n    n & (n - 1) == 0\n}\n";
const POW2_OLD: &str = "    n != 0 && n & (n - 1) == 0";
const POW2_TAIL: &str = "n & (n - 1) == 0";

const EVEN_SOURCE: &str = "pub fn is_even(n: u32) -> bool {\n    n != 0 && n % 2 == 0\n}\n";
const EVEN_OLD: &str = "    n % 2 == 0";
const EVEN_TAIL: &str = "n != 0 && n % 2 == 0";

#[test]
fn a_dropped_guard_no_pinned_call_makes_decide_stays_below_exposed() -> Result<(), String> {
    let agreeing = direct_test(
        "short_circuit_or_tail",
        &["assert!(is_pow2(4));", "assert!(!is_pow2(6));"],
    );
    let (class, _) = return_value_class(POW2_SOURCE, POW2_OLD, POW2_TAIL, &agreeing)?;
    assert_ne!(class, ExposureClass::Exposed, "{agreeing}");
    let table = table_test(
        "short_circuit_or_tail",
        "is_pow2(n)",
        "(4, true), (6, false), (1, true)",
        "n",
    );
    let (class, _) = return_value_class(POW2_SOURCE, POW2_OLD, POW2_TAIL, &table)?;
    assert_ne!(class, ExposureClass::Exposed, "{table}");
    Ok(())
}

#[test]
fn a_dropped_guard_a_pinned_call_makes_decide_keeps_its_exposure() -> Result<(), String> {
    // `is_pow2(0)` is where the dropped `n != 0` was `false`: the old
    // version returned `false` there, so this pin tells the versions apart.
    for asserts in [
        &["assert!(is_pow2(0));"][..],
        &["assert!(is_pow2(4));", "assert!(is_pow2(0));"][..],
    ] {
        let test = direct_test("short_circuit_or_tail", asserts);
        let (class, _) = return_value_class(POW2_SOURCE, POW2_OLD, POW2_TAIL, &test)?;
        assert_eq!(class, ExposureClass::Exposed, "{test}");
    }
    // An argument ripr cannot read keeps today's reading: the agreement is
    // shown, never assumed.
    let unread = "use short_circuit_or_tail::*;\n\n#[test]\nfn direct() {\n    let n = 4;\n    assert!(is_pow2(n));\n}\n";
    let (class, _) = return_value_class(POW2_SOURCE, POW2_OLD, POW2_TAIL, unread)?;
    assert_eq!(class, ExposureClass::Exposed, "{unread}");
    Ok(())
}

#[test]
fn an_added_guard_follows_the_same_agreement_rule() -> Result<(), String> {
    let agreeing = direct_test(
        "short_circuit_or_tail",
        &["assert!(is_even(4));", "assert!(!is_even(3));"],
    );
    let (class, _) = return_value_class(EVEN_SOURCE, EVEN_OLD, EVEN_TAIL, &agreeing)?;
    assert_ne!(class, ExposureClass::Exposed, "{agreeing}");
    // A table whose `(0, false)` row makes the added `n != 0` decide keeps
    // the credit its `(4, true)` row earns under item 2's run check: the
    // agreement must hold on every run of the call. (A lone
    // `assert!(!is_even(0))` stops at the guard and was never a pin.)
    let deciding = table_test(
        "short_circuit_or_tail",
        "is_even(n)",
        "(4, true), (0, false)",
        "n",
    );
    let (class, _) = return_value_class(EVEN_SOURCE, EVEN_OLD, EVEN_TAIL, &deciding)?;
    assert_eq!(class, ExposureClass::Exposed, "{deciding}");
    Ok(())
}
