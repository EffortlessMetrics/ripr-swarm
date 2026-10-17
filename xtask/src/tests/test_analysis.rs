use super::*;

// Enum contract tests
// ============================================================================

#[test]
fn test_oracle_class_labels_are_stable() {
    assert_eq!(TestOracleClass::Strong.as_str(), "strong");
    assert_eq!(TestOracleClass::Medium.as_str(), "medium");
    assert_eq!(TestOracleClass::Weak.as_str(), "weak");
    assert_eq!(TestOracleClass::Smoke.as_str(), "smoke");
}

#[test]
fn test_oracle_class_rank_is_monotonic() {
    assert!(TestOracleClass::Strong.rank() > TestOracleClass::Medium.rank());
    assert!(TestOracleClass::Medium.rank() > TestOracleClass::Weak.rank());
    assert!(TestOracleClass::Weak.rank() > TestOracleClass::Smoke.rank());
}

#[test]
fn test_intent_kind_round_trips_supported_values() {
    for value in TestIntentKind::supported() {
        let parsed = TestIntentKind::from_str(value).expect("supported intent should parse");
        assert_eq!(parsed.as_str(), *value);
    }
}

#[test]
fn test_intent_kind_rejects_unknown_values() {
    assert_eq!(TestIntentKind::from_str("not_a_real_intent"), None);
    assert_eq!(TestIntentKind::from_str(""), None);
    assert_eq!(TestIntentKind::from_str("SMOKE"), None);
}

#[test]
fn test_intent_kind_supported_list_has_expected_values() {
    let supported = TestIntentKind::supported();
    assert!(supported.contains(&"smoke"));
    assert!(supported.contains(&"business_case_duplicate"));
    assert!(supported.contains(&"opaque_external_oracle"));
    assert!(supported.contains(&"integration_contract"));
    assert!(supported.contains(&"performance_guard"));
    assert!(supported.contains(&"documentation_example"));
    assert_eq!(supported.len(), 6);
}

#[test]
fn test_oracle_parser_classifies_strong_weak_and_smoke_tests() {
    let source = r#"
#[test]
fn exact_json_contract() {
    assert_eq!(actual_json, expected_json);
}

#[test]
fn weak_status_check() {
    assert!(result.is_err());
}

#[test]
fn smoke_command_runs() {
    assert!(status.success());
}
"#;

    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);

    assert_eq!(tests.len(), 3);
    assert_eq!(tests[0].name, "exact_json_contract");
    assert_eq!(tests[0].class, TestOracleClass::Strong);
    assert_eq!(tests[1].name, "weak_status_check");
    assert_eq!(tests[1].class, TestOracleClass::Weak);
    assert_eq!(tests[2].name, "smoke_command_runs");
    assert_eq!(tests[2].class, TestOracleClass::Smoke);
}

#[test]
fn test_oracle_reports_include_advisory_debt() {
    let source = r#"
#[test]
fn weak_contains() {
    assert!(stdout.contains("warning"));
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    let markdown = test_oracle_report_markdown(&tests);
    let json = test_oracle_report_json(&tests);

    assert!(markdown.contains("Status: warn"));
    assert!(markdown.contains("Weak Or Smoke Tests"));
    assert!(markdown.contains("BDD-shaped names: 0 / 1"));
    assert!(json.contains("\"advisory\": true"));
    assert!(json.contains("\"weak\": 1"));
}

#[test]
fn test_oracle_discarded_matches_is_not_an_observing_oracle() {
    let source = r#"
#[test]
fn unused_matches() {
    let _ = matches!(actual, Some(42));
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].name, "unused_matches");
    assert_eq!(tests[0].class, TestOracleClass::Smoke);
    assert!(
        tests[0]
            .observations
            .iter()
            .all(|observation| observation.pattern == "no assertion"),
        "a discarded predicate must not contribute observing-oracle evidence"
    );
}

#[test]
fn test_oracle_assertion_spelling_in_string_is_not_an_observing_oracle() {
    let source = r##"
#[test]
fn assertion_in_string() {
    let example = "assert_eq!(actual, expected);";
}

#[test]
fn assertion_in_raw_string() {
    let example = r#"assert_eq!(actual, expected);"#;
}
"##;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 2);
    assert_eq!(tests[0].name, "assertion_in_string");
    assert_eq!(tests[1].name, "assertion_in_raw_string");
    for test in &tests {
        assert_eq!(test.class, TestOracleClass::Smoke);
        assert!(
            test.observations
                .iter()
                .all(|observation| observation.pattern == "no assertion"),
            "inert string text must not contribute observing-oracle evidence"
        );
    }
}

