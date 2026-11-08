//! Production-path red witnesses for #13037 tier values.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn lib(function: &str, assertions: &str) -> String {
    format!(
        "{function}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn it_works() {{
        {assertions}
    }}
}}
"
    )
}

/// A unified diff covering the whole function, with correct hunk line counts,
/// so every changed line pairs at its real source line.
fn fn_diff(before_fn: &str, after_fn: &str) -> String {
    let before_lines: Vec<&str> = before_fn.lines().collect();
    let after_lines: Vec<&str> = after_fn.lines().collect();
    assert_eq!(
        before_lines.len(),
        after_lines.len(),
        "fixture functions must align line for line"
    );
    assert!(
        before_lines
            .iter()
            .zip(after_lines.iter())
            .any(|(before, after)| before != after),
        "fixture functions must differ somewhere"
    );
    let count = before_lines.len();
    let mut hunk = String::new();
    for index in 0..count {
        match before_lines[index] == after_lines[index] {
            true => {
                hunk.push(' ');
                hunk.push_str(before_lines[index]);
                hunk.push('\n');
            }
            false => {
                hunk.push('-');
                hunk.push_str(before_lines[index]);
                hunk.push('\n');
                hunk.push('+');
                hunk.push_str(after_lines[index]);
                hunk.push('\n');
            }
        }
    }
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,{count} +1,{count} @@
{hunk}"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-early-return-scalar-credit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"early-return-scalar\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

fn require_exposed(findings: &[Finding], line: usize, literal: &str) -> Result<(), String> {
    let found: Vec<_> = findings
        .iter()
        .filter(|finding| {
            finding.probe.location.line == line && finding.probe.expression.trim() == literal
        })
        .collect();
    assert!(
        !found.is_empty(),
        "the changed tier literal must produce a finding: {findings:?}"
    );
    assert!(
        found
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "an exact pin for the selected current tier should observe the change: {found:?}"
    );
    for finding in found
        .iter()
        .filter(|finding| finding.class == ExposureClass::Exposed)
    {
        assert_eq!(finding.flow_sinks.len(), 1, "return sink: {finding:?}");
        assert_eq!(
            finding.flow_sinks[0].kind,
            ripr::domain::FlowSinkKind::ReturnValue,
            "return sink: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn current_pin_distinguishes_a_changed_tier_returned_as_a_binding() -> Result<(), String> {
    let before = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        9\n    } else {\n        20\n    };\n    base\n}";
    let after = before.replace("        9\n", "        6\n");
    let findings = TempRepo::create(
        &lib(&after, "assert_eq!(tier(4), 6);"),
        &fn_diff(before, &after),
    )?
    .findings()?;
    require_exposed(&findings, 3, "6")
}

#[test]
fn a_new_shipping_shaped_function_exposes_a_selected_tier() -> Result<(), String> {
    let function = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express {\n        base * 2\n    } else {\n        base\n    }\n}";
    let count = function.lines().count();
    let added = function
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -0,0 +1,{count} @@\n{added}"
    );
    let findings = TempRepo::create(&lib(function, "assert_eq!(tier(4, 30, false), 6);"), &patch)?
        .findings()?;
    require_exposed(&findings, 6, "6")
}

#[test]
fn a_new_shipping_shape_with_a_named_limit_exposes_the_selected_tier() -> Result<(), String> {
    let constant = "pub const FREE_OVER: u32 = 100;";
    let function = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {
    if order >= FREE_OVER && !express {
        return 0;
    }
    let base = if weight <= 10 {
        6
    } else {
        20
    };
    if express {
        base * 2
    } else {
        base
    }
}";
    let count = function.lines().count() + 1;
    let added = function
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,{count} @@\n {constant}\n{added}"
    );
    let source = format!("{constant}\n{function}");
    let findings = TempRepo::create(&lib(&source, "assert_eq!(tier(4, 30, false), 6);"), &patch)?
        .findings()?;
    require_exposed(&findings, 7, "6")
}

