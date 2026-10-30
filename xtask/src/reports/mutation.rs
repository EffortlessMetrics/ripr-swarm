//! Mutation calibration command (`mutation-calibration`): joins static
//! repo-exposure seam evidence to imported cargo-mutants runtime outcome data
//! and renders the advisory calibration report, plus its exclusive argument
//! parsing, runtime JSON import, seam join, and JSON/markdown rendering
//! helpers.
//!
//! Cargo-mutants JSON import is owned by
//! `ripr::output::parse_cargo_mutants_outcomes_json` so this command cannot
//! drift from `ripr calibrate cargo-mutants` (#5374). Location join after an
//! explicit `seam_id` is owned by `ripr::output::join_cargo_mutants_location`
//! so file/line-only pairing cannot silently disagree with the product
//! importer (#6708).

use crate::run::run_output_owned;
use crate::{
    StaticSeamRecord, markdown_cell, normalize_path, parse_repo_exposure_static_seams,
    read_json_value, read_text_lossy, repo_seam_inventory_command_args_for_root, write_report,
};
use ripr::output::{
    CargoMutantsLocationJoin, CargoMutantsOutcomeRecord, CargoMutantsSeamLocation,
    join_cargo_mutants_location, parse_cargo_mutants_outcomes_json,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MutationCalibrationArgs {
    pub(crate) root: String,
    pub(crate) mutants_json: PathBuf,
    pub(crate) repo_exposure_json: Option<PathBuf>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MutationOutcomeRecord {
    pub(crate) mutant_id: Option<String>,
    pub(crate) seam_id: Option<String>,
    pub(crate) file: Option<String>,
    pub(crate) line: Option<usize>,
    pub(crate) mutation_operator: String,
    pub(crate) runtime_outcome: String,
    pub(crate) duration: Option<String>,
    pub(crate) test_command: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MutationCalibrationReport {
    pub(crate) static_seams_total: usize,
    pub(crate) mutants_total: usize,
    pub(crate) agreement: MutationCalibrationAgreement,
    pub(crate) precision_notes: Vec<String>,
    pub(crate) missed_runtime_signals: Vec<MutationCalibrationRuntimeSignal>,
    pub(crate) missed_runtime_signals_total: usize,
    pub(crate) static_only_findings: Vec<MutationCalibrationStaticOnlyFinding>,
    pub(crate) static_only_findings_total: usize,
    pub(crate) matched: Vec<MutationCalibrationMatch>,
    pub(crate) ambiguous_file_line: Vec<AmbiguousMutationCalibrationMatch>,
    pub(crate) ambiguous_span_overlap: Vec<AmbiguousMutationCalibrationMatch>,
    pub(crate) unmatched_mutants: Vec<MutationOutcomeRecord>,
    pub(crate) static_without_runtime: Vec<StaticSeamRecord>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MutationCalibrationAgreement {
    pub(crate) static_gap_and_runtime_signal: usize,
    pub(crate) static_gap_without_runtime_signal: usize,
    pub(crate) runtime_signal_without_static_gap: usize,
    pub(crate) static_clean_and_runtime_clean: usize,
    pub(crate) runtime_inconclusive: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MutationCalibrationRuntimeSignal {
    pub(crate) runtime: MutationOutcomeRecord,
    pub(crate) static_seam: Option<StaticSeamRecord>,
    pub(crate) confidence_label: &'static str,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MutationCalibrationStaticOnlyFinding {
    pub(crate) seam: StaticSeamRecord,
    pub(crate) confidence_label: &'static str,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MutationCalibrationMatch {
    pub(crate) join_method: &'static str,
    pub(crate) seam: StaticSeamRecord,
    pub(crate) mutation: MutationOutcomeRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AmbiguousMutationCalibrationMatch {
    pub(crate) mutation: MutationOutcomeRecord,
    pub(crate) candidates: Vec<StaticSeamRecord>,
}

pub(crate) const MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT: usize = 50;
pub(crate) const MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT: usize = 50;
pub(crate) fn mutation_calibration_impl(args: &[String]) -> Result<(), String> {
    let parsed = parse_mutation_calibration_args(args)?;
    let repo_exposure_json = match parsed.repo_exposure_json.as_ref() {
        Some(path) => read_text_lossy(path)?,
        None => {
            let json_args =
                repo_seam_inventory_command_args_for_root("repo-exposure-json", &parsed.root);
            let json_output = run_output_owned("cargo", &json_args)?;
            write_report("repo-exposure.json", &json_output)?;
            json_output
        }
    };
    let mutants_json = read_mutation_input_json(&parsed.mutants_json)?;
    let static_seams = parse_repo_exposure_static_seams(&repo_exposure_json)?;
    let runtime_mutants = parse_cargo_mutants_outcomes_json(&mutants_json)?;
    let seam_locations = seam_locations_from_repo_exposure(&repo_exposure_json, &static_seams)?;
    let report = build_mutation_calibration_report_with_locations(
        static_seams,
        runtime_mutants,
        &seam_locations,
    );
    write_report(
        "mutation-calibration.json",
        &mutation_calibration_report_json(&report)?,
    )?;
    write_report(
        "mutation-calibration.md",
        &mutation_calibration_report_markdown(&report),
    )
}

pub(crate) fn parse_mutation_calibration_args(
    args: &[String],
) -> Result<MutationCalibrationArgs, String> {
    let mut root: Option<String> = None;
    let mut mutants_json: Option<PathBuf> = None;
    let mut repo_exposure_json: Option<PathBuf> = None;
    let mut index = 0;

    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--mutants-json" | "--cargo-mutants-json" | "--input" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err(format!(
                        "missing value for `{arg}`\n{}",
                        mutation_calibration_usage()
                    ));
                };
                mutants_json = Some(PathBuf::from(path));
            }
            "--repo-exposure-json" | "--static-json" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err(format!(
                        "missing value for `{arg}`\n{}",
                        mutation_calibration_usage()
                    ));
                };
                repo_exposure_json = Some(PathBuf::from(path));
            }
            "--help" | "-h" => return Err(mutation_calibration_usage()),
            flag if flag.starts_with('-') => {
                return Err(format!(
                    "unknown mutation-calibration option `{flag}`\n{}",
                    mutation_calibration_usage()
                ));
            }
            positional => {
                if root.is_some() {
                    return Err(format!(
                        "unexpected extra positional argument `{positional}`\n{}",
                        mutation_calibration_usage()
                    ));
                }
                root = Some(positional.to_string());
            }
        }
        index += 1;
    }

    let Some(mutants_json) = mutants_json else {
        return Err(format!(
            "mutation-calibration requires `--mutants-json <path>`\n{}",
            mutation_calibration_usage()
        ));
    };

    Ok(MutationCalibrationArgs {
        root: root.unwrap_or_else(|| ".".to_string()),
        mutants_json,
        repo_exposure_json,
    })
}