#[test]
fn test_oracle_assertion_spelling_in_block_comment_is_not_an_observing_oracle() {
    let source = r#"
#[test]
fn assertion_in_block_comment() {
    /* assert_eq!(actual, expected); */
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Smoke);
    assert!(
        tests[0]
            .observations
            .iter()
            .all(|observation| observation.pattern == "no assertion"),
        "block-comment text must not contribute observing-oracle evidence"
    );
}

#[test]
fn test_oracle_asserted_matches_keeps_its_evidence() {
    let source = r#"
#[test]
fn asserted_variant() {
    assert!(matches!(actual, Some(42)));
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Strong);
}

#[test]
fn test_oracle_same_line_discarded_matches_earns_nothing() {
    let source = r#"
#[test]
fn same_line_discard() {
    let _ = matches!(actual, Some(1)); assert!(true);
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Weak);
    assert!(
        tests[0]
            .observations
            .iter()
            .all(|observation| observation.pattern != "matches!"),
        "a discarded matches! sharing a line with an unrelated assert must not credit Strong"
    );
}

#[test]
fn test_oracle_matches_mention_in_assert_message_earns_nothing() {
    let source = r#"
#[test]
fn message_mention() {
    assert!(ready, "matches!(actual, Some(1))");
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Weak);
    assert!(
        tests[0]
            .observations
            .iter()
            .all(|observation| observation.pattern != "matches!"),
        "a matches! spelling confined to an assertion message must not credit Strong"
    );
}

#[test]
fn test_oracle_multiline_asserted_matches_keeps_its_evidence() {
    let source = r#"
#[test]
fn multiline_asserted_variant() {
    assert!(
        matches!(actual, Some(1))
    );
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Strong);
    assert!(
        tests[0]
            .observations
            .iter()
            .any(|observation| observation.pattern == "matches!"),
        "a split assert!(matches!(..)) form must keep exactly one Strong observation"
    );
    assert_eq!(
        tests[0]
            .observations
            .iter()
            .filter(|observation| observation.pattern == "matches!")
            .count(),
        1
    );
}

#[test]
fn test_oracle_brace_inside_string_does_not_truncate_the_body() {
    let source = r#"
#[test]
fn brace_string_before_real_end() {
    let template = "value: } assert_eq!(a, b);";
    let _ = template.len();
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Smoke);
    assert!(
        tests[0]
            .observations
            .iter()
            .all(|observation| observation.pattern == "no assertion"),
        "a brace inside a string must not truncate the body into the unfiltered fallback"
    );
}

#[test]
fn test_oracle_brace_inside_block_comment_does_not_truncate_the_body() {
    let source = r#"
#[test]
fn brace_comment_before_real_end() {
    /* } assert_eq!(a, b); */
    let _ = 1;
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Smoke);
}

#[test]
fn test_oracle_real_assert_after_brace_string_survives() {
    let source = r#"
#[test]
fn real_assert_after_brace_string() {
    let template = "value: }";
    assert_eq!(compute(template), 1);
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].class, TestOracleClass::Strong);
}

#[test]
fn test_oracle_macro_nested_test_is_selected() {
    let source = "proptest! {\n    #[test]\n    fn generated_case(seed: u32) {\n        assert_eq!(seed % 2, 0);\n    }\n}\n";
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].name, "generated_case");
    assert_eq!(tests[0].class, TestOracleClass::Strong);
}

#[test]
fn test_oracle_macro_nested_test_inside_fn_body_stays_invisible() {
    let source = "fn outer() {\n    harness! {\n        #[test]\n        fn inner() {\n            assert_eq!(1, 1);\n        }\n    }\n}\n";
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert!(tests.is_empty());
}

#[test]
fn test_oracle_unparseable_file_keeps_legacy_selection() {
    // Missing closing brace: the syntax path declines and the legacy scan
    // still selects the test with prior classification behavior.
    let source = "#[test]\nfn truncated_body() {\n    assert_eq!(actual, expected);\n";
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].name, "truncated_body");
    assert_eq!(tests[0].class, TestOracleClass::Strong);
}