#[test]
fn unsafe_or_ambiguous_admission_never_credits_a_tier() -> Result<(), String> {
    use ripr::domain::StageState;
    let cases = [
        (
            "same-line calls",
            "weight <= 10",
            "assert_eq!(tier(4), 6); assert_eq!(tier(15), 20);",
        ),
        (
            "unreachable assertion",
            "weight <= 10",
            "if false { assert_eq!(tier(4), 6); }",
        ),
        (
            "invalid oversized comparison",
            "weight <= 4294967296",
            "assert_eq!(tier(4), 6);",
        ),
        ("untyped comparison", "2 < 3", "assert_eq!(tier(4), 6);"),
        (
            "wrong selected input",
            "weight <= 10",
            "assert_eq!(tier(15), 6);",
        ),
    ];
    for (label, condition, assertion) in cases {
        let after = format!(
            "pub fn tier(weight: u32) -> u32 {{\n    let base = if {condition} {{\n        6\n    }} else {{\n        20\n    }};\n    base\n}}"
        );
        let before = after.replace("        6\n", "        9\n");
        let findings =
            TempRepo::create(&lib(&after, assertion), &fn_diff(&before, &after))?.findings()?;
        let changed: Vec<_> = findings
            .iter()
            .filter(|finding| {
                finding.probe.location.line == 3 && finding.probe.expression.trim() == "6"
            })
            .collect();
        assert!(!changed.is_empty(), "{label}: missing changed tier finding");
        assert!(
            changed.iter().all(|finding| !matches!(
                finding.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ) && finding.ripr.reveal.discriminate.state
                != StageState::Yes),
            "{label}: {changed:?}"
        );
    }
    Ok(())
}

#[test]
fn assertion_and_name_resolution_uncertainty_withholds_credit() -> Result<(), String> {
    use ripr::domain::StageState;
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    let assertion = "assert_eq!(tier(4), 6);";
    let cases = [
        (
            "ignored test",
            lib(after, assertion).replace("#[test]", "#[test]\n    #[ignore]"),
        ),
        (
            "expected panic",
            lib(after, assertion).replace("#[test]", "#[test]\n    #[should_panic]"),
        ),
        (
            "shadowed macro",
            lib(
                after,
                "macro_rules! assert_eq { ($($t:tt)*) => {}; }\n        assert_eq!(tier(4), 6);",
            ),
        ),
        (
            "rival local owner",
            lib(
                after,
                "fn tier(_: u32) -> u32 { 6 }\n        assert_eq!(tier(4), 6);",
            ),
        ),
        (
            "uncertain closure",
            lib(
                after,
                "let f = || assert_eq!(tier(4), 6);\n        let _ = f;",
            ),
        ),
    ];
    for (label, source) in cases {
        let findings = TempRepo::create(&source, &fn_diff(&before, after))?.findings()?;
        let changed: Vec<_> = findings
            .iter()
            .filter(|finding| {
                finding.probe.location.line == 3 && finding.probe.expression.trim() == "6"
            })
            .collect();
        assert!(!changed.is_empty(), "{label}: missing changed tier finding");
        assert!(
            changed.iter().all(|finding| !matches!(
                finding.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ) && finding.ripr.reveal.discriminate.state
                != StageState::Yes),
            "{label}: {changed:?}"
        );
    }
    Ok(())
}

