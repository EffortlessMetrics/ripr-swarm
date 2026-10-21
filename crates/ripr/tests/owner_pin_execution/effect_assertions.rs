//! Effect-family execution admission uses the same refusal as return pins (#8927).

use super::*;

fn effect_source(assertion: &str, context: &str) -> Result<String, String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_semicolon_effect/input/src/lib.rs");
    let production = std::fs::read_to_string(fixture).map_err(|error| error.to_string())?;
    let oracle = match context {
        "direct" => assertion.to_string(),
        "after_return" => format!("return;\n{assertion}"),
        "if_false" => format!("if false {{\n{assertion}\n}}"),
        "uncalled_closure" => format!("let _check = || {{\n{assertion}\n}};"),
        "mixed" => format!("{assertion}\nreturn;\n{assertion}"),
        _ => return Err(format!("unknown effect assertion context {context}")),
    };
    Ok(format!(
        "{production}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn process_order_sends_exact_order_id() {{\nlet notifier = Notifier::new();\nprocess_order(&notifier, \"order-42\");\n{oracle}\n}}\n}}\n"
    ))
}

const ASSERTIONS: [(&str, &str, &str, &str); 3] = [
    (
        "assert_eq",
        "assert_eq!(*notifier.sent.borrow(), vec![\"order-42\".to_string()]);",
        "exact_value",
        "strong",
    ),
    (
        "assert",
        "assert!(matches!(notifier.sent.borrow().first().map(String::as_str), Some(\"order-42\")));",
        "exact_value",
        "strong",
    ),
    (
        "assert",
        "assert!(*notifier.sent.borrow() == vec![\"order-42\".to_string()]);",
        "mock_expectation",
        "medium",
    ),
];

fn human_has_effect_guidance(report: &ripr::CheckOutput) -> Result<bool, String> {
    Ok(render_check(report, &OutputFormat::Human)?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains("check the changed effect"))
}

#[test]
fn effect_assertions_require_execution_but_keep_reachable_oracles() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_verified_semicolon_effect");
    for (macro_name, assertion, kind, strength) in ASSERTIONS {
        for (context, exposed) in [
            ("direct", true),
            ("after_return", false),
            ("if_false", false),
            ("uncalled_closure", false),
            ("mixed", true),
        ] {
            let scratch = Scratch::create()?;
            std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
            std::fs::copy(
                fixture.join("input/Cargo.toml"),
                scratch.0.join("Cargo.toml"),
            )
            .map_err(|error| error.to_string())?;
            std::fs::write(
                scratch.0.join("src/lib.rs"),
                effect_source(assertion, context)?,
            )
            .map_err(|error| error.to_string())?;
            let report = check_workspace(CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(fixture.join("diff.patch")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            })?;
            let json: serde_json::Value =
                serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                    .map_err(|error| error.to_string())?;
            assert_eq!(json["analysis_outcome"]["analysis_complete"], true);
            let findings = json["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(
                findings.len(),
                2,
                "{macro_name}/{context}: exact effect subjects"
            );
            for family in ["side_effect", "call_deletion"] {
                let selected = findings
                    .iter()
                    .filter(|finding| finding["probe"]["family"] == family)
                    .collect::<Vec<_>>();
                assert_eq!(selected.len(), 1, "{macro_name}/{context}/{family}");
                let finding = selected[0];
                assert_eq!(
                    finding["classification"],
                    if exposed {
                        if strength == "strong" {
                            "exposed"
                        } else {
                            "weakly_exposed"
                        }
                    } else {
                        "reachable_unrevealed"
                    },
                    "{macro_name}/{context}/{family}"
                );
                assert_eq!(finding["ripr"]["reach"]["state"], "yes");
                assert_eq!(
                    finding["ripr"]["observe"]["state"],
                    if exposed { "yes" } else { "no" }
                );
                assert_eq!(
                    finding["ripr"]["discriminate"]["state"],
                    if exposed {
                        if strength == "strong" { "yes" } else { "weak" }
                    } else {
                        "no"
                    }
                );
                assert_eq!(
                    finding["oracle_kind"],
                    if exposed { kind } else { "unknown" }
                );
                assert_eq!(
                    finding["oracle_strength"],
                    if exposed { strength } else { "none" }
                );
                let related = finding["related_tests"]
                    .as_array()
                    .ok_or("missing related tests")?;
                assert_eq!(related.len(), 1);
                assert_eq!(related[0]["file"], "src/lib.rs");
                if !exposed {
                    assert_eq!(related[0]["oracle"], "");
                    assert_eq!(related[0]["oracle_strength"], "none");
                }
                // #8924 may discard a never-run medium oracle at extraction.
                // Strong assertions deliberately survive for this refusal.
                if !exposed && strength == "strong" {
                    assert!(
                        finding["recommended_next_step"]
                            .as_str()
                            .is_some_and(|text| text.contains("check the changed effect"))
                    );
                    assert!(
                        human_has_effect_guidance(&report)?,
                        "{macro_name}/{context}/{family}: Human output must retain effect guidance"
                    );
                    let mut without_guidance = report.clone();
                    for finding in &mut without_guidance.findings {
                        finding.recommended_next_step = None;
                    }
                    assert!(
                        !human_has_effect_guidance(&without_guidance)?,
                        "missing guidance must not satisfy the Human output check"
                    );
                    assert!(
                        finding["ripr"]["observe"]["summary"].as_str().is_some_and(
                            |text| text.contains("rust_assertion_context_unestablished")
                        )
                    );
                    let note = format!("`{macro_name}!` in process_order_sends_exact_order_id");
                    assert!(
                        finding["evidence"]
                            .as_array()
                            .ok_or("missing evidence")?
                            .iter()
                            .any(|line| line.as_str().is_some_and(|text| text.contains(&note))),
                        "{macro_name}/{context}/{family}: refusal identifies the actual assertion"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn effect_assertion_contexts_match_executed_mutant_controls() -> Result<(), String> {
    for (macro_name, assertion, _, strength) in ASSERTIONS {
        for (context, detects_wrong_effect) in [
            ("direct", true),
            ("after_return", false),
            ("if_false", false),
            ("uncalled_closure", false),
            ("mixed", true),
        ] {
            let source = effect_source(assertion, context)?;
            let wrong = source.replace("notifier.send(order_id)", "notifier.send(\"wrong\")");
            assert_ne!(source, wrong, "mutation must change the effect producer");
            source_runtime_control(
                &source,
                &format!("{macro_name}/{strength}/{context}/correct"),
                1,
                false,
            )?;
            source_runtime_control(
                &wrong,
                &format!("{macro_name}/{strength}/{context}/wrong"),
                1,
                detects_wrong_effect,
            )?;
        }
    }
    Ok(())
}