fn mutation_calibration_usage() -> String {
    "usage: cargo xtask mutation-calibration [root] --mutants-json <path> [--repo-exposure-json <path>]"
        .to_string()
}
pub(crate) fn read_mutation_input_json(path: &Path) -> Result<String, String> {
    if path.is_dir() {
        let outcomes_path = path.join("outcomes.json");
        let mutants_path = path.join("mutants.json");
        let outcomes_exists = outcomes_path.exists();
        let mutants_exists = mutants_path.exists();

        if outcomes_exists && mutants_exists {
            let outcomes = read_json_value(&outcomes_path)?;
            let mutants = read_json_value(&mutants_path)?;
            return serde_json::to_string(&Value::Array(vec![outcomes, mutants]))
                .map_err(|err| format!("failed to combine cargo-mutants directory JSON: {err}"));
        }

        if outcomes_exists {
            return read_text_lossy(&outcomes_path);
        }
        if mutants_exists {
            return read_text_lossy(&mutants_path);
        }
        return Err(format!(
            "{} is a directory but contains neither outcomes.json nor mutants.json",
            normalize_path(path)
        ));
    }
    read_text_lossy(path)
}

#[cfg(test)]
pub(crate) fn parse_mutation_outcomes_json(
    json: &str,
) -> Result<Vec<MutationOutcomeRecord>, String> {
    Ok(parse_cargo_mutants_outcomes_json(json)?
        .into_iter()
        .map(imported_to_xtask)
        .collect())
}

fn imported_to_xtask(record: CargoMutantsOutcomeRecord) -> MutationOutcomeRecord {
    MutationOutcomeRecord {
        mutant_id: record.mutant_id,
        seam_id: record.seam_id,
        file: record.file,
        line: record.line,
        mutation_operator: record.mutation_operator,
        runtime_outcome: record.runtime_outcome,
        duration: record.duration,
        test_command: record.test_command,
    }
}

#[cfg(test)]
fn xtask_to_imported(record: MutationOutcomeRecord) -> CargoMutantsOutcomeRecord {
    CargoMutantsOutcomeRecord {
        mutant_id: record.mutant_id,
        seam_id: record.seam_id,
        file: record.file,
        line: record.line,
        span: None,
        span_conflict: false,
        mutation_operator: record.mutation_operator,
        runtime_outcome: record.runtime_outcome,
        duration: record.duration,
        test_command: record.test_command,
    }
}

#[cfg(test)]
fn line_only_seam_locations(seams: &[StaticSeamRecord]) -> Vec<CargoMutantsSeamLocation> {
    seams
        .iter()
        .map(|seam| CargoMutantsSeamLocation {
            file: seam.file.clone(),
            line: seam.line,
            column: None,
            end_line: None,
            end_column: None,
        })
        .collect()
}

fn json_scalar_as_usize(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

fn seam_locations_from_repo_exposure(
    json: &str,
    seams: &[StaticSeamRecord],
) -> Result<Vec<CargoMutantsSeamLocation>, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|err| format!("failed to parse repo exposure JSON: {err}"))?;
    let rows = value
        .get("seams")
        .and_then(Value::as_array)
        .ok_or_else(|| "repo exposure JSON is missing `seams` array".to_string())?;
    struct SpanCoords {
        column: Option<usize>,
        end_line: Option<usize>,
        end_column: Option<usize>,
    }
    let mut by_id = BTreeMap::new();
    for seam in rows {
        let Some(id) = seam.get("seam_id").and_then(Value::as_str) else {
            continue;
        };
        by_id.insert(
            id,
            SpanCoords {
                column: seam.get("column").and_then(json_scalar_as_usize),
                end_line: seam.get("end_line").and_then(json_scalar_as_usize),
                end_column: seam.get("end_column").and_then(json_scalar_as_usize),
            },
        );
    }
    Ok(seams
        .iter()
        .map(|seam| {
            let extra = by_id.get(seam.seam_id.as_str());
            CargoMutantsSeamLocation {
                file: seam.file.clone(),
                line: seam.line,
                column: extra.and_then(|coords| coords.column),
                end_line: extra.and_then(|coords| coords.end_line),
                end_column: extra.and_then(|coords| coords.end_column),
            }
        })
        .collect())
}

#[cfg(test)]
pub(crate) fn build_mutation_calibration_report(
    static_seams: Vec<StaticSeamRecord>,
    runtime_mutants: Vec<MutationOutcomeRecord>,
) -> MutationCalibrationReport {
    let locations = line_only_seam_locations(&static_seams);
    build_mutation_calibration_report_with_locations(
        static_seams,
        runtime_mutants.into_iter().map(xtask_to_imported).collect(),
        &locations,
    )
}