#[test]
fn test_oracle_rejected_inputs_agree_across_json_and_markdown() -> Result<(), String> {
    let source = r#"
#[test]
fn unused_matches() {
    let _ = matches!(actual, Some(42));
}

#[test]
fn assertion_in_string() {
    let example = "assert_eq!(actual, expected);";
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    assert_eq!(tests.len(), 2);
    let markdown = test_oracle_report_markdown(&tests);
    let json = test_oracle_report_json(&tests);
    for name in ["unused_matches", "assertion_in_string"] {
        assert!(
            !markdown
                .lines()
                .filter(|line| line.contains(name))
                .any(|line| line.contains("`strong`")),
            "markdown must not credit {name} as strong"
        );
        let report: Value =
            serde_json::from_str(&json).map_err(|err| format!("oracle json must parse: {err}"))?;
        let tests_array = report
            .get("tests")
            .and_then(Value::as_array)
            .ok_or_else(|| "oracle json must carry tests".to_string())?;
        let entry = tests_array
            .iter()
            .find(|test| test.get("name").and_then(Value::as_str) == Some(name))
            .ok_or_else(|| format!("rejected test {name} must appear in json"))?;
        assert_eq!(
            entry.get("class").and_then(Value::as_str),
            Some("smoke"),
            "json must not credit {name} as strong"
        );
    }
    assert!(markdown.contains("Status: warn"));
    assert!(json.contains("\"status\": \"warn\""));
    for test in &tests {
        let entry = test_efficiency_entry(test);
        assert_eq!(
            entry.oracle_strength, "smoke",
            "the efficiency consumer must not retain strong-oracle credit"
        );
    }
    Ok(())
}

#[test]
fn test_oracle_command_reports_empty_selection_as_not_run() -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("clock before unix epoch: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-xtask-test-oracle-empty-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&root)
        .map_err(|err| format!("failed to create empty oracle test directory: {err}"))?;
    let missing_root = root.join("no-selected-tests");
    let report_dir = root.join("reports");
    let result = (|| {
        test_oracle_report_impl_for_roots(&[missing_root.as_path()], &report_dir)?;

        let markdown = fs::read_to_string(report_dir.join("test-oracles.md"))
            .map_err(|err| format!("failed to read empty oracle markdown: {err}"))?;
        let json = fs::read_to_string(report_dir.join("test-oracles.json"))
            .map_err(|err| format!("failed to read empty oracle JSON: {err}"))?;

        if !markdown.contains("Status: not_run") {
            return Err("empty oracle markdown did not report not_run".to_string());
        }
        if !markdown.contains("Explanation: No tests were selected") {
            return Err("empty oracle markdown omitted the explanation".to_string());
        }
        if !markdown.contains("BDD-shaped names: 0 / 0") {
            return Err("empty oracle markdown omitted the zero denominator".to_string());
        }
        let value: Value = serde_json::from_str(&json)
            .map_err(|err| format!("empty oracle JSON is invalid: {err}"))?;
        if value.get("status").and_then(Value::as_str) != Some("not_run") {
            return Err("empty oracle JSON did not report not_run".to_string());
        }
        if value.get("explanation").and_then(Value::as_str)
            != Some("No tests were selected; oracle evidence was not established for this report.")
        {
            return Err("empty oracle JSON omitted the explanation".to_string());
        }
        if value.get("tests").and_then(Value::as_array).map(Vec::len) != Some(0) {
            return Err("empty oracle JSON did not contain zero tests".to_string());
        }
        Ok(())
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|err| format!("failed to clean empty oracle test directory: {err}"));
    result.and(cleanup)
}

#[test]
fn test_oracle_report_status_preserves_nonempty_controls() -> Result<(), String> {
    let strong = test_oracle_tests_in_text(
        Path::new("crates/ripr/tests/strong.rs"),
        "#[test]\nfn exact() { assert_eq!(actual, expected); }\n",
    );
    let warning = test_oracle_tests_in_text(
        Path::new("crates/ripr/tests/warning.rs"),
        "#[test]\nfn broad() { assert!(actual.is_ok()); }\n",
    );
    if strong.len() != 1 || warning.len() != 1 {
        return Err(format!(
            "controls selected unexpected test counts: strong={}, warning={}",
            strong.len(),
            warning.len()
        ));
    }

    let strong_json = test_oracle_report_json(&strong);
    let strong_markdown = test_oracle_report_markdown(&strong);
    if !strong_json.contains("\"status\": \"pass\"") || !strong_markdown.contains("Status: pass") {
        return Err("strong oracle control lost its positive status".to_string());
    }

    let warning_json = test_oracle_report_json(&warning);
    let warning_markdown = test_oracle_report_markdown(&warning);
    if !warning_json.contains("\"status\": \"warn\"") || !warning_markdown.contains("Status: warn")
    {
        return Err("weak oracle control lost its warning status".to_string());
    }
    Ok(())
}

#[test]
fn test_efficiency_ledger_records_owner_oracle_values_and_limitations() {
    let source = r#"
#[test]
fn creates_invoice_smoke() {
    let invoice = create_invoice("acct-1", 100);
    assert!(invoice.is_ok());
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    let entry = test_efficiency_entry(&tests[0]);

    assert_eq!(entry.name, "creates_invoice_smoke");
    assert_eq!(entry.class, "useful_but_broad");
    assert_eq!(entry.oracle_kind, "broad predicate");
    assert_eq!(entry.oracle_strength, "weak");
    assert!(entry.reasons.contains(&"broad_oracle".to_string()));
    assert!(
        entry
            .reasons
            .contains(&"assertion_may_not_match_detected_owner".to_string())
    );
    assert!(entry.reached_owners.contains(&"create_invoice".to_string()));
    assert!(
        entry
            .observed_values
            .iter()
            .any(|value| value.value == "\"acct-1\"")
    );
    assert!(
        entry
            .observed_values
            .iter()
            .any(|value| value.value == "100")
    );
    assert!(
        entry
            .static_limitations
            .iter()
            .any(|limitation| limitation.contains("broad oracle"))
    );
}

#[test]
fn test_efficiency_empty_selection_is_not_evidence_of_pass() -> Result<(), String> {
    with_temp_cwd("efficiency-empty-selection", |root| {
        for with_source in [false, true] {
            if with_source {
                let source_root = root.join("crates/ripr/src");
                fs::create_dir_all(&source_root).map_err(|e| e.to_string())?;
                fs::write(source_root.join("lib.rs"), "pub fn value() -> u32 { 1 }\n")
                    .map_err(|e| e.to_string())?;
            }
            super::super::test_efficiency_report_impl()?;
            let report_dir = root.join("target/ripr/reports");
            let markdown = fs::read_to_string(report_dir.join("test-efficiency.md"))
                .map_err(|e| e.to_string())?;
            let json: Value = serde_json::from_str(
                &fs::read_to_string(report_dir.join("test-efficiency.json"))
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            assert_eq!(json["status"], "not_run", "with_source={with_source}");
            assert_eq!(json["metrics"]["tests_scanned"], 0);
            assert_eq!(json["tests"], serde_json::json!([]));
            assert_eq!(json["advisory"], true);
            let explanation = json["explanation"].as_str().ok_or("missing explanation")?;
            assert!(explanation.contains("No tests were selected"));
            assert!(explanation.contains("evidence was not established"));
            assert!(explanation.contains("workspace root"));
            assert!(markdown.contains("Status: not_run"));
            assert!(markdown.contains(explanation));
            assert!(markdown.contains("Tests scanned: 0"));
        }
        Ok(())
    })
}

#[test]
fn test_efficiency_nonempty_selection_keeps_pass_and_warn() -> Result<(), String> {
    with_temp_cwd("efficiency-selected-controls", |root| {
        let source_root = root.join("crates/ripr/src");
        fs::create_dir_all(&source_root).map_err(|e| e.to_string())?;
        for (body, class, status) in [
            (
                "assert_eq!(create_invoice(100), expected_invoice);",
                "strong_discriminator",
                "pass",
            ),
            (
                "let status = run_cli();\n    assert!(status.success());",
                "smoke_only",
                "warn",
            ),
        ] {
            fs::write(
                source_root.join("lib.rs"),
                format!("#[test]\nfn selected() {{\n    {body}\n}}\n"),
            )
            .map_err(|e| e.to_string())?;
            super::super::test_efficiency_report_impl()?;
            let report_dir = root.join("target/ripr/reports");
            let markdown = fs::read_to_string(report_dir.join("test-efficiency.md"))
                .map_err(|e| e.to_string())?;
            let json: Value = serde_json::from_str(
                &fs::read_to_string(report_dir.join("test-efficiency.json"))
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            assert_eq!(json["metrics"]["tests_scanned"], 1);
            assert_eq!(json["tests"].as_array().map(Vec::len), Some(1));
            assert_eq!(json["tests"][0]["class"], class);
            assert_eq!(json["status"], status);
            assert_eq!(json["advisory"], true);
            assert!(json.get("explanation").is_none());
            assert!(markdown.contains(&format!("Status: {status}")));
            assert!(!markdown.contains("Explanation:"));
        }
        Ok(())
    })
}

#[test]
fn test_efficiency_reports_are_advisory() {
    let source = r#"
#[test]
fn smoke_status_check() {
    let status = run_cli();
    assert!(status.success());
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    let entry = test_efficiency_entry(&tests[0]);
    let markdown = test_efficiency_report_markdown(
        std::slice::from_ref(&entry),
        &[],
        &TestIntentReportSummary::default(),
    );
    let json = test_efficiency_report_json(
        std::slice::from_ref(&entry),
        &[],
        &TestIntentReportSummary::default(),
    );

    assert_eq!(entry.class, "smoke_only");
    assert!(entry.reasons.contains(&"smoke_oracle_only".to_string()));
    assert!(markdown.contains("Mode: advisory"));
    assert!(markdown.contains("smoke_status_check"));
    assert!(json.contains("\"advisory\": true"));
    assert!(json.contains("\"smoke_only\": 1"));
}

#[test]
fn test_efficiency_signals_likely_vacuous_and_possibly_circular_tests() {
    let source = r#"
#[test]
fn creates_invoice_without_assertion() {
    create_invoice("acct-1", 100);
}

#[test]
fn expected_uses_same_owner_path() {
    assert_eq!(actual_invoice, create_invoice("acct-1", 100));
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    let vacuous = test_efficiency_entry(&tests[0]);
    let circular = test_efficiency_entry(&tests[1]);
    let json = test_efficiency_report_json(
        &[vacuous.clone(), circular.clone()],
        &[],
        &TestIntentReportSummary::default(),
    );

    assert_eq!(vacuous.class, "likely_vacuous");
    assert!(
        vacuous
            .reasons
            .contains(&"no_assertion_detected".to_string())
    );
    assert_eq!(circular.class, "possibly_circular");
    assert!(
        circular
            .reasons
            .contains(&"expected_value_computed_from_detected_owner_path".to_string())
    );
    assert!(json.contains("\"likely_vacuous\": 1"));
    assert!(json.contains("\"possibly_circular\": 1"));
    assert!(json.contains("\"reason_counts\""));
}

#[test]
fn test_efficiency_does_not_treat_actual_side_owner_call_as_circular() {
    let source = r#"
#[test]
fn exact_owner_call_has_external_expected_value() {
    assert_eq!(create_invoice("acct-1", 100), expected_invoice);
}
"#;
    let tests = test_oracle_tests_in_text(Path::new("crates/ripr/tests/example.rs"), source);
    let entry = test_efficiency_entry(&tests[0]);

    assert_eq!(entry.class, "strong_discriminator");
    assert!(
        !entry
            .reasons
            .contains(&"expected_value_computed_from_detected_owner_path".to_string())
    );
}

#[test]
fn test_intent_parses_valid_reasoned_entries() {
    let text = r#"
schema_version = 1

[[test_intent]]
test = "cli_prints_help"
intent = "smoke"
reason = "CLI startup and help text smoke test."
owner = "devtools"

[[test_intent]]
test = "escapes_json"
path = "crates/ripr/src/output/json/mod.rs"
intent = "business_case_duplicate"
reason = "These duplicate-looking tests document distinct escaping cases."
owner = "output"
"#;
    let (entries, violations) = parse_test_intent_manifest(text);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].test, "cli_prints_help");
    assert_eq!(entries[0].intent, TestIntentKind::Smoke);
    assert_eq!(entries[0].owner, "devtools");
    assert!(entries[0].path.is_none());
    assert_eq!(entries[1].intent, TestIntentKind::BusinessCaseDuplicate);
    assert_eq!(
        entries[1].path.as_deref(),
        Some("crates/ripr/src/output/json/mod.rs")
    );
}

#[test]
fn test_intent_requires_schema_version() {
    let text = r#"
[[test_intent]]
test = "cli_prints_help"
intent = "smoke"
reason = "CLI startup and help text smoke test."
owner = "devtools"
"#;
    let (_, violations) = parse_test_intent_manifest(text);
    assert!(
        violations.iter().any(|v| v.contains("schema_version = 1")),
        "expected schema_version violation, got {violations:?}"
    );
}

#[test]
fn test_intent_requires_test_field() {
    let text = r#"
schema_version = 1

[[test_intent]]
intent = "smoke"
reason = "..."
owner = "devtools"
"#;
    let (entries, violations) = parse_test_intent_manifest(text);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing required `test`")),
        "expected missing-test violation, got {violations:?}"
    );
}

#[test]
fn test_intent_requires_intent_field() {
    let text = r#"
schema_version = 1

[[test_intent]]
test = "x"
reason = "y"
owner = "z"
"#;
    let (entries, violations) = parse_test_intent_manifest(text);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing required `intent`")),
        "expected missing-intent violation, got {violations:?}"
    );
}

#[test]
fn test_intent_requires_owner_and_reason() {
    let missing_owner = r#"
schema_version = 1

[[test_intent]]
test = "x"
intent = "smoke"
reason = "y"
"#;
    let (entries, violations) = parse_test_intent_manifest(missing_owner);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing required `owner`")),
        "expected missing-owner violation, got {violations:?}"
    );

    let missing_reason = r#"
schema_version = 1

[[test_intent]]
test = "x"
intent = "smoke"
owner = "z"
"#;
    let (entries, violations) = parse_test_intent_manifest(missing_reason);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing required `reason`")),
        "expected missing-reason violation, got {violations:?}"
    );
}

#[test]
fn test_intent_rejects_blank_owner_or_reason() {
    let text = r#"
schema_version = 1

[[test_intent]]
test = "x"
intent = "smoke"
owner = "  "
reason = "  "
"#;
    let (entries, violations) = parse_test_intent_manifest(text);
    assert!(entries.is_empty());
    assert!(violations.iter().any(|v| v.contains("`owner` is blank")));
    assert!(violations.iter().any(|v| v.contains("`reason` is blank")));
}

#[test]
fn test_intent_rejects_unknown_intent_value() {
    let text = r#"
schema_version = 1

[[test_intent]]
test = "x"
intent = "vibe_check"
owner = "z"
reason = "y"
"#;
    let (entries, violations) = parse_test_intent_manifest(text);
    assert!(entries.is_empty());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("unsupported intent `vibe_check`")),
        "expected unsupported-intent violation, got {violations:?}"
    );
}