#[test]
fn distinct_assertions_remain_distinct_in_the_evidence() -> Result<(), String> {
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    let assertions = "assert_eq!(tier(4), 6);\n        assert_eq!(tier(5), 6);";
    let findings =
        TempRepo::create(&lib(after, assertions), &fn_diff(&before, after))?.findings()?;
    let finding = findings
        .iter()
        .find(|finding| {
            finding.probe.location.line == 3
                && finding.probe.expression.trim() == "6"
                && finding.class == ExposureClass::Exposed
        })
        .ok_or("missing exposed finding")?;
    let rows: Vec<_> = finding
        .related_tests
        .iter()
        .filter_map(|row| row.oracle.as_deref())
        .collect();
    assert!(rows.contains(&"assert_eq!(tier(4), 6);"), "{rows:?}");
    assert!(
        rows.iter()
            .all(|row| *row == "assert_eq!(tier(4), 6);" || *row == "assert_eq!(tier(5), 6);"),
        "{rows:?}"
    );
    assert_eq!(
        rows.iter()
            .filter(|row| **row == "assert_eq!(tier(4), 6);")
            .count(),
        1,
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn shipping_tier_witness_adds_its_own_counted_assertion_row() -> Result<(), String> {
    let constant = "pub const FREE_OVER: u32 = 5_000;";
    let function = "pub fn shipping_cost(weight: u32, order: u32, express: bool) -> u32 {
    if order >= FREE_OVER && !express {
        return 0;
    }
    let base = if weight <= 500 {
        400
    } else if weight <= 2_000 {
        900
    } else {
        1_800
    };
    if express {
        base * 2
    } else {
        base
    }
}";
    let count = function.lines().count() + 1;
    let added = function
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,{count} @@\n {constant}\n{added}"
    );
    let repo = TempRepo::create(&format!("{constant}\n{function}\n"), &patch)?;
    std::fs::create_dir_all(repo.root.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(repo.root.join("tests/shipping.rs"), "use early_return_scalar::shipping_cost;\n#[test]\nfn small_order() { assert!(shipping_cost(100, 1_000, false) > 0); }\n#[test]\nfn costs() {\n    assert_eq!(shipping_cost(100, 5000, false), 0);\n    assert_eq!(shipping_cost(100, 4999, false), 400);\n    assert_eq!(shipping_cost(100, 9000, true), 800);\n    assert_eq!(shipping_cost(500, 10, false), 400);\n    assert_eq!(shipping_cost(501, 10, false), 900);\n    assert_eq!(shipping_cost(2000, 10, false), 900);\n    assert_eq!(shipping_cost(2001, 10, false), 1800);\n    assert_eq!(shipping_cost(100, 10, true), 800);\n    assert_eq!(shipping_cost(600, 10, true), 1800);\n}\n").map_err(|error| error.to_string())?;
    let findings = repo.findings()?;
    let finding = findings
        .iter()
        .find(|finding| finding.probe.expression.trim() == "1_800")
        .ok_or("missing large tier")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    let oracles: Vec<_> = finding
        .related_tests
        .iter()
        .filter_map(|row| row.oracle.as_deref())
        .collect();
    assert!(
        oracles.contains(&"assert_eq!(shipping_cost(2001, 10, false), 1800);"),
        "{oracles:?}"
    );
    assert!(
        oracles.contains(&"assert_eq!(shipping_cost(100, 5000, false), 0);"),
        "{oracles:?}"
    );
    assert_eq!(finding.related_tests_total(), finding.related_tests.len());
    Ok(())
}

#[test]
fn full_related_row_window_does_not_invent_a_witness_row() -> Result<(), String> {
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    let mut tests = String::new();
    for index in 0..10 {
        tests.push_str(&format!(
            "#[test] fn a{index:02}() {{ assert_eq!(tier(11), 20); }}\n"
        ));
    }
    tests.push_str("#[test] fn z_witness() { assert_eq!(tier(4), 6); }\n");
    let source = format!("{after}\n#[cfg(test)] mod tests {{ use super::*;\n{tests}}}\n");
    let findings = TempRepo::create(&source, &fn_diff(&before, after))?.findings()?;
    let finding = findings
        .iter()
        .find(|finding| finding.probe.location.line == 3 && finding.probe.expression.trim() == "6")
        .ok_or("missing changed tier")?;
    assert!(
        finding.related_tests_total() > finding.related_tests.len(),
        "premise: row window must be full: {finding:?}"
    );
    assert!(
        !matches!(
            finding.class,
            ExposureClass::Exposed | ExposureClass::WeaklyExposed
        ),
        "an omitted witness is unresolved: {finding:?}"
    );
    Ok(())
}

#[test]
fn only_selected_unchanged_return_path_gets_a_witness() -> Result<(), String> {
    use ripr::domain::StageState;
    let after = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * 2 } else { base }\n}";
    let before = after.replace("        6\n", "        9\n");
    let cases = [
        (
            "selected current pin",
            "assert_eq!(tier(4, 30, false), 6);",
            true,
        ),
        (
            "selected old pin",
            "assert_eq!(tier(4, 30, false), 9);",
            true,
        ),
        ("other tier", "assert_eq!(tier(11, 30, false), 20);", false),
        ("early return", "assert_eq!(tier(4, 100, false), 0);", false),
        (
            "transformed selected tier",
            "assert_eq!(tier(4, 30, true), 12);",
            true,
        ),
        (
            "transformed other tier",
            "assert_eq!(tier(11, 30, true), 40);",
            false,
        ),
        (
            "express overrides free guard",
            "assert_eq!(tier(4, 100, true), 12);",
            true,
        ),
    ];
    for (label, assertion, should_credit) in cases {
        let findings =
            TempRepo::create(&lib(after, assertion), &fn_diff(&before, after))?.findings()?;
        let finding = findings
            .iter()
            .find(|finding| {
                finding.probe.location.line == 6 && finding.probe.expression.trim() == "6"
            })
            .ok_or_else(|| format!("{label}: missing tier finding"))?;
        if should_credit {
            assert_eq!(
                finding.class,
                ExposureClass::Exposed,
                "{label}: {finding:?}"
            );
        } else {
            assert!(
                !matches!(
                    finding.class,
                    ExposureClass::Exposed | ExposureClass::WeaklyExposed
                ),
                "{label}: {finding:?}"
            );
            assert_ne!(
                finding.ripr.reveal.discriminate.state,
                StageState::Yes,
                "{label}: {finding:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn preceding_failing_assertions_do_not_credit_unreachable_tier_pins() -> Result<(), String> {
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    for prior in [
        "assert!(false);",
        "assert_eq!(1, 2);",
        "assert_eq!(tier(4), 7);",
        "std::hint::black_box(0);",
        "panic!();",
    ] {
        let assertions = format!("{prior}\n        assert_eq!(tier(4), 6);");
        let findings =
            TempRepo::create(&lib(after, &assertions), &fn_diff(&before, after))?.findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
            .ok_or("missing tier")?;
        assert!(
            !matches!(
                f.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ),
            "{prior}: {f:?}"
        );
    }
    Ok(())
}

#[test]
fn proven_passing_assertion_before_selected_pin_is_allowed() -> Result<(), String> {
    let before = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        9\n    } else {\n        20\n    };\n    base\n}";
    let after = before.replace("        9\n", "        6\n");
    let assertions = "assert_eq!(tier(11), 20);\n        assert_eq!(tier(4), 6);";
    let findings =
        TempRepo::create(&lib(&after, assertions), &fn_diff(before, &after))?.findings()?;
    require_exposed(&findings, 3, "6")
}

