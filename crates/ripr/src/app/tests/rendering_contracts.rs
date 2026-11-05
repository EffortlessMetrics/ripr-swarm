use super::{
    OutputFormat, check_output_with, check_output_with_temp_seam_workspace,
    render_check_with_config, sample_finding,
};
use crate::app::{
    AnalysisProgressEvent, AnalysisProgressScope, AnalysisProgressSink, AnalysisProgressStage,
};
use crate::config::RiprConfig;
use crate::domain::Summary;
use crate::output::render::render_check_with_config_and_progress;
use std::sync::Mutex;

/// #4945: rendering a full-repo audit-path format must report the same
/// closed producer-stage shape the diff-scoped path reports, at repo scope,
/// to whatever sink the caller supplies.
#[test]
fn repo_format_render_reports_stage_events_to_the_progress_sink() -> Result<(), String> {
    struct Recorder(Mutex<Vec<AnalysisProgressEvent>>);
    impl AnalysisProgressSink for Recorder {
        fn emit(&self, event: AnalysisProgressEvent) {
            if let Ok(mut events) = self.0.lock() {
                events.push(event);
            }
        }
    }
    let output = check_output_with_temp_seam_workspace(vec![])?;
    let recorder = Recorder(Mutex::new(Vec::new()));
    let rendered = render_check_with_config_and_progress(
        &output,
        &OutputFormat::RepoSeamsJson,
        &RiprConfig::default(),
        Some(&recorder),
    )?;
    let events = recorder
        .0
        .lock()
        .map(|events| events.clone())
        .unwrap_or_default();
    let stages: Vec<AnalysisProgressStage> = events.iter().map(|event| event.stage).collect();
    assert_eq!(
        stages,
        [
            AnalysisProgressStage::LoadingInput,
            AnalysisProgressStage::Analyzing,
            AnalysisProgressStage::BuildingOutput,
            AnalysisProgressStage::Completed,
        ],
        "repo format must reuse the diff-scoped stage shape: {stages:?}"
    );
    assert!(
        events
            .iter()
            .all(|event| event.scope == AnalysisProgressScope::Repo),
        "every repo-format stage must carry repo scope: {events:?}"
    );
    assert!(
        rendered.contains("\"seams\"") || rendered.starts_with('{'),
        "control: RepoSeamsJson must still render its artifact: {rendered}"
    );
    Ok(())
}

/// #13103: `json-compact` is the same document as `json`, minified. The
/// parsed values must be identical and the `json` bytes must not move.
#[test]
fn json_compact_parses_identically_to_json_and_is_smaller() -> Result<(), String> {
    let output = check_output_with(vec![sample_finding("src/lib.rs", 1)]);
    let config = RiprConfig::default();
    let json = render_check_with_config(&output, &OutputFormat::Json, &config)?;
    let compact = render_check_with_config(&output, &OutputFormat::JsonCompact, &config)?;
    let json_value: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    let compact_value: serde_json::Value =
        serde_json::from_str(&compact).map_err(|error| error.to_string())?;
    assert_eq!(json_value, compact_value);
    assert!(compact.len() < json.len(), "compact {compact}");
    assert!(!compact.contains("\n  "), "compact {compact}");
    Ok(())
}

