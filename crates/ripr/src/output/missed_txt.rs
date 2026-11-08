//! `ripr check --format missed-txt` (#13626): one sorted plain-text line per
//! actionable static gap, the shape a `cargo mutants` `missed.txt` reader
//! expects, in `ripr`'s static vocabulary.
//!
//! The selection mirrors the `github` renderer so no second classifier
//! exists: a finding is listed only when it is candidate-actionable
//! (`Finding::is_candidate_actionable`), is not suppressed by an explicit
//! `--suppression-policy`, is not severity-`off` for its file, and its class
//! is not `exposed`. The class is the shared owner's wording verbatim; this
//! module claims no runtime mutation outcome.
//!
//! Line shape:
//!
//! ```text
//! path:line:col: <classification> <probe family>: <expression> -> <missing discriminator>
//! ```
//!
//! A leading `# ` line is a comment, not a gap: it discloses a run that did
//! not complete, because an empty stream must not read as clean then.

use crate::app::CheckOutput;
use crate::config::RiprConfig;
use crate::domain::{ExposureClass, Finding, MISSING_DISCRIMINATOR_VALUE_PREFIX};
use crate::output::path::repository_display_path;

/// Render the gap lines for a computed check.
pub(crate) fn render_with_config(output: &CheckOutput, config: &RiprConfig) -> String {
    let suppressed_ids: std::collections::BTreeSet<&str> = output
        .suppression
        .iter()
        .flat_map(|outcome| {
            outcome
                .suppressed
                .iter()
                .map(|entry| entry.finding_id.as_str())
        })
        .collect();
    let mut rows: Vec<GapRow> = output
        .findings
        .iter()
        .filter(|finding| !suppressed_ids.contains(finding.id.as_str()))
        .filter(|finding| finding.class != ExposureClass::Exposed)
        .filter(|finding| finding.is_candidate_actionable())
        .filter(|finding| {
            config
                .severity_for_file(&finding.probe.location.file)
                .for_exposure(&finding.class)
                .github_annotation_level()
                .is_some()
        })
        .map(|finding| GapRow::new(output, finding))
        .collect();
    rows.sort();
    let mut out = String::new();
    if let Some(outcome) = output
        .analysis_outcome
        .as_ref()
        .filter(|outcome| !outcome.kind.is_complete())
    {
        out.push_str(&format!(
            "# ripr: analysis outcome {}; listed gaps cover only the analyzed scope, and no line is not a clean result\n",
            outcome.kind.as_str()
        ));
    }
    for row in rows {
        out.push_str(&row.line());
        out.push('\n');
    }
    out
}

/// Render a refusal as one comment line so the stream is never zero bytes
/// (an empty stream would read as "no gaps") yet no refusal can parse as a
/// `path:line:col:` gap.
pub(crate) fn render_refusal(notice: &crate::core_error::RefusalNotice) -> String {
    let text = format!(
        "run refused: run_status={}; repair_route={}; {}",
        notice.identity, notice.repair_route, notice.message
    );
    format!("# ripr {}\n", single_line(&text))
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GapRow {
    path: String,
    line: usize,
    column: usize,
    class: &'static str,
    family: &'static str,
    expression: String,
    missing: String,
}

impl GapRow {
    fn new(output: &CheckOutput, finding: &Finding) -> Self {
        let missing = finding
            .activation
            .missing_discriminators
            .first()
            .map(|fact| fact.value.as_str())
            .or_else(|| {
                finding.missing.first().map(|text| {
                    text.strip_prefix(MISSING_DISCRIMINATOR_VALUE_PREFIX)
                        .unwrap_or(text)
                })
            })
            .unwrap_or("-");
        Self {
            path: single_line(&repository_display_path(
                &output.root,
                &finding.probe.location.file,
            )),
            line: finding.probe.location.line,
            column: finding.probe.location.column,
            class: finding.class.as_str(),
            family: finding.probe.family.as_str(),
            expression: single_line(&finding.probe.expression),
            missing: single_line(missing),
        }
    }

    fn line(&self) -> String {
        format!(
            "{}:{}:{}: {} {}: {} -> {}",
            self.path,
            self.line,
            self.column,
            self.class,
            self.family,
            or_dash(&self.expression),
            or_dash(&self.missing)
        )
    }
}

fn or_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}

/// Collapse whitespace to single spaces and escape control and bidi
/// characters, so one record is always exactly one terminal-safe line.
fn single_line(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::output::human::terminal_safe(collapsed)
}