#[test]
fn failing_or_opaque_suffix_does_not_prove_discrimination() -> Result<(), String> {
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    for suffix in [
        "assert!(false);",
        "assert_eq!(1, 2);",
        "assert_eq!(tier(11), 7);",
        "std::hint::black_box(0);",
        "panic!();",
    ] {
        let assertions = format!("assert_eq!(tier(4), 6);\n        {suffix}");
        let findings =
            TempRepo::create(&lib(after, &assertions), &fn_diff(&before, after))?.findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
            .ok_or("missing tier")?;
        assert!(
            !matches!(
                f.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ),
            "{suffix}: {f:?}"
        );
    }
    Ok(())
}

#[test]
fn proven_passing_assertion_after_selected_pin_is_allowed() -> Result<(), String> {
    let before = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        9\n    } else {\n        20\n    };\n    base\n}";
    let after = before.replace("        9\n", "        6\n");
    for selected in ["6", "9"] {
        let assertions =
            format!("assert_eq!(tier(4), {selected});\n        assert_eq!(tier(11), 20);");
        let findings =
            TempRepo::create(&lib(&after, &assertions), &fn_diff(before, &after))?.findings()?;
        require_exposed(&findings, 3, "6")?;
    }
    Ok(())
}

#[test]
fn computed_and_overflowing_guard_constants_remain_unknown() -> Result<(), String> {
    let function = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= LIMIT && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * 2 } else { base }\n}";
    for initializer in ["50 + 50", "0 - 1 + 101"] {
        let constant = format!("pub const LIMIT: u32 = {initializer};");
        let added = function
            .lines()
            .map(|line| format!("+{line}\n"))
            .collect::<String>();
        let count = function.lines().count() + 1;
        let patch = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,{count} @@\n {constant}\n{added}"
        );
        let source = format!("{constant}\n{function}");
        let findings =
            TempRepo::create(&lib(&source, "assert_eq!(tier(4, 30, false), 6);"), &patch)?
                .findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 7 && f.probe.expression.trim() == "6")
            .ok_or("missing tier")?;
        assert!(
            !matches!(
                f.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ),
            "{initializer}: {f:?}"
        );
    }
    Ok(())
}

