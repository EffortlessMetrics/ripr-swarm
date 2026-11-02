//! `cargo xtask repair-funnel-report` (#6308): joins accepted install/first-run,
//! repair-attempt verification, and static-movement facts into one versioned
//! first-hour repair funnel without reconstructing attempt lifecycle or
//! executing new real-repository work.
//!
//! JSON and Markdown derive from one normalized [`FirstHourRepairFunnelReportV1`].
//! `cargo xtask dx-scoreboard --boards repair --ingest` consumes the report
//! receipt via [`repair_funnel_to_input`].

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

pub(crate) const INPUTS_SCHEMA_VERSION: &str = "first_hour_repair_funnel_inputs.v1";
pub(crate) const REPORT_SCHEMA_VERSION: &str = "first_hour_repair_funnel.v1";

const CONTROLS_CORPUS: &str = "fixtures/repair_funnel/controls/corpus.json";
const CLAIM_BOUNDARY: &str = "Projects accepted receipt facts into a first-hour repair funnel and dx-scoreboard metrics; it does not execute governed attempts, promote support tiers, or claim product performance meets a release bar.";

const COHORTS: [&str; 4] = [
    "deterministic_fixture",
    "installed_deterministic_journey",
    "blind_operator_journey",
    "governed_real_repository_attempt",
];

const STAGES: [&str; 11] = [
    "install_complete",
    "first_doctor_or_preflight_complete",
    "first_check_complete",
    "first_honest_result",
    "first_actionable_card",
    "repair_started",
    "edit_understood_or_applied",
    "project_verification_executed",
    "after_analysis_complete",
    "verified_improvement_or_closure",
    "terminal_receipt_issued",
];

const EXITS: [&str; 15] = [
    "verified_improvement",
    "verified_closure",
    "honest_no_action",
    "honest_static_limitation",
    "setup_or_environment_blocker",
    "product_discoverability_failure",
    "wrong_or_unsafe_target",
    "verification_failed",
    "verification_not_run",
    "stale_or_wrong_subject",
    "manual_plumbing_required",
    "performance_timeout",
    "product_defect",
    "instrument_failure",
    "abandoned_with_reason",
];

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StageObservationV1 {
    pub stage: String,
    pub at_ms: Option<i64>,
    pub receipt_digest: String,
    pub not_measured_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MetricObservationV1 {
    pub id: String,
    pub value: Option<f64>,
    pub unit: String,
    pub sample_count: u64,
    pub state: String,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FunnelCaseRowV1 {
    pub case_id: String,
    pub cohort: String,
    pub selected_for_denominator: bool,
    pub funnel_exit: String,
    pub stages: Vec<StageObservationV1>,
    pub metrics: Vec<MetricObservationV1>,
    pub identity: BTreeMap<String, String>,
    pub control_violations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FirstHourRepairFunnelReportV1 {
    pub schema_version: String,
    pub claim_boundary: String,
    pub corpus_path: String,
    pub case_count: u64,
    pub selected_count: u64,
    pub control_failures: Vec<String>,
    pub cases: Vec<FunnelCaseRowV1>,
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn read_json(path: &Path, label: &str) -> Result<Value, String> {
    let body = fs::read_to_string(path)
        .map_err(|error| format!("read {label} {}: {error}", path.display()))?;
    serde_json::from_str(&body)
        .map_err(|error| format!("parse {label} {}: {error}", path.display()))
}

fn install_step_names() -> BTreeSet<&'static str> {
    BTreeSet::from(["install_published", "install_archive"])
}

fn stage_ms_from_first_run(value: &Value, step: &str) -> Option<i64> {
    let setup = value.get("setup")?.as_array()?;
    let mut total = 0.0;
    let mut seen = false;
    for entry in setup {
        if entry.get("step")?.as_str()? != step {
            continue;
        }
        seen = true;
        if entry.get("exit")?.as_i64()? != 0 {
            return None;
        }
        total += entry.get("secs")?.as_f64()?;
    }
    seen.then(|| (total * 1000.0).round() as i64)
}

fn first_run_case_step_ms(case: &Value, step: &str) -> Option<i64> {
    let mut total = 0.0;
    for entry in case.get("steps")?.as_array()? {
        if entry.get("step")?.as_str()? != step {
            continue;
        }
        if entry.get("exit")?.as_i64()? != 0 {
            return None;
        }
        total += entry.get("secs")?.as_f64()?;
    }
    (total > 0.0).then(|| (total * 1000.0).round() as i64)
}

fn first_honest_result_ms(case: &Value) -> Option<i64> {
    let verdict = case.get("verdict")?.as_str()?;
    if verdict.ends_with("_unknown") {
        return None;
    }
    first_run_case_step_ms(case, "check")
}

fn cumulative_first_run_ms(value: &Value, through: &str) -> Option<i64> {
    let install = install_step_names()
        .iter()
        .find_map(|step| stage_ms_from_first_run(value, step))
        .unwrap_or(0);
    let cases = value.get("cases")?.as_array()?;
    let first = cases.first()?;
    let case_offset = match through {
        "first_check_complete" | "first_honest_result" => first_run_case_step_ms(first, "check")?,
        _ => return None,
    };
    Some(install + case_offset)
}

fn static_movement_from_attempt(attempt: &Value) -> Option<&'static str> {
    if let Some(state) = attempt.get("static_movement") {
        return match state.as_str()? {
            "improved" => Some("improved"),
            "unchanged" => Some("unchanged"),
            "regressed" => Some("regressed"),
            "closed" => Some("closed"),
            "unknown" => Some("unknown"),
            _ => None,
        };
    }
    match attempt.get("receipt_state")?.as_str()? {
        "receipt_movement_improved" | "receipt_movement_improved_closed" => Some("improved"),
        "receipt_movement_unchanged" => Some("unchanged"),
        "receipt_movement_regressed" => Some("regressed"),
        "receipt_movement_closed" | "receipt_gap_closed" => Some("closed"),
        _ => Some("unknown"),
    }
}

fn verify_executed(attempt: &Value) -> bool {
    attempt
        .get("verify_executed")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            matches!(
                attempt.get("verify_result").and_then(Value::as_str),
                Some("pass") | Some("fail") | Some("failed")
            )
        })
}

