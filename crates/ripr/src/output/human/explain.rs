//! The section `ripr explain` adds after the finding block (#5356).
//!
//! `check --format human-full` shows a window of the evidence. `explain` is
//! the drill-in, so it lists every retained test ripr examined with what it
//! concluded about each one, says what a test would need to change the
//! verdict, and spells out each stop reason.

use crate::domain::{
    ExposureClass, Finding, OracleStrength, ProbeFamily, RelatedTest, RelatedTestMiss, StopReason,
    exact_assertion_fact, input_boundary_fact,
};
use crate::output::path::display_path;
use crate::output::related_test_miss::{checked_assertion_text, related_test_miss_reason};

pub(crate) fn render_verdict_explanation(
    finding: &Finding,
    owner_signature: Option<&str>,
) -> String {
    let mut out = String::from("\nWhy this verdict\n");
    let total = finding.related_tests_total();
    if finding.related_tests.is_empty() {
        out.push_str("  ripr found no test related to this change.\n");
    } else {
        // `related_tests` holds one row per matched assertion, so a test can
        // appear more than once; print each test once with every assertion
        // it was judged by.
        let groups = group_rows_by_test(&finding.related_tests);
        out.push_str(&format!(
            "  Tests examined: {} listed ({} assertion row(s) of {total})\n",
            groups.len(),
            finding.related_tests.len()
        ));
        for rows in groups {
            // The test is judged by its strongest assertion; a weaker row's
            // reason must not speak for a test that also asserts exactly.
            let lead = rows
                .iter()
                .copied()
                .min_by_key(|row| strength_order(&row.oracle_strength))
                .unwrap_or(rows[0]);
            let lead_verdict = row_verdict(lead, finding);
            // #8746: when the lead row's assertion text was not recorded but
            // another row of the same test printed one below, the bare
            // "assertion not recorded" fallback is suppressed — it would
            // contradict the `checked:` lines the reader is about to see.
            let group_printed_an_assertion = rows.iter().any(|row| row.oracle.is_some());
            if lead_verdict == ASSERTION_NOT_RECORDED && group_printed_an_assertion {
                out.push_str(&format!(
                    "  - {}:{} {}\n",
                    display_path(&lead.file),
                    lead.line,
                    lead.name
                ));
            } else {
                out.push_str(&format!(
                    "  - {}:{} {}: {lead_verdict}\n",
                    display_path(&lead.file),
                    lead.line,
                    lead.name
                ));
            }
            for row in &rows {
                if let Some(oracle) = &row.oracle {
                    let verdict = row_verdict(row, finding);
                    let note = if verdict == lead_verdict {
                        String::new()
                    } else {
                        format!(" ({verdict})")
                    };
                    out.push_str(&format!(
                        "      checked: {}{note}\n",
                        checked_assertion_text(oracle)
                    ));
                }
            }
        }
        if total > finding.related_tests.len() {
            out.push_str(&format!(
                "  ({} more row(s) examined; ripr keeps the {} most closely related)\n",
                total - finding.related_tests.len(),
                finding.related_tests.len()
            ));
        }
    }
    let needs = verdict_changers(finding, owner_signature);
    if !needs.is_empty() {
        out.push_str("  To change the verdict, a test needs to:\n");
        for need in needs {
            out.push_str(&format!("  - {need}\n"));
        }
    }
    if matches!(
        finding.class,
        ExposureClass::InfectionUnknown
            | ExposureClass::PropagationUnknown
            | ExposureClass::StaticUnknown
    ) {
        let step = crate::output::next_step::reconcile_next_step(finding);
        if !step.is_empty() {
            out.push_str("  To investigate this uncertainty:\n");
            out.push_str(&format!("  - {step}\n"));
        }
    }
    if !finding.stop_reasons.is_empty() {
        out.push_str("  Stop reasons:\n");
        for reason in &finding.stop_reasons {
            out.push_str(&format!(
                "  - {}: {}\n",
                reason.as_str(),
                stop_reason_meaning(reason)
            ));
        }
    }
    out
}