#[test]
fn test_intent_rejects_unknown_fields() {
    let text = r#"
schema_version = 1

[[test_intent]]
test = "x"
intent = "smoke"
owner = "z"
reason = "y"
priority = "high"
"#;
    let (_, violations) = parse_test_intent_manifest(text);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("unsupported `[[test_intent]]` field `priority`")),
        "expected unknown-field violation, got {violations:?}"
    );
}

#[test]
fn test_intent_rejects_absolute_and_backslash_paths() {
    let abs_unix = r#"
schema_version = 1

[[test_intent]]
test = "x"
path = "/abs/path.rs"
intent = "smoke"
owner = "z"
reason = "y"
"#;
    let (_, violations) = parse_test_intent_manifest(abs_unix);
    assert!(
        violations.iter().any(|v| v.contains("is absolute")),
        "expected absolute-path violation, got {violations:?}"
    );

    let drive = "Z";
    let sep = ":/";
    let win = format!(
        r#"
schema_version = 1

[[test_intent]]
test = "x"
path = "{drive}{sep}abs/path.rs"
intent = "smoke"
owner = "z"
reason = "y"
"#
    );
    let (_, violations) = parse_test_intent_manifest(&win);
    assert!(violations.iter().any(|v| v.contains("is absolute")));

    let backslash = r#"
schema_version = 1

[[test_intent]]
test = "x"
path = "crates\\ripr\\src\\lib.rs"
intent = "smoke"
owner = "z"
reason = "y"
"#;
    let (_, violations) = parse_test_intent_manifest(backslash);
    assert!(
        violations.iter().any(|v| v.contains("backslashes")),
        "expected backslash-path violation, got {violations:?}"
    );
}

