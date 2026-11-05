//! The section `ripr explain` adds after the finding block (#5356).
//!
//! `check --format human-full` shows a window of the evidence. `explain` is
//! the drill-in, so it lists every retained test ripr examined with what it
//! concluded about each one, says what a test would need to change the
//! verdict, and spells out each stop reason.
//!
//! Each fact renders exactly once (the agent-efficiency study, Nov 2-4, measured
//! the explain context triple-rendering each related test's assertion and
//! repeating the observation hedge once per mention): rows inside the Evidence
//! window keep their assertion and miss sentence in the Evidence row, and the
//! verdict block below references that row instead of re-listing either. Rows
//! beyond the window have no Evidence row, so their full verdict and checked
//! assertion stay here — they are the only copy.

use crate::domain::{
    ExposureClass, Finding, OracleStrength, ProbeFamily, RelatedTest, RelatedTestMiss,
    StaticLimitKind, StopReason, exact_assertion_fact, input_boundary_fact,
};
use crate::output::path::display_path;
use crate::output::related_test_miss::{checked_assertion_text, related_test_miss_reason};

/// How many leading assertion rows the Evidence window renders
/// (`evidence_lines::MAX_RELATED_TESTS_SHOWN`): those rows carry the
/// assertion and miss sentence the verdict block would otherwise repeat.
const EVIDENCE_WINDOW_ROWS: usize = super::evidence_lines::MAX_RELATED_TESTS_SHOWN;

/// Pointer appended to a verdict whose facts the Evidence rows above already
/// state, so the drill-in keeps the verdict label without re-listing the
/// reason or assertion.
const EVIDENCE_REFERENCE: &str = " (see Evidence above)";

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
            let (lead_index, lead) = rows
                .iter()
                .copied()
                .min_by_key(|(index, row)| (strength_order(&row.oracle_strength), *index))
                .unwrap_or(rows[0]);
            let lead_verdict = row_verdict(lead, finding);
            // #8746: when the lead row's assertion text was not recorded but
            // another row of the same test printed one below, the bare
            // "assertion not recorded" fallback is suppressed — it would
            // contradict the `checked:` lines the reader is about to see.
            let group_printed_an_assertion = rows.iter().any(|(_, row)| row.oracle.is_some());
            // Rows inside the Evidence window already state their assertion
            // and miss sentence in the Evidence rows above; the verdict block
            // keeps the verdict label and points there instead of re-listing
            // the reason or the assertion. The reference is honest only when
            // an Evidence row actually carries the facts, so a row with a
            // recorded miss (or a recorded assertion) but no Evidence slot
            // keeps its full form below.
            let lead_covered = evidence_window_covers(lead_index, lead, finding);
            if lead_verdict == ASSERTION_NOT_RECORDED && group_printed_an_assertion {
                out.push_str(&format!(
                    "  - {}:{} {}\n",
                    display_path(&lead.file),
                    lead.line,
                    lead.name
                ));
            } else if lead_covered {
                out.push_str(&format!(
                    "  - {}:{} {}: {}{EVIDENCE_REFERENCE}\n",
                    display_path(&lead.file),
                    lead.line,
                    lead.name,
                    lead_verdict_label(lead, finding)
                ));
            } else {
                out.push_str(&format!(
                    "  - {}:{} {}: {lead_verdict}\n",
                    display_path(&lead.file),
                    lead.line,
                    lead.name
                ));
            }
            for (index, row) in &rows {
                if let Some(oracle) = &row.oracle {
                    if evidence_window_covers(*index, row, finding) {
                        // The assertion and this row's verdict are in the
                        // Evidence row above; re-listing them here would
                        // duplicate facts the reader just read.
                        continue;
                    }
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
    let mut investigation_step_printed = false;
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
    ) || crate::output::next_step::unresolved_edge_guidance(finding).is_some()
    {
        let step = crate::output::next_step::reconcile_next_step(finding);
        investigation_step_printed = !step.is_empty();
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
            // The unknown classes already print one investigation step above.
            if !investigation_step_printed {
                out.push_str(&format!("    Next: {}\n", reason.next_step()));
            }
        }
    }
    out
}