fn verify_passed(attempt: &Value) -> bool {
    matches!(
        attempt.get("verify_result").and_then(Value::as_str),
        Some("pass")
    )
}

fn cohort_allowed(cohort: &str) -> bool {
    COHORTS.contains(&cohort)
}

pub(crate) fn project_case(inputs: &Value) -> Result<FunnelCaseRowV1, String> {
    if inputs.get("schema_version").and_then(Value::as_str) != Some(INPUTS_SCHEMA_VERSION) {
        return Err(format!(
            "inputs schema_version must be `{INPUTS_SCHEMA_VERSION}`"
        ));
    }
    let case_id = inputs
        .get("case_id")
        .and_then(Value::as_str)
        .ok_or("inputs need case_id")?
        .to_string();
    let cohort = inputs
        .get("cohort")
        .and_then(Value::as_str)
        .ok_or("inputs need cohort")?
        .to_string();
    if !cohort_allowed(&cohort) {
        return Err(format!("unknown cohort `{cohort}`"));
    }
    let selected = inputs
        .get("selected_for_denominator")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let clocks_compatible = inputs
        .get("clocks_compatible")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut identity = BTreeMap::new();
    if let Some(block) = inputs.get("identity").and_then(Value::as_object) {
        for (key, value) in block {
            if let Some(text) = value.as_str() {
                identity.insert(key.clone(), text.to_string());
            }
        }
    }

    let first_run = inputs.get("first_run");
    let attempt = inputs
        .get("repair_attempt")
        .or_else(|| inputs.get("real_repair_attempt"));
    let repair_card = inputs.get("repair_card");
    let terminal = inputs
        .get("terminal_receipt_issued")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut stages = Vec::new();
    let mut push_stage = |stage: &str, at_ms: Option<i64>, digest: &str| {
        stages.push(StageObservationV1 {
            stage: stage.to_string(),
            at_ms,
            receipt_digest: digest.to_string(),
            not_measured_reason: None,
        });
    };

    if let Some(fr) = first_run
        && fr.get("schema_version").and_then(Value::as_str) == Some("first_run.v1")
    {
        if let Some(ms) = install_step_names()
            .iter()
            .find_map(|step| stage_ms_from_first_run(fr, step))
        {
            push_stage("install_complete", Some(ms), "first_run.v1/setup/install");
        }
        if stage_ms_from_first_run(fr, "doctor").is_some() {
            push_stage(
                "first_doctor_or_preflight_complete",
                stage_ms_from_first_run(fr, "doctor"),
                "first_run.v1/setup/doctor",
            );
        }
        if let Some(ms) = cumulative_first_run_ms(fr, "first_check_complete") {
            push_stage("first_check_complete", Some(ms), "first_run.v1/cases/check");
        }
        if let Some(case) = fr
            .get("cases")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            && let Some(ms) = first_honest_result_ms(case)
        {
            let install = install_step_names()
                .iter()
                .find_map(|step| stage_ms_from_first_run(fr, step))
                .unwrap_or(0);
            push_stage(
                "first_honest_result",
                Some(install + ms),
                "first_run.v1/cases/decisive_verdict",
            );
        }
    }

    if repair_card
        .and_then(|c| c.get("actionable"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        push_stage(
            "first_actionable_card",
            repair_card
                .and_then(|c| c.get("at_ms"))
                .and_then(Value::as_i64),
            "repair_card_usability/actionable",
        );
    }
    if inputs
        .get("repair_started")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        push_stage(
            "repair_started",
            inputs.get("repair_started_ms").and_then(Value::as_i64),
            "repair_attempt/started",
        );
    }
    if inputs
        .get("edit_understood_or_applied")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        push_stage(
            "edit_understood_or_applied",
            inputs.get("edit_applied_ms").and_then(Value::as_i64),
            "repair_attempt/edit",
        );
    }

    let movement = attempt.and_then(static_movement_from_attempt);
    let verification_ran = attempt.is_some_and(verify_executed);
    if verification_ran {
        push_stage(
            "project_verification_executed",
            attempt
                .and_then(|a| a.get("verify_ms"))
                .and_then(Value::as_i64),
            "repair_attempt/verify",
        );
    }
    if inputs
        .get("after_analysis_complete")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        push_stage(
            "after_analysis_complete",
            inputs.get("after_analysis_ms").and_then(Value::as_i64),
            "repair_attempt/after_analysis",
        );
    }

    let verified_improvement = verification_ran
        && verify_passed(attempt.unwrap_or(&Value::Null))
        && matches!(movement, Some("improved") | Some("closed"));
    if verified_improvement {
        push_stage(
            "verified_improvement_or_closure",
            attempt
                .and_then(|a| a.get("verified_improvement_ms"))
                .and_then(Value::as_i64),
            "repair_attempt/verify+static_movement",
        );
    }
    if terminal {
        push_stage(
            "terminal_receipt_issued",
            inputs.get("terminal_receipt_ms").and_then(Value::as_i64),
            "terminal_receipt",
        );
    }

    let funnel_exit = derive_exit(inputs, movement, verification_ran, attempt);
    if !EXITS.contains(&funnel_exit.as_str()) {
        return Err(format!("derived invalid funnel exit `{funnel_exit}`"));
    }
    for stage in &stages {
        if !STAGES.contains(&stage.stage.as_str()) {
            return Err(format!("unknown funnel stage `{}`", stage.stage));
        }
    }

    let mut metrics = Vec::new();
    let stage_at = |name: &str| {
        stages
            .iter()
            .find(|s| s.stage == name)
            .and_then(|s| s.at_ms)
    };
    let not_measured = |reason: &str| MetricObservationV1 {
        id: String::new(),
        value: None,
        unit: String::new(),
        sample_count: 0,
        state: "not_measured".to_string(),
        non_claims: vec![reason.to_string()],
    };
    let mut time_metric = |id: &str, start: Option<i64>, end: Option<i64>| {
        let mut row = not_measured("missing stage timestamps");
        row.id = id.to_string();
        row.unit = "ms".to_string();
        if !clocks_compatible {
            row.non_claims = vec!["incompatible_clocks".to_string()];
            metrics.push(row);
            return;
        }
        let (Some(start), Some(end)) = (start, end) else {
            if start.is_none() && id.contains("install") {
                row.non_claims = vec!["missing_install_receipt".to_string()];
            }
            metrics.push(row);
            return;
        };
        if end < start {
            row.non_claims = vec!["negative_duration".to_string()];
            metrics.push(row);
            return;
        }
        row.value = Some((end - start) as f64);
        row.sample_count = 1;
        row.state = "measured".to_string();
        row.non_claims.clear();
        metrics.push(row);
    };

    time_metric(
        "time_to_first_honest_result",
        stage_at("install_complete"),
        stage_at("first_honest_result"),
    );
    time_metric(
        "time_to_first_actionable_card",
        stage_at("install_complete"),
        stage_at("first_actionable_card"),
    );
    time_metric(
        "time_to_verified_improvement",
        stage_at("install_complete"),
        stage_at("verified_improvement_or_closure"),
    );

    let mut control_violations = validate_controls(inputs, &stages, &funnel_exit, movement);
    control_violations.sort();

    Ok(FunnelCaseRowV1 {
        case_id,
        cohort,
        selected_for_denominator: selected,
        funnel_exit,
        stages,
        metrics,
        identity,
        control_violations,
    })
}

