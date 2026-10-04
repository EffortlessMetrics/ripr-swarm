//! The section `ripr explain` adds after the finding block (#5356).
//!
//! `check --format human-full` shows a window of the evidence. `explain` is
//! the drill-in, so it lists every retained test ripr examined with what it
//! concluded about each one, says what a test would need to change the
//! verdict, and spells out each stop reason.

use crate::domain::{
    ExposureClass, Finding, RelatedTest, RelatedTestMiss, StopReason, exact_assertion_fact,
    input_boundary_fact,
};
use crate::output::path::display_path;
use crate::output::related_test_miss::{checked_assertion_text, related_test_miss_reason};

pub(crate) fn render_verdict_explanation(finding: &Finding) -> String {
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
            let test = rows[0];
            let verdict =
                match related_test_miss_reason(test, &finding.activation.missing_discriminators) {
                    Some(why) => format!("misses: {why}"),
                    None => match &test.oracle {
                        Some(_) => format!(
                            "{} {} oracle",
                            test.oracle_strength.as_str(),
                            test.oracle_kind.as_str().replace('_', " ")
                        ),
                        None => "no oracle row".to_string(),
                    },
                };
            out.push_str(&format!(
                "  - {}:{} {}: {verdict}\n",
                display_path(&test.file),
                test.line,
                test.name
            ));
            for row in &rows {
                if let Some(oracle) = &row.oracle {
                    out.push_str(&format!(
                        "      checked: {}\n",
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
    let needs = verdict_changers(finding);
    if !needs.is_empty() {
        out.push_str("  To change the verdict, a test needs to:\n");
        for need in needs {
            out.push_str(&format!("  - {need}\n"));
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
fn verdict_changers(finding: &Finding) -> Vec<String> {
    if !matches!(
        finding.class,
        ExposureClass::NoStaticPath
            | ExposureClass::ReachableUnrevealed
            | ExposureClass::WeaklyExposed
    ) || finding.recommended_next_step.is_none()
    {
        return Vec::new();
    }
    let owner = finding
        .probe
        .owner
        .as_ref()
        .and_then(|owner| owner.0.rsplit("::").next().map(str::to_string))
        .filter(|name| !name.is_empty())
        .map_or_else(
            || "the changed code".to_string(),
            |name| format!("`{name}`"),
        );
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
    // must use; an error variant or field value is an assertion it must make.
    let facts = &finding.activation.missing_discriminators;
    if let Some(fact) = input_boundary_fact(facts, &finding.probe.family) {
        push(format!("use an input that reaches `{}`", fact.value));
    }
    if let Some(fact) = exact_assertion_fact(facts, &finding.probe.family) {
        push(format!("assert the exact `{}`", fact.value));
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
        StopReason::ProcMacroOpaque => "a procedural macro hides the code ripr would need to read",
        StopReason::FixtureOpaque => "a test fixture builds the input in a way ripr cannot read",
        StopReason::FeatureUnknown => "a cfg or feature gate may change whether the code runs",
        StopReason::AsyncBoundaryOpaque => {
            "the path crosses an async boundary ripr does not follow"
        }
        StopReason::NoChangedRustLine => "the diff changed no Rust line ripr can probe",
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
    }
}