fn build_mutation_calibration_report_with_locations(
    static_seams: Vec<StaticSeamRecord>,
    runtime_mutants: Vec<CargoMutantsOutcomeRecord>,
    seam_locations: &[CargoMutantsSeamLocation],
) -> MutationCalibrationReport {
    let mut static_by_id: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, seam) in static_seams.iter().enumerate() {
        static_by_id.insert(seam.seam_id.clone(), idx);
    }

    let mut matched_static_ids = BTreeSet::new();
    let mut ambiguous_static_ids = BTreeSet::new();
    let mut matched = Vec::new();
    let mut ambiguous_file_line = Vec::new();
    let mut ambiguous_span_overlap = Vec::new();
    let mut unmatched_mutants = Vec::new();

    for mutation in runtime_mutants {
        let by_seam_id = mutation
            .seam_id
            .as_ref()
            .and_then(|seam_id| static_by_id.get(seam_id).copied())
            .map(|idx| CargoMutantsLocationJoin::Matched("seam_id", idx));
        let seam_match =
            by_seam_id.unwrap_or_else(|| join_cargo_mutants_location(&mutation, seam_locations));
        let mutation = imported_to_xtask(mutation);

        let (bucket, candidates) = match seam_match {
            CargoMutantsLocationJoin::Matched(join_method, idx) => {
                let seam = static_seams[idx].clone();
                matched_static_ids.insert(seam.seam_id.clone());
                matched.push(MutationCalibrationMatch {
                    join_method,
                    seam,
                    mutation,
                });
                continue;
            }
            CargoMutantsLocationJoin::Unmatched(_) => {
                unmatched_mutants.push(mutation);
                continue;
            }
            CargoMutantsLocationJoin::AmbiguousFileLine(candidates) => {
                (&mut ambiguous_file_line, candidates)
            }
            CargoMutantsLocationJoin::AmbiguousSpanOverlap(candidates) => {
                (&mut ambiguous_span_overlap, candidates)
            }
        };
        let mut candidates = candidates;
        candidates.sort_by(|&left, &right| {
            candidate_order_key(&static_seams[left], &seam_locations[left]).cmp(
                &candidate_order_key(&static_seams[right], &seam_locations[right]),
            )
        });
        let candidates = candidates
            .iter()
            .map(|idx| {
                let seam = static_seams[*idx].clone();
                ambiguous_static_ids.insert(seam.seam_id.clone());
                seam
            })
            .collect::<Vec<_>>();
        bucket.push(AmbiguousMutationCalibrationMatch {
            mutation,
            candidates,
        });
    }

    let static_without_runtime = static_seams
        .iter()
        .filter(|seam| {
            !matched_static_ids.contains(&seam.seam_id)
                && !ambiguous_static_ids.contains(&seam.seam_id)
        })
        .cloned()
        .collect::<Vec<_>>();

    let (agreement, precision_notes, mut missed_runtime_signals, mut static_only_findings) =
        mutation_calibration_agreement(
            &static_seams,
            &matched,
            ambiguous_file_line.iter().chain(&ambiguous_span_overlap),
            &unmatched_mutants,
        );
    let missed_runtime_signals_total = missed_runtime_signals.len();
    let static_only_findings_total = static_only_findings.len();
    missed_runtime_signals.truncate(MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT);
    static_only_findings.truncate(MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT);

    MutationCalibrationReport {
        static_seams_total: static_seams.len(),
        mutants_total: matched.len()
            + ambiguous_file_line.len()
            + ambiguous_span_overlap.len()
            + unmatched_mutants.len(),
        agreement,
        precision_notes,
        missed_runtime_signals,
        missed_runtime_signals_total,
        static_only_findings,
        static_only_findings_total,
        matched,
        ambiguous_file_line,
        ambiguous_span_overlap,
        unmatched_mutants,
        static_without_runtime,
    }
}

fn candidate_order_key<'a>(
    seam: &'a StaticSeamRecord,
    location: &CargoMutantsSeamLocation,
) -> (usize, usize, usize, usize, &'a str) {
    (
        seam.line,
        location.column.unwrap_or(0),
        location.end_line.unwrap_or(0),
        location.end_column.unwrap_or(0),
        seam.seam_id.as_str(),
    )
}