/// What ripr concluded about one assertion row.
fn row_verdict(row: &RelatedTest, finding: &Finding) -> String {
    match related_test_miss_reason(row, &finding.activation.missing_discriminators) {
        Some(why) => format!(
            "{}: {why}",
            crate::output::related_test_miss::related_test_miss_label(row)
        ),
        None => match &row.oracle {
            Some(_) => format!(
                "{} {} oracle",
                row.oracle_strength.as_str(),
                row.oracle_kind.as_str().replace('_', " ")
            ),
            // #8746: the row is a retention fact, not evidence the test
            // lacks an oracle — the test may well assert something ripr
            // simply did not keep. Name the limit in user vocabulary
            // instead of the producer's "no oracle row" record shape.
            None => ASSERTION_NOT_RECORDED.to_string(),
        },
    }
}

/// #8746: user-facing label for a row whose assertion text ripr did not
/// retain. Shared so the render loop can recognize and suppress the bare
/// fallback when the same test's other rows already printed an assertion.
const ASSERTION_NOT_RECORDED: &str = "assertion not recorded";

/// Strongest first, so the minimum is the assertion a test is judged by.
fn strength_order(strength: &OracleStrength) -> u8 {
    match strength {
        OracleStrength::Strong => 0,
        OracleStrength::Medium => 1,
        OracleStrength::Weak => 2,
        OracleStrength::Smoke => 3,
        OracleStrength::Unknown => 4,
        OracleStrength::None => 5,
    }
}

/// Rows of the same test (name, file, line), in first-seen order.
fn group_rows_by_test(rows: &[RelatedTest]) -> Vec<Vec<&RelatedTest>> {
    let mut groups: Vec<Vec<&RelatedTest>> = Vec::new();
    for row in rows {
        match groups.iter_mut().find(|group| {
            group[0].name == row.name && group[0].file == row.file && group[0].line == row.line
        }) {
            Some(group) => group.push(row),
            None => groups.push(vec![row]),
        }
    }
    groups
}