#[test]
fn test_intent_rejects_duplicate_selector() {
    let text = r#"
schema_version = 1

[[test_intent]]
test = "cli_prints_help"
intent = "smoke"
owner = "devtools"
reason = "first"

[[test_intent]]
test = "cli_prints_help"
intent = "smoke"
owner = "devtools"
reason = "second"
"#;
    let (_, violations) = parse_test_intent_manifest(text);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("duplicate selector `cli_prints_help`")),
        "expected duplicate-selector violation, got {violations:?}"
    );
}

#[test]
fn test_intent_matches_by_test_name_when_unique() {
    let mut entries = vec![intent_entry(
        "cli_prints_help",
        "tests/cli.rs",
        12,
        "smoke_only",
    )];
    let decls = vec![declaration("cli_prints_help", None, TestIntentKind::Smoke)];

    let violations = apply_test_intent_to_entries(&mut entries, &decls);

    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    assert_eq!(
        entries[0].declared_intent.as_ref().map(|i| i.intent),
        Some(TestIntentKind::Smoke)
    );
    assert_eq!(
        entries[0]
            .declared_intent
            .as_ref()
            .map(|i| i.source.as_str()),
        Some(".ripr/test_intent.toml")
    );
}

#[test]
fn test_intent_matches_by_test_name_and_path() {
    let mut entries = vec![
        intent_entry(
            "escapes_json",
            "crates/ripr/src/output/json/mod.rs",
            22,
            "useful_but_broad",
        ),
        intent_entry(
            "escapes_json",
            "crates/ripr/src/output/json/formatter.rs",
            74,
            "useful_but_broad",
        ),
    ];
    let decls = vec![declaration(
        "escapes_json",
        Some("crates/ripr/src/output/json/formatter.rs"),
        TestIntentKind::BusinessCaseDuplicate,
    )];

    let violations = apply_test_intent_to_entries(&mut entries, &decls);

    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    assert!(entries[0].declared_intent.is_none());
    assert_eq!(
        entries[1].declared_intent.as_ref().map(|i| i.intent),
        Some(TestIntentKind::BusinessCaseDuplicate)
    );
}