fn mutation_calibration_agreement<'a>(
    static_seams: &[StaticSeamRecord],
    matched: &[MutationCalibrationMatch],
    ambiguous: impl Iterator<Item = &'a AmbiguousMutationCalibrationMatch>,
    unmatched_mutants: &[MutationOutcomeRecord],
) -> (
    MutationCalibrationAgreement,
    Vec<String>,
    Vec<MutationCalibrationRuntimeSignal>,
    Vec<MutationCalibrationStaticOnlyFinding>,
) {
    let mut matches_by_seam: BTreeMap<&str, Vec<&MutationCalibrationMatch>> = BTreeMap::new();
    for record in matched {
        matches_by_seam
            .entry(record.seam.seam_id.as_str())
            .or_default()
            .push(record);
    }

    let mut agreement = MutationCalibrationAgreement::default();
    let mut missed_runtime_signals = Vec::new();
    let mut static_only_findings = Vec::new();

    for seam in static_seams {
        let records = matches_by_seam
            .get(seam.seam_id.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let has_runtime_gap = records
            .iter()
            .any(|record| runtime_gap_signal(&record.mutation.runtime_outcome));
        let has_runtime_clean = records
            .iter()
            .any(|record| runtime_clean_signal(&record.mutation.runtime_outcome));
        let has_runtime_inconclusive = records.iter().any(|record| {
            !runtime_gap_signal(&record.mutation.runtime_outcome)
                && !runtime_clean_signal(&record.mutation.runtime_outcome)
        });
        let has_static_gap = static_gap_signal(seam);

        match (has_static_gap, has_runtime_gap, has_runtime_clean) {
            (true, true, _) => agreement.static_gap_and_runtime_signal += 1,
            (true, false, _) => {
                agreement.static_gap_without_runtime_signal += 1;
                static_only_findings.push(MutationCalibrationStaticOnlyFinding {
                    seam: seam.clone(),
                    confidence_label: static_only_confidence_label(records),
                    reason: static_only_reason(records),
                });
            }
            (false, true, _) => {
                agreement.runtime_signal_without_static_gap += 1;
                for record in records
                    .iter()
                    .filter(|record| runtime_gap_signal(&record.mutation.runtime_outcome))
                {
                    missed_runtime_signals.push(MutationCalibrationRuntimeSignal {
                        runtime: record.mutation.clone(),
                        static_seam: Some(seam.clone()),
                        confidence_label: "contradicts_static_clean",
                        reason: "runtime gap signal joined to a static-clean seam".to_string(),
                    });
                }
            }
            (false, false, true) => agreement.static_clean_and_runtime_clean += 1,
            (false, false, false) => {}
        }

        if has_runtime_inconclusive {
            agreement.runtime_inconclusive += 1;
        }
    }

    for record in unmatched_mutants
        .iter()
        .filter(|record| runtime_gap_signal(&record.runtime_outcome))
    {
        agreement.runtime_signal_without_static_gap += 1;
        missed_runtime_signals.push(MutationCalibrationRuntimeSignal {
            runtime: record.clone(),
            static_seam: None,
            confidence_label: "runtime_only_signal",
            reason: "runtime gap signal did not join to a static seam".to_string(),
        });
    }

    for record in ambiguous {
        if runtime_gap_signal(&record.mutation.runtime_outcome) {
            agreement.runtime_inconclusive += 1;
        }
    }

    (
        agreement,
        mutation_calibration_precision_notes(),
        missed_runtime_signals,
        static_only_findings,
    )
}

fn push_agreement_sample_note(out: &mut String, shown: usize, total: usize) {
    if total > shown {
        out.push_str(&format!("Sample only; showing {shown} of {total}.\n\n"));
    }
}

fn mutation_calibration_precision_notes() -> Vec<String> {
    vec![
        "runtime gap signals are imported runtime labels such as missed, survived, not_caught, or uncaught".to_string(),
        "runtime clean signals are imported runtime labels such as caught or timeout".to_string(),
        "static_gap_without_runtime_signal includes static gap seams with no matched runtime gap signal in this import".to_string(),
        "ambiguous file/line runtime gap signals are counted as runtime_inconclusive until a seam_id or unambiguous location is available".to_string(),
    ]
}

fn static_only_reason(records: &[&MutationCalibrationMatch]) -> String {
    if records.is_empty() {
        "static gap seam has no matched runtime record in this import".to_string()
    } else if records
        .iter()
        .any(|record| runtime_clean_signal(&record.mutation.runtime_outcome))
    {
        "static gap seam matched runtime data without a runtime gap signal".to_string()
    } else {
        "static gap seam matched only runtime-inconclusive labels".to_string()
    }
}

fn static_only_confidence_label(records: &[&MutationCalibrationMatch]) -> &'static str {
    if records
        .iter()
        .any(|record| runtime_clean_signal(&record.mutation.runtime_outcome))
    {
        "contradicts_static_gap"
    } else {
        "no_runtime_data"
    }
}

fn static_gap_signal(seam: &StaticSeamRecord) -> bool {
    !matches!(
        seam.seam_grip_class.as_str(),
        "strongly_gripped" | "intentional" | "suppressed"
    )
}

fn runtime_gap_signal(outcome: &str) -> bool {
    matches!(
        normalize_runtime_label(outcome).as_str(),
        "missed" | "survived" | "survive" | "not_caught" | "uncaught"
    )
}

fn runtime_clean_signal(outcome: &str) -> bool {
    matches!(
        normalize_runtime_label(outcome).as_str(),
        "caught" | "timeout" | "timed_out" | "killed"
    )
}

fn confidence_label_for_match(record: &MutationCalibrationMatch) -> &'static str {
    let has_static_gap = static_gap_signal(&record.seam);
    if runtime_gap_signal(&record.mutation.runtime_outcome) {
        if has_static_gap {
            "supports_static_gap"
        } else {
            "contradicts_static_clean"
        }
    } else if runtime_clean_signal(&record.mutation.runtime_outcome) {
        if has_static_gap {
            "contradicts_static_gap"
        } else {
            "supports_static_clean"
        }
    } else {
        "no_runtime_data"
    }
}