/// What ripr concluded about one assertion row.
fn row_verdict(row: &RelatedTest, finding: &Finding) -> String {
    match related_test_miss_reason(row, &finding.probe.family) {
        Some(why) => format!(
            "{}: {why}",
            crate::output::related_test_miss::related_test_miss_label(row)
        ),
        None => match &row.oracle {
            Some(_) => crate::output::related_test_miss::oracle_projection(
                &row.oracle_strength,
                &row.oracle_kind,
                true,
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

/// Rows of the same test (name, file, line), in first-seen order, with each
/// row's index into `finding.related_tests` so the loop can tell which rows
/// the Evidence window above already rendered.
fn group_rows_by_test(rows: &[RelatedTest]) -> Vec<Vec<(usize, &RelatedTest)>> {
    let mut groups: Vec<Vec<(usize, &RelatedTest)>> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        match groups.iter_mut().find(|group| {
            group[0].1.name == row.name
                && group[0].1.file == row.file
                && group[0].1.line == row.line
        }) {
            Some(group) => group.push((index, row)),
            None => groups.push(vec![(index, row)]),
        }
    }
    groups
}

/// Whether the Evidence window renders this row: the window shows the leading
/// [`EVIDENCE_WINDOW_ROWS`] assertion rows, and a row's facts (assertion and
/// miss sentence) appear there whenever the row kept its assertion or carries
/// a miss reason. A row with neither — the retention fact #8746 glosses — has
/// no Evidence row carrying its verdict, so the verdict block stays the only
/// copy and renders the full form.
fn evidence_window_covers(index: usize, row: &RelatedTest, finding: &Finding) -> bool {
    index < EVIDENCE_WINDOW_ROWS
        && (row.oracle.is_some() || related_test_miss_reason(row, &finding.probe.family).is_some())
}

/// The short verdict label for a covered lead row: the miss/unconfirmed word
/// when the row carries a reason (the sentence itself is in the Evidence row),
/// otherwise the oracle projection or retention verdict as usual.
fn lead_verdict_label(row: &RelatedTest, finding: &Finding) -> String {
    if related_test_miss_reason(row, &finding.probe.family).is_some() {
        crate::output::related_test_miss::related_test_miss_label(row).to_string()
    } else {
        row_verdict(row, finding)
    }
}

/// Whether the finding stopped at a reach edge ripr could not resolve, which
/// no test shape is promised to close.
fn reach_edge_unresolved(finding: &Finding) -> bool {
    finding.effective_stop_reasons().iter().any(|reason| {
        matches!(
            reason,
            StopReason::MaxDepthReached
                | StopReason::TransitiveReachUnresolved
                | StopReason::MacroReachUnresolved
        )
    }) || matches!(
        finding.static_limit_kind,
        Some(
            StaticLimitKind::RustTransitiveReachUnresolved
                | StaticLimitKind::RustIntegrationPublicApiPathUnresolved
                | StaticLimitKind::RustMacroReachUnresolved
        )
    )
}

/// What a test would have to do for ripr to see a discriminator, one line per
/// distinct miss. Only gap classes with a repair step get this list: for
/// `exposed` there is nothing to change, for the unknown classes ripr has not
/// established what is missing, and a gap whose next step was withheld (an
/// exact oracle already covers the direct sink) indicates no assertion repair.
fn verdict_changers(finding: &Finding, owner_signature: Option<&str>) -> Vec<String> {
    if crate::output::next_step::unresolved_edge_guidance(finding).is_some() {
        return Vec::new();
    }
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
    // An unresolved reach edge may end at a private helper no test can name,
    // so a recipe that tells the reader to call the owner would promise a fix
    // an exact public-API assertion cannot deliver (#10220).
    let unresolved_edge = reach_edge_unresolved(finding);
    let conditional_call = format!(
        "call {owner} directly if tests can name it; if it is a private helper behind a \
         public API, this edge can stay unresolved even with an exact assertion"
    );
    let mut needs: Vec<String> = Vec::new();
    let mut push = |need: String| {
        if !needs.contains(&need) {
            needs.push(need);
        }
    };
    if finding.related_tests.is_empty() {
        push(if unresolved_edge {
            format!("{conditional_call}; assert on its result")
        } else {
            format!("call {owner} and assert on its result")
        });
    }
    for test in &finding.related_tests {
        let Some(miss) = test.miss else { continue };
        push(match miss {
            RelatedTestMiss::NoCallPath if unresolved_edge => conditional_call.clone(),
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
            // #11147: a returned error variant under an unentered guard.
            _ if fact
                .reason
                .starts_with(crate::domain::GUARD_UNENTERED_REASON_PREFIX) =>
            {
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
        StopReason::TransitiveReachUnresolved => reason.describe(),
        StopReason::MacroReachUnresolved => {
            "a test may reach this through a macro ripr does not expand"
        }
        StopReason::GapEvidenceUnresolved => {
            "a related test asserts, but ripr could not tie that assertion to this change"
        }
        // ADR 0019: the domain gloss is the stop-reason owner. Do not invent
        // a second identity-unknown sentence here.
        StopReason::HelperIdentityUnresolved
        | StopReason::CompileTimeInvariant
        | StopReason::UnsafeObligation => reason.describe(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MissingDiscriminatorFact, OracleKind};
    use crate::output::perl_preview_card::tests::sample_perl_finding;
    use std::path::PathBuf;

    #[test]
    fn limited_journey_explain_names_the_analysis_boundary_instead_of_a_test_recipe() {
        let mut finding = sample_perl_finding();
        finding.class = ExposureClass::NoStaticPath;
        finding.static_limit_kind =
            Some(crate::domain::StaticLimitKind::RustTransitiveReachUnresolved);
        finding.evidence =
            vec!["limitation_first_unresolved_edge: entry `outer` -> owner `inner`".to_string()];
        finding.recommended_next_step = Some("call inner directly".to_string());
        finding.related_tests.clear();
        let text = render_verdict_explanation(&finding, None);
        assert!(
            !text.contains("To change the verdict, a test needs to:"),
            "{text}"
        );
        assert!(
            text.contains("Tests cannot close this static-analysis limitation"),
            "{text}"
        );
        assert!(text.contains("entry `outer` -> owner `inner`"), "{text}");
    }

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
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
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
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }
    }

    #[test]
    fn unresolved_reach_edge_recipe_does_not_promise_a_direct_call_closes_it() {
        let mut finding = sample_perl_finding();
        finding.class = ExposureClass::NoStaticPath;
        finding.recommended_next_step = Some("add a test that calls the owner".to_string());
        finding.related_tests = Vec::new();
        finding.stop_reasons = vec![StopReason::TransitiveReachUnresolved];

        let text = render_verdict_explanation(&finding, None);
        assert!(text.contains("directly if tests can name it"), "{text}");
        assert!(
            text.contains("can stay unresolved even with an exact assertion"),
            "{text}"
        );
        assert!(!text.contains("directly or through a helper"), "{text}");
        assert!(text.contains("; assert on its result"), "{text}");

        // The same rule holds for a related test with no call path and a
        // retained static limitation instead of a stop reason.
        finding.stop_reasons = Vec::new();
        finding.static_limit_kind = Some(StaticLimitKind::RustIntegrationPublicApiPathUnresolved);
        finding.related_tests = vec![row(
            "assert_eq!(outer(3, 3), 0)",
            OracleStrength::Strong,
            RelatedTestMiss::NoCallPath,
        )];
        let limited = render_verdict_explanation(&finding, None);
        assert!(
            limited.contains("directly if tests can name it"),
            "{limited}"
        );
        assert!(
            !limited.contains("directly or through a helper"),
            "{limited}"
        );

        finding.static_limit_kind = None;
        let plain = render_verdict_explanation(&finding, None);
        assert!(plain.contains("directly or through a helper"), "{plain}");
        assert!(!plain.contains("if tests can name it"), "{plain}");
    }

    #[test]
    fn explain_renders_a_next_line_under_each_stop_reason() {
        let mut finding = sample_perl_finding();
        finding.class = ExposureClass::NoStaticPath;
        finding.stop_reasons = vec![StopReason::HelperIdentityUnresolved];

        let text = render_verdict_explanation(&finding, None);
        assert!(
            text.contains(&format!(
                "    Next: {}\n",
                StopReason::HelperIdentityUnresolved.next_step()
            )),
            "{text}"
        );
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
        finding.probe.family = crate::domain::ProbeFamily::Predicate;
        let mut strong = row(
            "assert_eq!(parse(\"a\"), Ok(1));",
            OracleStrength::Strong,
            RelatedTestMiss::MissingInput,
        );
        // #5564: the boundary is the row's own support, not the finding list.
        strong.miss_support = Some(crate::domain::RelatedTestMissSupport {
            value: "len == 0".to_string(),
            flow_sink: None,
        });
        finding.related_tests = vec![
            row(
                "assert!(parse(\"a\").is_ok());",
                OracleStrength::Weak,
                RelatedTestMiss::WeakAssertion,
            ),
            strong,
        ];
        let text = render_verdict_explanation(&finding, None);
        // The lead row is the strong assertion; its miss sentence and both
        // rows' assertions live in the Evidence rows (stated once), so the
        // verdict block keeps the label and references them.
        assert!(
            text.contains("  - tests/parse.rs:7 parses_empty: misses (see Evidence above)\n"),
            "{text}"
        );
        assert!(
            !text.contains("      checked: "),
            "in-window rows must not re-list assertions: {text}"
        );
        assert!(!text.contains("no test input reaches `len == 0`"), "{text}");
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
    /// unknown, so the explanation does not introduce it as a miss. The
    /// reason sentence itself lives in the Evidence row (stated once per
    /// test); the verdict block keeps the label and references it.
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
            text.contains("  - tests/parse.rs:7 parses_empty: unconfirmed (see Evidence above)\n"),
            "{text}"
        );
        assert!(!text.contains("misses: ripr could not confirm"), "{text}");
        assert!(
            !text.contains("unconfirmed: ripr could not confirm"),
            "{text}"
        );
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
        // Both rows are inside the Evidence window, so the sibling's
        // assertion and verdict stay in the Evidence rows; nothing is
        // re-listed here.
        assert!(
            !text.contains("      checked: "),
            "in-window rows must not re-list assertions: {text}"
        );
    }

    /// A second distinct test identity, so a finding can carry more than one
    /// examined test.
    fn row_for(
        name: &str,
        line: usize,
        oracle: &str,
        strength: OracleStrength,
        miss: RelatedTestMiss,
    ) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/parse.rs"),
            line,
            oracle: Some(oracle.to_string()),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: strength,
            relation_reason: None,
            relation_confidence: None,
            miss: Some(miss),
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }
    }

    /// The full `ripr explain` render (finding sections plus the verdict
    /// explanation), the surface the agent-efficiency study (Nov 2-4) measured.
    fn full_explain_render(finding: &Finding) -> String {
        crate::output::human::render_finding_with_context_command_and_files(
            finding,
            &crate::config::RiprConfig::default(),
            "ripr context --at probe:test",
            std::path::Path::new("."),
            None,
        )
    }

    /// Agent-efficiency study (Nov 2-4): the explain context triple-rendered
    /// each related test's assertion (Evidence row + `checked:` re-list) and
    /// repeated the observation hedge once per mention. The needed-subset
    /// contract: every fact appears exactly once — assertions live in the
    /// Evidence rows, the verdict block references them, and the hedge is
    /// stated once per test.
    #[test]
    fn explain_states_each_assertion_and_hedge_once_per_test() {
        const HEDGE: &str =
            "ripr could not confirm that this assertion observes the changed behavior";
        let mut finding = sample_perl_finding();
        finding.related_tests = vec![
            row_for(
                "discount_below_threshold",
                7,
                "is(My::App::discount(100), 10, 'below')",
                OracleStrength::Strong,
                RelatedTestMiss::ObservationUnconfirmed,
            ),
            row_for(
                "discount_far_above",
                12,
                "is(My::App::discount(10_000), 1_000, 'above')",
                OracleStrength::Strong,
                RelatedTestMiss::ObservationUnconfirmed,
            ),
        ];

        let rendered = full_explain_render(&finding);

        assert_eq!(
            rendered.matches(HEDGE).count(),
            2,
            "the observation hedge must be stated once per test, not per assertion mention: {rendered}"
        );
        assert_eq!(
            rendered
                .matches("is(My::App::discount(100), 10, 'below')")
                .count(),
            1,
            "each assertion must render exactly once (in Evidence): {rendered}"
        );
        assert_eq!(
            rendered
                .matches("is(My::App::discount(10_000), 1_000, 'above')")
                .count(),
            1,
            "each assertion must render exactly once (in Evidence): {rendered}"
        );
        assert!(
            !rendered.contains("      checked: "),
            "in-window assertions must not be re-listed in Why this verdict: {rendered}"
        );
    }

    /// Rows beyond the Evidence window have no Evidence row to reference, so
    /// their full verdict and checked assertion must survive in the verdict
    /// block — they are the only copy of those facts.
    #[test]
    fn explain_keeps_the_full_verdict_for_rows_outside_the_evidence_window() {
        const HEDGE: &str =
            "ripr could not confirm that this assertion observes the changed behavior";
        let mut finding = sample_perl_finding();
        let mut rows = (0..5)
            .map(|index| {
                row_for(
                    &format!("in_window_{index}"),
                    7 + index,
                    &format!("is(My::App::discount({index}), 10, 'row {index}')"),
                    OracleStrength::Strong,
                    RelatedTestMiss::ObservationUnconfirmed,
                )
            })
            .collect::<Vec<_>>();
        rows.push(row_for(
            "beyond_window",
            42,
            "is(My::App::discount(50), 5, 'beyond')",
            OracleStrength::Strong,
            RelatedTestMiss::ObservationUnconfirmed,
        ));
        finding.related_tests = rows;

        let rendered = full_explain_render(&finding);

        // Exactly one hedge mention per test: five in Evidence rows, and the
        // sixth only in its verdict line (no Evidence row carries it).
        assert_eq!(
            rendered.matches(HEDGE).count(),
            6,
            "one hedge mention per examined test: {rendered}"
        );
        assert_eq!(
            rendered
                .matches("is(My::App::discount(50), 5, 'beyond')")
                .count(),
            1,
            "the out-of-window assertion renders once, in its checked line: {rendered}"
        );
        assert!(
            rendered.contains("      checked: is(My::App::discount(50), 5, 'beyond')"),
            "the out-of-window row keeps its full verdict and checked assertion: {rendered}"
        );
    }

    /// A test judged by several retained assertion rows states the shared
    /// miss reason once, not once per assertion row.
    #[test]
    fn evidence_states_a_repeated_miss_reason_once_per_test() {
        const HEDGE: &str =
            "ripr could not confirm that this assertion observes the changed behavior";
        let mut finding = sample_perl_finding();
        finding.related_tests = vec![
            row_for(
                "parses_empty",
                7,
                "is(My::App::discount(1), 0, 'first row')",
                OracleStrength::Strong,
                RelatedTestMiss::ObservationUnconfirmed,
            ),
            row_for(
                "parses_empty",
                7,
                "is(My::App::discount(2), 0, 'second row')",
                OracleStrength::Strong,
                RelatedTestMiss::ObservationUnconfirmed,
            ),
        ];

        let rendered = full_explain_render(&finding);

        assert_eq!(
            rendered.matches(HEDGE).count(),
            1,
            "one miss sentence per test across the whole explain render: {rendered}"
        );
        assert_eq!(
            rendered
                .matches("is(My::App::discount(1), 0, 'first row')")
                .count(),
            1,
            "each assertion still renders exactly once: {rendered}"
        );
        assert_eq!(
            rendered
                .matches("is(My::App::discount(2), 0, 'second row')")
                .count(),
            1,
            "each assertion still renders exactly once: {rendered}"
        );
    }
}
