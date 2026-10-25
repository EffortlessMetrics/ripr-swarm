//! Execution-admission for the assertion-macro family (#8983): an assertion that
//! cannot execute in its test must not credit the finding, for every admitted
//! macro name, bare or `std::`-qualified. A live assertion keeps its credit and
//! a dead sibling next to it does not change it.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::{Path, PathBuf};

#[test]
fn dead_context_assertions_never_credit_and_live_ones_keep_credit() -> Result<(), String> {
    let evidence =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("dead_context_assertion_admission");
    std::fs::create_dir_all(&evidence).map_err(|e| e.to_string())?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_side_effect");
    let effect_source =
        std::fs::read_to_string(fixture.join("input/src/lib.rs")).map_err(|e| e.to_string())?;
    let effect_diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -16,4 +16,4 @@ impl Notifier {\n \n pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {\n-    notifier.send(\"static_msg\")\n+    notifier.send(order_id)\n }\n".to_string();
    assert_eq!(
        effect_source.lines().nth(17),
        Some("    notifier.send(order_id)")
    );
    assert!(effect_diff.contains("@@ -16,4 +16,4 @@"));
    let return_source = "pub fn compute_score(base: i32) -> i32 {\n    base * 2\n}\n";
    let return_diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn compute_score(base: i32) -> i32 {\n-    base + 1\n+    base * 2\n }\n";
    let mut outcomes = std::collections::BTreeMap::new();
    let mut count = 0usize;
    for subject in ["effect", "return"] {
        for qualification in ["", "std::"] {
            for macro_name in [
                "assert_eq",
                "assert",
                "assert_ne",
                "debug_assert_eq",
                "debug_assert",
                "debug_assert_ne",
            ] {
                let actual = if subject == "effect" {
                    "notifier.sent.borrow().first().map(String::as_str)"
                } else {
                    "compute_score(4)"
                };
                let expected = if subject == "effect" {
                    "Some(\"order-42\")"
                } else {
                    "8"
                };
                let wrong = if subject == "effect" {
                    "Some(\"static_msg\")"
                } else {
                    "5"
                };
                let arguments = if macro_name.ends_with("_ne") {
                    format!("{actual}, {wrong}")
                } else if macro_name.ends_with("_eq") {
                    format!("{actual}, {expected}")
                } else {
                    format!("{actual} == {expected}")
                };
                let assertion = format!("{qualification}{macro_name}!({arguments});");
                for context in [
                    "direct",
                    "block_return",
                    "loop_return",
                    "else_true",
                    "fn_uncalled",
                    "match_impossible",
                    "cfg_false",
                    "after_return",
                    "if_false",
                    "closure_uncalled",
                    "mixed",
                ] {
                    let body = match context {
                        "direct" => assertion.clone(),
                        "block_return" => format!("{{ return; }}\n{assertion}"),
                        "loop_return" => format!("loop {{ return; }}\n{assertion}"),
                        "else_true" => format!("if true {{ }} else {{\n{assertion}\n}}"),
                        "fn_uncalled" if subject == "effect" => {
                            format!("fn never_called(notifier: &Notifier) {{\n{assertion}\n}}")
                        }
                        "fn_uncalled" => format!("fn never_called() {{\n{assertion}\n}}"),
                        "match_impossible" => {
                            format!("match 1 {{ 2 => {{\n{assertion}\n}} _ => {{}} }}")
                        }
                        "cfg_false" => format!("if cfg!(any()) {{\n{assertion}\n}}"),
                        "after_return" => format!("return;\n{assertion}"),
                        "if_false" => format!("if false {{\n{assertion}\n}}"),
                        "closure_uncalled" => format!("let _never = || {{\n{assertion}\n}};"),
                        "mixed" => format!("{assertion}\n{{ return; }}\n{assertion}"),
                        _ => return Err("unknown context".into()),
                    };
                    let (production, diff, setup, test_name, line, expression, families) =
                        if subject == "effect" {
                            (
                                effect_source.as_str(),
                                effect_diff.as_str(),
                                "let notifier = Notifier::new();\nprocess_order(&notifier, \"order-42\");",
                                "process_order_observed",
                                18,
                                "notifier.send(order_id)",
                                vec!["return_value", "side_effect"],
                            )
                        } else {
                            (
                                return_source,
                                return_diff,
                                "",
                                "compute_score_observed",
                                2,
                                "base * 2",
                                vec!["return_value"],
                            )
                        };
                    let source = format!(
                        "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn {test_name}() {{\n{setup}\n{body}\n}}\n}}\n"
                    );
                    let key = format!(
                        "{subject}_{}{macro_name}_{context}",
                        if qualification.is_empty() { "" } else { "std_" }
                    );
                    let root = evidence.join(&key);
                    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
                    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"issue8983_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
                    std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
                    std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
                    let report = check_workspace(CheckInput {
                        root: root.clone(),
                        diff_file: Some(root.join("diff.patch")),
                        mode: Mode::Fast,
                        format: OutputFormat::Json,
                        include_unchanged_tests: true,
                        ..CheckInput::default()
                    })?;
                    let rendered = render_check(&report, &OutputFormat::Json)?;
                    std::fs::write(root.join("report.json"), &rendered)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(
                        root.join("report.txt"),
                        render_check(&report, &OutputFormat::Human)?,
                    )
                    .map_err(|e| e.to_string())?;
                    let json: serde_json::Value =
                        serde_json::from_str(&rendered).map_err(|e| e.to_string())?;
                    assert_eq!(json["analysis_outcome"]["analysis_complete"], true, "{key}");
                    assert_eq!(
                        report.findings.len(),
                        families.len(),
                        "{key}: {:#?}",
                        report.findings
                    );
                    let mut observed = report
                        .findings
                        .iter()
                        .map(|f| f.probe.family.as_str())
                        .collect::<Vec<_>>();
                    observed.sort();
                    assert_eq!(observed, families, "{key}");
                    for finding in &report.findings {
                        assert_eq!(finding.probe.location.line, line, "{key}");
                        assert_eq!(finding.probe.expression.trim(), expression, "{key}");
                        assert!(!finding.related_tests.is_empty(), "{key}");
                    }
                    let credited = |family: &str| {
                        report
                            .findings
                            .iter()
                            .find(|f| f.probe.family.as_str() == family)
                            .map(|f| matches!(f.class.as_str(), "exposed" | "weakly_exposed"))
                            .unwrap_or(false)
                    };
                    let classes = report
                        .findings
                        .iter()
                        .map(|f| {
                            (
                                f.probe.family.as_str().to_string(),
                                f.class.as_str().to_string(),
                            )
                        })
                        .collect::<Vec<_>>();
                    let live = context == "direct" || context == "mixed";
                    if !live {
                        for family in &families {
                            assert!(
                                !credited(family),
                                "{key}: dead assertion credited {family}: {classes:?}"
                            );
                        }
                    }
                    if subject == "return" && live && macro_name == "assert_eq" {
                        assert!(
                            credited("return_value"),
                            "{key}: live return credit lost: {classes:?}"
                        );
                    }
                    if subject == "effect" && live {
                        assert!(
                            credited("side_effect"),
                            "{key}: live credit lost: {classes:?}"
                        );
                    }
                    outcomes.insert(key, classes);
                    count += 1;
                }
            }
        }
    }
    for (key, classes) in &outcomes {
        if let Some(prefix) = key.strip_suffix("_mixed") {
            assert_eq!(
                outcomes.get(&format!("{prefix}_direct")),
                Some(classes),
                "{key}: a dead sibling changed a live assertion's credit"
            );
        }
    }
    assert_eq!(count, 264);
    Ok(())
}

/// A check helper that owns the assertion keeps its credit for every admitted
/// macro: the helper collector reads the same six names as the test body.
#[test]
fn helper_borne_assertions_keep_credit_for_every_admitted_macro() -> Result<(), String> {
    let root_dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("helper_borne_assertion_admission");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_side_effect");
    let production =
        std::fs::read_to_string(fixture.join("input/src/lib.rs")).map_err(|e| e.to_string())?;
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -16,4 +16,4 @@ impl Notifier {\n \n pub fn process_order(notifier: &Notifier, order_id: &str) -> bool {\n-    notifier.send(\"static_msg\")\n+    notifier.send(order_id)\n }\n";
    for macro_name in [
        "assert_eq",
        "assert",
        "assert_ne",
        "debug_assert_eq",
        "debug_assert",
        "debug_assert_ne",
    ] {
        let actual = "notifier.sent.borrow().first().map(String::as_str)";
        let arguments = if macro_name.ends_with("_ne") {
            format!("{actual}, Some(\"static_msg\")")
        } else if macro_name.ends_with("_eq") {
            format!("{actual}, Some(\"order-42\")")
        } else {
            format!("{actual} == Some(\"order-42\")")
        };
        let source = format!(
            "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\nfn check(notifier: &Notifier) {{\n{macro_name}!({arguments});\n}}\n#[test]\nfn process_order_observed() {{\nlet notifier = Notifier::new();\nprocess_order(&notifier, \"order-42\");\ncheck(&notifier);\n}}\n}}\n"
        );
        let root = root_dir.join(macro_name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"helper_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let effect = report
            .findings
            .iter()
            .find(|f| f.probe.family.as_str() == "side_effect")
            .ok_or_else(|| format!("{macro_name}: no side_effect finding"))?;
        assert!(
            matches!(effect.class.as_str(), "exposed" | "weakly_exposed"),
            "{macro_name}: helper-borne assertion lost credit: {}",
            effect.class.as_str()
        );
    }
    Ok(())
}

/// A boolean `assert!` whose operand only nests an unresolved third-party
/// comparison macro (`float_cmp::approx_eq!`) keeps its return-value credit
/// when it runs, while an operand that can leave the test does not (#10389).
#[test]
fn boolean_assert_with_nested_macro_operand_credits_only_when_it_runs_and_cannot_exit()
-> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("boolean_assert_nested_macro");
    let production = "pub fn compute_score(base: i32) -> i32 {\n    base * 2\n}\n";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn compute_score(base: i32) -> i32 {\n-    base + 1\n+    base * 2\n }\n";
    let cases = [
        (
            "live_nested_macro",
            "assert!(approx_eq!(i32, compute_score(4), 8));",
            true,
        ),
        (
            "nested_macro_with_exit",
            "assert!(approx_eq!(i32, compute_score(4), { return; }));",
            false,
        ),
        (
            "nested_macro_after_return",
            "return;\nassert!(approx_eq!(i32, compute_score(4), 8));",
            false,
        ),
        (
            "dead_plain",
            "return;\nassert!(compute_score(4) == 8);",
            false,
        ),
        (
            "dead_after_unrelated_nested_macro",
            "println!(\"{}\", foo!(1));\nreturn;\nassert!(compute_score(4) == 8);",
            false,
        ),
        (
            "if_false_after_unrelated_nested_macro",
            "println!(\"{}\", foo!(1));\nif false {\nassert!(compute_score(4) == 8);\n}",
            false,
        ),
        (
            "dead_nested_and_dead_plain",
            "return;\nassert!(approx_eq!(i32, compute_score(4), 8));\nassert!(compute_score(4) == 8);",
            false,
        ),
        (
            "if_false_nested_macro",
            "if false {\nassert!(approx_eq!(i32, compute_score(4), 8));\n}",
            false,
        ),
        (
            "local_macro_that_returns",
            "macro_rules! chk {\n($e:expr) => {{ return; }};\n}\nassert!(chk!(compute_score(4) == 8));",
            false,
        ),
        (
            "unknown_attribute_nested_macro",
            "@#[custom::harness]|assert!(approx_eq!(i32, compute_score(4), 8));",
            false,
        ),
        (
            "cfg_any_nested_macro",
            "@#[cfg(any())]|assert!(approx_eq!(i32, compute_score(4), 8));",
            false,
        ),
        (
            "live_nested_macro_with_negated_group",
            "assert!(approx_eq!(i32, compute_score(4), 8) && !(compute_score(4) < 0));",
            true,
        ),
        (
            "uncalled_fn_holding_nested_macro",
            "fn never() {\nassert!(approx_eq!(i32, compute_score(4), 8));\n}",
            false,
        ),
        (
            "loop_before_nested_macro",
            "loop {}\nassert!(approx_eq!(i32, compute_score(4), 8));",
            false,
        ),
        (
            "cfg_any_branch_nested_macro",
            "if cfg!(any()) {\nassert!(approx_eq!(i32, compute_score(4), 8));\n}",
            false,
        ),
        (
            "labeled_break_before_nested_macro_in_block",
            "'a: {\nif true { break 'a; }\nassert!(approx_eq!(i32, compute_score(4), 8));\n}",
            false,
        ),
        (
            "live_nested_after_unrelated_block",
            "'a: {\nif true { break 'a; }\n}\nassert!(approx_eq!(i32, compute_score(4), 8));",
            true,
        ),
        (
            "live_nested_then_dead_plain",
            "assert!(approx_eq!(i32, compute_score(4), 8));\nreturn;\nassert!(compute_score(4) == 8);",
            true,
        ),
    ];
    for (name, body, credited) in cases {
        let (attribute, body) = body
            .strip_prefix('@')
            .and_then(|rest| rest.split_once('|'))
            .unwrap_or(("", body));
        let source = format!(
            "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\nuse float_cmp::approx_eq;\n#[test]\n{attribute}\nfn compute_score_observed() {{\n{body}\n}}\n}}\n"
        );
        let root = root_dir.join(name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"nested_macro_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let finding = report
            .findings
            .iter()
            .find(|f| f.probe.family.as_str() == "return_value")
            .ok_or_else(|| format!("{name}: no return_value finding"))?;
        assert_eq!(
            matches!(finding.class.as_str(), "exposed" | "weakly_exposed"),
            credited,
            "{name}: {}",
            finding.class.as_str()
        );
    }
    Ok(())
}