fn derive_exit(
    inputs: &Value,
    movement: Option<&str>,
    verification_ran: bool,
    attempt: Option<&Value>,
) -> String {
    if let Some(exit) = inputs.get("funnel_exit").and_then(Value::as_str) {
        return exit.to_string();
    }
    if inputs
        .get("instrument_failure")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return "instrument_failure".to_string();
    }
    if !verification_ran {
        return "verification_not_run".to_string();
    }
    let attempt = attempt.unwrap_or(&Value::Null);
    if !verify_passed(attempt) {
        return "verification_failed".to_string();
    }
    match movement {
        Some("improved") => "verified_improvement".to_string(),
        Some("closed") => "verified_closure".to_string(),
        Some("unchanged") | Some("unknown") => "honest_static_limitation".to_string(),
        Some("regressed") => "product_defect".to_string(),
        _ => "honest_no_action".to_string(),
    }
}

fn validate_controls(
    inputs: &Value,
    stages: &[StageObservationV1],
    funnel_exit: &str,
    movement: Option<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let attempt = inputs
        .get("repair_attempt")
        .or_else(|| inputs.get("real_repair_attempt"));
    let has_verified_stage = stages
        .iter()
        .any(|s| s.stage == "verified_improvement_or_closure");
    let verification_ran = attempt.is_some_and(verify_executed);

    // 1. Later receipt without verification must not claim verified-improvement stage.
    if has_verified_stage && !verification_ran {
        violations.push("control_1: verified_improvement stage without verification".into());
    }
    // 2. Passing verification with unchanged/regressed static movement is not improvement.
    if has_verified_stage
        && attempt.is_some_and(verify_passed)
        && matches!(movement, Some("unchanged") | Some("regressed"))
    {
        violations.push("control_2: verified_improvement with unchanged/regressed movement".into());
    }
    // 3. Static gap closed but verification failed/not-run.
    if matches!(movement, Some("improved") | Some("closed"))
        && attempt.is_some_and(|a| !verify_passed(a))
        && matches!(funnel_exit, "verified_improvement" | "verified_closure")
    {
        violations.push("control_3: verified exit without passing verification".into());
    }
    // 4. Hidden assistance poisons blind success.
    if inputs
        .get("cohort")
        .and_then(Value::as_str)
        .is_some_and(|c| c == "blind_operator_journey")
        && inputs
            .get("hidden_assistance")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                attempt
                    .and_then(|a| a.get("hidden_assistance"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
        && matches!(funnel_exit, "verified_improvement" | "verified_closure")
    {
        violations
            .push("control_4: blind journey counted as verified with hidden assistance".into());
    }
    // 5. Fixture cohort cannot masquerade as governed real attempt in real-route metrics.
    if inputs.get("cohort").and_then(Value::as_str) == Some("deterministic_fixture")
        && inputs
            .get("claim_real_attempt")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        violations.push("control_5: fixture cohort claimed as real attempt".into());
    }
    // 6. Selected denominator must not drop failed rows silently.
    if inputs
        .get("drop_from_denominator")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        violations.push("control_6: row removed from denominator".into());
    }
    violations
}