#[cfg(test)]
mod tests {
    use super::{render_refusal, render_with_config};
    use crate::app::{CheckOutput, Mode};
    use crate::config::RiprConfig;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, DiscriminatorShape, ExposureClass, Finding,
        MissingDiscriminatorFact, Probe, ProbeFamily, ProbeId, RevealEvidence, RiprEvidence,
        SourceCurrentness, SourceLocation, StageEvidence, StageState, Summary,
    };
    use crate::output::suppressions::{CheckSuppressionOutcome, SuppressedCheckFinding};
    use std::path::PathBuf;

    fn finding(
        id: &str,
        file: &str,
        line: usize,
        column: usize,
        class: ExposureClass,
        family: ProbeFamily,
        expression: &str,
    ) -> Finding {
        let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "s");
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: id.to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId(id.to_string()),
                family,
                location: SourceLocation::new(file, line, column),
                owner: None,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: expression.to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class,
            ripr: RiprEvidence {
                reach: stage(),
                infect: stage(),
                propagate: stage(),
                reveal: RevealEvidence {
                    observe: stage(),
                    discriminate: stage(),
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
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    fn with_discriminator(mut finding: Finding, value: &str) -> Finding {
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: value.to_string(),
            reason: "r".to_string(),
            flow_sink: None,
            drafted_boundary: None,
            shape: DiscriminatorShape::Boundary,
        }];
        finding
    }

    fn output(findings: Vec<Finding>) -> CheckOutput {
        CheckOutput {
            value_origins: Default::default(),
            harness_projections: Vec::new(),
            schema_version: "0.2".to_string(),
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

    fn fixture() -> Vec<Finding> {
        vec![
            finding(
                "b",
                "src/zeta.rs",
                9,
                5,
                ExposureClass::WeaklyExposed,
                ProbeFamily::Predicate,
                "amount  >=\n threshold",
            ),
            with_discriminator(
                finding(
                    "a",
                    "src/alpha.rs",
                    30,
                    1,
                    ExposureClass::ReachableUnrevealed,
                    ProbeFamily::ReturnValue,
                    "Ok(total)",
                ),
                "total == 0",
            ),
            finding(
                "c",
                "src/alpha.rs",
                4,
                2,
                ExposureClass::Exposed,
                ProbeFamily::Predicate,
                "covered",
            ),
            finding(
                "d",
                "src/alpha.rs",
                4,
                7,
                ExposureClass::NoStaticPath,
                ProbeFamily::ErrorPath,
                "Err(e)",
            ),
        ]
    }

    #[test]
    fn lists_sorted_gap_lines_and_omits_exposed() {
        let mut findings = fixture();
        findings[0].missing = vec!["strong oracle".to_string()];
        let rendered = render_with_config(&output(findings), &RiprConfig::default());
        assert_eq!(
            rendered,
            "src/alpha.rs:4:7: no_static_path error_path: Err(e) -> -\n\
             src/alpha.rs:30:1: reachable_unrevealed return_value: Ok(total) -> total == 0\n\
             src/zeta.rs:9:5: weakly_exposed predicate: amount >= threshold -> strong oracle\n"
        );
    }

    #[test]
    fn ordering_is_independent_of_finding_order() {
        let forward = render_with_config(&output(fixture()), &RiprConfig::default());
        let mut reversed = fixture();
        reversed.reverse();
        let backward = render_with_config(&output(reversed), &RiprConfig::default());
        assert_eq!(forward, backward);
        assert_eq!(forward.lines().count(), 3);
    }

    #[test]
    fn exposed_only_and_empty_runs_print_nothing() {
        let exposed = vec![finding(
            "c",
            "src/lib.rs",
            1,
            1,
            ExposureClass::Exposed,
            ProbeFamily::Predicate,
            "x",
        )];
        assert_eq!(
            render_with_config(&output(exposed), &RiprConfig::default()),
            ""
        );
        assert_eq!(
            render_with_config(&output(Vec::new()), &RiprConfig::default()),
            ""
        );
    }

    #[test]
    fn suppressed_and_non_current_findings_are_not_listed() {
        let mut out = output(fixture());
        out.suppression = Some(CheckSuppressionOutcome {
            policy_path: "p.toml".to_string(),
            suppressed: vec![SuppressedCheckFinding {
                finding_id: "d".to_string(),
                selector: "src/**".to_string(),
            }],
            warnings: Vec::new(),
        });
        out.findings[1].source_currentness = SourceCurrentness::BaseDeleted;
        let rendered = render_with_config(&out, &RiprConfig::default());
        assert_eq!(
            rendered,
            "src/zeta.rs:9:5: weakly_exposed predicate: amount >= threshold -> -\n"
        );
    }

    #[test]
    fn output_carries_no_runtime_mutation_claims() {
        let rendered = render_with_config(&output(fixture()), &RiprConfig::default());
        let lowered = rendered.to_ascii_lowercase();
        assert!(!lowered.contains("missed"), "{rendered}");
        assert!(!lowered.contains("caught"), "{rendered}");
    }

    #[test]
    fn refusal_is_one_comment_line_never_a_gap() {
        let notice = crate::core_error::CoreError::repository_root_unusable("boom")
            .refusal_notice(&crate::config::config_error_summary);
        assert_eq!(
            render_refusal(&notice),
            "# ripr run refused: run_status=repository_root_unusable; repair_route=analysis/repository-root; boom\n"
        );
    }
}
