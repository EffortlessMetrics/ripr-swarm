use crate::domain::{Finding, MISSING_DISCRIMINATOR_VALUE_PREFIX};
use crate::output::path::display_path;
use crate::output::preview_actionability::{
    is_preview_actionability_evidence_line, is_preview_actionability_missing_summary,
};

/// #4320: the human evidence window shows at most this many related tests per
/// finding (`--format json` carries more plus `related_tests_total`). Number of
/// reaching tests is core exposure evidence, so a window that does not say it
/// is a window reads as the whole evidence.
const MAX_RELATED_TESTS_SHOWN: usize = 5;

/// #4320: the human evidence window shows at most this many observed values
/// per finding (`--format json` keeps its own ranked 32-value window and
/// discloses the pre-cap count as `observed_values_total`). Observed values
/// are the raw material for writing the missing-discriminator test, so an
/// unmarked cap can hide the one boundary value the reader needs.
const MAX_OBSERVED_VALUES_SHOWN: usize = 8;

/// #7476: which wording the evidence-window truncation pointers use for the
/// fuller machine-readable list. `ripr check` accepts `--format json`, so its
/// pointer names the flag; `ripr explain` rejects that flag, so its pointer
/// names check in prose instead of a command the reader cannot run there.
/// The wording is selected while the lines are built, so quoted source
/// fragments and evidence text are never rewritten after rendering.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonPointerWording {
    /// `ripr check` / `human-full`: the pointer names the accepted flag.
    CheckFlag,
    /// `ripr explain`: the pointer names `ripr check` as prose, so a
    /// printed-command paste does not try to run it under explain.
    ExplainNamesCheck,
}

