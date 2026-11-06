//! Structured staged-evidence rows (RIPR-SPEC-0423).
//!
//! The staged evidence chain — reach, infection, propagation, observation,
//! discrimination — has always had one producer: the finding's `ripr`
//! stages. The human Evidence block renders it as prose, `--format json`
//! carried the states under `ripr` and the same chain again as English
//! sentences in `evidence_path`, and the agent format carried no stages at
//! all. A script that wanted "which stage is capped" had to parse sentences.
//! This module is the one shared row-form projection both machine surfaces
//! consume:
//!
//! - `check --format json` (and its `json-compact`/`json-terse` minifications)
//!   emits `evidence_stages`: one row per stage carrying `stage`, `state`,
//!   `confidence`, and the capping basis entries the finding's
//!   `confidence_basis` names for that stage (where applicable).
//! - `--format agent` / `ripr explain --format agent` emit one compact
//!   `stages=` row after each finding record.
//!
//! It is a pure projection: no classification, ranking or decision lives
//! here. Absence is honest — rows the finding's basis does not name carry no
//! `basis` field. No row carries test or assertion references: no producer
//! records which assertion established a stage, and the general
//! `related_tests` decision-set role is not stage provenance (#13453 owns the
//! producer-backed attribution that would let a row claim one).

use crate::domain::Finding;

/// One machine-readable staged-evidence row (RIPR-SPEC-0423). `stage` uses
/// the `ripr` object's own keys, so a consumer joins rows to the typed stage
/// objects by the same token. `basis` is present only when the finding's
/// `confidence_basis` names this stage.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StageRow {
    pub stage: &'static str,
    pub state: &'static str,
    pub confidence: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub basis: Vec<StageBasisEntry>,
}

/// One capping reason a stage row claims, in the shared `{code, summary}`
/// shape the finding-level `confidence_basis` list already uses (#10700).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StageBasisEntry {
    pub code: &'static str,
    pub summary: String,
}

/// The five staged-evidence rows for a finding, in the fixed Evidence-block
/// order. A pure projection of the finding's own evidence.
pub fn evidence_stage_rows(finding: &Finding) -> Vec<StageRow> {
    let basis_for = |stage: &str| {
        finding
            .confidence_basis
            .iter()
            .filter(|cap| cap.capped_stage() == Some(stage))
            .map(|cap| StageBasisEntry {
                code: cap.as_str(),
                summary: cap.summary(finding.language),
            })
            .collect::<Vec<_>>()
    };
    let stage = |name: &'static str, evidence: &crate::domain::StageEvidence| StageRow {
        stage: name,
        state: evidence.state.as_str(),
        confidence: evidence.confidence.as_str(),
        basis: basis_for(name),
    };
    vec![
        stage("reach", &finding.ripr.reach),
        stage("infect", &finding.ripr.infect),
        stage("propagate", &finding.ripr.propagate),
        stage("observe", &finding.ripr.reveal.observe),
        stage("discriminate", &finding.ripr.reveal.discriminate),
    ]
}

/// The compact `stages=` row the agent surfaces print after a finding
/// record: one `stage:state` token per stage, comma-separated, in the same
/// fixed order. States use the same closed vocabulary as the JSON rows.
pub fn stage_rows_agent_line(rows: &[StageRow]) -> String {
    let parts = rows
        .iter()
        .map(|row| format!("{}:{}", row.stage, row.state))
        .collect::<Vec<_>>()
        .join(",");
    format!("stages={parts}")
}

