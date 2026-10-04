//! Render advisory static/runtime calibration reports.
//!
//! `ripr calibrate cargo-mutants` imports already-produced cargo-mutants JSON
//! and joins it to a `repo-exposure-json` snapshot. Runtime mutation
//! vocabulary is intentionally isolated to this calibration report; static
//! RIPR outputs keep their evidence vocabulary.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MUTATION_CALIBRATION_SCHEMA_VERSION: &str = "0.2";

const STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT: usize = 50;
const AGREEMENT_SAMPLE_LIMIT: usize = 50;

mod outcome_records;

use outcome_records::parse_mutation_outcomes_json;

#[derive(Clone, Debug, Eq, PartialEq)]
struct StaticSeamRecord {
    seam_id: String,
    seam_kind: String,
    file: String,
    line: usize,
    column: Option<usize>,
    end_line: Option<usize>,
    end_column: Option<usize>,
    seam_grip_class: String,
    oracle_kind: String,
    oracle_strength: String,
    observed_values: Vec<String>,
    missing_discriminators: Vec<String>,
}

/// A runtime mutant's complete source range as 1-based `(line, character
/// column)` positions with an exclusive `end`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuntimeSpan {
    start: (usize, usize),
    end: (usize, usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MutationOutcomeRecord {
    mutant_id: Option<String>,
    seam_id: Option<String>,
    file: Option<String>,
    line: Option<usize>,
    span: Option<RuntimeSpan>,
    /// Set when merged duplicates of this mutant carried different complete
    /// spans; the span is then dropped rather than choosing one.
    span_conflict: bool,
    mutation_operator: String,
    runtime_outcome: String,
    duration: Option<String>,
    test_command: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MutationCalibrationReport {
    static_seams_total: usize,
    mutants_total: usize,
    agreement: MutationCalibrationAgreement,
    precision_notes: Vec<String>,
    missed_runtime_signals: Vec<MutationCalibrationRuntimeSignal>,
    static_only_findings: Vec<MutationCalibrationStaticOnlyFinding>,
    matched: Vec<MutationCalibrationMatch>,
    ambiguous_file_line: Vec<AmbiguousMutationCalibrationMatch>,
    ambiguous_span_overlap: Vec<AmbiguousMutationCalibrationMatch>,
    unmatched_mutants: Vec<UnmatchedMutation>,
    static_without_runtime: Vec<StaticSeamRecord>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct MutationCalibrationAgreement {
    static_gap_and_runtime_signal: usize,
    static_gap_without_runtime_signal: usize,
    runtime_signal_without_static_gap: usize,
    static_clean_and_runtime_clean: usize,
    runtime_inconclusive: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MutationCalibrationRuntimeSignal {
    runtime: MutationOutcomeRecord,
    static_seam: Option<StaticSeamRecord>,
    confidence_label: &'static str,
    reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MutationCalibrationStaticOnlyFinding {
    seam: StaticSeamRecord,
    confidence_label: &'static str,
    reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MutationCalibrationMatch {
    join_method: &'static str,
    seam: StaticSeamRecord,
    mutation: MutationOutcomeRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AmbiguousMutationCalibrationMatch {
    mutation: MutationOutcomeRecord,
    candidates: Vec<StaticSeamRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnmatchedMutation {
    mutation: MutationOutcomeRecord,
    reason: &'static str,
}

pub(crate) fn mutation_calibration_report_from_json(
    repo_exposure_json: &str,
    mutants_json: &str,
) -> Result<MutationCalibrationReport, String> {
    let static_seams = parse_repo_exposure_static_seams(repo_exposure_json)?;
    let runtime_mutants = parse_mutation_outcomes_json(mutants_json)?;
    Ok(build_mutation_calibration_report(
        static_seams,
        runtime_mutants,
    ))
}

pub(crate) fn render_mutation_calibration_json(
    report: &MutationCalibrationReport,
) -> Result<String, String> {
    let value = serde_json::json!({
        "schema_version": MUTATION_CALIBRATION_SCHEMA_VERSION,
        "scope": "repo",
        "status": "advisory",
        "metrics": {
            "static_seams_total": report.static_seams_total,
            "mutants_total": report.mutants_total,
            "matched_total": report.matched.len(),
            "ambiguous_file_line_total": report.ambiguous_file_line.len(),
            "ambiguous_span_overlap_total": report.ambiguous_span_overlap.len(),
            "unmatched_mutants_total": report.unmatched_mutants.len(),
            "unmatched_reason_counts": unmatched_reason_counts(report),
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
            .map(unmatched_mutation_json)
            .collect::<Vec<_>>(),
        "static_without_runtime_sample": report
            .static_without_runtime
            .iter()
            .take(STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT)
            .map(static_seam_json)
            .collect::<Vec<_>>(),
    });
    super::json::render_pretty_with_newline(&value, "mutation calibration")
}

pub(crate) fn render_mutation_calibration_md(report: &MutationCalibrationReport) -> String {
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
        out.push_str(&format!("- {}\n", md_cell(note)));
    }

    out.push_str("\n### Runtime signals without static gaps\n\n");
    if report.missed_runtime_signals.is_empty() {
        out.push_str("No imported runtime gap signals lacked a matching static gap.\n");
    } else {
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
                md_cell(mutant),
                md_cell(&location),
                md_cell(&record.runtime.runtime_outcome),
                md_cell(static_class),
                record.confidence_label,
                md_cell(&record.reason)
            ));
        }
    }

    out.push_str("\n### Static gaps without runtime signals\n\n");
    if report.static_only_findings.is_empty() {
        out.push_str("No static gap seams lacked a runtime gap signal in this import.\n");
    } else {
        out.push_str("| Seam | Class | Location | Confidence label | Reason |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for record in &report.static_only_findings {
            let location = format!("{}:{}", record.seam.file, record.seam.line);
            out.push_str(&format!(
                "| `{}` | `{}` | {} | `{}` | {} |\n",
                md_cell(&record.seam.seam_id),
                md_cell(&record.seam.seam_grip_class),
                md_cell(&location),
                record.confidence_label,
                md_cell(&record.reason)
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
            out.push_str(&format!("| {} | {} |\n", md_cell(&outcome), count));
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
                md_cell(&record.seam.seam_id),
                md_cell(&record.seam.seam_grip_class),
                md_cell(&record.seam.oracle_kind),
                md_cell(&record.seam.oracle_strength),
                md_cell(&record.mutation.mutation_operator),
                md_cell(&record.mutation.runtime_outcome),
                record.join_method,
                confidence_label_for_match(record)
            ));
        }
    }

    out.push_str("\n## Ambiguous File/Line Matches\n\n");
    push_ambiguous_md(
        &mut out,
        &report.ambiguous_file_line,
        "No line-only runtime mutants matched multiple static seams.\n",
    );

    out.push_str("\n## Ambiguous Span Overlaps\n\n");
    push_ambiguous_md(
        &mut out,
        &report.ambiguous_span_overlap,
        "No runtime mutant span was contained by equal or crossing innermost seam spans.\n",
    );

    out.push_str("\n## Unmatched Runtime Mutants\n\n");
    if report.unmatched_mutants.is_empty() {
        out.push_str("All imported runtime mutants matched a static seam.\n");
    } else {
        out.push_str(
            "| Location | Mutation operator | Runtime outcome | Reason | Test command |\n",
        );
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for unmatched in &report.unmatched_mutants {
            let record = &unmatched.mutation;
            let location = mutation_location_label(record);
            let command = record.test_command.as_deref().unwrap_or("unknown");
            out.push_str(&format!(
                "| {} | {} | {} | `{}` | {} |\n",
                md_cell(&location),
                md_cell(&record.mutation_operator),
                md_cell(&record.runtime_outcome),
                unmatched.reason,
                md_cell(command)
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
            .take(STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT)
        {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` | {}:{} | `no_runtime_data` |\n",
                md_cell(&seam.seam_id),
                md_cell(&seam.seam_kind),
                md_cell(&seam.seam_grip_class),
                md_cell(&seam.file),
                seam.line
            ));
        }
    }

    out
}

fn parse_repo_exposure_static_seams(json: &str) -> Result<Vec<StaticSeamRecord>, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|err| format!("failed to parse repo exposure JSON: {err}"))?;
    let seams = value
        .get("seams")
        .and_then(Value::as_array)
        .ok_or_else(|| "repo exposure JSON is missing `seams` array".to_string())?;

    let mut records = Vec::new();
    for seam in seams {
        let seam_id = required_json_string(seam, "seam_id")?;
        let seam_kind = required_json_string(seam, "kind")?;
        let file = normalize_report_path(&required_json_string(seam, "file")?);
        let line = required_json_usize(seam, "line")?;
        let column = seam.get("column").and_then(json_scalar_as_usize);
        let end_line = seam.get("end_line").and_then(json_scalar_as_usize);
        let end_column = seam.get("end_column").and_then(json_scalar_as_usize);
        let seam_grip_class = required_json_string(seam, "grip_class")?;
        let (oracle_kind, oracle_strength) = strongest_related_oracle(seam);
        records.push(StaticSeamRecord {
            seam_id,
            seam_kind,
            file,
            line,
            column,
            end_line,
            end_column,
            seam_grip_class,
            oracle_kind,
            oracle_strength,
            observed_values: string_array_field(seam, "observed_values"),
            missing_discriminators: missing_discriminator_strings(seam),
        });
    }
    Ok(records)
}

fn build_mutation_calibration_report(
    static_seams: Vec<StaticSeamRecord>,
    runtime_mutants: Vec<MutationOutcomeRecord>,
) -> MutationCalibrationReport {
    let mut static_by_id: BTreeMap<String, usize> = BTreeMap::new();
    let mut static_by_line: BTreeMap<(String, usize), Vec<usize>> = BTreeMap::new();
    let mut spanned_by_file: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, seam) in static_seams.iter().enumerate() {
        static_by_id.insert(seam.seam_id.clone(), idx);
        static_by_line
            .entry((normalize_report_path(&seam.file), seam.line))
            .or_default()
            .push(idx);
        if seam_span(seam).is_some() {
            spanned_by_file
                .entry(normalize_report_path(&seam.file))
                .or_default()
                .push(idx);
        }
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
            .map(|idx| LocationJoin::Matched("seam_id", idx));
        let seam_match = by_seam_id.unwrap_or_else(|| {
            location_join(&static_seams, &static_by_line, &spanned_by_file, &mutation)
        });

        let (bucket, candidates) = match seam_match {
            LocationJoin::Matched(join_method, idx) => {
                let seam = static_seams[idx].clone();
                matched_static_ids.insert(seam.seam_id.clone());
                matched.push(MutationCalibrationMatch {
                    join_method,
                    seam,
                    mutation,
                });
                continue;
            }
            LocationJoin::Unmatched(reason) => {
                unmatched_mutants.push(UnmatchedMutation { mutation, reason });
                continue;
            }
            LocationJoin::AmbiguousFileLine(candidates) => (&mut ambiguous_file_line, candidates),
            LocationJoin::AmbiguousSpanOverlap(candidates) => {
                (&mut ambiguous_span_overlap, candidates)
            }
        };
        let mut candidates = candidates
            .iter()
            .map(|idx| {
                let seam = static_seams[*idx].clone();
                ambiguous_static_ids.insert(seam.seam_id.clone());
                seam
            })
            .collect::<Vec<_>>();
        // Candidate order follows source geometry and seam identity, never
        // snapshot order, so the evidence is stable across inputs.
        candidates
            .sort_by(|left, right| candidate_order_key(left).cmp(&candidate_order_key(right)));
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

    let (agreement, precision_notes, missed_runtime_signals, static_only_findings) =
        mutation_calibration_agreement(
            &static_seams,
            &matched,
            ambiguous_file_line.iter().chain(&ambiguous_span_overlap),
            &unmatched_mutants,
        );

    MutationCalibrationReport {
        static_seams_total: static_seams.len(),
        mutants_total: matched.len()
            + ambiguous_file_line.len()
            + ambiguous_span_overlap.len()
            + unmatched_mutants.len(),
        agreement,
        precision_notes,
        missed_runtime_signals,
        static_only_findings,
        matched,
        ambiguous_file_line,
        ambiguous_span_overlap,
        unmatched_mutants,
        static_without_runtime,
    }
}

fn candidate_order_key(seam: &StaticSeamRecord) -> (usize, usize, usize, usize, &str) {
    (
        seam.line,
        seam.column.unwrap_or(0),
        seam.end_line.unwrap_or(0),
        seam.end_column.unwrap_or(0),
        seam.seam_id.as_str(),
    )
}

enum LocationJoin {
    Matched(&'static str, usize),
    /// Several seams share the mutant's line and no span decides between
    /// them.
    AmbiguousFileLine(Vec<usize>),
    /// Several seam spans contain the mutant and none is strictly inside the
    /// others: equal or crossing ranges.
    AmbiguousSpanOverlap(Vec<usize>),
    Unmatched(&'static str),
}

/// Unmatched reason: the runtime record names no file.
const UNMATCHED_NO_LOCATION: &str = "no_location";
/// Unmatched reason: no static seam starts on the runtime record's line and
/// no seam span is available to compare against.
const UNMATCHED_NO_SEAM_ON_LINE: &str = "no_seam_on_line";
/// Unmatched reason: the mutant span and the file's seam spans are complete
/// and no seam span contains the mutant.
const UNMATCHED_NO_CONTAINING_SEAM: &str = "no_containing_seam";

/// Join a runtime mutant to a static seam by location.
///
/// Precedence after an explicit `seam_id`:
///
/// 1. Span containment. When the mutant carries a complete span, every seam
///    in the same file with a complete span is a candidate when its half-open
///    range contains the mutated range, whatever line the seam starts on.
///    The unique innermost candidate (strictly inside every other) joins as
///    `span_containment`. Equal or crossing innermost ranges are a true
///    overlap and stay ambiguous; source order, length and ID never break
///    the tie. Span-less seams do not compete with a containment match.
/// 2. Line fallback. With no containing span, seams without a span (match
///    arms, or snapshots older than `repo-exposure-json` 0.4) on the
///    mutant's start line join by `file_line`, unique or ambiguous. A mutant
///    without a complete span uses this fallback over every seam on its line.
/// 3. Otherwise the mutant is unmatched. When the file has seam spans that
///    could have contained it, the reason is `no_containing_seam`: a seam
///    that merely starts on the same line is never joined.
fn location_join(
    static_seams: &[StaticSeamRecord],
    static_by_line: &BTreeMap<(String, usize), Vec<usize>>,
    spanned_by_file: &BTreeMap<String, Vec<usize>>,
    mutation: &MutationOutcomeRecord,
) -> LocationJoin {
    let Some(file) = mutation.file.as_deref().map(normalize_report_path) else {
        return LocationJoin::Unmatched(UNMATCHED_NO_LOCATION);
    };
    let same_line = mutation
        .line
        .and_then(|line| static_by_line.get(&(file.clone(), line)))
        .map(Vec::as_slice)
        .unwrap_or_default();

    let Some(mutant_span) = mutation.span else {
        return file_line_join(same_line, UNMATCHED_NO_SEAM_ON_LINE);
    };

    let spanned = spanned_by_file
        .get(&file)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let containing: Vec<(usize, SeamRange)> = spanned
        .iter()
        .filter_map(|idx| {
            let range = seam_span(&static_seams[*idx])?;
            span_contains(range, mutant_span).then_some((*idx, range))
        })
        .collect();
    if containing.is_empty() {
        let line_only = same_line
            .iter()
            .copied()
            .filter(|idx| seam_span(&static_seams[*idx]).is_none())
            .collect::<Vec<_>>();
        let reason = if spanned.is_empty() {
            UNMATCHED_NO_SEAM_ON_LINE
        } else {
            UNMATCHED_NO_CONTAINING_SEAM
        };
        return file_line_join(&line_only, reason);
    }

    // Minimal elements under strict containment: seams that strictly enclose
    // no other containing seam. Equal ranges do not enclose each other
    // strictly, so both survive and stay ambiguous, as do crossing ranges.
    let innermost = containing
        .iter()
        .filter(|(_, range)| {
            !containing
                .iter()
                .any(|(_, inner)| strictly_encloses(*range, *inner))
        })
        .map(|(idx, _)| *idx)
        .collect::<Vec<_>>();
    match innermost.as_slice() {
        [idx] => LocationJoin::Matched("span_containment", *idx),
        tied => LocationJoin::AmbiguousSpanOverlap(tied.to_vec()),
    }
}

fn file_line_join(candidates: &[usize], unmatched_reason: &'static str) -> LocationJoin {
    match candidates {
        [] => LocationJoin::Unmatched(unmatched_reason),
        [idx] => LocationJoin::Matched("file_line", *idx),
        several => LocationJoin::AmbiguousFileLine(several.to_vec()),
    }
}

/// A `[start, end)` source range of 1-based `(line, character column)`
/// positions.
type SeamRange = ((usize, usize), (usize, usize));

/// A seam's range when the snapshot carries all three span coordinates and
/// they are ordered; otherwise the seam is line-only.
fn seam_span(seam: &StaticSeamRecord) -> Option<SeamRange> {
    let start = (seam.line, seam.column?);
    let end = (seam.end_line?, seam.end_column?);
    (start < end).then_some((start, end))
}

/// Half-open containment of the mutated range in a seam range.
fn span_contains(seam: SeamRange, mutant: RuntimeSpan) -> bool {
    let (seam_start, seam_end) = seam;
    seam_start <= mutant.start && mutant.start < seam_end && mutant.end <= seam_end
}

fn strictly_encloses(outer: SeamRange, inner: SeamRange) -> bool {
    outer != inner && outer.0 <= inner.0 && inner.1 <= outer.1
}

fn mutation_calibration_agreement<'a>(
    static_seams: &[StaticSeamRecord],
    matched: &[MutationCalibrationMatch],
    ambiguous: impl Iterator<Item = &'a AmbiguousMutationCalibrationMatch>,
    unmatched_mutants: &[UnmatchedMutation],
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
        .map(|unmatched| &unmatched.mutation)
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

    missed_runtime_signals.truncate(AGREEMENT_SAMPLE_LIMIT);
    static_only_findings.truncate(AGREEMENT_SAMPLE_LIMIT);

    (
        agreement,
        mutation_calibration_precision_notes(),
        missed_runtime_signals,
        static_only_findings,
    )
}

fn mutation_calibration_precision_notes() -> Vec<String> {
    vec![
        "runtime gap signals are imported runtime labels such as missed, survived, not_caught, or uncaught".to_string(),
        "runtime clean signals are imported runtime labels such as caught or timeout".to_string(),
        "static_gap_without_runtime_signal includes static gap seams with no matched runtime gap signal in this import".to_string(),
        "runtime records with a complete span join by span_containment to the unique innermost static seam whose span contains the mutated range; with no containing span they fall back to file and line over seams without a span, and records without a complete span join by file and line".to_string(),
        "ambiguous runtime gap signals (ambiguous_file_line: several line-only seams share the line; ambiguous_span_overlap: equal or crossing innermost seam spans) are counted as runtime_inconclusive until a seam_id or unambiguous location is available".to_string(),
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

fn push_ambiguous_md(
    out: &mut String,
    records: &[AmbiguousMutationCalibrationMatch],
    empty_line: &str,
) {
    if records.is_empty() {
        out.push_str(empty_line);
        return;
    }
    out.push_str(
        "| Runtime mutant | Location | Runtime outcome | Confidence label | Candidate seams |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for record in records {
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
            md_cell(mutant),
            md_cell(&location),
            md_cell(&record.mutation.runtime_outcome),
            "ambiguous_runtime_join",
            md_cell(&candidates)
        ));
    }
}

fn unmatched_mutation_json(record: &UnmatchedMutation) -> Value {
    let mut value = mutation_outcome_json(&record.mutation);
    if let Some(object) = value.as_object_mut() {
        object.insert("unmatched_reason".to_string(), record.reason.into());
    }
    value
}

fn unmatched_reason_counts(report: &MutationCalibrationReport) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for record in &report.unmatched_mutants {
        *counts.entry(record.reason).or_insert(0) += 1;
    }
    counts
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
    let mut value = serde_json::json!({
        "seam_id": record.seam_id.as_str(),
        "seam_kind": record.seam_kind.as_str(),
        "file": record.file.as_str(),
        "line": record.line,
        "seam_grip_class": record.seam_grip_class.as_str(),
        "oracle_kind": record.oracle_kind.as_str(),
        "oracle_strength": record.oracle_strength.as_str(),
        "observed_values": &record.observed_values,
        "missing_discriminators": &record.missing_discriminators,
    });
    // Span fields appear only when the seam has a complete range, so
    // line-only snapshots render exactly as before.
    if let (Some(((_, column), (end_line, end_column))), Some(object)) =
        (seam_span(record), value.as_object_mut())
    {
        object.insert("column".to_string(), column.into());
        object.insert("end_line".to_string(), end_line.into());
        object.insert("end_column".to_string(), end_column.into());
    }
    value
}

fn mutation_outcome_json(record: &MutationOutcomeRecord) -> Value {
    let mut value = serde_json::json!({
        "mutant_id": record.mutant_id.as_deref(),
        "seam_id": record.seam_id.as_deref(),
        "file": record.file.as_deref(),
        "line": record.line,
        "mutation_operator": record.mutation_operator.as_str(),
        "runtime_outcome": record.runtime_outcome.as_str(),
        "duration": record.duration.as_deref(),
        "test_command": record.test_command.as_deref(),
    });
    if let Some(object) = value.as_object_mut() {
        if let Some(span) = record.span {
            object.insert("column".to_string(), span.start.1.into());
            object.insert("end_line".to_string(), span.end.0.into());
            object.insert("end_column".to_string(), span.end.1.into());
        }
        if record.span_conflict {
            object.insert(
                "span_status".to_string(),
                "conflicting_runtime_spans".into(),
            );
        }
    }
    value
}

fn merge_mutation_outcome_records(
    records: Vec<MutationOutcomeRecord>,
) -> Vec<MutationOutcomeRecord> {
    let mut by_id: BTreeMap<String, MutationOutcomeRecord> = BTreeMap::new();
    let mut without_id = Vec::new();

    for record in records {
        match record.mutant_id.clone() {
            Some(id) => {
                if let Some(existing) = by_id.get_mut(&id) {
                    merge_mutation_outcome_record(existing, record);
                } else {
                    by_id.insert(id, record);
                }
            }
            None => without_id.push(record),
        }
    }

    by_id.into_values().chain(without_id).collect::<Vec<_>>()
}

fn merge_mutation_outcome_record(
    target: &mut MutationOutcomeRecord,
    source: MutationOutcomeRecord,
) {
    if target.seam_id.is_none() {
        target.seam_id = source.seam_id;
    }
    if target.file.is_none() {
        target.file = source.file;
    }
    if target.line.is_none() {
        target.line = source.line;
    }
    // A complete span fills an absent one. Two different complete spans for
    // one mutant cannot both be right, so the record drops to line-only
    // evidence and says why rather than keeping either.
    match (target.span, source.span) {
        (_, None) => {}
        (None, Some(span)) if !target.span_conflict && target.line == Some(span.start.0) => {
            target.span = Some(span);
        }
        (Some(existing), Some(span)) if existing != span => {
            target.span = None;
            target.span_conflict = true;
        }
        _ => {}
    }
    target.span_conflict |= source.span_conflict;
    if target.mutation_operator == "unknown" && source.mutation_operator != "unknown" {
        target.mutation_operator = source.mutation_operator;
    }
    if target.runtime_outcome == "unknown" && source.runtime_outcome != "unknown" {
        target.runtime_outcome = source.runtime_outcome;
    }
    if target.duration.is_none() {
        target.duration = source.duration;
    }
    if target.test_command.is_none() {
        target.test_command = source.test_command;
    }
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
                .chain(&report.ambiguous_span_overlap)
                .map(|ambiguous| &ambiguous.mutation),
        )
        .chain(
            report
                .unmatched_mutants
                .iter()
                .map(|unmatched| &unmatched.mutation),
        )
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

fn required_json_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(json_scalar_as_string)
        .ok_or_else(|| format!("repo exposure seam is missing string field `{key}`"))
}

fn required_json_usize(value: &Value, key: &str) -> Result<usize, String> {
    value
        .get(key)
        .and_then(json_scalar_as_usize)
        .ok_or_else(|| format!("repo exposure seam is missing numeric field `{key}`"))
}

fn strongest_related_oracle(seam: &Value) -> (String, String) {
    let mut best_kind = "unknown".to_string();
    let mut best_strength = "unknown".to_string();
    let mut best_rank = 0;

    if let Some(related) = seam.get("related_tests").and_then(Value::as_array) {
        for test in related {
            let strength = test
                .get("oracle_strength")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let rank = oracle_strength_rank(strength);
            if rank > best_rank {
                best_rank = rank;
                best_strength = strength.to_string();
                best_kind = test
                    .get("oracle_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
            }
        }
    }

    (best_kind, best_strength)
}

fn oracle_strength_rank(strength: &str) -> u8 {
    match strength {
        "strong" => 5,
        "medium" => 4,
        "weak" => 3,
        "smoke" => 2,
        "none" => 1,
        _ => 0,
    }
}

fn string_array_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(json_scalar_as_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn missing_discriminator_strings(seam: &Value) -> Vec<String> {
    seam.get("missing_discriminators")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    if let Some(value) = json_scalar_as_string(item) {
                        return Some(value);
                    }
                    let value = item.get("value").and_then(json_scalar_as_string)?;
                    match item.get("reason").and_then(json_scalar_as_string) {
                        Some(reason) if !reason.is_empty() => Some(format!("{value} ({reason})")),
                        _ => Some(value),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn json_scalar_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
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

fn normalize_report_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(normalized.as_str())
        .to_string()
}

fn md_cell(value: &str) -> String {
    value.replace('|', "\\|").replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_calibration_summarizes_static_runtime_agreement() -> Result<(), String> {
        let static_seams = vec![
            static_seam("gap-runtime", "weakly_gripped", "src/pricing.rs", 10),
            static_seam("gap-clean", "weakly_gripped", "src/pricing.rs", 20),
            static_seam("clean-clean", "strongly_gripped", "src/pricing.rs", 30),
            static_seam("clean-gap", "strongly_gripped", "src/pricing.rs", 40),
            static_seam("gap-none", "ungripped", "src/pricing.rs", 50),
        ];
        let runtime_mutants = vec![
            runtime("m1", Some("gap-runtime"), None, None, "missed"),
            runtime("m2", Some("gap-clean"), None, None, "caught"),
            runtime("m3", Some("clean-clean"), None, None, "caught"),
            runtime("m4", Some("clean-gap"), None, None, "missed"),
            runtime("m5", None, Some("src/other.rs"), Some(99), "missed"),
        ];

        let report = build_mutation_calibration_report(static_seams, runtime_mutants);
        assert_eq!(report.agreement.static_gap_and_runtime_signal, 1);
        assert_eq!(report.agreement.static_gap_without_runtime_signal, 2);
        assert_eq!(report.agreement.static_clean_and_runtime_clean, 1);
        assert_eq!(report.agreement.runtime_signal_without_static_gap, 2);
        assert_eq!(report.missed_runtime_signals.len(), 2);
        assert_eq!(report.static_only_findings.len(), 2);

        let json = render_mutation_calibration_json(&report)?;
        assert!(json.contains(r#""schema_version": "0.2""#));
        assert!(json.contains(r#""static_gap_and_runtime_signal": 1"#));
        assert!(json.contains(r#""confidence_label": "supports_static_gap""#));
        assert!(json.contains(r#""confidence_label": "contradicts_static_gap""#));
        assert!(json.contains(r#""confidence_label": "supports_static_clean""#));
        assert!(json.contains(r#""confidence_label": "contradicts_static_clean""#));
        assert!(json.contains(r#""confidence_label": "runtime_only_signal""#));
        assert!(json.contains(r#""confidence_label": "no_runtime_data""#));

        let markdown = render_mutation_calibration_md(&report);
        assert!(markdown.contains("# ripr mutation calibration report"));
        assert!(markdown.contains("| static_gap_and_runtime_signal | 1 |"));
        assert!(markdown.contains("Confidence label"));
        assert!(markdown.contains("Runtime signals without static gaps"));
        assert!(markdown.contains("Static gaps without runtime signals"));
        Ok(())
    }

    #[test]
    fn mutation_calibration_joins_by_seam_id_then_file_line_and_keeps_ambiguous() {
        let static_seams = vec![
            static_seam("id-match", "weakly_gripped", "src/pricing.rs", 10),
            static_seam("line-a", "weakly_gripped", "src/pricing.rs", 20),
            static_seam("line-b", "ungripped", "src/pricing.rs", 30),
            static_seam("ambiguous-a", "ungripped", "src/ambiguous.rs", 40),
            static_seam("ambiguous-b", "ungripped", "src/ambiguous.rs", 40),
        ];
        let runtime_mutants = vec![
            runtime("m-id", Some("id-match"), None, None, "missed"),
            runtime("m-line", None, Some("src/pricing.rs"), Some(20), "missed"),
            runtime(
                "m-ambiguous",
                None,
                Some("src/ambiguous.rs"),
                Some(40),
                "missed",
            ),
        ];

        let report = build_mutation_calibration_report(static_seams, runtime_mutants);
        assert_eq!(report.matched.len(), 2);
        assert_eq!(report.matched[0].join_method, "seam_id");
        assert_eq!(report.matched[1].join_method, "file_line");
        assert_eq!(report.ambiguous_file_line.len(), 1);
        assert_eq!(report.ambiguous_file_line[0].candidates.len(), 2);
        assert!(report.unmatched_mutants.is_empty());
    }

    #[test]
    fn mutation_calibration_parses_repo_exposure_and_cargo_mutants_json() -> Result<(), String> {
        let repo = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-a",
      "kind": "predicate_boundary",
      "file": ".\\src\\pricing.rs",
      "line": "42",
      "grip_class": "weakly_gripped",
      "related_tests": [
        {"oracle_kind": "exact_value", "oracle_strength": "weak"}
      ],
      "observed_values": [50, true],
      "missing_discriminators": [
        {"value": "threshold equality", "reason": "not observed"}
      ]
    }
  ]
}"#;
        let mutants = r#"{
  "outcomes": [
    {
      "id": "m1",
      "mutant": {
        "seam_id": "seam-a",
        "operator": "replace >= with >"
      },
      "outcome": "missed"
    }
  ]
}"#;

        let report = mutation_calibration_report_from_json(repo, mutants)?;
        assert_eq!(report.static_seams_total, 1);
        assert_eq!(report.mutants_total, 1);
        assert_eq!(report.matched.len(), 1);
        assert_eq!(report.matched[0].seam.file, "src/pricing.rs");
        assert_eq!(
            report.matched[0].mutation.mutation_operator,
            "replace >= with >"
        );
        Ok(())
    }

    #[test]
    fn mutation_calibration_parses_nested_runtime_locations_and_aliases() -> Result<(), String> {
        let repo = r#"{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-nested",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 42,
      "grip_class": "weakly_gripped",
      "related_tests": [
        {"oracle_kind": "smoke", "oracle_strength": "smoke"}
      ],
      "observed_values": [],
      "missing_discriminators": [
        "scalar discriminator",
        {"value": "boundary value"}
      ]
    },
    {
      "seam_id": "seam-location",
      "kind": "predicate_boundary",
      "file": "src/location.rs",
      "line": 17,
      "grip_class": "weakly_gripped",
      "related_tests": [
        {"oracle_kind": "none", "oracle_strength": "none"}
      ],
      "observed_values": [],
      "missing_discriminators": []
    },
    {
      "seam_id": "seam-span",
      "kind": "predicate_boundary",
      "file": "src/span.rs",
      "line": 18,
      "grip_class": "weakly_gripped",
      "related_tests": [
        {"oracle_kind": "unknown", "oracle_strength": "custom"}
      ],
      "observed_values": [],
      "missing_discriminators": []
    }
  ]
}"#;
        let mutants = r#"[
  true,
  {
    "mutation": {
      "seamId": "seam-nested",
      "path": "./src/pricing.rs",
      "startLine": "42",
      "replacement": "replace >= with >"
    },
    "status": "missed"
  },
  {
    "location": {
      "file_name": "src/location.rs",
      "line_start": "17"
    },
    "mutator": "replace location",
    "result": "caught"
  },
  {
    "span": {
      "file_name": "src/span.rs",
      "start": {
        "line": "18"
      }
    },
    "operator": "replace span",
    "state": "not caught"
  }
]"#;

        let report = mutation_calibration_report_from_json(repo, mutants)?;
        assert_eq!(report.mutants_total, 3);
        assert_eq!(report.matched.len(), 3);
        assert!(
            report
                .matched
                .iter()
                .any(|record| record.join_method == "file_line"
                    && record.seam.seam_id == "seam-location")
        );
        assert!(
            report
                .matched
                .iter()
                .any(|record| record.mutation.mutation_operator == "replace >= with >")
        );
        assert!(
            report
                .matched
                .iter()
                .any(|record| record.mutation.runtime_outcome == "not caught")
        );

        let json = render_mutation_calibration_json(&report)?;
        assert!(json.contains("scalar discriminator"));
        assert!(json.contains("boundary value"));
        assert!(json.contains("not_caught"));
        Ok(())
    }

    #[test]
    fn mutation_calibration_parses_optional_static_spans() -> Result<(), String> {
        let repo = r#"{
  "schema_version": "0.4",
  "scope": "repo",
  "seams": [
    {
      "seam_id": "seam-spanned",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 42,
      "column": 8,
      "end_line": 42,
      "end_column": 36,
      "grip_class": "weakly_gripped",
      "related_tests": [],
      "observed_values": [],
      "missing_discriminators": []
    },
    {
      "seam_id": "seam-lines-only",
      "kind": "predicate_boundary",
      "file": "src/pricing.rs",
      "line": 50,
      "grip_class": "weakly_gripped",
      "related_tests": [],
      "observed_values": [],
      "missing_discriminators": []
    }
  ]
}"#;
        let records = parse_repo_exposure_static_seams(repo)?;
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].column, Some(8));
        assert_eq!(records[0].end_line, Some(42));
        assert_eq!(records[0].end_column, Some(36));
        assert_eq!(records[1].column, None);
        assert_eq!(records[1].end_line, None);
        assert_eq!(records[1].end_column, None);
        Ok(())
    }

    #[test]
    fn mutation_calibration_renders_empty_ambiguous_unmatched_and_inconclusive()
    -> Result<(), String> {
        let empty_report = build_mutation_calibration_report(Vec::new(), Vec::new());
        let empty_markdown = render_mutation_calibration_md(&empty_report);
        assert!(empty_markdown.contains("| none | 0 |"));
        assert!(empty_markdown.contains("No runtime mutants matched static seams."));

        let static_seams = vec![
            static_seam("ambiguous-a", "ungripped", "src/ambiguous.rs", 40),
            static_seam("ambiguous-b", "ungripped", "src/ambiguous.rs", 40),
            static_seam("inconclusive", "weakly_gripped", "src/inconclusive.rs", 50),
        ];
        let runtime_mutants = vec![
            runtime(
                "m-ambiguous",
                None,
                Some("src/ambiguous.rs"),
                Some(40),
                "missed",
            ),
            runtime(
                "m-inconclusive",
                Some("inconclusive"),
                None,
                None,
                "skipped",
            ),
            MutationOutcomeRecord {
                mutant_id: Some("m-line-only".to_string()),
                seam_id: None,
                file: None,
                line: Some(77),
                span: None,
                span_conflict: false,
                mutation_operator: "replace line".to_string(),
                runtime_outcome: "missed".to_string(),
                duration: None,
                test_command: Some("cargo test targeted".to_string()),
            },
            MutationOutcomeRecord {
                mutant_id: Some("m-unknown".to_string()),
                seam_id: None,
                file: None,
                line: None,
                span: None,
                span_conflict: false,
                mutation_operator: "replace unknown".to_string(),
                runtime_outcome: "missed".to_string(),
                duration: None,
                test_command: None,
            },
        ];

        let report = build_mutation_calibration_report(static_seams, runtime_mutants);
        assert_eq!(report.matched.len(), 1);
        assert_eq!(report.ambiguous_file_line.len(), 1);
        assert_eq!(report.unmatched_mutants.len(), 2);
        assert_eq!(report.agreement.runtime_inconclusive, 2);
        assert!(
            report
                .static_only_findings
                .iter()
                .any(|record| record.reason
                    == "static gap seam matched only runtime-inconclusive labels")
        );

        let markdown = render_mutation_calibration_md(&report);
        assert!(markdown.contains("| `m-ambiguous` | src/ambiguous.rs:40 | missed |"));
        assert!(
            markdown.contains(
                "| line 77 | replace line | missed | `no_location` | cargo test targeted |"
            )
        );
        assert!(
            markdown.contains("| unknown | replace unknown | missed | `no_location` | unknown |")
        );

        let json = render_mutation_calibration_json(&report)?;
        let value: Value = serde_json::from_str(&json)
            .map_err(|err| format!("mutation calibration JSON should parse: {err}"))?;
        assert_eq!(
            value["ambiguous_file_line_matches"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            value["ambiguous_file_line_matches"][0]["confidence_label"],
            "ambiguous_runtime_join"
        );
        assert_eq!(
            value["static_only_findings"][0]["confidence_label"],
            "no_runtime_data"
        );
        assert_eq!(value["unmatched_mutants"].as_array().map(Vec::len), Some(2));
        Ok(())
    }

    #[test]
    fn mutation_calibration_merges_mutants_and_outcomes_by_id() -> Result<(), String> {
        let repo = repo_json_for("seam-a", "weakly_gripped", "src/pricing.rs", 42);
        let mutants = r#"[
  {"mutants": [{"id": "m1", "file": "src/pricing.rs", "line": 42, "operator": "replace"}]},
  {"outcomes": [{"id": "m1", "outcome": "caught", "duration_ms": 10}]}
]"#;

        let report = mutation_calibration_report_from_json(&repo, mutants)?;
        assert_eq!(report.matched.len(), 1);
        assert_eq!(report.matched[0].mutation.runtime_outcome, "caught");
        assert_eq!(report.matched[0].mutation.duration.as_deref(), Some("10"));
        Ok(())
    }

    #[test]
    fn mutation_calibration_reports_are_advisory_and_structured() -> Result<(), String> {
        let report = mutation_calibration_report_from_json(
            &repo_json_for("seam-a", "weakly_gripped", "src/pricing.rs", 42),
            r#"[{"id":"m1","seam_id":"seam-a","outcome":"missed","operator":"replace"}]"#,
        )?;
        let json = render_mutation_calibration_json(&report)?;
        let value: Value = serde_json::from_str(&json)
            .map_err(|err| format!("mutation calibration JSON should parse: {err}"))?;
        assert_eq!(value["status"], "advisory");
        assert_eq!(value["metrics"]["matched_total"], 1);
        assert_eq!(
            value["agreement"]["static_gap_and_runtime_signal"],
            Value::from(1)
        );

        let markdown = render_mutation_calibration_md(&report);
        assert!(markdown.contains("Status: advisory"));
        assert!(markdown.contains("Runtime Outcome Counts"));
        Ok(())
    }

    fn repo_json_for(id: &str, grip_class: &str, file: &str, line: usize) -> String {
        format!(
            r#"{{
  "schema_version": "0.2",
  "scope": "repo",
  "seams": [
    {{
      "seam_id": "{id}",
      "kind": "predicate_boundary",
      "file": "{file}",
      "line": {line},
      "grip_class": "{grip_class}",
      "related_tests": [],
      "observed_values": [],
      "missing_discriminators": []
    }}
  ]
}}"#
        )
    }

    fn joins(report: &MutationCalibrationReport) -> Vec<(&str, &str, &'static str)> {
        report
            .matched
            .iter()
            .map(|record| {
                (
                    record.mutation.mutant_id.as_deref().unwrap_or(""),
                    record.seam.seam_id.as_str(),
                    record.join_method,
                )
            })
            .collect()
    }

    fn tied(records: &[AmbiguousMutationCalibrationMatch]) -> Vec<(&str, Vec<&str>)> {
        records
            .iter()
            .map(|record| {
                (
                    record.mutation.mutant_id.as_deref().unwrap_or(""),
                    record
                        .candidates
                        .iter()
                        .map(|seam| seam.seam_id.as_str())
                        .collect(),
                )
            })
            .collect()
    }

    fn unmatched(report: &MutationCalibrationReport) -> Vec<(&str, &'static str)> {
        report
            .unmatched_mutants
            .iter()
            .map(|record| {
                (
                    record.mutation.mutant_id.as_deref().unwrap_or(""),
                    record.reason,
                )
            })
            .collect()
    }

    /// The #5336 wrong pairing: semver `display.rs:20` holds a call seam
    /// `digits(self.minor)` and an arithmetic `+` on the same line. The `+`
    /// mutant falls outside the call span, so it is unmatched with
    /// `no_containing_seam` instead of rescued by the shared line; a mutant
    /// inside the call joins it by containment.
    #[test]
    fn span_join_refuses_a_same_line_seam_that_does_not_contain_the_mutant() {
        let static_seams = vec![spanned_seam("call", "src/display.rs", (20, 9), (20, 27))];
        let runtime_mutants = vec![
            spanned_runtime("m-plus", "src/display.rs", (20, 30), (20, 31)),
            spanned_runtime("m-inside", "src/display.rs", (20, 15), (20, 16)),
        ];

        let report = build_mutation_calibration_report(static_seams, runtime_mutants);

        assert_eq!(
            joins(&report),
            vec![("m-inside", "call", "span_containment")]
        );
        assert_eq!(
            unmatched(&report),
            vec![("m-plus", UNMATCHED_NO_CONTAINING_SEAM)]
        );
        assert!(report.ambiguous_file_line.is_empty());
        assert!(report.ambiguous_span_overlap.is_empty());
    }

    /// atuin `context.rs:40` shape: a predicate seam nested inside a return
    /// seam on one line. File/line alone is ambiguous; containment picks the
    /// innermost seam, and a mutant only the outer seam contains joins it.
    #[test]
    fn span_join_scores_multi_seam_lines_on_the_innermost_containing_seam() {
        let static_seams = vec![
            spanned_seam("return", "src/context.rs", (40, 5), (40, 40)),
            spanned_seam("predicate", "src/context.rs", (40, 12), (40, 25)),
        ];
        let runtime_mutants = vec![
            spanned_runtime("m-predicate", "src/context.rs", (40, 18), (40, 20)),
            spanned_runtime("m-return", "src/context.rs", (40, 30), (40, 31)),
        ];

        let report = build_mutation_calibration_report(static_seams, runtime_mutants);

        assert!(report.ambiguous_file_line.is_empty(), "{report:?}");
        assert!(report.ambiguous_span_overlap.is_empty(), "{report:?}");
        assert_eq!(
            joins(&report),
            vec![
                ("m-predicate", "predicate", "span_containment"),
                ("m-return", "return", "span_containment"),
            ]
        );
    }

    /// Equal and crossing innermost spans are true overlaps: reported with
    /// every candidate in geometric order, never broken by snapshot order,
    /// length or ID. A mutant on a later line of a multi-line seam joins it.
    #[test]
    fn span_join_reports_equal_and_crossing_overlaps_and_multiline_containment() {
        let seams = || {
            vec![
                spanned_seam("twin-b", "src/lib.rs", (5, 5), (5, 20)),
                spanned_seam("twin-a", "src/lib.rs", (5, 5), (5, 20)),
                spanned_seam("cross-right", "src/lib.rs", (7, 10), (7, 30)),
                spanned_seam("cross-left", "src/lib.rs", (7, 1), (7, 20)),
                spanned_seam("multi", "src/lib.rs", (8, 9), (10, 6)),
            ]
        };
        let mutants = || {
            vec![
                spanned_runtime("m-tie", "src/lib.rs", (5, 10), (5, 11)),
                spanned_runtime("m-cross", "src/lib.rs", (7, 12), (7, 14)),
                spanned_runtime("m-multi", "src/lib.rs", (9, 3), (9, 5)),
            ]
        };

        let report = build_mutation_calibration_report(seams(), mutants());
        let mut reversed_seams = seams();
        reversed_seams.reverse();
        let reversed = build_mutation_calibration_report(reversed_seams, mutants());

        assert_eq!(
            joins(&report),
            vec![("m-multi", "multi", "span_containment")]
        );
        assert_eq!(
            tied(&report.ambiguous_span_overlap),
            vec![
                ("m-tie", vec!["twin-a", "twin-b"]),
                ("m-cross", vec!["cross-left", "cross-right"]),
            ]
        );
        assert_eq!(
            tied(&reversed.ambiguous_span_overlap),
            tied(&report.ambiguous_span_overlap),
            "candidate order does not follow snapshot order"
        );
        assert!(report.ambiguous_file_line.is_empty());
    }

    /// Span-less seams (match arms) cannot block a unique containment match;
    /// they join only as the line fallback when no span contains the mutant.
    /// A runtime record without a complete span keeps the line join over
    /// every seam on its line, and an explicit seam_id beats geometry.
    #[test]
    fn span_join_ranks_containment_over_line_fallback_and_seam_id_over_both() {
        let mut arm = static_seam("arm", "ungripped", "src/lib.rs", 3);
        arm.seam_kind = "match_arm".to_string();
        let mut lone_arm = static_seam("lone-arm", "ungripped", "src/lib.rs", 12);
        lone_arm.seam_kind = "match_arm".to_string();
        let static_seams = vec![
            spanned_seam("call", "src/lib.rs", (2, 5), (5, 7)),
            arm,
            lone_arm,
            spanned_seam("pred", "src/lib.rs", (20, 5), (20, 15)),
            spanned_seam("ret", "src/lib.rs", (20, 5), (20, 30)),
        ];
        let mut no_span = runtime("m-no-span", None, Some("src/lib.rs"), Some(20), "missed");
        no_span.span = None;
        let mut by_id = spanned_runtime("m-by-id", "src/lib.rs", (20, 7), (20, 8));
        by_id.seam_id = Some("ret".to_string());
        let runtime_mutants = vec![
            spanned_runtime("m-arm", "src/lib.rs", (3, 9), (3, 20)),
            spanned_runtime("m-lone-arm", "src/lib.rs", (12, 14), (12, 20)),
            no_span,
            by_id,
        ];

        let report = build_mutation_calibration_report(static_seams, runtime_mutants);

        assert_eq!(
            joins(&report),
            vec![
                ("m-arm", "call", "span_containment"),
                ("m-lone-arm", "lone-arm", "file_line"),
                ("m-by-id", "ret", "seam_id"),
            ]
        );
        assert_eq!(
            tied(&report.ambiguous_file_line),
            vec![("m-no-span", vec!["pred", "ret"])]
        );
        assert!(report.ambiguous_span_overlap.is_empty());
    }

    /// A complete span fills a span-less duplicate; two different complete
    /// spans for one mutant drop to line-only evidence and say so.
    #[test]
    fn merging_runtime_records_keeps_complete_spans_and_refuses_conflicts() -> Result<(), String> {
        let spanned = spanned_runtime("m1", "src/lib.rs", (4, 9), (4, 10));
        let mut bare = spanned.clone();
        bare.span = None;
        let mut other = spanned.clone();
        other.span = Some(RuntimeSpan {
            start: (4, 12),
            end: (4, 13),
        });

        let filled = merge_mutation_outcome_records(vec![bare.clone(), spanned.clone()]);
        assert_eq!(filled.len(), 1);
        assert_eq!(filled[0].span, spanned.span);
        assert!(!filled[0].span_conflict);

        let conflicting = merge_mutation_outcome_records(vec![spanned, other, bare]);
        assert_eq!(conflicting.len(), 1);
        assert_eq!(conflicting[0].span, None);
        assert!(conflicting[0].span_conflict);
        let json = mutation_outcome_json(&conflicting[0]);
        assert_eq!(json["span_status"], "conflicting_runtime_spans");
        assert!(json.get("column").is_none());
        Ok(())
    }

    #[test]
    fn span_join_reads_cargo_mutants_columns_end_to_end() -> Result<(), String> {
        let exposure = r#"{"seams": [
  {"seam_id": "ret", "kind": "return_value", "file": "src/lib.rs", "line": 3, "column": 5, "end_line": 3, "end_column": 14, "grip_class": "strongly_gripped"},
  {"seam_id": "cmp", "kind": "predicate_boundary", "file": "src/lib.rs", "line": 3, "column": 5, "end_line": 3, "end_column": 10, "grip_class": "ungripped"},
  {"seam_id": "call", "kind": "call_presence", "file": "src/lib.rs", "line": 6, "column": 5, "end_line": 6, "end_column": 12, "grip_class": "ungripped"}
]}"#;
        let mutants = r#"[{
  "name": "src/lib.rs:3:7: replace > with >= in f",
  "file": "src/lib.rs",
  "span": {"start": {"line": 3, "column": 7}, "end": {"line": 3, "column": 8}},
  "replacement": ">=",
  "genre": "BinaryOperator",
  "summary": "MissedMutant"
}, {
  "name": "src/lib.rs:6:14: replace + with - in f",
  "file": "src/lib.rs",
  "span": {"start": {"line": 6, "column": 14}, "end": {"line": 6, "column": 15}},
  "replacement": "-",
  "genre": "BinaryOperator",
  "summary": "MissedMutant"
}]"#;

        let report = mutation_calibration_report_from_json(exposure, mutants)?;

        assert_eq!(report.matched.len(), 1, "{report:?}");
        assert_eq!(report.matched[0].seam.seam_id, "cmp");
        assert_eq!(report.matched[0].join_method, "span_containment");
        let json: Value = serde_json::from_str(&render_mutation_calibration_json(&report)?)
            .map_err(|err| err.to_string())?;
        assert_eq!(json["schema_version"], "0.2");
        let matched = &json["matches"][0];
        assert_eq!(matched["runtime"]["column"], 7);
        assert_eq!(matched["runtime"]["end_column"], 8);
        assert_eq!(matched["static"]["end_column"], 10);
        assert_eq!(json["metrics"]["join_method_counts"]["span_containment"], 1);
        assert_eq!(json["metrics"]["ambiguous_span_overlap_total"], 0);
        assert_eq!(
            json["unmatched_mutants"][0]["unmatched_reason"],
            UNMATCHED_NO_CONTAINING_SEAM
        );
        assert_eq!(
            json["metrics"]["unmatched_reason_counts"][UNMATCHED_NO_CONTAINING_SEAM],
            1
        );
        Ok(())
    }

    fn spanned_seam(
        id: &str,
        file: &str,
        start: (usize, usize),
        end: (usize, usize),
    ) -> StaticSeamRecord {
        let mut seam = static_seam(id, "strongly_gripped", file, start.0);
        seam.column = Some(start.1);
        seam.end_line = Some(end.0);
        seam.end_column = Some(end.1);
        seam
    }

    fn spanned_runtime(
        id: &str,
        file: &str,
        start: (usize, usize),
        end: (usize, usize),
    ) -> MutationOutcomeRecord {
        let mut record = runtime(id, None, Some(file), Some(start.0), "missed");
        record.span = Some(RuntimeSpan { start, end });
        record
    }

    fn static_seam(id: &str, grip_class: &str, file: &str, line: usize) -> StaticSeamRecord {
        StaticSeamRecord {
            seam_id: id.to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: file.to_string(),
            line,
            column: None,
            end_line: None,
            end_column: None,
            seam_grip_class: grip_class.to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "unknown".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        }
    }

    fn runtime(
        id: &str,
        seam_id: Option<&str>,
        file: Option<&str>,
        line: Option<usize>,
        outcome: &str,
    ) -> MutationOutcomeRecord {
        MutationOutcomeRecord {
            mutant_id: Some(id.to_string()),
            seam_id: seam_id.map(str::to_string),
            file: file.map(str::to_string),
            line,
            span: None,
            span_conflict: false,
            mutation_operator: "replace".to_string(),
            runtime_outcome: outcome.to_string(),
            duration: None,
            test_command: None,
        }
    }
}