#[test]
fn test_intent_rejects_unmatched_selector() {
    let mut entries = vec![intent_entry("real_test", "tests/a.rs", 1, "smoke_only")];
    let decls = vec![declaration(
        "test_that_does_not_exist",
        None,
        TestIntentKind::Smoke,
    )];

    let violations = apply_test_intent_to_entries(&mut entries, &decls);

    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("did not match any test"));
    assert!(entries[0].declared_intent.is_none());
}

#[test]
fn test_intent_rejects_ambiguous_name_only_selector() {
    let mut entries = vec![
        intent_entry("escapes_json", "crates/a/mod.rs", 1, "useful_but_broad"),
        intent_entry("escapes_json", "crates/b/mod.rs", 1, "useful_but_broad"),
    ];
    let decls = vec![declaration(
        "escapes_json",
        None,
        TestIntentKind::BusinessCaseDuplicate,
    )];

    let violations = apply_test_intent_to_entries(&mut entries, &decls);

    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("matched multiple tests"));
    assert!(violations[0].contains("add `path`"));
    assert!(violations[0].contains("crates/a/mod.rs"));
    assert!(violations[0].contains("crates/b/mod.rs"));
    assert!(entries[0].declared_intent.is_none());
    assert!(entries[1].declared_intent.is_none());
}