pub(crate) fn mutation_calibration_report_json(
    report: &MutationCalibrationReport,
) -> Result<String, String> {
    let value = serde_json::json!({
        "schema_version": "0.1",
        "scope": "repo",
        "status": "advisory",
        "metrics": {
            "static_seams_total": report.static_seams_total,
            "mutants_total": report.mutants_total,
            "matched_total": report.matched.len(),
            "ambiguous_file_line_total": report.ambiguous_file_line.len(),
            "ambiguous_span_overlap_total": report.ambiguous_span_overlap.len(),
            "unmatched_mutants_total": report.unmatched_mutants.len(),
            "static_without_runtime_total": report.static_without_runtime.len(),
            "runtime_outcome_counts": runtime_outcome_counts(report),
            "join_method_counts": join_method_counts(report),
        },
        "agreement": mutation_calibration_agreement_json(&report.agreement),
        "precision_notes": &report.precision_notes,
        "missed_runtime_signals": report
            .missed_runtime_signals
            .iter()
            .map(mutation_calibration_runtime_signal_json)
            .collect::<Vec<_>>(),
        "static_only_findings": report
            .static_only_findings
            .iter()
            .map(mutation_calibration_static_only_json)
            .collect::<Vec<_>>(),
        "matches": report
            .matched
            .iter()
            .map(mutation_calibration_match_json)
            .collect::<Vec<_>>(),
        "ambiguous_file_line_matches": report
            .ambiguous_file_line
            .iter()
            .map(ambiguous_mutation_calibration_match_json)
            .collect::<Vec<_>>(),
        "ambiguous_span_overlap_matches": report
            .ambiguous_span_overlap
            .iter()
            .map(ambiguous_mutation_calibration_match_json)
            .collect::<Vec<_>>(),
        "unmatched_mutants": report
            .unmatched_mutants
            .iter()
            .map(mutation_outcome_json)
            .collect::<Vec<_>>(),
        "static_without_runtime_sample": report
            .static_without_runtime
            .iter()
            .take(MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT)
            .map(static_seam_json)
            .collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value)
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|err| format!("failed to render mutation calibration JSON: {err}"))
}

fn mutation_calibration_agreement_json(agreement: &MutationCalibrationAgreement) -> Value {
    serde_json::json!({
        "static_gap_and_runtime_signal": agreement.static_gap_and_runtime_signal,
        "static_gap_without_runtime_signal": agreement.static_gap_without_runtime_signal,
        "runtime_signal_without_static_gap": agreement.runtime_signal_without_static_gap,
        "static_clean_and_runtime_clean": agreement.static_clean_and_runtime_clean,
        "runtime_inconclusive": agreement.runtime_inconclusive,
    })
}

fn mutation_calibration_runtime_signal_json(record: &MutationCalibrationRuntimeSignal) -> Value {
    serde_json::json!({
        "runtime": mutation_outcome_json(&record.runtime),
        "static": record.static_seam.as_ref().map(static_seam_json),
        "confidence_label": record.confidence_label,
        "reason": record.reason.as_str(),
    })
}

fn mutation_calibration_static_only_json(record: &MutationCalibrationStaticOnlyFinding) -> Value {
    serde_json::json!({
        "static": static_seam_json(&record.seam),
        "confidence_label": record.confidence_label,
        "reason": record.reason.as_str(),
    })
}

fn mutation_calibration_match_json(record: &MutationCalibrationMatch) -> Value {
    serde_json::json!({
        "join_method": record.join_method,
        "static": static_seam_json(&record.seam),
        "runtime": mutation_outcome_json(&record.mutation),
        "confidence_label": confidence_label_for_match(record),
    })
}

fn ambiguous_mutation_calibration_match_json(record: &AmbiguousMutationCalibrationMatch) -> Value {
    serde_json::json!({
        "runtime": mutation_outcome_json(&record.mutation),
        "confidence_label": "ambiguous_runtime_join",
        "candidates": record
            .candidates
            .iter()
            .map(static_seam_json)
            .collect::<Vec<_>>(),
    })
}

fn static_seam_json(record: &StaticSeamRecord) -> Value {
    serde_json::json!({
        "seam_id": record.seam_id.as_str(),
        "seam_kind": record.seam_kind.as_str(),
        "file": record.file.as_str(),
        "line": record.line,
        "seam_grip_class": record.seam_grip_class.as_str(),
        "oracle_kind": record.oracle_kind.as_str(),
        "oracle_strength": record.oracle_strength.as_str(),
        "observed_values": &record.observed_values,
        "missing_discriminators": &record.missing_discriminators,
    })
}

fn mutation_outcome_json(record: &MutationOutcomeRecord) -> Value {
    serde_json::json!({
        "mutant_id": record.mutant_id.as_deref(),
        "seam_id": record.seam_id.as_deref(),
        "file": record.file.as_deref(),
        "line": record.line,
        "mutation_operator": record.mutation_operator.as_str(),
        "runtime_outcome": record.runtime_outcome.as_str(),
        "duration": record.duration.as_deref(),
        "test_command": record.test_command.as_deref(),
    })
}

fn runtime_outcome_counts(report: &MutationCalibrationReport) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for record in report
        .matched
        .iter()
        .map(|matched| &matched.mutation)
        .chain(
            report
                .ambiguous_file_line
                .iter()
                .map(|ambiguous| &ambiguous.mutation),
        )
        .chain(
            report
                .ambiguous_span_overlap
                .iter()
                .map(|ambiguous| &ambiguous.mutation),
        )
        .chain(report.unmatched_mutants.iter())
    {
        let key = normalize_runtime_label(&record.runtime_outcome);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn join_method_counts(report: &MutationCalibrationReport) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for record in &report.matched {
        *counts.entry(record.join_method).or_insert(0) += 1;
    }
    counts
}