pub(crate) fn assess_controls_corpus(corpus: &Value) -> FirstHourRepairFunnelReportV1 {
    let mut cases = Vec::new();
    let mut control_failures = Vec::new();
    let entries = corpus
        .get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for entry in entries {
        let inputs = entry.get("inputs").cloned().unwrap_or(Value::Null);
        let expect_ok = entry
            .get("expect_projection_ok")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let expect_controls = entry
            .get("expect_control_violations")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        match project_case(&inputs) {
            Ok(row) => {
                if !expect_ok {
                    control_failures.push(format!(
                        "{}: expected projection failure but succeeded",
                        row.case_id
                    ));
                }
                for expected in &expect_controls {
                    let Some(expected) = expected.as_str() else {
                        continue;
                    };
                    if !row
                        .control_violations
                        .iter()
                        .any(|v| v.starts_with(expected))
                    {
                        control_failures.push(format!(
                            "{}: missing expected control violation prefix `{expected}`",
                            row.case_id
                        ));
                    }
                }
                if expect_controls.is_empty() && !row.control_violations.is_empty() {
                    control_failures.push(format!(
                        "{}: unexpected control violations {:?}",
                        row.case_id, row.control_violations
                    ));
                }
                cases.push(row);
            }
            Err(error) => {
                if expect_ok {
                    control_failures.push(format!(
                        "{}: projection failed: {error}",
                        entry.get("id").and_then(Value::as_str).unwrap_or("case")
                    ));
                }
            }
        }
    }
    let selected_count = cases.iter().filter(|c| c.selected_for_denominator).count() as u64;
    FirstHourRepairFunnelReportV1 {
        schema_version: REPORT_SCHEMA_VERSION.to_string(),
        claim_boundary: CLAIM_BOUNDARY.to_string(),
        corpus_path: CONTROLS_CORPUS.to_string(),
        case_count: cases.len() as u64,
        selected_count,
        control_failures,
        cases,
    }
}