/// What a test would have to do for ripr to see a discriminator, one line per
/// distinct miss. Only gap classes with a repair step get this list: for
/// `exposed` there is nothing to change, for the unknown classes ripr has not
/// established what is missing, and a gap whose next step was withheld (an
/// exact oracle already covers the direct sink) indicates no assertion repair.
fn verdict_changers(finding: &Finding, owner_signature: Option<&str>) -> Vec<String> {
    if !matches!(
        finding.class,
        ExposureClass::NoStaticPath
            | ExposureClass::ReachableUnrevealed
            | ExposureClass::WeaklyExposed
    ) || finding.recommended_next_step.is_none()
    {
        return Vec::new();
    }
    // #7796: the recipe names the owner; when the observed call shape is
    // available (the same producer `ripr agent stub` reads), the name carries
    // its signature so the user is not sent to the source to learn how to
    // call it. This is observed source data, not a synthesized assertion.
    let owner = owner_signature
        .map(|signature| format!("`{signature}`"))
        .or_else(|| {
            finding
                .probe
                .owner
                .as_ref()
                .and_then(|owner| owner.0.rsplit("::").next().map(str::to_string))
                .filter(|name| !name.is_empty())
                .map(|name| format!("`{name}`"))
        })
        .unwrap_or_else(|| "the changed code".to_string());
    let mut needs: Vec<String> = Vec::new();
    let mut push = |need: String| {
        if !needs.contains(&need) {
            needs.push(need);
        }
    };
    if finding.related_tests.is_empty() {
        push(format!("call {owner} and assert on its result"));
    }
    for test in &finding.related_tests {
        let Some(miss) = test.miss else { continue };
        push(match miss {
            RelatedTestMiss::NoCallPath => format!("call {owner}, directly or through a helper"),
            RelatedTestMiss::NoAssertion | RelatedTestMiss::AssertionNotObserving => {
                format!("assert on what {owner} returns or changes")
            }
            RelatedTestMiss::AssertionNotCredited => {
                "use a plain, always-run `assert_eq!`/`assert!` ripr can resolve".to_string()
            }
            RelatedTestMiss::ObservationUnconfirmed => {
                "assert on the changed value itself, by name".to_string()
            }
            RelatedTestMiss::WeakAssertion => {
                "assert the exact value, not only success or presence".to_string()
            }
            // These needs are added once below, from the finding itself.
            RelatedTestMiss::MissingInput | RelatedTestMiss::MissingExactAssertion => continue,
        });
    }
    // A missing discriminator is a need whether or not any listed test got
    // far enough to be judged on it. A predicate boundary is an input a test
    // must use, as is an unselected match arm (RIPR-SPEC-0229) or an
    // unentered guard (RIPR-SPEC-0263); an error
    // variant or field value is an assertion it must make.
    let facts = &finding.activation.missing_discriminators;
    if let Some(fact) = input_boundary_fact(facts, &finding.probe.family) {
        push(match finding.probe.family {
            ProbeFamily::MatchArm => format!("use an input that selects arm `{} =>`", fact.value),
            // RIPR-SPEC-0263: the value is the guard, `if x > 10`.
            ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
                format!("use an input that enters `{}`", fact.value)
            }
            _ => format!("use an input that reaches `{}`", fact.value),
        });
    }
    if let Some(fact) = exact_assertion_fact(facts, &finding.probe.family) {
        push(format!("assert the exact `{}`", fact.value));
    }
    // #8744: when the only flow-sink evidence says ripr could not classify
    // where the changed value ends up, a recipe that names what the owner
    // "returns or changes" asserts a target shape the evidence says is
    // unclassified. Keep the recipes — they remain the best static guess —
    // but qualify them so they do not read as settled targets.
    if !finding.flow_sinks.is_empty()
        && finding
            .flow_sinks
            .iter()
            .all(|sink| sink.kind == crate::domain::FlowSinkKind::Unknown)
    {
        push(
            "treat the assertion target as provisional: ripr could not classify \
             where the changed value ends up"
                .to_string(),
        );
    }
    needs
}

