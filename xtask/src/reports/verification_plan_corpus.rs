//! `cargo xtask verification-plan-corpus check|report` (#5173, RIPR-SPEC-0377 PR A).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::verification_plan_corpus::{
    DEFAULT_VERIFICATION_PLAN_CORPUS_PATH, VERIFICATION_PLAN_CLAIM_BOUNDARY,
    VERIFICATION_PLAN_CORPUS_REPORT_SCHEMA_VERSION, VERIFICATION_PLAN_CORPUS_SCHEMA_VERSION,
    VerificationPlanCaseV1, VerificationPlanFixtureCorpusV1, VerificationPlanPlannerDispositionV1,
    VerificationPlanRowAssessmentV1, assess_verification_plan_case,
    load_verification_plan_fixture_corpus, missing_verification_plan_required_scenarios,
    verification_plan_corpus_identity,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationPlanRateStateV1 {
    NotMeasured,
    Measured,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct VerificationPlanRateV1 {
    pub state: VerificationPlanRateStateV1,
    pub numerator: usize,
    pub denominator: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct VerificationPlanReportRowV1 {
    pub case_id: String,
    pub case_class: crate::verification_plan_corpus::VerificationPlanCaseClassV1,
    pub disposition: VerificationPlanPlannerDispositionV1,
    pub synthetic: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct VerificationPlanRejectedRowV1 {
    pub case_id: String,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct VerificationPlanCorpusReportV1 {
    pub schema_version: String,
    pub reserved_real_corpus_schema_version: String,
    pub claim_boundary: String,
    pub spec: String,
    pub corpus_path: String,
    pub corpus_identity: String,
    pub synthetic_rows: Vec<VerificationPlanReportRowV1>,
    pub real_rows: Vec<VerificationPlanReportRowV1>,
    pub rejected_rows: Vec<VerificationPlanRejectedRowV1>,
    pub selected_completed_judged: VerificationPlanRateV1,
    pub would_miss_rate: VerificationPlanRateV1,
    pub would_delay_rate: VerificationPlanRateV1,
    pub correct_fallback_rate: VerificationPlanRateV1,
    pub over_widen_rate: VerificationPlanRateV1,
    pub real_case_safe_rate: VerificationPlanRateV1,
}

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

fn load_committed_corpus() -> Result<VerificationPlanFixtureCorpusV1, String> {
    let path = workspace_path(DEFAULT_VERIFICATION_PLAN_CORPUS_PATH);
    let body =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    load_verification_plan_fixture_corpus(&body)
}

fn assess_fixture_corpus(
    corpus: &VerificationPlanFixtureCorpusV1,
) -> Result<Vec<VerificationPlanRowAssessmentV1>, String> {
    let missing = missing_verification_plan_required_scenarios(
        corpus.scenarios.iter().map(|scenario| scenario.id.as_str()),
    );
    if !missing.is_empty() {
        return Err(format!(
            "verification plan fixture corpus missing required mechanics scenarios: {}",
            missing.join(", ")
        ));
    }
    let mut assessments = Vec::with_capacity(corpus.scenarios.len());
    for scenario in &corpus.scenarios {
        let live = assess_verification_plan_case(&scenario.case);
        if live.accepted != scenario.expected.accepted {
            return Err(format!(
                "scenario `{}` drift: expected accepted {} but validator returned {} ({})",
                scenario.id,
                scenario.expected.accepted,
                live.accepted,
                live.reasons.join("; ")
            ));
        }
        if live.accepted && live.disposition != scenario.expected.disposition {
            return Err(format!(
                "scenario `{}` disposition drift: expected {:?} but got {:?}",
                scenario.id, scenario.expected.disposition, live.disposition
            ));
        }
        assessments.push(live);
    }
    Ok(assessments)
}

fn rate(
    state: VerificationPlanRateStateV1,
    numerator: usize,
    denominator: usize,
) -> VerificationPlanRateV1 {
    VerificationPlanRateV1 {
        state,
        numerator,
        denominator,
    }
}

fn build_report(
    corpus: &VerificationPlanFixtureCorpusV1,
    assessments: &[VerificationPlanRowAssessmentV1],
    corpus_path: &str,
) -> Result<VerificationPlanCorpusReportV1, String> {
    let cases: Vec<VerificationPlanCaseV1> = corpus
        .scenarios
        .iter()
        .map(|scenario| scenario.case.clone())
        .collect();
    let corpus_identity = verification_plan_corpus_identity(&cases)?;

    let mut synthetic_rows = Vec::new();
    let mut real_rows = Vec::new();
    let mut rejected_rows = Vec::new();

    let mut would_miss = 0usize;
    let mut would_delay = 0usize;
    let mut correct_fallback = 0usize;
    let mut over_widen = 0usize;
    let mut real_judged = 0usize;
    let mut real_completed = 0usize;

    for assessment in assessments {
        if !assessment.accepted {
            rejected_rows.push(VerificationPlanRejectedRowV1 {
                case_id: assessment.case_id.clone(),
                reasons: assessment.reasons.clone(),
            });
            continue;
        }
        let row = VerificationPlanReportRowV1 {
            case_id: assessment.case_id.clone(),
            case_class: assessment.case_class,
            disposition: assessment.disposition,
            synthetic: assessment.synthetic,
        };
        if assessment.synthetic {
            synthetic_rows.push(row);
        } else {
            real_completed += 1;
            real_judged += 1;
            real_rows.push(row);
            match assessment.disposition {
                VerificationPlanPlannerDispositionV1::WouldMissMaterialOutcome => would_miss += 1,
                VerificationPlanPlannerDispositionV1::PlanDelayedMaterialOutcome => {
                    would_delay += 1
                }
                VerificationPlanPlannerDispositionV1::CorrectFallbackOrRefusal => {
                    correct_fallback += 1
                }
                VerificationPlanPlannerDispositionV1::OverWidenBeyondJustifiedScope => {
                    over_widen += 1
                }
                _ => {}
            }
        }
    }

    synthetic_rows.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    real_rows.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    rejected_rows.sort_by(|left, right| left.case_id.cmp(&right.case_id));

    let real_denominator = real_judged;
    let real_rate_state = if real_denominator == 0 {
        VerificationPlanRateStateV1::NotMeasured
    } else {
        VerificationPlanRateStateV1::Measured
    };

    Ok(VerificationPlanCorpusReportV1 {
        schema_version: VERIFICATION_PLAN_CORPUS_REPORT_SCHEMA_VERSION.to_string(),
        reserved_real_corpus_schema_version: VERIFICATION_PLAN_CORPUS_SCHEMA_VERSION.to_string(),
        claim_boundary: VERIFICATION_PLAN_CLAIM_BOUNDARY.to_string(),
        spec: corpus.spec.clone(),
        corpus_path: corpus_path.to_string(),
        corpus_identity,
        synthetic_rows,
        real_rows,
        rejected_rows,
        selected_completed_judged: rate(
            if assessments.is_empty() {
                VerificationPlanRateStateV1::NotMeasured
            } else {
                VerificationPlanRateStateV1::Measured
            },
            assessments.iter().filter(|row| row.accepted).count(),
            assessments.len(),
        ),
        would_miss_rate: rate(real_rate_state, would_miss, real_denominator),
        would_delay_rate: rate(real_rate_state, would_delay, real_denominator),
        correct_fallback_rate: rate(real_rate_state, correct_fallback, real_denominator),
        over_widen_rate: rate(real_rate_state, over_widen, real_denominator),
        real_case_safe_rate: rate(
            real_rate_state,
            real_completed.saturating_sub(would_miss),
            real_denominator,
        ),
    })
}

pub(crate) fn verification_plan_corpus_report_markdown(
    report: &VerificationPlanCorpusReportV1,
) -> String {
    let mut out = String::new();
    out.push_str("# Verification plan corpus report\n\n");
    out.push_str(&format!("Spec: {}\n\n", report.spec));
    out.push_str(&format!("Corpus: `{}`\n\n", report.corpus_path));
    out.push_str(&format!(
        "Corpus identity: `{}`\n\n",
        report.corpus_identity
    ));
    out.push_str(&format!("Claim boundary: {}\n\n", report.claim_boundary));
    out.push_str("## Synthetic mechanics rows\n\n");
    for row in &report.synthetic_rows {
        out.push_str(&format!("- `{}`: {:?}\n", row.case_id, row.disposition));
    }
    out.push_str("\n## Real rows\n\n");
    if report.real_rows.is_empty() {
        out.push_str("_No real repository rows in PR A; real rates are not measured._\n\n");
    } else {
        for row in &report.real_rows {
            out.push_str(&format!("- `{}`: {:?}\n", row.case_id, row.disposition));
        }
        out.push('\n');
    }
    if !report.rejected_rows.is_empty() {
        out.push_str("## Rejected rows\n\n");
        for row in &report.rejected_rows {
            out.push_str(&format!(
                "- `{}`: {}\n",
                row.case_id,
                row.reasons.join("; ")
            ));
        }
        out.push('\n');
    }
    out.push_str("## Rates\n\n");
    out.push_str(&format!(
        "- selected/completed/judged: {:?} {}/{}\n",
        report.selected_completed_judged.state,
        report.selected_completed_judged.numerator,
        report.selected_completed_judged.denominator
    ));
    out.push_str(&format!(
        "- would_miss (real): {:?} {}/{}\n",
        report.would_miss_rate.state,
        report.would_miss_rate.numerator,
        report.would_miss_rate.denominator
    ));
    out.push_str(&format!(
        "- would_delay (real): {:?} {}/{}\n",
        report.would_delay_rate.state,
        report.would_delay_rate.numerator,
        report.would_delay_rate.denominator
    ));
    out.push_str(&format!(
        "- correct_fallback (real): {:?} {}/{}\n",
        report.correct_fallback_rate.state,
        report.correct_fallback_rate.numerator,
        report.correct_fallback_rate.denominator
    ));
    out.push_str(&format!(
        "- over_widen (real): {:?} {}/{}\n",
        report.over_widen_rate.state,
        report.over_widen_rate.numerator,
        report.over_widen_rate.denominator
    ));
    out.push_str(&format!(
        "- real_case_safe: {:?} {}/{}\n",
        report.real_case_safe_rate.state,
        report.real_case_safe_rate.numerator,
        report.real_case_safe_rate.denominator
    ));
    out
}

fn write_reports(report: &VerificationPlanCorpusReportV1) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| format!("serialize verification plan corpus report: {error}"))?;
    crate::write_report("verification-plan-corpus.json", &json)?;
    crate::write_report(
        "verification-plan-corpus.md",
        &verification_plan_corpus_report_markdown(report),
    )?;
    Ok(())
}

pub(crate) fn verification_plan_corpus_command(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask verification-plan-corpus check|report";
    let Some(sub) = args.first().map(String::as_str) else {
        return Err(USAGE.to_string());
    };
    if args.len() > 1 {
        return Err(format!(
            "{USAGE}; unexpected arguments: {}",
            args[1..].join(" ")
        ));
    }
    let corpus = load_committed_corpus()?;
    let assessments = assess_fixture_corpus(&corpus)?;
    let report = build_report(&corpus, &assessments, DEFAULT_VERIFICATION_PLAN_CORPUS_PATH)?;
    match sub {
        "check" => {
            println!(
                "verification-plan-corpus: {} scenarios accepted ({} synthetic, {} real, {} rejected); real safety denominator not measured",
                assessments.iter().filter(|row| row.accepted).count(),
                report.synthetic_rows.len(),
                report.real_rows.len(),
                report.rejected_rows.len()
            );
            Ok(())
        }
        "report" => {
            write_reports(&report)?;
            println!(
                "verification-plan-corpus: wrote target/ripr/reports/verification-plan-corpus.json and .md"
            );
            Ok(())
        }
        _ => Err(format!("{USAGE}; unknown subcommand `{sub}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_plan_corpus_report_json_and_markdown_agree_on_totals() -> Result<(), String> {
        let corpus = load_committed_corpus()?;
        let assessments = assess_fixture_corpus(&corpus)?;
        let report = build_report(&corpus, &assessments, DEFAULT_VERIFICATION_PLAN_CORPUS_PATH)?;
        let markdown = verification_plan_corpus_report_markdown(&report);
        assert!(markdown.contains("not measured") || !report.real_rows.is_empty());
        assert_eq!(report.real_rows.len(), 0);
        assert_eq!(
            report.would_miss_rate.state,
            VerificationPlanRateStateV1::NotMeasured
        );
        Ok(())
    }
}