#[test]
fn branch_and_assertion_budgets_fail_closed_at_the_boundary() -> Result<(), String> {
    for (extra_branches, should_credit) in [(14, true), (15, false)] {
        let mut after = String::from(
            "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 0 {\n        6\n    }",
        );
        for n in 1..=extra_branches {
            after.push_str(&format!(" else if weight <= {n} {{\n        20\n    }}"));
        }
        after.push_str(" else {\n        20\n    };\n    base\n}");
        let before = after.replace("        6\n", "        9\n");
        let findings = TempRepo::create(
            &lib(&after, "assert_eq!(tier(0), 6);"),
            &fn_diff(&before, &after),
        )?
        .findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
            .ok_or("missing budget tier")?;
        assert_eq!(
            f.class == ExposureClass::Exposed,
            should_credit,
            "branches={extra_branches}: {f:?}"
        );
        if !should_credit {
            assert_ne!(f.class, ExposureClass::WeaklyExposed);
        }
    }
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    for (extra_assertions, should_credit) in [(15, true), (16, false)] {
        let mut assertions = String::from("assert_eq!(tier(4), 6);\n");
        for _ in 0..extra_assertions {
            assertions.push_str("        assert_eq!(tier(11), 20);\n");
        }
        let findings =
            TempRepo::create(&lib(after, &assertions), &fn_diff(&before, after))?.findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
            .ok_or("missing assertion budget tier")?;
        assert_eq!(
            f.class == ExposureClass::Exposed,
            should_credit,
            "assertions={extra_assertions}: {f:?}"
        );
        if !should_credit {
            assert_ne!(f.class, ExposureClass::WeaklyExposed);
        }
    }
    Ok(())
}

#[test]
fn changed_line_budget_fails_closed_at_the_boundary() -> Result<(), String> {
    let function = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    for (total, should_credit) in [(256usize, true), (257usize, false)] {
        let filler = total - function.lines().count();
        let source = format!("{}{function}", "// unchanged context\n".repeat(filler));
        let added = source
            .lines()
            .map(|line| format!("+{line}\n"))
            .collect::<String>();
        let patch = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -0,0 +1,{total} @@\n{added}"
        );
        let findings =
            TempRepo::create(&lib(&source, "assert_eq!(tier(4), 6);"), &patch)?.findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == filler + 3 && f.probe.expression.trim() == "6")
            .ok_or("missing changed tier")?;
        assert_eq!(
            f.class == ExposureClass::Exposed,
            should_credit,
            "lines={total}: {f:?}"
        );
        if !should_credit {
            assert_ne!(f.class, ExposureClass::WeaklyExposed);
        }
    }
    Ok(())
}