/// Emits the finding's `evidence_stages` array into the hand-rolled
/// diff-check JSON document, in the same compact one-line-object style the
/// `ripr` stage objects use. `trailing` governs the array's own trailing
/// comma, matching the sibling emitters.
pub(crate) fn stage_rows_json(out: &mut String, indent: usize, finding: &Finding, trailing: bool) {
    let sp = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);
    let rows = evidence_stage_rows(finding);
    out.push_str(&format!("{sp}\"evidence_stages\": [\n"));
    for (index, row) in rows.iter().enumerate() {
        let row_trailing = if index + 1 != rows.len() { "," } else { "" };
        out.push_str(&format!(
            "{inner}{{\"stage\":\"{}\",\"state\":\"{}\",\"confidence\":\"{}\"",
            row.stage, row.state, row.confidence
        ));
        if !row.basis.is_empty() {
            let entries = row
                .basis
                .iter()
                .map(|entry| {
                    format!(
                        "{{\"code\":\"{}\",\"summary\":\"{}\"}}",
                        entry.code,
                        crate::output::json::escape(&entry.summary)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!(",\"basis\":[{entries}]"));
        }
        out.push_str(&format!("}}{row_trailing}\n"));
    }
    let trailing_comma = if trailing { "," } else { "" };
    out.push_str(&format!("{sp}]{trailing_comma}\n"));
}

#[cfg(test)]
mod tests {
    use super::{StageRow, evidence_stage_rows, stage_rows_agent_line, stage_rows_json};
    use crate::domain::{
        ActivationEvidence, Confidence, ConfidenceCap, DeltaKind, ExposureClass, Finding,
        OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest, RelatedTestMiss,
        RelatedTestRole, RevealEvidence, RiprEvidence, SourceLocation, StageEvidence, StageState,
    };
    use std::path::PathBuf;

    fn stage(state: StageState, confidence: Confidence) -> StageEvidence {
        StageEvidence::new(state, confidence, "fixture summary")
    }

    fn finding() -> Finding {
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "probe:src_lib_rs:1:predicate".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:src_lib_rs:1:predicate".to_string()),
                family: ProbeFamily::Predicate,
                location: SourceLocation::new("src/lib.rs", 1, 1),
                owner: None,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "expr".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, Confidence::Medium),
                infect: stage(StageState::Weak, Confidence::Medium),
                propagate: stage(StageState::Yes, Confidence::Medium),
                reveal: RevealEvidence {
                    observe: stage(StageState::Yes, Confidence::Medium),
                    discriminate: stage(StageState::Yes, Confidence::Medium),
                },
            },
            confidence: 0.5,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: Vec::new(),
            recommended_next_step: None,
            language: None,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    fn related(name: &str, miss: Option<RelatedTestMiss>, contributing: bool) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/pricing.rs"),
            line: 12,
            oracle: Some("assert_eq!(total, 90);".to_string()),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: None,
            relation_confidence: None,
            miss,
            role: if contributing {
                RelatedTestRole::Contributing
            } else {
                RelatedTestRole::ExaminedOnly
            },
            miss_support: None,
        }
    }

    #[test]
    fn rows_project_the_five_stages_in_evidence_order() {
        let mut finding = finding();
        finding.ripr = RiprEvidence {
            reach: stage(StageState::Yes, Confidence::Medium),
            infect: stage(StageState::Weak, Confidence::Medium),
            propagate: stage(StageState::Yes, Confidence::High),
            reveal: RevealEvidence {
                observe: stage(StageState::Yes, Confidence::Medium),
                discriminate: stage(StageState::No, Confidence::High),
            },
        };
        let rows = evidence_stage_rows(&finding);
        let names = rows.iter().map(|row| row.stage).collect::<Vec<_>>();
        assert_eq!(
            names,
            ["reach", "infect", "propagate", "observe", "discriminate"]
        );
        let states = rows.iter().map(|row| row.state).collect::<Vec<_>>();
        assert_eq!(states, ["yes", "weak", "yes", "yes", "no"]);
        let confidences = rows.iter().map(|row| row.confidence).collect::<Vec<_>>();
        assert_eq!(confidences, ["medium", "medium", "high", "medium", "high"]);
        // No confidence_basis on a Rust finding: no row claims a basis.
        assert!(rows.iter().all(|row: &StageRow| row.basis.is_empty()));
    }

    #[test]
    fn stage_named_basis_entries_land_on_their_stage_row_only() -> Result<(), String> {
        let mut finding = finding();
        finding.confidence_basis = vec![
            ConfidenceCap::PreviewStagesNotModeled,
            ConfidenceCap::NoRelatedTest,
            ConfidenceCap::ObservationNotConfirmed,
            ConfidenceCap::BoundaryInputNotMatched,
        ];
        let rows = evidence_stage_rows(&finding);
        let reach = rows
            .iter()
            .find(|row| row.stage == "reach")
            .ok_or("reach row missing")?;
        assert_eq!(reach.basis.len(), 1);
        assert_eq!(reach.basis[0].code, "no_related_test");
        assert_eq!(
            reach.basis[0].summary,
            "no related test reaches the changed owner"
        );
        let observe = rows
            .iter()
            .find(|row| row.stage == "observe")
            .ok_or("observe row missing")?;
        assert_eq!(observe.basis.len(), 1);
        assert_eq!(observe.basis[0].code, "observation_not_confirmed");
        // Finding-level and oracle-level caps are claimed by no stage row.
        for name in ["infect", "propagate", "discriminate"] {
            let row = rows
                .iter()
                .find(|row| row.stage == name)
                .ok_or_else(|| format!("{name} row missing"))?;
            assert!(row.basis.is_empty(), "{name} claimed a finding-level cap");
        }
        Ok(())
    }

    /// #13453 negative control: the general related-test decision set is not
    /// stage provenance, so no row — the discriminate row included — may
    /// reference a related test, whatever its role or miss.
    #[test]
    fn no_row_claims_related_test_attribution() {
        let mut finding = finding();
        finding.related_tests = vec![
            related("contributing_a", None, true),
            related("examined_only_b", Some(RelatedTestMiss::NoAssertion), false),
            related(
                "contributing_no_call",
                Some(RelatedTestMiss::NoCallPath),
                true,
            ),
        ];
        let mut out = String::new();
        stage_rows_json(&mut out, 1, &finding, true);
        assert!(!out.contains("\"tests\""), "{out}");
        assert!(!out.contains("contributing_a"), "{out}");
        assert!(
            out.contains(
                "{\"stage\":\"discriminate\",\"state\":\"yes\",\"confidence\":\"medium\"}"
            ),
            "{out}"
        );
    }

    #[test]
    fn agent_line_is_the_compact_stage_state_row() {
        let mut finding = finding();
        finding.ripr = RiprEvidence {
            reach: stage(StageState::Yes, Confidence::Medium),
            infect: stage(StageState::Weak, Confidence::Low),
            propagate: stage(StageState::Unknown, Confidence::Low),
            reveal: RevealEvidence {
                observe: stage(StageState::Yes, Confidence::Medium),
                discriminate: stage(StageState::No, Confidence::High),
            },
        };
        let line = stage_rows_agent_line(&evidence_stage_rows(&finding));
        assert_eq!(
            line,
            "stages=reach:yes,infect:weak,propagate:unknown,observe:yes,discriminate:no"
        );
    }
}
