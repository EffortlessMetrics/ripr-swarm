use crate::app::{CheckOutput, Mode};
use crate::domain::{ExposureClass, LanguageId};
use crate::output::python_repair_card::{PythonRepairCard, python_repair_card};
use std::path::{Path, PathBuf};

pub(crate) const PILOT_SUMMARY_SCHEMA_VERSION: &str = "0.4";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PilotArtifacts {
    pub(crate) repo_exposure_json: PathBuf,
    pub(crate) repo_exposure_md: PathBuf,
    pub(crate) agent_seam_packets_json: PathBuf,
    pub(crate) pilot_summary_json: PathBuf,
    pub(crate) pilot_summary_md: PathBuf,
}

impl PilotArtifacts {
    /// Files a timeout run does not rewrite. A prior complete run may have
    /// left them in the same packet directory (#7425).
    pub(crate) fn unwritten_timeout_siblings(&self) -> [&Path; 3] {
        [
            self.repo_exposure_json.as_path(),
            self.repo_exposure_md.as_path(),
            self.agent_seam_packets_json.as_path(),
        ]
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PilotSummaryContext<'a> {
    pub(crate) root: &'a Path,
    pub(crate) mode: &'a Mode,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) max_seams: usize,
    pub(crate) timeout_ms: u64,
    /// Automatic deadline extension granted when `--timeout-ms` was omitted
    /// (#2424). Zero when the budget was a hard limit or this summary is
    /// not a timeout path.
    pub(crate) timeout_extension_ms: u64,
    pub(crate) artifacts: &'a PilotArtifacts,
    pub(crate) python_first_use: Option<&'a PilotPythonFirstUse>,
    /// Routes for languages pilot's Rust seam scan does not rank. `None` when
    /// they were not collected (the timeout summary).
    pub(crate) language_routes: Option<&'a super::PilotLanguageRoutes>,
    /// The current change (see `PilotCurrentChange`). `None` when it was not
    /// collected (the timeout summary).
    pub(crate) current_change: Option<&'a super::PilotCurrentChange>,
    /// Set when a seam limit (pilot budget or inventory limit) cut the
    /// classified seams before ranking, so counts over them are lower bounds.
    pub(crate) seam_limit: Option<&'a crate::analysis::SeamLimitInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PilotPythonFirstUse {
    pub(crate) status: PilotPythonFirstUseStatus,
    pub(crate) findings_total: usize,
    pub(crate) repair_cards_total: usize,
    pub(crate) limitation_count: usize,
    pub(crate) analysis_error: Option<String>,
    pub(crate) top_repair_card: Option<PythonRepairCard>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PilotPythonFirstUseStatus {
    AnalysisUnavailable,
    NoPythonFindings,
    NoRepairCards,
    Ready,
}

impl PilotPythonFirstUse {
    pub(crate) fn analysis_unavailable(error: String) -> Self {
        Self {
            status: PilotPythonFirstUseStatus::AnalysisUnavailable,
            findings_total: 0,
            repair_cards_total: 0,
            limitation_count: 1,
            analysis_error: Some(error),
            top_repair_card: None,
        }
    }

    pub(crate) fn from_check_output(output: &CheckOutput) -> Self {
        let python_findings = output
            .findings
            .iter()
            .filter(|finding| finding.language == Some(LanguageId::Python));
        let findings_total = python_findings.clone().count();
        let limitation_count = python_findings
            .filter(|finding| is_python_limitation(finding))
            .count();
        let cards = output
            .findings
            .iter()
            .filter_map(python_repair_card)
            .collect::<Vec<_>>();
        let repair_cards_total = cards.len();
        let top_repair_card = cards.into_iter().next();
        let status = if top_repair_card.is_some() {
            PilotPythonFirstUseStatus::Ready
        } else if findings_total == 0 {
            PilotPythonFirstUseStatus::NoPythonFindings
        } else {
            PilotPythonFirstUseStatus::NoRepairCards
        };

        Self {
            status,
            findings_total,
            repair_cards_total,
            limitation_count,
            analysis_error: None,
            top_repair_card,
        }
    }
}

impl PilotPythonFirstUseStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            PilotPythonFirstUseStatus::AnalysisUnavailable => "analysis_unavailable",
            PilotPythonFirstUseStatus::NoPythonFindings => "no_python_findings",
            PilotPythonFirstUseStatus::NoRepairCards => "no_repair_cards",
            PilotPythonFirstUseStatus::Ready => "ready",
        }
    }
}