#[test]
fn unary_and_long_expression_budgets_withhold_unreadable_inputs() -> Result<(), String> {
    for prefix in [
        "!".repeat(33),
        "- ".repeat(33),
        "* ".repeat(33),
        "& ".repeat(33),
    ] {
        let after = format!(
            "pub fn tier(flag: bool) -> u32 {{\n    let base = if {prefix}flag {{\n        6\n    }} else {{\n        20\n    }};\n    base\n}}"
        );
        let before = after.replace("        6\n", "        9\n");
        let findings = TempRepo::create(
            &lib(&after, "assert_eq!(tier(true), 6);"),
            &fn_diff(&before, &after),
        )?
        .findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
            .ok_or("missing unary budget tier")?;
        assert!(
            !matches!(
                f.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ),
            "unary: {f:?}"
        );
    }
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    let assertion = format!("assert_eq!(tier(4{}), 6);", " + 0".repeat(130));
    let findings =
        TempRepo::create(&lib(after, &assertion), &fn_diff(&before, after))?.findings()?;
    let f = findings
        .iter()
        .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
        .ok_or("missing long oracle tier")?;
    assert!(
        !matches!(
            f.class,
            ExposureClass::Exposed | ExposureClass::WeaklyExposed
        ),
        "long oracle: {f:?}"
    );
    Ok(())
}

#[test]
fn unrelated_unary_constant_cannot_overrun_the_const_reader() -> Result<(), String> {
    let first = format!("const BOMB: bool = {}false;", "!".repeat(129));
    let second = "pub const LIMIT: u32 = 100;";
    let function = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= LIMIT && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * 2 } else { base }\n}";
    let added = function
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    let count = function.lines().count() + 2;
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,{count} @@\n {first}\n {second}\n{added}"
    );
    let source = format!("{first}\n{second}\n{function}");
    let findings = TempRepo::create(&lib(&source, "assert_eq!(tier(4, 30, false), 6);"), &patch)?
        .findings()?;
    let f = findings
        .iter()
        .find(|f| f.probe.location.line == 8 && f.probe.expression.trim() == "6")
        .ok_or("missing const budget tier")?;
    assert!(
        !matches!(
            f.class,
            ExposureClass::Exposed | ExposureClass::WeaklyExposed
        ),
        "const budget: {f:?}"
    );
    Ok(())
}

#[test]
fn test_file_budget_has_an_unpadded_positive_twin() -> Result<(), String> {
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    for (padding, should_credit) in [(0, true), (9_000, false)] {
        let repo = TempRepo::create(after, &fn_diff(&before, after))?;
        std::fs::create_dir_all(repo.root.join("tests")).map_err(|error| error.to_string())?;
        let test = format!(
            "use early_return_scalar::tier;\n#[test]\nfn pin() {{\n    assert_eq!(tier(4), 6);\n}}\n{}",
            "// extra padding\n".repeat(padding)
        );
        std::fs::write(repo.root.join("tests/pin.rs"), test).map_err(|error| error.to_string())?;
        let findings = repo.findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
            .ok_or("missing file budget tier")?;
        assert_eq!(
            f.class == ExposureClass::Exposed,
            should_credit,
            "test padding={padding}: {f:?}"
        );
        if !should_credit {
            assert_ne!(f.class, ExposureClass::WeaklyExposed);
        }
    }
    Ok(())
}

