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
        "[severity.findings]\nweakly_exposed = \"info\"\noverrides = []\n",
    )?;
    let left = render_check_with_config(&output, &OutputFormat::Json, &repo_wide)?;
    let right = render_check_with_config(&output, &OutputFormat::Json, &empty_overrides)?;
    if left != right {
        return Err("empty overrides must not change rendered JSON".to_string());
    }
    Ok(())
}