pub(crate) fn repair_funnel_report_value() -> Result<FirstHourRepairFunnelReportV1, String> {
    let corpus = read_json(&workspace_path(CONTROLS_CORPUS), "repair funnel controls")?;
    Ok(assess_controls_corpus(&corpus))
}

pub(crate) fn repair_funnel_report() -> Result<(), String> {
    let report = repair_funnel_report_value()?;
    if !report.control_failures.is_empty() {
        return Err(format!(
            "repair funnel controls failed: {}",
            report.control_failures.join("; ")
        ));
    }
    let json_body = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("serialize repair funnel report: {error}"))?;
    crate::write_report("repair-funnel.json", &format!("{json_body}\n"))?;
    crate::write_report("repair-funnel.md", &repair_funnel_markdown(&report))?;
    println!("{json_body}");
    Ok(())
}

pub(crate) fn repair_funnel_markdown(report: &FirstHourRepairFunnelReportV1) -> String {
    let mut out = String::new();
    out.push_str("# First-hour repair funnel report\n\n");
    out.push_str(&format!("Schema: `{REPORT_SCHEMA_VERSION}`\n\n"));
    out.push_str(&format!("{}\n\n", report.claim_boundary));
    out.push_str(&format!(
        "Cases: {} (selected for denominator: {})\n\n",
        report.case_count, report.selected_count
    ));
    for case in &report.cases {
        out.push_str(&format!(
            "## {} (`{}` → `{}`)\n\n",
            case.case_id, case.cohort, case.funnel_exit
        ));
        out.push_str("Stages present:\n\n");
        for stage in &case.stages {
            out.push_str(&format!(
                "- `{}` at {:?} ({}) \n",
                stage.stage, stage.at_ms, stage.receipt_digest
            ));
        }
        out.push('\n');
    }
    out
}