/// `json-terse` pins the semantic-equivalence contract against a real
/// multi-finding `ripr check --format json` document: every finding, verdict
/// and identity field survives; coded prose expands through the document-head
/// `evidence_legend` to the exact original strings; and the terse document is
/// meaningfully smaller than the minified `json-compact` document.
#[test]
fn json_terse_round_trips_fixture_prose_and_beats_compact_in_size() -> Result<(), String> {
    let full_text = include_str!(
        "../../../../../fixtures/returned_closure_arithmetic_masking/expected/check.json"
    );
    let full: serde_json::Value =
        serde_json::from_str(full_text).map_err(|error| error.to_string())?;
    let terse_text = crate::output::json::render_terse(full_text)?;
    let terse: serde_json::Value =
        serde_json::from_str(&terse_text).map_err(|error| error.to_string())?;
    let legend = terse
        .get("evidence_legend")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    assert!(
        !legend.is_empty(),
        "a real multi-finding document must code repeated prose: {terse_text}"
    );

    let expand = |text: &str| -> String {
        legend
            .get(text)
            .and_then(serde_json::Value::as_str)
            .unwrap_or(text)
            .to_string()
    };
    let full_findings = full["findings"].as_array().cloned().unwrap_or_default();
    let terse_findings = terse["findings"].as_array().cloned().unwrap_or_default();
    assert_eq!(full_findings.len(), terse_findings.len());
    assert!(
        full_findings.len() >= 3,
        "fixture must exercise several findings, got {}",
        full_findings.len()
    );
    for (full_finding, terse_finding) in full_findings.iter().zip(terse_findings.iter()) {
        // Identity and verdict fields move verbatim.
        for key in [
            "id",
            "classification",
            "severity",
            "confidence",
            "probe",
            "ripr",
        ] {
            assert_eq!(full_finding[key], terse_finding[key], "{key} moved");
        }
        // Coded prose surfaces round-trip through the legend, in order.
        for key in ["evidence_path", "evidence", "missing"] {
            let expanded: Vec<String> = terse_finding[key]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(expand)
                .collect();
            let original: Vec<String> = full_finding[key]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_string))
                .collect();
            assert_eq!(expanded, original, "{key} did not round-trip");
        }
    }
    // Head identity fields are untouched.
    for key in [
        "schema_version",
        "tool",
        "mode",
        "root",
        "summary",
        "analysis_outcome",
    ] {
        assert_eq!(full[key], terse[key], "{key} moved");
    }

    let compact_text = crate::output::json::render_compact(full_text);
    assert!(
        terse_text.len() < compact_text.len(),
        "terse {} bytes must beat json-compact {} bytes on a multi-finding document",
        terse_text.len(),
        compact_text.len()
    );
    assert!(
        compact_text.len() < full_text.len(),
        "fixture control: json-compact {} must already beat json {} (whitespace)",
        compact_text.len(),
        full_text.len()
    );

    // The terse head keys must be declared by the published check schema,
    // whose top level is closed (`additionalProperties: false`): the schema
    // is the consumer contract, and json-terse documents must validate.
    let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/ripr/check.schema.json");
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&schema_path)
            .map_err(|error| format!("{}: {error}", schema_path.display()))?,
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        schema["additionalProperties"],
        serde_json::Value::Bool(false),
        "premise: closed check schema"
    );
    let declared = schema["properties"]
        .as_object()
        .ok_or("schema properties")?;
    for key in ["rendering", "evidence_legend", "dropped_aliases"] {
        assert!(declared.contains_key(key), "schema must declare {key}");
        assert!(terse.get(key).is_some(), "terse document must carry {key}");
    }
    assert_eq!(
        declared["rendering"]["const"].as_str(),
        Some("terse"),
        "schema rendering const must match the producer"
    );
    Ok(())
}