#[test]
fn test_intent_keeps_declared_tests_visible_with_original_class() {
    let mut entries = vec![intent_entry(
        "cli_prints_help",
        "tests/cli.rs",
        12,
        "smoke_only",
    )];
    let decls = vec![declaration("cli_prints_help", None, TestIntentKind::Smoke)];

    let violations = apply_test_intent_to_entries(&mut entries, &decls);
    assert!(violations.is_empty());

    // Class is preserved — intent is additive metadata, not a replacement.
    assert_eq!(entries[0].class, "smoke_only");
    assert!(entries[0].declared_intent.is_some());
}

#[test]
fn test_intent_json_marks_declared_test_with_full_metadata() {
    let mut entries = vec![intent_entry(
        "cli_prints_help",
        "tests/cli.rs",
        12,
        "smoke_only",
    )];
    let decls = vec![declaration("cli_prints_help", None, TestIntentKind::Smoke)];
    let violations = apply_test_intent_to_entries(&mut entries, &decls);
    assert!(violations.is_empty());

    let json = test_efficiency_report_json(
        &entries,
        &[],
        &TestIntentReportSummary {
            declared: 1,
            matched: 1,
        },
    );

    assert!(json.contains("\"declared_intent\":"));
    assert!(json.contains("\"intent\": \"smoke\""));
    assert!(json.contains("\"owner\": \"team\""));
    assert!(json.contains("\"reason\": \"stated reason\""));
    assert!(json.contains("\"source\": \".ripr/test_intent.toml\""));
    assert!(json.contains("\"test_intent\":"));
    assert!(json.contains("\"declared\": 1"));
    assert!(json.contains("\"matched\": 1"));
    // Original class is preserved.
    assert!(json.contains("\"class\": \"smoke_only\""));
}

