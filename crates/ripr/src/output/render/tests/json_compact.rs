//! The compact spelling is a presentation change, never a new evidence owner.

use super::{
    CheckDiffProvenance, FindingDrillIn, FindingNavigation, OutputFormat, RiprConfig,
    attach_canonical_next_action, check_output_with, json,
    render_check_with_config_and_navigation_and_progress, sample_finding,
};
use crate::output::compact_json;

fn same_document(pretty: &str, compact: &str) -> Result<serde_json::Value, String> {
    let expected: serde_json::Value = serde_json::from_str(pretty)
        .map_err(|error| format!("legacy JSON must parse: {error}"))?;
    let actual: serde_json::Value = serde_json::from_str(compact)
        .map_err(|error| format!("compact JSON must parse: {error}"))?;
    if actual != expected {
        return Err("compact JSON changed the report, evidence or recovery guidance".to_string());
    }
    if !compact.ends_with('\n') || compact.lines().count() != 1 {
        return Err("compact JSON must be one document line with a final newline".to_string());
    }
    if compact.len() >= pretty.len() {
        return Err("the formatted report must become strictly smaller".to_string());
    }
    Ok(actual)
}

#[test]
fn json_compact_public_render_preserves_findings_and_legacy_bytes() -> Result<(), String> {
    let mut finding = sample_finding("src/lib.rs", 1);
    finding.evidence.push(" keep  spaces; café 雪; \"quoted\" \\ path\nnext\tline ".to_string());
    let output = check_output_with(vec![finding]);
    let pretty = crate::render_check(&output, &OutputFormat::Json)?;
    let compact = crate::render_check(&output, &OutputFormat::JsonCompact)?;
    let value = same_document(&pretty, &compact)?;
    if value["findings"].as_array().map(Vec::len) != Some(1) {
        return Err("fixture must retain a real finding".to_string());
    }
    if value.get("canonical_next_action").is_some() {
        return Err("generic JSON must not invent navigation authority".to_string());
    }
    let legacy = super::super::stamp_check_json(
        json::render_with_config(
            &output,
            &RiprConfig::default(),
            json::check_findings_byte_budget()?,
        ),
        &output.root,
    );
    if pretty != legacy {
        return Err("existing json output bytes changed".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_empty_public_report_is_the_same_document() -> Result<(), String> {
    let output = check_output_with(Vec::new());
    let pretty = crate::render_check(&output, &OutputFormat::Json)?;
    let compact = crate::render_check(&output, &OutputFormat::JsonCompact)?;
    let value = same_document(&pretty, &compact)?;
    if value["findings"].as_array().map(Vec::len) != Some(0) {
        return Err("empty fixture must keep an empty findings array".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_navigation_keeps_the_complete_action_and_scope() -> Result<(), String> {
    let output = check_output_with(vec![sample_finding("src/lib.rs", 1)]);
    let config = RiprConfig::default();
    let drill_in = FindingDrillIn::Commands(FindingNavigation::legacy());
    for (pretty_scope, compact_scope) in [
        (CheckDiffProvenance::Worktree, CheckDiffProvenance::Worktree),
        (
            CheckDiffProvenance::CommittedHistory,
            CheckDiffProvenance::CommittedHistory,
        ),
        (
            CheckDiffProvenance::SuppliedScope,
            CheckDiffProvenance::SuppliedScope,
        ),
    ] {
        let pretty = render_check_with_config_and_navigation_and_progress(
            &output,
            &OutputFormat::Json,
            &config,
            Some(&drill_in),
            None,
            pretty_scope,
        )?;
        let compact = render_check_with_config_and_navigation_and_progress(
            &output,
            &OutputFormat::JsonCompact,
            &config,
            Some(&drill_in),
            None,
            compact_scope,
        )?;
        let value = same_document(&pretty, &compact)?;
        if value["canonical_next_action"]["action_class"] != "inspect_details" {
            return Err("the selected gap must keep its inspect-only action".to_string());
        }
        if value["canonical_next_action"]["subject"]["item"]
            != "probe:src_lib_rs:42:error_path"
        {
            return Err("the canonical action lost its finding identity".to_string());
        }
    }
    Ok(())
}

#[test]
fn json_compact_missing_scope_keeps_the_prerequisite_not_a_clean_result() -> Result<(), String> {
    let mut output = check_output_with(Vec::new());
    output.no_scope_provided = true;
    output.base = None;
    let config = RiprConfig::default();
    let render = |format: &OutputFormat| {
        render_check_with_config_and_navigation_and_progress(
            &output,
            format,
            &config,
            None,
            None,
            CheckDiffProvenance::CommittedHistory,
        )
    };
    let pretty = render(&OutputFormat::Json)?;
    let compact = render(&OutputFormat::JsonCompact)?;
    let value = same_document(&pretty, &compact)?;
    if value["canonical_next_action"]["action_class"] != "satisfy_prerequisite" {
        return Err("no-scope output must retain the prerequisite stop".to_string());
    }
    if !value["canonical_next_action"]["command"].is_null() {
        return Err("compaction must not mint an executable command".to_string());
    }
    Ok(())
}

#[test]
fn json_compact_preserves_the_bounded_findings_document_and_disclosure() -> Result<(), String> {
    let first = sample_finding("src/lib.rs", 1);
    let mut second = sample_finding("src/other.rs", 2);
    second.id = "probe:src_other_rs:2:error_path".to_string();
    let output = check_output_with(vec![first, second]);
    let config = RiprConfig::default();
    let drill_in = FindingDrillIn::Commands(FindingNavigation::legacy());
    let bounded = json::render_with_config(
        &output,
        &config,
        Some((1, json::FindingsBudgetSource::Default)),
    );
    let bounded = super::super::stamp_check_json(bounded, &output.root);
    let bounded = attach_canonical_next_action(
        bounded,
        &output,
        &config,
        Some(&drill_in),
        CheckDiffProvenance::Worktree,
    );
    if !bounded.contains(json::FINDINGS_BOUND_RUN_STATUS) {
        return Err("bounded fixture must disclose its limitation".to_string());
    }
    let compact = compact_json::finish(bounded.clone(), &OutputFormat::JsonCompact);
    let value = same_document(&bounded, &compact)?;
    let findings = value["findings"].as_array().ok_or("missing findings array")?;
    if findings.len() >= output.findings.len() {
        return Err("bounded fixture must actually omit at least one finding".to_string());
    }
    if value["canonical_next_action"].is_null() {
        return Err("compaction must retain the appended canonical action".to_string());
    }
    Ok(())
}