#[test]
fn owner_file_budget_has_an_unpadded_positive_twin() -> Result<(), String> {
    let after = "pub fn tier(weight: u32) -> u32 {\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    base\n}";
    let before = after.replace("        6\n", "        9\n");
    for (padding, should_credit) in [(0, true), (9_000, false)] {
        let source = format!("{after}\n{}", "// extra padding\n".repeat(padding));
        let repo = TempRepo::create(&source, &fn_diff(&before, after))?;
        std::fs::create_dir_all(repo.root.join("tests")).map_err(|error| error.to_string())?;
        std::fs::write(
            repo.root.join("tests/pin.rs"),
            "use early_return_scalar::tier;\n#[test]\nfn pin() {\n    assert_eq!(tier(4), 6);\n}\n",
        )
        .map_err(|error| error.to_string())?;
        let findings = repo.findings()?;
        let f = findings
            .iter()
            .find(|f| f.probe.location.line == 3 && f.probe.expression.trim() == "6")
            .ok_or("missing owner budget tier")?;
        assert_eq!(
            f.class == ExposureClass::Exposed,
            should_credit,
            "owner padding={padding}: {f:?}"
        );
        if !should_credit {
            assert_ne!(f.class, ExposureClass::WeaklyExposed);
        }
    }
    Ok(())
}

#[test]
fn computed_selected_tier_return_is_credited() -> Result<(), String> {
    // now * K: 6 * 2 = 12 holds on the new code, 9 * 2 = 18 on the old.
    let after = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * 2 } else { base }\n}";
    let before = after.replace("        6\n", "        9\n");
    for assertion in [
        "assert_eq!(tier(4, 30, true), 12);",
        "assert_eq!(tier(4, 30, true), 18);",
    ] {
        let findings =
            TempRepo::create(&lib(after, assertion), &fn_diff(&before, after))?.findings()?;
        require_exposed(&findings, 6, "6")?;
    }
    // The multiplied branch may be the else tail instead.
    let swapped = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base } else { base * 2 }\n}";
    let swapped_before = swapped.replace("        6\n", "        9\n");
    let findings = TempRepo::create(
        &lib(swapped, "assert_eq!(tier(4, 30, false), 12);"),
        &fn_diff(&swapped_before, swapped),
    )?
    .findings()?;
    require_exposed(&findings, 6, "6")
}

#[test]
fn shipping_scaled_computed_tier_is_credited() -> Result<(), String> {
    let after = "pub fn shipping_cost(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 5000 && !express {\n        return 0;\n    }\n    let base = if weight <= 500 {\n        400\n    } else if weight <= 2000 {\n        900\n    } else {\n        1800\n    };\n    if express {\n        base * 2\n    } else {\n        base\n    }\n}";
    let before = after.replace("        400\n", "        450\n");
    let findings = TempRepo::create(
        &lib(after, "assert_eq!(shipping_cost(100, 10, true), 800);"),
        &fn_diff(&before, after),
    )?
    .findings()?;
    require_exposed(&findings, 6, "400")
}