#[test]
fn test_intent_markdown_lists_declared_intent_section() {
    let mut entries = vec![intent_entry(
        "cli_prints_help",
        "tests/cli.rs",
        12,
        "smoke_only",
    )];
    let decls = vec![declaration("cli_prints_help", None, TestIntentKind::Smoke)];
    let _ = apply_test_intent_to_entries(&mut entries, &decls);

    let markdown = test_efficiency_report_markdown(
        &entries,
        &[],
        &TestIntentReportSummary {
            declared: 1,
            matched: 1,
        },
    );

    assert!(markdown.contains("## Declared Test Intent"));
    assert!(markdown.contains("declared: 1 \u{00b7} matched: 1"));
    assert!(markdown.contains("`cli_prints_help`"));
    assert!(markdown.contains("`smoke`"));
    assert!(markdown.contains("`team`"));
    assert!(markdown.contains("stated reason"));
}

#[test]
fn test_intent_report_with_no_file_has_zero_declared_summary() {
    let entries: Vec<TestEfficiencyEntry> = Vec::new();
    let summary = TestIntentReportSummary::default();
    assert_eq!(summary.declared, 0);
    assert_eq!(summary.matched, 0);

    let json = test_efficiency_report_json(&entries, &[], &summary);
    assert!(json.contains("\"test_intent\":"));
    assert!(json.contains("\"declared\": 0"));
    assert!(json.contains("\"matched\": 0"));

    let markdown = test_efficiency_report_markdown(&entries, &[], &summary);
    assert!(markdown.contains("## Declared Test Intent"));
    assert!(markdown.contains("None declared."));
}

#[test]
fn test_intent_does_not_mutate_existing_class_strings() {
    let mut entries = vec![
        intent_entry("smoke_test", "tests/a.rs", 1, "smoke_only"),
        intent_entry("dup_test", "tests/b.rs", 1, "duplicative"),
        intent_entry("opaque_test", "tests/c.rs", 1, "opaque"),
    ];
    let decls = vec![
        declaration("smoke_test", None, TestIntentKind::Smoke),
        declaration("dup_test", None, TestIntentKind::BusinessCaseDuplicate),
        declaration("opaque_test", None, TestIntentKind::OpaqueExternalOracle),
    ];
    let violations = apply_test_intent_to_entries(&mut entries, &decls);
    assert!(violations.is_empty());

    assert_eq!(entries[0].class, "smoke_only");
    assert_eq!(entries[1].class, "duplicative");
    assert_eq!(entries[2].class, "opaque");
    for entry in &entries {
        assert!(entry.declared_intent.is_some());
    }

    let intent_strings: Vec<&'static str> = entries
        .iter()
        .filter_map(|e| e.declared_intent.as_ref().map(|i| i.intent.as_str()))
        .collect();
    assert_eq!(
        intent_strings,
        vec!["smoke", "business_case_duplicate", "opaque_external_oracle"]
    );

    let _ = DeclaredIntent {
        intent: TestIntentKind::IntegrationContract,
        owner: "x".to_string(),
        reason: "y".to_string(),
        source: ".ripr/test_intent.toml".to_string(),
    };
}

#[test]
fn test_efficiency_entry_structure_matches_expected_shape() {
    let entry = TestEfficiencyEntry {
        path: PathBuf::from("tests/test.rs"),
        name: "test_foo".to_string(),
        line: 5,
        class: "smoke_only",
        oracle_kind: "assert".to_string(),
        oracle_strength: "smoke",
        reached_owners: vec!["parse".to_string()],
        observed_values: vec![],
        reasons: vec!["no explicit discriminator".to_string()],
        static_limitations: vec![],
        duplicate_group_id: None,
        declared_intent: None,
    };
    assert_eq!(entry.class, "smoke_only");
    assert_eq!(entry.oracle_strength, "smoke");
}

#[test]
fn test_efficiency_value_tracks_context_and_line() {
    let value = TestEfficiencyValue {
        line: 10,
        context: "activation",
        value: "foo()".to_string(),
        text: "let x = foo();".to_string(),
    };
    assert_eq!(value.line, 10);
    assert_eq!(value.context, "activation");
    assert_eq!(value.value, "foo()");
}