/// Pipeline check: `json-terse` through `render_check_with_config` keeps the
/// finding set and verdicts of `json` and stamps the rendering marker. The
/// default `json` bytes are untouched by the new mode.
#[test]
fn json_terse_pipeline_keeps_findings_and_marks_rendering() -> Result<(), String> {
    let output = check_output_with(vec![
        sample_finding("src/lib.rs", 1),
        sample_finding("src/lib.rs", 2),
    ]);
    let config = RiprConfig::default();
    let json = render_check_with_config(&output, &OutputFormat::Json, &config)?;
    let terse = render_check_with_config(&output, &OutputFormat::JsonTerse, &config)?;
    let json_value: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    let terse_value: serde_json::Value =
        serde_json::from_str(&terse).map_err(|error| error.to_string())?;
    // The terse pass rewrites prose fields, so compare identity/verdicts.
    let json_findings = json_value["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let terse_findings = terse_value["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(json_findings.len(), terse_findings.len());
    for (json_finding, terse_finding) in json_findings.iter().zip(terse_findings.iter()) {
        for key in ["id", "classification", "severity", "confidence"] {
            assert_eq!(json_finding[key], terse_finding[key], "{key} moved");
        }
    }
    assert_eq!(
        terse_value["rendering"].as_str(),
        Some("terse"),
        "terse document must name its rendering profile: {terse}"
    );
    assert!(
        terse.len() < json.len(),
        "terse {} must be smaller than json {}",
        terse.len(),
        json.len()
    );
    // The `json` document never gains the terse marker.
    assert!(!json.contains("\"rendering\""), "json bytes moved: {json}");
    Ok(())
}

/// #13195: `sarif-compact` is the same SARIF document as `sarif`, minified,
/// through the public render path; the `sarif` bytes must not move.
#[test]
fn sarif_compact_parses_identically_to_sarif_and_is_smaller() -> Result<(), String> {
    let output = check_output_with(vec![sample_finding("src/lib.rs", 1)]);
    let config = RiprConfig::default();
    let sarif = render_check_with_config(&output, &OutputFormat::Sarif, &config)?;
    let compact = render_check_with_config(&output, &OutputFormat::SarifCompact, &config)?;
    let sarif_value: serde_json::Value =
        serde_json::from_str(&sarif).map_err(|error| error.to_string())?;
    let compact_value: serde_json::Value =
        serde_json::from_str(&compact).map_err(|error| error.to_string())?;
    assert_eq!(sarif_value, compact_value);
    assert_eq!(compact_value["version"], "2.1.0");
    assert!(compact.len() < sarif.len(), "compact {compact}");
    assert!(!compact.contains("\n  "), "compact {compact}");
    Ok(())
}

#[test]
fn summary_default_is_empty() {
    let summary = Summary::default();
    assert_eq!(summary.findings, 0);
    assert_eq!(summary.exposed, 0);
    assert_eq!(summary.weakly_exposed, 0);
}

#[test]
fn configured_finding_severity_applies_to_human_json_and_github() -> Result<(), String> {
    let output = check_output_with(vec![sample_finding("src/lib.rs", 1)]);
    let config =
        crate::config::tests_only_parse("[severity.findings]\nweakly_exposed = \"info\"\n")?;

    let human = render_check_with_config(&output, &OutputFormat::Human, &config)?;
    let json = render_check_with_config(&output, &OutputFormat::Json, &config)?;
    let github = render_check_with_config(&output, &OutputFormat::Github, &config)?;

    if !human.contains("Static exposure: weak (severity: info, confidence") {
        return Err(format!("human severity was not configured: {human}"));
    }
    if !json.contains("\"severity\": \"info\"") {
        return Err(format!("json severity was not configured: {json}"));
    }
    if !github.starts_with("::notice ") {
        return Err(format!("github severity was not configured: {github}"));
    }
    Ok(())
}

#[test]
fn per_crate_severity_overrides_change_json_independently() -> Result<(), String> {
    let config = crate::config::tests_only_parse(
        "[severity.findings]\nweakly_exposed = \"warning\"\n\n\
         [[overrides]]\npath = \"crates/alpha/**\"\nseverity.findings.weakly_exposed = \"info\"\n\n\
         [[overrides]]\npath = \"crates/beta/**\"\nseverity.findings.weakly_exposed = \"note\"\n",
    )?;
    let alpha = render_check_with_config(
        &check_output_with(vec![sample_finding("crates/alpha/src/lib.rs", 1)]),
        &OutputFormat::Json,
        &config,
    )?;
    let beta = render_check_with_config(
        &check_output_with(vec![sample_finding("crates/beta/src/lib.rs", 1)]),
        &OutputFormat::Json,
        &config,
    )?;
    if !alpha.contains("\"severity\": \"info\"") {
        return Err(format!("alpha crate expected info: {alpha}"));
    }
    if !beta.contains("\"severity\": \"note\"") {
        return Err(format!("beta crate expected note: {beta}"));
    }
    Ok(())
}

#[test]
fn no_overrides_keep_configured_severity_byte_identical() -> Result<(), String> {
    let output = check_output_with(vec![sample_finding("crates/alpha/src/lib.rs", 1)]);
    let repo_wide =
        crate::config::tests_only_parse("[severity.findings]\nweakly_exposed = \"info\"\n")?;
    let empty_overrides = crate::config::tests_only_parse(
        "overrides = []\n\n[severity.findings]\nweakly_exposed = \"info\"\n",
    )?;
    let left = render_check_with_config(&output, &OutputFormat::Json, &repo_wide)?;
    let right = render_check_with_config(&output, &OutputFormat::Json, &empty_overrides)?;
    if left != right {
        return Err("empty overrides must not change rendered JSON".to_string());
    }
    Ok(())
}