fn normalize_runtime_label(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

pub(crate) fn mutation_calibration_report_markdown(report: &MutationCalibrationReport) -> String {
    let mut out = String::new();
    out.push_str("# ripr mutation calibration report\n\n");
    out.push_str("Status: advisory\n\n");
    out.push_str(
        "This report joins static seam evidence to supplied cargo-mutants runtime data. \
         Runtime outcome vocabulary in this report comes from that runtime data; static \
         ripr reports continue to use audit vocabulary only.\n\n",
    );
    out.push_str("## Summary\n\n");
    out.push_str("| Metric | Count |\n| --- | ---: |\n");
    out.push_str(&format!(
        "| static_seams_total | {} |\n",
        report.static_seams_total
    ));
    out.push_str(&format!("| mutants_total | {} |\n", report.mutants_total));
    out.push_str(&format!("| matched_total | {} |\n", report.matched.len()));
    out.push_str(&format!(
        "| ambiguous_file_line_total | {} |\n",
        report.ambiguous_file_line.len()
    ));
    out.push_str(&format!(
        "| ambiguous_span_overlap_total | {} |\n",
        report.ambiguous_span_overlap.len()
    ));
    out.push_str(&format!(
        "| unmatched_mutants_total | {} |\n",
        report.unmatched_mutants.len()
    ));
    out.push_str(&format!(
        "| static_without_runtime_total | {} |\n",
        report.static_without_runtime.len()
    ));

    out.push_str("\n## Static/runtime agreement\n\n");
    out.push_str("| Agreement bucket | Count |\n| --- | ---: |\n");
    out.push_str(&format!(
        "| static_gap_and_runtime_signal | {} |\n",
        report.agreement.static_gap_and_runtime_signal
    ));
    out.push_str(&format!(
        "| static_gap_without_runtime_signal | {} |\n",
        report.agreement.static_gap_without_runtime_signal
    ));
    out.push_str(&format!(
        "| runtime_signal_without_static_gap | {} |\n",
        report.agreement.runtime_signal_without_static_gap
    ));
    out.push_str(&format!(
        "| static_clean_and_runtime_clean | {} |\n",
        report.agreement.static_clean_and_runtime_clean
    ));
    out.push_str(&format!(
        "| runtime_inconclusive | {} |\n",
        report.agreement.runtime_inconclusive
    ));

    out.push_str("\nPrecision notes:\n\n");
    for note in &report.precision_notes {
        out.push_str(&format!("- {}\n", markdown_cell(note)));
    }

    out.push_str("\n### Runtime signals without static gaps\n\n");
    if report.missed_runtime_signals.is_empty() {
        out.push_str("No imported runtime gap signals lacked a matching static gap.\n");
    } else {
        push_agreement_sample_note(
            &mut out,
            report.missed_runtime_signals.len(),
            report.missed_runtime_signals_total,
        );
        out.push_str("| Runtime mutant | Location | Runtime outcome | Static class | Confidence label | Reason |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for record in &report.missed_runtime_signals {
            let mutant = record.runtime.mutant_id.as_deref().unwrap_or("unknown");
            let location = mutation_location_label(&record.runtime);
            let static_class = record
                .static_seam
                .as_ref()
                .map(|seam| seam.seam_grip_class.as_str())
                .unwrap_or("unmatched");
            out.push_str(&format!(
                "| `{}` | {} | {} | `{}` | `{}` | {} |\n",
                markdown_cell(mutant),
                markdown_cell(&location),
                markdown_cell(&record.runtime.runtime_outcome),
                markdown_cell(static_class),
                record.confidence_label,
                markdown_cell(&record.reason)
            ));
        }
    }

    out.push_str("\n### Static gaps without runtime signals\n\n");
    if report.static_only_findings.is_empty() {
        out.push_str("No static gap seams lacked a runtime gap signal in this import.\n");
    } else {
        push_agreement_sample_note(
            &mut out,
            report.static_only_findings.len(),
            report.static_only_findings_total,
        );
        out.push_str("| Seam | Class | Location | Confidence label | Reason |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for record in &report.static_only_findings {
            let location = format!("{}:{}", record.seam.file, record.seam.line);
            out.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` | {} |\n",
                markdown_cell(&record.seam.seam_id),
                markdown_cell(&record.seam.seam_grip_class),
                markdown_cell(&location),
                record.confidence_label,
                markdown_cell(&record.reason)
            ));
        }
    }

    out.push_str("\n## Runtime Outcome Counts\n\n");
    out.push_str("| Runtime outcome | Count |\n| --- | ---: |\n");
    let counts = runtime_outcome_counts(report);
    if counts.is_empty() {
        out.push_str("| none | 0 |\n");
    } else {
        for (outcome, count) in counts {
            out.push_str(&format!("| {} | {} |\n", markdown_cell(&outcome), count));
        }
    }

    out.push_str("\n## Matched Mutants\n\n");
    if report.matched.is_empty() {
        out.push_str("No runtime mutants matched static seams.\n");
    } else {
        out.push_str("| Seam | Class | Oracle | Mutation operator | Runtime outcome | Join | Confidence label |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for record in &report.matched {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}`/`{}` | {} | {} | `{}` | `{}` |\n",
                markdown_cell(&record.seam.seam_id),
                markdown_cell(&record.seam.seam_grip_class),
                markdown_cell(&record.seam.oracle_kind),
                markdown_cell(&record.seam.oracle_strength),
                markdown_cell(&record.mutation.mutation_operator),
                markdown_cell(&record.mutation.runtime_outcome),
                record.join_method,
                confidence_label_for_match(record)
            ));
        }
    }

    out.push_str("\n## Ambiguous File/Line Matches\n\n");
    if report.ambiguous_file_line.is_empty() {
        out.push_str(
            "No runtime mutants matched multiple static seams at the same file and line.\n",
        );
    } else {
        out.push_str("| Runtime mutant | Location | Runtime outcome | Confidence label | Candidate seams |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for record in &report.ambiguous_file_line {
            let mutant = record.mutation.mutant_id.as_deref().unwrap_or("unknown");
            let location = mutation_location_label(&record.mutation);
            let candidates = record
                .candidates
                .iter()
                .map(|candidate| format!("`{}`", candidate.seam_id))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "| `{}` | {} | {} | `{}` | {} |\n",
                markdown_cell(mutant),
                markdown_cell(&location),
                markdown_cell(&record.mutation.runtime_outcome),
                "ambiguous_runtime_join",
                markdown_cell(&candidates)
            ));
        }
    }

    out.push_str("\n## Ambiguous Span Overlaps\n\n");
    if report.ambiguous_span_overlap.is_empty() {
        out.push_str("No runtime mutants matched multiple innermost seam spans.\n");
    } else {
        out.push_str("| Runtime mutant | Location | Runtime outcome | Confidence label | Candidate seams |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for record in &report.ambiguous_span_overlap {
            let mutant = record.mutation.mutant_id.as_deref().unwrap_or("unknown");
            let location = mutation_location_label(&record.mutation);
            let candidates = record
                .candidates
                .iter()
                .map(|candidate| format!("`{}`", candidate.seam_id))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "| `{}` | {} | {} | `{}` | {} |\n",
                markdown_cell(mutant),
                markdown_cell(&location),
                markdown_cell(&record.mutation.runtime_outcome),
                "ambiguous_runtime_join",
                markdown_cell(&candidates)
            ));
        }
    }

    out.push_str("\n## Unmatched Runtime Mutants\n\n");
    if report.unmatched_mutants.is_empty() {
        out.push_str("All imported runtime mutants matched a static seam.\n");
    } else {
        out.push_str("| Location | Mutation operator | Runtime outcome | Test command |\n");
        out.push_str("| --- | --- | --- | --- |\n");
        for record in &report.unmatched_mutants {
            let location = mutation_location_label(record);
            let command = record.test_command.as_deref().unwrap_or("unknown");
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                markdown_cell(&location),
                markdown_cell(&record.mutation_operator),
                markdown_cell(&record.runtime_outcome),
                markdown_cell(command)
            ));
        }
    }

    out.push_str("\n## Static Seams Without Runtime Data\n\n");
    if report.static_without_runtime.is_empty() {
        out.push_str(
            "Every static seam matched at least one runtime mutant in the imported data.\n",
        );
    } else {
        out.push_str(
            "Sample only; see JSON `static_without_runtime_total` for the full count.\n\n",
        );
        out.push_str("| Seam | Kind | Class | Location | Confidence label |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for seam in report
            .static_without_runtime
            .iter()
            .take(MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT)
        {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` | {}:{} | `no_runtime_data` |\n",
                markdown_cell(&seam.seam_id),
                markdown_cell(&seam.seam_kind),
                markdown_cell(&seam.seam_grip_class),
                markdown_cell(&seam.file),
                seam.line
            ));
        }
    }

    out
}

fn mutation_location_label(record: &MutationOutcomeRecord) -> String {
    if let Some(seam_id) = record.seam_id.as_ref() {
        return format!("seam:{seam_id}");
    }
    match (&record.file, record.line) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.clone(),
        (None, Some(line)) => format!("line {line}"),
        (None, None) => "unknown".to_string(),
    }
}

pub(crate) use self::mutation_calibration_impl as mutation_calibration;

#[cfg(test)]
mod tests {
    use super::*;
    use ripr::output::CargoMutantsRuntimeSpan;
    use std::fs;
    use std::path::Path;

    fn seam(id: &str, file: &str, line: usize, grip: &str) -> StaticSeamRecord {
        StaticSeamRecord {
            seam_id: id.to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: file.to_string(),
            line,
            seam_grip_class: grip.to_string(),
            oracle_kind: "unknown".to_string(),
            oracle_strength: "unknown".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        }
    }

    fn imported(
        id: &str,
        file: &str,
        line: usize,
        span: Option<((usize, usize), (usize, usize))>,
        span_conflict: bool,
    ) -> CargoMutantsOutcomeRecord {
        CargoMutantsOutcomeRecord {
            mutant_id: Some(id.to_string()),
            seam_id: None,
            file: Some(file.to_string()),
            line: Some(line),
            span: span.map(|(start, end)| CargoMutantsRuntimeSpan { start, end }),
            span_conflict,
            mutation_operator: "BinaryOperator".to_string(),
            runtime_outcome: "missed".to_string(),
            duration: None,
            test_command: None,
        }
    }

    fn location(
        file: &str,
        line: usize,
        start_column: Option<usize>,
        end: Option<(usize, usize)>,
    ) -> CargoMutantsSeamLocation {
        CargoMutantsSeamLocation {
            file: file.to_string(),
            line,
            column: start_column,
            end_line: end.map(|(end_line, _)| end_line),
            end_column: end.map(|(_, end_column)| end_column),
        }
    }

    #[test]
    fn parse_keeps_runtime_span_and_conflict_on_the_shared_record() -> Result<(), String> {
        let json = r#"{
  "outcomes": [
    {
      "scenario": {"Mutant": {
        "name": "src/lib.rs:4:9: replace x with y",
        "file": "src/lib.rs",
        "span": {"start": {"line": 4, "column": 9}, "end": {"line": 4, "column": 10}},
        "replacement": "y",
        "genre": "BinaryOperator"
      }},
      "summary": "CaughtMutant"
    },
    {
      "scenario": {"Mutant": {
        "name": "src/lib.rs:4:9: replace x with y",
        "file": "src/lib.rs",
        "span": {"start": {"line": 4, "column": 12}, "end": {"line": 4, "column": 13}},
        "replacement": "y",
        "genre": "BinaryOperator"
      }},
      "summary": "CaughtMutant"
    }
  ]
}"#;
        let records = parse_cargo_mutants_outcomes_json(json)?;
        assert_eq!(records.len(), 1);
        assert!(records[0].span_conflict, "{records:?}");
        assert_eq!(records[0].span, None);
        assert_eq!(records[0].file.as_deref(), Some("src/lib.rs"));
        assert_eq!(records[0].line, Some(4));
        Ok(())
    }

    #[test]
    fn conflicted_runtime_location_stays_unmatched_instead_of_file_line() {
        let seams = vec![seam("call", "src/lib.rs", 4, "ungripped")];
        let locations = vec![location("src/lib.rs", 4, Some(1), Some((4, 20)))];
        let mutants = vec![imported("m-conflict", "src/lib.rs", 4, None, true)];

        let report = build_mutation_calibration_report_with_locations(seams, mutants, &locations);

        assert!(report.matched.is_empty(), "{report:?}");
        assert!(report.ambiguous_file_line.is_empty(), "{report:?}");
        assert_eq!(report.unmatched_mutants.len(), 1);
        assert_eq!(
            report.unmatched_mutants[0].mutant_id.as_deref(),
            Some("m-conflict")
        );
    }

    #[test]
    fn span_join_resolves_a_multi_seam_line_that_file_line_leaves_ambiguous() {
        let seams = vec![
            seam("return", "src/context.rs", 40, "strongly_gripped"),
            seam("predicate", "src/context.rs", 40, "ungripped"),
        ];
        let locations = vec![
            location("src/context.rs", 40, Some(5), Some((40, 40))),
            location("src/context.rs", 40, Some(12), Some((40, 25))),
        ];
        let mutants = vec![
            imported(
                "m-predicate",
                "src/context.rs",
                40,
                Some(((40, 18), (40, 20))),
                false,
            ),
            imported(
                "m-return",
                "src/context.rs",
                40,
                Some(((40, 30), (40, 31))),
                false,
            ),
        ];

        let report = build_mutation_calibration_report_with_locations(seams, mutants, &locations);

        assert!(
            report.ambiguous_file_line.is_empty(),
            "containment must not leave the atuin-shaped line as file/line ambiguous: {report:?}"
        );
        assert_eq!(
            report
                .matched
                .iter()
                .map(|record| (
                    record.mutation.mutant_id.as_deref(),
                    record.seam.seam_id.as_str(),
                    record.join_method,
                ))
                .collect::<Vec<_>>(),
            vec![
                (Some("m-predicate"), "predicate", "span_containment"),
                (Some("m-return"), "return", "span_containment"),
            ]
        );
    }

    #[test]
    fn equal_innermost_spans_stay_ambiguous_span_overlap() {
        let seams = vec![
            seam("twin-b", "src/lib.rs", 5, "ungripped"),
            seam("twin-a", "src/lib.rs", 5, "ungripped"),
        ];
        let locations = vec![
            location("src/lib.rs", 5, Some(5), Some((5, 20))),
            location("src/lib.rs", 5, Some(5), Some((5, 20))),
        ];
        let mutants = vec![imported(
            "m-tie",
            "src/lib.rs",
            5,
            Some(((5, 10), (5, 11))),
            false,
        )];

        let report = build_mutation_calibration_report_with_locations(seams, mutants, &locations);

        assert!(report.matched.is_empty(), "{report:?}");
        assert!(report.ambiguous_file_line.is_empty(), "{report:?}");
        assert_eq!(report.ambiguous_span_overlap.len(), 1);
        let ids = report.ambiguous_span_overlap[0]
            .candidates
            .iter()
            .map(|seam| seam.seam_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["twin-a", "twin-b"]);
    }

    #[test]
    fn line_only_builder_keeps_file_line_ambiguity_for_constructed_records() {
        let seams = vec![
            seam("seam-a", "src/pricing.rs", 42, "weakly_gripped"),
            seam("seam-b", "src/pricing.rs", 42, "ungripped"),
        ];
        let mutants = vec![MutationOutcomeRecord {
            mutant_id: Some("m1".to_string()),
            seam_id: None,
            file: Some("src/pricing.rs".to_string()),
            line: Some(42),
            mutation_operator: "replace >= with >".to_string(),
            runtime_outcome: "caught".to_string(),
            duration: None,
            test_command: None,
        }];

        let report = build_mutation_calibration_report(seams, mutants);

        assert!(report.matched.is_empty());
        assert_eq!(report.ambiguous_file_line.len(), 1);
        assert!(report.ambiguous_span_overlap.is_empty());
    }

    #[test]
    fn atuin_span_containment_fixture_agrees_with_the_product_join_counts() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let fixture = root.join("fixtures/boundary_gap/calibration/span-containment-atuin");
        let exposure = fs::read_to_string(fixture.join("repo-exposure.json"))
            .map_err(|err| format!("read repo-exposure.json: {err}"))?;
        let mutants = fs::read_to_string(fixture.join("runtime-mutants.json"))
            .map_err(|err| format!("read runtime-mutants.json: {err}"))?;

        let static_seams = parse_repo_exposure_static_seams(&exposure)?;
        let runtime_mutants = parse_cargo_mutants_outcomes_json(&mutants)?;
        let locations = seam_locations_from_repo_exposure(&exposure, &static_seams)?;
        assert_eq!(
            locations.len(),
            static_seams.len(),
            "span overlay must stay aligned with parsed static seams"
        );
        assert!(
            locations.iter().any(|seam| seam.column.is_some()),
            "atuin fixture seams must retain span coordinates"
        );
        assert!(
            runtime_mutants.iter().any(|record| record.span.is_some()),
            "atuin fixture mutants must retain runtime spans"
        );

        let report = build_mutation_calibration_report_with_locations(
            static_seams,
            runtime_mutants,
            &locations,
        );
        let counts = join_method_counts(&report);

        assert_eq!(report.ambiguous_file_line.len(), 0, "{report:?}");
        assert_eq!(counts.get("span_containment").copied().unwrap_or(0), 8);
        assert_eq!(counts.get("file_line").copied().unwrap_or(0), 5);
        assert_eq!(report.ambiguous_span_overlap.len(), 6);
        assert_eq!(report.unmatched_mutants.len(), 8);
        assert_eq!(report.mutants_total, 27);
        Ok(())
    }
}
