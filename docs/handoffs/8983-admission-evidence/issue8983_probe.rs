//! Disposable investigation, not an acceptance suite or production patch.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::{Path, PathBuf};

#[test]
fn authentic_nonempty_admission_matrix() -> Result<(), String> {
    let evidence = PathBuf::from(std::env::var("ISSUE8983_EVIDENCE").map_err(|e| e.to_string())?);
    std::fs::create_dir_all(&evidence).map_err(|e| e.to_string())?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_side_effect");
    let effect_source = std::fs::read_to_string(fixture.join("input/src/lib.rs")).map_err(|e| e.to_string())?;
    let effect_diff = std::fs::read_to_string(evidence.join("aligned-effect.patch")).map_err(|e| e.to_string())?;
    assert_eq!(effect_source.lines().nth(17), Some("    notifier.send(order_id)"));
    assert!(effect_diff.contains("@@ -16,4 +16,4 @@"));
    let return_source = "pub fn compute_score(base: i32) -> i32 {\n    base * 2\n}\n";
    let return_diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn compute_score(base: i32) -> i32 {\n-    base + 1\n+    base * 2\n }\n";
    let mut rows = Vec::new();
    for subject in ["effect", "return"] {
        for qualification in ["", "std::"] {
            for macro_name in ["assert_eq", "assert", "assert_ne", "debug_assert_eq", "debug_assert", "debug_assert_ne"] {
                let actual = if subject == "effect" { "notifier.sent.borrow().first().map(String::as_str)" } else { "compute_score(4)" };
                let expected = if subject == "effect" { "Some(\"order-42\")" } else { "8" };
                let wrong = if subject == "effect" { "Some(\"static_msg\")" } else { "5" };
                let arguments = if macro_name.ends_with("_ne") { format!("{actual}, {wrong}") }
                    else if macro_name.ends_with("_eq") { format!("{actual}, {expected}") }
                    else { format!("{actual} == {expected}") };
                let assertion = format!("{qualification}{macro_name}!({arguments});");
                for context in ["direct", "block_return", "loop_return", "else_true", "fn_uncalled", "match_impossible", "cfg_false", "after_return", "if_false", "closure_uncalled", "mixed"] {
                    let body = match context {
                        "direct" => assertion.clone(),
                        "block_return" => format!("{{ return; }}\n{assertion}"),
                        "loop_return" => format!("loop {{ return; }}\n{assertion}"),
                        "else_true" => format!("if true {{ }} else {{\n{assertion}\n}}"),
                        "fn_uncalled" if subject == "effect" => format!("fn never_called(notifier: &Notifier) {{\n{assertion}\n}}"),
                        "fn_uncalled" => format!("fn never_called() {{\n{assertion}\n}}"),
                        "match_impossible" => format!("match 1 {{ 2 => {{\n{assertion}\n}} _ => {{}} }}"),
                        "cfg_false" => format!("if cfg!(any()) {{\n{assertion}\n}}"),
                        "after_return" => format!("return;\n{assertion}"),
                        "if_false" => format!("if false {{\n{assertion}\n}}"),
                        "closure_uncalled" => format!("let _never = || {{\n{assertion}\n}};"),
                        "mixed" => format!("{assertion}\n{{ return; }}\n{assertion}"),
                        _ => return Err("unknown context".into()),
                    };
                    let (production, diff, setup, test_name, line, expression, families) = if subject == "effect" {
                        (effect_source.as_str(), effect_diff.as_str(), "let notifier = Notifier::new();\nprocess_order(&notifier, \"order-42\");", "process_order_observed", 18, "notifier.send(order_id)", vec!["return_value", "side_effect"])
                    } else {
                        (return_source, return_diff, "", "compute_score_observed", 2, "base * 2", vec!["return_value"])
                    };
                    let source = format!("{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn {test_name}() {{\n{setup}\n{body}\n}}\n}}\n");
                    let key = format!("{subject}_{}{macro_name}_{context}", if qualification.is_empty() { "" } else { "std_" });
                    let root = evidence.join(&key);
                    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
                    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"issue8983_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
                    std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
                    std::fs::write(root.join("diff.patch"), diff).map_err(|e| e.to_string())?;
                    let report = check_workspace(CheckInput { root: root.clone(), diff_file: Some(root.join("diff.patch")), mode: Mode::Fast, format: OutputFormat::Json, include_unchanged_tests: true, ..CheckInput::default() })?;
                    let rendered = render_check(&report, &OutputFormat::Json)?;
                    std::fs::write(root.join("report.json"), &rendered).map_err(|e| e.to_string())?;
                    std::fs::write(root.join("report.txt"), render_check(&report, &OutputFormat::Human)?).map_err(|e| e.to_string())?;
                    let json: serde_json::Value = serde_json::from_str(&rendered).map_err(|e| e.to_string())?;
                    assert_eq!(json["analysis_outcome"]["analysis_complete"], true, "{key}");
                    assert_eq!(report.findings.len(), families.len(), "{key}: {:#?}", report.findings);
                    let mut observed = report.findings.iter().map(|f| f.probe.family.as_str()).collect::<Vec<_>>();
                    observed.sort();
                    assert_eq!(observed, families, "{key}");
                    for finding in &report.findings {
                        assert_eq!(finding.probe.location.line, line, "{key}");
                        assert_eq!(finding.probe.expression.trim(), expression, "{key}");
                        assert!(!finding.related_tests.is_empty(), "{key}");
                    }
                    let summary = serde_json::json!({"case":key,"subject":subject,"macro":format!("{qualification}{macro_name}"),"context":context,"findings":json["findings"]});
                    println!("{}", serde_json::json!({"case":key,"findings":json["findings"].as_array().map(|fs|fs.iter().map(|f|serde_json::json!({"family":f["probe"]["family"],"class":f["classification"],"observe":f["ripr"]["observe"]["state"],"oracle":f["oracle_kind"]})).collect::<Vec<_>>())}));
                    rows.push(summary);
                }
            }
        }
    }
    std::fs::write(evidence.join("matrix.json"), serde_json::to_vec_pretty(&rows).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    assert_eq!(rows.len(), 264);
    Ok(())
}