/// Convert a `first_hour_repair_funnel.v1` report into dx-scoreboard input rows.
pub(crate) fn repair_funnel_to_input(value: &Value) -> Result<Value, String> {
    if value.get("schema_version").and_then(Value::as_str) != Some(REPORT_SCHEMA_VERSION) {
        return Err(format!(
            "repair funnel ingest needs schema `{REPORT_SCHEMA_VERSION}`"
        ));
    }
    let cases = value
        .get("cases")
        .and_then(Value::as_array)
        .ok_or("repair funnel report needs cases[]")?;
    let selected: Vec<&Value> = cases
        .iter()
        .filter(|case| {
            case.get("selected_for_denominator")
                .and_then(Value::as_bool)
                != Some(false)
        })
        .collect();
    let sample_count = selected.len() as u64;
    let mut rows = Vec::new();
    let mut push_metric = |id: &str, _unit: &str, values: Vec<f64>| {
        if sample_count == 0 || values.is_empty() {
            rows.push(json!({
                "id": id,
                "value": 0,
                "completed": false,
                "evidence": "zero selected cases; not_measured",
            }));
            return;
        }
        let sum: f64 = values.iter().sum();
        rows.push(json!({
            "id": id,
            "value": sum / values.len() as f64,
            "completed": true,
            "evidence": format!("{id}: mean over {} selected case(s)", values.len()),
        }));
    };

    let mut verified_ms = Vec::new();
    let mut honest_ms = Vec::new();
    for case in &selected {
        let metrics = case.get("metrics").and_then(Value::as_array);
        if let Some(metrics) = metrics {
            for metric in metrics {
                let id = metric.get("id").and_then(Value::as_str).unwrap_or("");
                if metric.get("state").and_then(Value::as_str) != Some("measured") {
                    continue;
                }
                let Some(v) = metric.get("value").and_then(Value::as_f64) else {
                    continue;
                };
                match id {
                    "time_to_verified_improvement" => verified_ms.push(v),
                    "time_to_first_honest_result" => honest_ms.push(v),
                    _ => {}
                }
            }
        }
    }
    push_metric("repair.time_to_verified_improvement_ms", "ms", verified_ms);
    push_metric("repair.time_to_first_honest_result_ms", "ms", honest_ms);
    push_metric(
        "repair.selected_case_count",
        "cases",
        if sample_count == 0 {
            vec![]
        } else {
            vec![sample_count as f64]
        },
    );
    Ok(json!({
        "schema_version": "ripr-dx-scoreboard-input-v1",
        "source": "repair-funnel",
        "evidence": "first_hour_repair_funnel.v1 report",
        "metrics": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_funnel_controls_corpus_projects_and_enforces_controls() -> Result<(), String> {
        let corpus = read_json(&workspace_path(CONTROLS_CORPUS), "controls")?;
        let report = assess_controls_corpus(&corpus);
        assert!(
            report.control_failures.is_empty(),
            "{:?}",
            report.control_failures
        );
        assert!(report.case_count >= 6);
        Ok(())
    }

    #[test]
    fn verified_improvement_requires_verification_and_movement() -> Result<(), String> {
        let inputs = json!({
            "schema_version": INPUTS_SCHEMA_VERSION,
            "case_id": "unit-verified",
            "cohort": "governed_real_repository_attempt",
            "clocks_compatible": true,
            "first_run": {
                "schema_version": "first_run.v1",
                "ripr": "1.0.0",
                "setup": [
                    {"step": "install_published", "exit": 0, "secs": 10.0}
                ],
                "cases": [{
                    "case": "semver",
                    "verdict": "exposed",
                    "steps": [{"step": "check", "exit": 0, "secs": 5.0}]
                }]
            },
            "repair_attempt": {
                "verify_result": "pass",
                "verify_executed": true,
                "static_movement": "improved",
                "verified_improvement_ms": 20000
            },
            "terminal_receipt_issued": true,
            "terminal_receipt_ms": 21000
        });
        let row = project_case(&inputs)?;
        assert!(
            row.stages
                .iter()
                .any(|s| s.stage == "verified_improvement_or_closure")
        );
        assert_eq!(row.funnel_exit, "verified_improvement");
        let metric = row
            .metrics
            .iter()
            .find(|m| m.id == "time_to_verified_improvement")
            .ok_or("missing metric")?;
        assert_eq!(metric.state, "measured");
        assert_eq!(metric.value, Some(10000.0));
        Ok(())
    }

    #[test]
    fn verification_pass_alone_does_not_claim_improvement() -> Result<(), String> {
        let inputs = json!({
            "schema_version": INPUTS_SCHEMA_VERSION,
            "case_id": "unit-static-unchanged",
            "cohort": "governed_real_repository_attempt",
            "repair_attempt": {
                "verify_result": "pass",
                "verify_executed": true,
                "static_movement": "unchanged"
            }
        });
        let row = project_case(&inputs)?;
        assert!(
            !row.stages
                .iter()
                .any(|s| s.stage == "verified_improvement_or_closure")
        );
        assert_eq!(row.funnel_exit, "honest_static_limitation");
        Ok(())
    }

    #[test]
    fn repair_funnel_to_input_zero_cases_is_not_measured() -> Result<(), String> {
        let report = json!({
            "schema_version": REPORT_SCHEMA_VERSION,
            "cases": []
        });
        let input = repair_funnel_to_input(&report)?;
        let metrics = input["metrics"].as_array().ok_or("metrics")?;
        assert!(metrics.iter().all(|row| row["completed"] == false));
        Ok(())
    }
}