/// Exact producer facts accompany the existing display projection. Window
/// disclosures stay local. Serialization is used only by CLI sharing; legacy
/// rendering does not pay for a second projection or serialized finding copies.
#[derive(serde::Serialize)]
enum EvidenceSource<'a> {
    Stage(&'static str, &'a crate::domain::StageEvidence),
    Discriminator(
        &'a crate::domain::StageEvidence,
        &'a crate::domain::ExposureClass,
        &'a [crate::domain::MissingDiscriminatorFact],
    ),
    Flow(&'a crate::domain::FlowSinkFact),
    Related(
        &'a crate::domain::RelatedTest,
        &'a [crate::domain::MissingDiscriminatorFact],
    ),
    Value(&'a crate::domain::ValueFact),
    Literal(&'a str),
    Window,
}

pub(super) struct EvidenceEntry {
    pub text: String,
    pub key: Option<Vec<u8>>,
}

pub(super) fn evidence_path_lines(finding: &Finding, wording: JsonPointerWording) -> Vec<String> {
    evidence_path(finding, wording, |text, _| text)
}

pub(super) fn evidence_path_entries(finding: &Finding, with_keys: bool) -> Vec<EvidenceEntry> {
    evidence_path(finding, JsonPointerWording::CheckFlag, |text, source| {
        // Multi-line literals retain their existing rendering locally and cannot
        // forge a catalogue definition or generated reference.
        let key = if !with_keys
            || matches!(source, EvidenceSource::Window)
            || text.contains(['\n', '\r'])
        {
            None
        } else {
            serde_json::to_vec(&(source, &text)).ok()
        };
        EvidenceEntry { text, key }
    })
}

fn evidence_path<T>(
    finding: &Finding,
    wording: JsonPointerWording,
    mut project: impl FnMut(String, EvidenceSource<'_>) -> T,
) -> Vec<T> {
    let mut lines = vec![
        project(
            format!(
                "reach {}: {}",
                finding.ripr.reach.state.as_str(),
                finding.ripr.reach.summary
            ),
            EvidenceSource::Stage("reach", &finding.ripr.reach),
        ),
        project(
            format!(
                "infection {}: {}",
                finding.ripr.infect.state.as_str(),
                finding.ripr.infect.summary
            ),
            EvidenceSource::Stage("infection", &finding.ripr.infect),
        ),
        project(
            format!(
                "propagation {}: {}",
                finding.ripr.propagate.state.as_str(),
                finding.ripr.propagate.summary
            ),
            EvidenceSource::Stage("propagation", &finding.ripr.propagate),
        ),
        project(
            format!(
                "observation {}: {}",
                finding.ripr.reveal.observe.state.as_str(),
                finding.ripr.reveal.observe.summary
            ),
            EvidenceSource::Stage("observation", &finding.ripr.reveal.observe),
        ),
        project(
            crate::output::discriminator_line::discriminator_evidence_line(finding),
            EvidenceSource::Discriminator(
                &finding.ripr.reveal.discriminate,
                &finding.class,
                &finding.activation.missing_discriminators,
            ),
        ),
    ];

    for sink in &finding.flow_sinks {
        lines.push(project(
            crate::output::flow_sink_line::flow_sink_evidence_line(sink),
            EvidenceSource::Flow(sink),
        ));
    }

    for test in finding.related_tests.iter().take(MAX_RELATED_TESTS_SHOWN) {
        let why = crate::output::related_test_miss::related_test_miss_reason(
            test,
            &finding.activation.missing_discriminators,
        );
        if let Some(why) = why.as_deref().filter(|_| test.is_unmatched()) {
            // #5344: an examined test with no matched oracle says why it
            // misses, then shows the assertion it was judged by, so the
            // claim can be checked in the source.
            let mut line = format!(
                "related test {}:{} {} {}: {why}",
                display_path(&test.file),
                test.line,
                test.name,
                crate::output::related_test_miss::related_test_miss_label(test),
            );
            if let Some(oracle) = &test.oracle {
                line.push_str(&format!(
                    "; checked `{}`",
                    crate::output::related_test_miss::checked_assertion_text(oracle)
                ));
            }
            lines.push(project(
                line,
                EvidenceSource::Related(
                    test,
                    match test.miss {
                        Some(
                            crate::domain::RelatedTestMiss::MissingInput
                            | crate::domain::RelatedTestMiss::MissingExactAssertion,
                        ) => &finding.activation.missing_discriminators,
                        _ => &[],
                    },
                ),
            ));
            continue;
        }
        let mut line = format!(
            "related test {}:{} {} uses {}",
            display_path(&test.file),
            test.line,
            test.name,
            crate::output::related_test_miss::oracle_projection(
                &test.oracle_strength,
                &test.oracle_kind
            )
        );
        if let Some(oracle) = &test.oracle {
            line.push_str(&format!(": {oracle}"));
        }
        // A matched row keeps its oracle projection and adds the reason it
        // still misses, so the oracle kind and strength stay readable.
        if let Some(why) = why {
            let kept = line.trim_end_matches(';').len();
            line.truncate(kept);
            line.push_str(&format!(
                "; {}: {why}",
                crate::output::related_test_miss::related_test_miss_label(test)
            ));
        }
        lines.push(project(
            line,
            EvidenceSource::Related(
                test,
                match test.miss {
                    Some(
                        crate::domain::RelatedTestMiss::MissingInput
                        | crate::domain::RelatedTestMiss::MissingExactAssertion,
                    ) => &finding.activation.missing_discriminators,
                    _ => &[],
                },
            ),
        ));
    }
    let related_tests_total = finding.related_tests_total();
    let related_tests_shown = finding.related_tests.len().min(MAX_RELATED_TESTS_SHOWN);
    if related_tests_total > related_tests_shown {
        let json_pointer = if finding.related_tests.len() > related_tests_shown {
            match wording {
                JsonPointerWording::CheckFlag => "; more in --format json",
                JsonPointerWording::ExplainNamesCheck => {
                    "; more via ripr check, using its JSON format"
                }
            }
        } else {
            ""
        };
        lines.push(project(format!(
            "related tests (showing {related_tests_shown} of {related_tests_total}{json_pointer})"
        ), EvidenceSource::Window));
    }

    let observed_values_total = finding.activation.observed_values.len();
    let shown_values = finding
        .activation
        .observed_values
        .iter()
        .take(MAX_OBSERVED_VALUES_SHOWN)
        .collect::<Vec<_>>();
    // #8389: a bare `at line N` is only ambiguous when the shown window mixes
    // facts from more than one test file; then each line names its file.
    let name_files = shown_values
        .iter()
        .filter_map(|value| value.file.as_deref())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1;
    for value in shown_values {
        lines.push(project(
            if name_files {
                crate::output::observed_values::source_value_evidence_line_with_file(value)
            } else {
                crate::output::observed_values::source_value_evidence_line(value)
            },
            EvidenceSource::Value(value),
        ));
    }
    if observed_values_total > MAX_OBSERVED_VALUES_SHOWN {
        // #4320 review: the pointer must not promise more than the target
        // carries. `--format json` holds every value only up to its own
        // ranked cap of 32; beyond that, both formats are windows and the
        // wording says so instead of claiming a full list.
        let json_cap = crate::output::observed_values::MAX_OBSERVED_VALUES_PER_FINDING;
        let json_window = match wording {
            JsonPointerWording::CheckFlag => {
                if observed_values_total > json_cap {
                    format!("--format json keeps a ranked {json_cap}")
                } else {
                    "full list in --format json".to_string()
                }
            }
            JsonPointerWording::ExplainNamesCheck => {
                if observed_values_total > json_cap {
                    format!("ripr check, using its JSON format, keeps a ranked {json_cap}")
                } else {
                    "full list via ripr check, using its JSON format".to_string()
                }
            }
        };
        lines.push(project(format!(
            "source values (showing {MAX_OBSERVED_VALUES_SHOWN} of {observed_values_total} read from related tests; {json_window})"
        ), EvidenceSource::Window));
    }

    if lines.len() == 5 && !finding.evidence.is_empty() {
        lines.extend(
            finding
                .evidence
                .iter()
                .filter(|line| !is_preview_actionability_evidence_line(line))
                .map(|line| project(line.clone(), EvidenceSource::Literal(line))),
        );
    }

    lines
}

pub(super) fn weakness_lines(finding: &Finding) -> Vec<String> {
    let discriminator_values: Vec<&str> = finding
        .activation
        .missing_discriminators
        .iter()
        .map(|fact| fact.value.as_str())
        .collect();
    let mut lines = finding
        .missing
        .iter()
        .filter(|missing| {
            !is_duplicate_discriminator_missing(missing, &discriminator_values)
                && !is_preview_actionability_missing_summary(missing)
        })
        .cloned()
        .collect::<Vec<_>>();
    for discriminator in &finding.activation.missing_discriminators {
        lines.push(format!(
            "missing discriminator {}: {}",
            discriminator.value, discriminator.reason
        ));
    }
    lines
}

fn is_duplicate_discriminator_missing(missing: &str, discriminator_values: &[&str]) -> bool {
    let Some(value) = missing.strip_prefix(MISSING_DISCRIMINATOR_VALUE_PREFIX) else {
        return false;
    };
    discriminator_values.contains(&value)
}