fn is_python_limitation(finding: &crate::domain::Finding) -> bool {
    finding.static_limit_kind.is_some()
        || matches!(
            finding.class,
            ExposureClass::InfectionUnknown
                | ExposureClass::PropagationUnknown
                | ExposureClass::StaticUnknown
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{CHECK_OUTPUT_SCHEMA_VERSION, Mode};
    use crate::domain::{
        ActivationEvidence, Confidence, Finding, Probe, ProbeFamily, ProbeId, RevealEvidence,
        RiprEvidence, SourceCurrentness, SourceLocation, StageEvidence, StageState,
        StaticLimitKind, Summary,
    };

    fn finding(
        language: Option<LanguageId>,
        class: ExposureClass,
        static_limit_kind: Option<StaticLimitKind>,
    ) -> Finding {
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            id: "probe:pkg_mod_py:7:predicate".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:pkg_mod_py:7:predicate".to_string()),
                location: SourceLocation::new("pkg/mod.py", 7, 1),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: crate::domain::DeltaKind::Control,
                before: None,
                after: None,
                expression: "amount >= limit".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class,
            ripr: RiprEvidence {
                reach: StageEvidence::new(StageState::Yes, Confidence::Medium, "reached"),
                infect: StageEvidence::new(StageState::Yes, Confidence::Medium, "infected"),
                propagate: StageEvidence::new(StageState::Yes, Confidence::Medium, "propagated"),
                reveal: RevealEvidence {
                    observe: StageEvidence::new(StageState::Weak, Confidence::Low, "observed"),
                    discriminate: StageEvidence::new(StageState::No, Confidence::Low, "none"),
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
            language,
            language_status: None,
            owner_kind: None,
            static_limit_kind,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    fn check_output(findings: Vec<Finding>) -> CheckOutput {
        CheckOutput {
            harness_projections: Vec::new(),
            schema_version: CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
            tool: "ripr".to_string(),
            mode: Mode::Draft,
            root: PathBuf::from("."),
            base: None,
            summary: Summary::default(),
            findings,
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            ignored_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            analysis_outcome: None,
            partial_scope: None,
            analyzed_revisions: None,
        }
    }

    #[test]
    fn schema_version_and_status_labels_are_pinned() {
        assert_eq!(PILOT_SUMMARY_SCHEMA_VERSION, "0.4");
        assert_eq!(
            PilotPythonFirstUseStatus::AnalysisUnavailable.as_str(),
            "analysis_unavailable"
        );
        assert_eq!(
            PilotPythonFirstUseStatus::NoPythonFindings.as_str(),
            "no_python_findings"
        );
        assert_eq!(
            PilotPythonFirstUseStatus::NoRepairCards.as_str(),
            "no_repair_cards"
        );
        assert_eq!(PilotPythonFirstUseStatus::Ready.as_str(), "ready");
    }

    #[test]
    fn timeout_siblings_name_exactly_the_three_unrewritten_files() {
        let artifacts = PilotArtifacts {
            repo_exposure_json: PathBuf::from("p/repo-exposure.json"),
            repo_exposure_md: PathBuf::from("p/repo-exposure.md"),
            agent_seam_packets_json: PathBuf::from("p/agent-seam-packets.json"),
            pilot_summary_json: PathBuf::from("p/pilot-summary.json"),
            pilot_summary_md: PathBuf::from("p/pilot-summary.md"),
        };
        let siblings = artifacts.unwritten_timeout_siblings();
        assert_eq!(
            siblings,
            [
                Path::new("p/repo-exposure.json"),
                Path::new("p/repo-exposure.md"),
                Path::new("p/agent-seam-packets.json"),
            ]
        );
        // The summary files a timeout run does rewrite are never listed.
        assert!(!siblings.contains(&artifacts.pilot_summary_json.as_path()));
        assert!(!siblings.contains(&artifacts.pilot_summary_md.as_path()));
    }

    #[test]
    fn analysis_unavailable_records_the_error_and_one_limitation() {
        assert_eq!(
            PilotPythonFirstUse::analysis_unavailable("boom".to_string()),
            PilotPythonFirstUse {
                status: PilotPythonFirstUseStatus::AnalysisUnavailable,
                findings_total: 0,
                repair_cards_total: 0,
                limitation_count: 1,
                analysis_error: Some("boom".to_string()),
                top_repair_card: None,
            }
        );
    }

    #[test]
    fn empty_output_has_no_python_findings() {
        assert_eq!(
            PilotPythonFirstUse::from_check_output(&check_output(Vec::new())),
            PilotPythonFirstUse {
                status: PilotPythonFirstUseStatus::NoPythonFindings,
                findings_total: 0,
                repair_cards_total: 0,
                limitation_count: 0,
                analysis_error: None,
                top_repair_card: None,
            }
        );
    }

    #[test]
    fn non_python_findings_are_not_counted_as_python() {
        let python = finding(Some(LanguageId::Python), ExposureClass::WeaklyExposed, None);
        let rust = finding(
            Some(LanguageId::Rust),
            ExposureClass::InfectionUnknown,
            None,
        );
        let unlabeled = finding(None, ExposureClass::StaticUnknown, None);

        // Positive control: the same shape with a Python label is counted.
        let counted = PilotPythonFirstUse::from_check_output(&check_output(vec![python]));
        assert_eq!(counted.findings_total, 1);

        let other = PilotPythonFirstUse::from_check_output(&check_output(vec![rust, unlabeled]));
        assert_eq!(other.status, PilotPythonFirstUseStatus::NoPythonFindings);
        assert_eq!(other.findings_total, 0);
        assert_eq!(other.limitation_count, 0);
    }

    #[test]
    fn python_findings_without_a_repair_card_report_no_repair_cards() {
        // A weakly exposed Python finding with no canonical gap is not
        // actionable, so no card is produced.
        let weak = finding(Some(LanguageId::Python), ExposureClass::WeaklyExposed, None);
        let first_use = PilotPythonFirstUse::from_check_output(&check_output(vec![weak]));
        assert_eq!(first_use.status, PilotPythonFirstUseStatus::NoRepairCards);
        assert_eq!(first_use.findings_total, 1);
        assert_eq!(first_use.repair_cards_total, 0);
        assert_eq!(first_use.limitation_count, 0);
        assert_eq!(first_use.analysis_error, None);
        assert_eq!(first_use.top_repair_card, None);
    }

    #[test]
    fn limitations_count_unknown_classes_and_static_limit_kinds() {
        let python = Some(LanguageId::Python);
        let findings = vec![
            // Not limitations.
            finding(python, ExposureClass::WeaklyExposed, None),
            finding(python, ExposureClass::Exposed, None),
            // Limitations by class.
            finding(python, ExposureClass::InfectionUnknown, None),
            finding(python, ExposureClass::PropagationUnknown, None),
            finding(python, ExposureClass::StaticUnknown, None),
            // Limitation by static limit kind on an otherwise non-limiting class.
            finding(
                python,
                ExposureClass::Exposed,
                Some(StaticLimitKind::DynamicDispatch),
            ),
            // Rust limitations never count toward the Python total.
            finding(
                Some(LanguageId::Rust),
                ExposureClass::StaticUnknown,
                Some(StaticLimitKind::DynamicDispatch),
            ),
        ];
        let first_use = PilotPythonFirstUse::from_check_output(&check_output(findings));
        assert_eq!(first_use.status, PilotPythonFirstUseStatus::NoRepairCards);
        assert_eq!(first_use.findings_total, 6);
        assert_eq!(first_use.limitation_count, 4);
        assert_eq!(first_use.repair_cards_total, 0);
    }
}
