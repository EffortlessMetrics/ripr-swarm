//! #9361: the public Check JSON boundary rejects missing producer evidence.
#![cfg(feature = "lang-rust")]

use ripr::app::{CheckInput, CheckOutput, OutputFormat, check_workspace_with_config, render_check};
use ripr::domain::{ExposureClass, StageState};
use serde_json::Value;
use std::path::Path;

fn fixture(name: &str) -> Result<CheckOutput, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    let mut config = ripr::config::RiprConfig::default();
    #[cfg(feature = "lang-typescript")]
    config
        .languages
        .enabled
        .push(ripr::domain::LanguageId::TypeScript);
    let output = check_workspace_with_config(
        CheckInput {
            root: dir.join("input"),
            diff_file: Some(dir.join("diff.patch")),
            ..CheckInput::default()
        },
        &config,
    )?;
    if output.findings.is_empty() {
        return Err(format!("fixture {name} produced no findings"));
    }
    Ok(output)
}

fn parse(document: &str) -> Result<Value, String> {
    serde_json::from_str(document).map_err(|error| error.to_string())
}

fn rejection(output: &CheckOutput, format: OutputFormat) -> Result<(), String> {
    match render_check(output, &format) {
        Err(error)
            if error.contains("requires non-empty missing evidence")
                && error.contains(&output.findings[0].id) =>
        {
            Ok(())
        }
        Err(error) => Err(format!("wrong rejection for {format:?}: {error}")),
        Ok(_) => Err(format!("{format:?} accepted missing evidence")),
    }
}

#[test]
fn missing_evidence_public_renderers_reject_empty_and_blank_details() -> Result<(), String> {
    let mut output = fixture("boundary_gap")?;
    output.findings.truncate(1);
    for class in [
        ExposureClass::WeaklyExposed,
        ExposureClass::ReachableUnrevealed,
        ExposureClass::NoStaticPath,
        ExposureClass::InfectionUnknown,
        ExposureClass::PropagationUnknown,
        ExposureClass::StaticUnknown,
    ] {
        output.findings[0].class = class;
        for missing in [vec![], vec![" \t\n".to_string()]] {
            output.findings[0].missing = missing;
            for format in [
                OutputFormat::Json,
                OutputFormat::JsonCompact,
                OutputFormat::JsonTerse,
            ] {
                rejection(&output, format)?;
            }
        }
    }
    Ok(())
}

#[test]
fn missing_evidence_legacy_renderer_returns_non_consumable_refusal() -> Result<(), String> {
    let mut output = fixture("boundary_gap")?;
    output.findings[0].missing.clear();
    output.findings[0].class = ExposureClass::WeaklyExposed;
    let value = parse(&ripr::output::json::render(&output))?;
    assert_eq!(value["status"], "refused");
    assert_eq!(value["error"]["code"], "analysis_failed");
    assert_eq!(value["analysis_scope"]["downstream_consumable"], false);
    assert_eq!(value["findings"], serde_json::json!([]));
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains(&output.findings[0].id))
    );
    Ok(())
}

#[test]
fn missing_evidence_nearest_positives_keep_class_and_detail() -> Result<(), String> {
    let mut output = fixture("boundary_gap")?;
    output.findings.truncate(1);
    for class in ExposureClass::ALL {
        output.findings[0].class = class.clone();
        output.findings[0].missing = if class == ExposureClass::Exposed {
            vec![]
        } else {
            vec!["Observed producer detail: the changed boundary input is absent".into()]
        };
        for format in [
            OutputFormat::Json,
            OutputFormat::JsonCompact,
            OutputFormat::JsonTerse,
        ] {
            let value = parse(&render_check(&output, &format)?)?;
            assert!(value.get("status").is_none());
            assert_eq!(value["schema_version"], "0.2");
            assert_eq!(value["findings"][0]["classification"], class.as_str());
            assert_eq!(
                value["findings"][0]["missing"],
                serde_json::json!(output.findings[0].missing)
            );
        }
    }
    Ok(())
}