fn stop_reason_meaning(reason: &StopReason) -> &'static str {
    match reason {
        StopReason::MaxDepthReached => "the call walk hit its depth limit before reaching a test",
        StopReason::ExternalCrateBoundary => {
            "the path leaves this workspace; ripr does not follow it"
        }
        StopReason::DynamicDispatchUnresolved => {
            "the path goes through a trait object or generic call ripr cannot resolve"
        }
        StopReason::ProcMacroOpaque | StopReason::FixtureOpaque => reason.describe(),
        StopReason::FeatureUnknown => "a cfg or feature gate may change whether the code runs",
        StopReason::AsyncBoundaryOpaque | StopReason::NoChangedRustLine => reason.describe(),
        StopReason::InfectionEvidenceUnknown => {
            "ripr could not tell whether a test input changes this value"
        }
        StopReason::PropagationEvidenceUnknown => {
            "ripr could not trace the changed value to anything a test observes"
        }
        StopReason::StaticProbeUnknown => "ripr has no probe shape for this kind of change",
        StopReason::TransitiveReachUnresolved => {
            "a test may reach this through a chain of internal calls ripr could not finish walking"
        }
        StopReason::MacroReachUnresolved => {
            "a test may reach this through a macro ripr does not expand"
        }
        StopReason::GapEvidenceUnresolved => {
            "a related test asserts, but ripr could not tie that assertion to this change"
        }
        // ADR 0019: the domain gloss is the stop-reason owner. Do not invent
        // a second identity-unknown sentence here.
        StopReason::HelperIdentityUnresolved => reason.describe(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MissingDiscriminatorFact, OracleKind};
    use crate::output::perl_preview_card::tests::sample_perl_finding;
    use std::path::PathBuf;

    fn row(oracle: &str, strength: OracleStrength, miss: RelatedTestMiss) -> RelatedTest {
        RelatedTest {
            name: "parses_empty".to_string(),
            file: PathBuf::from("tests/parse.rs"),
            line: 7,
            oracle: Some(oracle.to_string()),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: strength,
            relation_reason: None,
            relation_confidence: None,
            miss: Some(miss),
        }
    }

    fn unknown_sink() -> crate::domain::FlowSinkFact {
        crate::domain::FlowSinkFact {
            kind: crate::domain::FlowSinkKind::Unknown,
            text: "unknown sink".to_string(),
            line: 3,
            owner: None,
        }
    }

    /// A matched row (no miss reason), with assertion retention controlled by
    /// the caller — `oracle: None` is the retention fact #8746 glosses.
    fn matched_row(oracle: Option<&str>, strength: OracleStrength) -> RelatedTest {
        RelatedTest {
            name: "parses_empty".to_string(),
            file: PathBuf::from("tests/parse.rs"),
            line: 7,
            oracle: oracle.map(str::to_string),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: strength,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
        }
    }

    #[test]
    fn verdict_change_recipe_carries_the_observed_call_shape_when_known() {
        let mut finding = sample_perl_finding();
        finding.class = ExposureClass::NoStaticPath;
        finding.recommended_next_step = Some("add a test that calls the owner".to_string());
        finding.related_tests = Vec::new();

        let with_shape = render_verdict_explanation(&finding, Some("greet(name: &str) -> String"));
        assert!(
            with_shape
                .contains("  - call `greet(name: &str) -> String` and assert on its result\n"),
            "{with_shape}"
        );

        let without_shape = render_verdict_explanation(&finding, None);
        assert!(
            without_shape.contains("  To change the verdict, a test needs to:\n"),
            "{without_shape}"
        );
    }

    #[test]
    fn a_test_is_judged_by_its_strongest_assertion_not_its_first_row() {
        let mut finding = sample_perl_finding();
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "len == 0".to_string(),
            reason: "no related test passes an empty input".to_string(),
            flow_sink: None,
        }];
        finding.related_tests = vec![
            row(
                "assert!(parse(\"a\").is_ok());",
                OracleStrength::Weak,
                RelatedTestMiss::WeakAssertion,
            ),
            row(
                "assert_eq!(parse(\"a\"), Ok(1));",
                OracleStrength::Strong,
                RelatedTestMiss::MissingInput,
            ),
        ];
        let text = render_verdict_explanation(&finding, None);
        assert!(
            text.contains(
                "  - tests/parse.rs:7 parses_empty: misses: no test input reaches `len == 0`\n"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "      checked: assert!(parse(\"a\").is_ok()) (misses: assertion too weak to tell the old behavior from the new)\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("      checked: assert_eq!(parse(\"a\"), Ok(1))\n"),
            "{text}"
        );
    }
    #[test]
    fn an_unselected_arm_is_an_input_to_select_not_an_assertion_to_make() {
        let mut finding = sample_perl_finding();
        finding.probe.family = ProbeFamily::MatchArm;
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "Kind::Beta".to_string(),
            reason: "No related test call selects arm `Kind::Beta =>`; observed `k` values: `Kind::Alpha`"
                .to_string(),
            flow_sink: None,
        }];
        let text = render_verdict_explanation(&finding, None);
        assert!(
            text.contains("use an input that selects arm `Kind::Beta =>`"),
            "{text}"
        );
        assert!(!text.contains("assert the exact `Kind::Beta`"), "{text}");
    }

    #[test]
    fn helper_identity_unresolved_explain_uses_domain_describe() {
        let reason = StopReason::HelperIdentityUnresolved;
        assert_eq!(stop_reason_meaning(&reason), reason.describe());
        assert!(
            reason.describe().contains("workspace function")
                && reason.describe().contains("not unique"),
            "{}",
            reason.describe()
        );
        assert!(
            !reason.describe().contains("probe"),
            "{}",
            reason.describe()
        );
        assert_ne!(
            stop_reason_meaning(&StopReason::StaticProbeUnknown),
            reason.describe()
        );
    }

    /// #5508: a Perl row whose observation ripr could not confirm is an
    /// unknown, so the explanation does not introduce it as a miss.
    #[test]
    fn an_unconfirmed_observation_is_not_labelled_a_miss() {
        let mut finding = sample_perl_finding();
        finding.related_tests = vec![row(
            "is(My::App::discount(100), 10, 'discount threshold')",
            OracleStrength::Strong,
            RelatedTestMiss::ObservationUnconfirmed,
        )];
        let text = render_verdict_explanation(&finding, None);
        assert!(
            text.contains(
                "  - tests/parse.rs:7 parses_empty: unconfirmed: ripr could not confirm that this assertion observes the changed behavior\n"
            ),
            "{text}"
        );
        assert!(!text.contains("misses: ripr could not confirm"), "{text}");
    }

    #[test]
    fn unknown_sink_only_recipe_is_qualified_as_provisional() {
        // #8744: when the only flow-sink evidence says the sink is
        // unclassified, the verdict-change recipe must not read as a settled
        // assertion target.
        let mut finding = sample_perl_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.recommended_next_step = Some("add an exact assertion".to_string());
        finding.related_tests = Vec::new();
        finding.flow_sinks = vec![unknown_sink()];

        let text = render_verdict_explanation(&finding, None);

        assert!(
            text.contains("treat the assertion target as provisional"),
            "{text}"
        );
    }

    #[test]
    fn classified_sink_recipe_is_not_qualified() {
        let mut finding = sample_perl_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.recommended_next_step = Some("add an exact assertion".to_string());
        finding.related_tests = Vec::new();
        finding.flow_sinks = vec![crate::domain::FlowSinkFact {
            kind: crate::domain::FlowSinkKind::ReturnValue,
            text: "amount - 10".to_string(),
            line: 3,
            owner: None,
        }];

        let text = render_verdict_explanation(&finding, None);

        assert!(!text.contains("provisional"), "{text}");
    }

    #[test]
    fn unrecorded_assertion_row_names_the_retention_limit() {
        // #8746: "no oracle row" is producer vocabulary and under-describes
        // the situation — the test may assert something ripr did not keep.
        let mut finding = sample_perl_finding();
        finding.related_tests = vec![matched_row(None, OracleStrength::Strong)];

        let text = render_verdict_explanation(&finding, None);

        assert!(
            text.contains("  - tests/parse.rs:7 parses_empty: assertion not recorded\n"),
            "{text}"
        );
        assert!(!text.contains("no oracle row"), "{text}");
    }

    #[test]
    fn bare_fallback_suppressed_when_sibling_row_printed_an_assertion() {
        // #8746: when the same test's other rows printed an assertion, the
        // bare fallback contradicts them and is suppressed entirely.
        let mut finding = sample_perl_finding();
        finding.related_tests = vec![
            matched_row(None, OracleStrength::Strong),
            matched_row(
                Some("assert_eq!(parse(\"a\"), Ok(1))"),
                OracleStrength::Weak,
            ),
        ];

        let text = render_verdict_explanation(&finding, None);

        assert!(
            text.contains("  - tests/parse.rs:7 parses_empty\n"),
            "{text}"
        );
        assert!(!text.contains("assertion not recorded"), "{text}");
        // The sibling row keeps its own verdict as a note, since the
        // suppressed lead no longer states one.
        assert!(
            text.contains(
                "      checked: assert_eq!(parse(\"a\"), Ok(1)) (weak exact value oracle)\n"
            ),
            "{text}"
        );
    }
}