#[test]
fn computed_tier_negatives_stay_unknown() -> Result<(), String> {
    use ripr::domain::StageState;
    let guard_fn = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * 2 } else { base }\n}";
    let cases: [(&str, &str, &str); 11] = [
        (
            "untransformed pin on transformed path",
            guard_fn,
            "assert_eq!(tier(4, 30, true), 6);",
        ),
        (
            "wrong multiplier",
            guard_fn,
            "assert_eq!(tier(4, 30, true), 24);",
        ),
        (
            "guard-taken early return",
            guard_fn,
            "assert_eq!(tier(4, 100, false), 0);",
        ),
        (
            "same-line multi-call",
            guard_fn,
            "assert_eq!(tier(4, 30, true), 12); assert_eq!(tier(11, 30, true), 40);",
        ),
        (
            "literal on the left",
            "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { 2 * base } else { base }\n}",
            "assert_eq!(tier(4, 30, true), 12);",
        ),
        (
            "added binding",
            "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base + base } else { base }\n}",
            "assert_eq!(tier(4, 30, true), 12);",
        ),
        (
            "parameter multiplier",
            "pub fn tier(weight: u32, order: u32, express: bool, factor: u32) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * factor } else { base }\n}",
            "assert_eq!(tier(4, 30, true, 2), 12);",
        ),
        (
            "chained multiplication",
            "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * 2 * 2 } else { base }\n}",
            "assert_eq!(tier(4, 30, true), 24);",
        ),
        (
            "cast return",
            "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { (base * 2) as u32 } else { base }\n}",
            "assert_eq!(tier(4, 30, true), 12);",
        ),
        (
            "helper-call return",
            "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { double(base) } else { base }\n}",
            "assert_eq!(tier(4, 30, true), 12);",
        ),
        (
            "overflowing product",
            "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        2500000000\n    } else {\n        20\n    };\n    if express { base * 2 } else { base }\n}",
            "assert_eq!(tier(4, 30, true), 5000000000);",
        ),
    ];
    for (label, after, assertion) in cases {
        let before = if label == "overflowing product" {
            after.replace("2500000000", "3000000000")
        } else {
            after.replace("        6\n", "        9\n")
        };
        let header = if label == "helper-call return" {
            "fn double(value: u32) -> u32 { value * 2 }\n"
        } else {
            ""
        };
        let full_after = format!("{header}{after}");
        let source = lib(&full_after, assertion);
        let before = format!("{header}{before}");
        let findings = TempRepo::create(&source, &fn_diff(&before, &full_after))?.findings()?;
        let changed: Vec<_> = findings
            .iter()
            .filter(|finding| {
                finding.probe.expression.trim() == "6"
                    || finding.probe.expression.trim() == "2500000000"
            })
            .collect();
        assert!(!changed.is_empty(), "{label}: missing changed tier finding");
        assert!(
            changed.iter().all(|finding| !matches!(
                finding.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ) && finding.ripr.reveal.discriminate.state
                != StageState::Yes),
            "{label}: {changed:?}"
        );
    }
    Ok(())
}

#[test]
fn computed_pin_context_and_identity_uncertainty_withholds_credit() -> Result<(), String> {
    use ripr::domain::StageState;
    let after = "pub fn tier(weight: u32, order: u32, express: bool) -> u32 {\n    if order >= 100 && !express {\n        return 0;\n    }\n    let base = if weight <= 10 {\n        6\n    } else {\n        20\n    };\n    if express { base * 2 } else { base }\n}";
    let before = after.replace("        6\n", "        9\n");
    let cases = [
        (
            "shadowed macro",
            lib(
                after,
                "macro_rules! assert_eq { ($($t:tt)*) => {}; }\n        assert_eq!(tier(4, 30, true), 12);",
            ),
        ),
        (
            "rival local owner",
            lib(
                after,
                "fn tier(_: u32, _: u32, _: bool) -> u32 { 12 }\n        assert_eq!(tier(4, 30, true), 12);",
            ),
        ),
        (
            "failing prefix",
            lib(
                after,
                "assert!(false);\n        assert_eq!(tier(4, 30, true), 12);",
            ),
        ),
        (
            "opaque suffix",
            lib(
                after,
                "assert_eq!(tier(4, 30, true), 12);\n        panic!();",
            ),
        ),
    ];
    for (label, source) in cases {
        let findings = TempRepo::create(&source, &fn_diff(&before, after))?.findings()?;
        let changed: Vec<_> = findings
            .iter()
            .filter(|finding| {
                finding.probe.location.line == 6 && finding.probe.expression.trim() == "6"
            })
            .collect();
        assert!(!changed.is_empty(), "{label}: missing changed tier finding");
        assert!(
            changed.iter().all(|finding| !matches!(
                finding.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            ) && finding.ripr.reveal.discriminate.state
                != StageState::Yes),
            "{label}: {changed:?}"
        );
    }
    Ok(())
}