#[test]
fn missing_evidence_artifact_consumer_propagates_failure() -> Result<(), String> {
    let mut output = fixture("boundary_gap")?;
    output.findings.truncate(1);
    // Establish the artifact path before removing just the missing evidence.
    let valid = parse(&render_check(&output, &OutputFormat::CheckFindingsJson)?)?;
    assert!(valid["artifact"].is_object());
    assert_eq!(valid["findings"].as_array().map(Vec::len), Some(1));
    output.findings[0].missing.clear();
    output.findings[0].class = ExposureClass::WeaklyExposed;
    rejection(&output, OutputFormat::CheckFindingsJson)
}

#[test]
fn missing_evidence_producer_retains_actual_weak_stage_explanations() -> Result<(), String> {
    for name in [
        "guarded_result_match_owner_observation",
        "invisible_change_pow_agreement",
        "invisible_change_relational_sibling",
        "rust_lossy_post_operation_pin",
        "rust_test_case_arrow_coincident_row",
        "unwrap_err_variant_single_line",
    ] {
        let output = fixture(name)?;
        let mut subjects = 0;
        for finding in &output.findings {
            if finding.class == ExposureClass::WeaklyExposed
                && finding.ripr.reveal.discriminate.state == StageState::Yes
            {
                subjects += 1;
                assert!(!finding.missing.is_empty(), "{name}: {}", finding.id);
                for stage in [&finding.ripr.infect, &finding.ripr.propagate] {
                    if stage.state == StageState::Weak {
                        assert!(
                            finding.missing.contains(&stage.summary),
                            "{name}: missing actual weak stage explanation"
                        );
                    }
                }
            }
        }
        assert!(
            subjects > 0,
            "{name}: no weak finding with strong discrimination"
        );
        let document = parse(&render_check(&output, &OutputFormat::Json)?)?;
        assert_eq!(
            document["findings"].as_array().map(Vec::len),
            Some(output.findings.len())
        );
    }
    Ok(())
}

#[test]
#[cfg(feature = "lang-typescript")]
fn missing_evidence_validation_uses_the_projected_preview_list() -> Result<(), String> {
    let mut output = fixture("ts_repair_packet_complete")?;
    output.findings.truncate(1);
    let before = parse(&render_check(&output, &OutputFormat::Json)?)?;
    assert_eq!(
        before["findings"][0]["preview_actionability"]["repair_packet_ready"],
        true
    );
    assert_eq!(before["findings"][0]["classification"], "weakly_exposed");
    output.findings[0].missing = vec![
        "TypeScript preview actionability `unknown` / `incomplete_repair_packet`: blocked".into(),
    ];
    // Raw presence is insufficient: the complete packet removes this stale
    // actionability line, leaving no missing-evidence detail in actual JSON.
    rejection(&output, OutputFormat::Json)?;
    let refusal = parse(&ripr::output::json::render(&output))?;
    assert_eq!(refusal["status"], "refused");
    Ok(())
}

#[test]
fn missing_evidence_cannot_hide_beyond_the_findings_byte_budget() -> Result<(), String> {
    let mut output = fixture("boundary_gap")?;
    let mut first = output.findings[0].clone();
    first.class = ExposureClass::ReachableUnrevealed;
    first.evidence = vec!["x".repeat(1_000_001)];
    let mut hidden = first.clone();
    hidden.id.push_str(":hidden");
    hidden.class = ExposureClass::StaticUnknown;
    hidden.evidence.clear();
    let hidden_id = hidden.id.clone();
    output.findings = vec![first, hidden];
    output.summary.findings = 2;
    let bounded = parse(&ripr::output::json::render(&output))?;
    assert_eq!(bounded["findings"].as_array().map(Vec::len), Some(1));
    assert_eq!(bounded["summary"]["findings"], 2);
    output.findings[1].missing.clear();
    match render_check(&output, &OutputFormat::Json) {
        Err(error)
            if error.contains("requires non-empty missing evidence")
                && error.contains(&hidden_id) => {}
        Err(error) => return Err(format!("wrong hidden-finding rejection: {error}")),
        Ok(_) => return Err("malformed omitted finding was accepted".into()),
    }
    let refusal = parse(&ripr::output::json::render(&output))?;
    assert_eq!(refusal["status"], "refused");
    Ok(())
}
