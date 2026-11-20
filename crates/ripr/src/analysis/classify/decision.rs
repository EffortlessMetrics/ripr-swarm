use super::super::rust_index::{FunctionSummary, TestSummary};
use super::boundary_pairing::SAME_TEST_PAIRING_MISSING;
use super::mask_pin::MASK_BITS_UNEXERCISED;
use super::operand_pin::FIELD_PINNED_EQUAL_TO_OPERAND;
use super::reveal::wrapper_error_seam_expression;
use crate::domain::*;
use crate::value_origins::LocatedActivationEvidence as ActivationEvidence;

pub(in crate::analysis) fn ensure_unknown_stop_reason(
    class: &ExposureClass,
    stop_reasons: &mut Vec<StopReason>,
) {
    if class.requires_stop_reason()
        && stop_reasons.is_empty()
        && let Some(reason) = StopReason::for_unknown_class(class)
    {
        stop_reasons.push(reason);
    }
}

pub(in crate::analysis) fn classify(
    reach: &StageEvidence,
    infect: &StageEvidence,
    propagate: &StageEvidence,
    observe: &StageEvidence,
    discriminate: &StageEvidence,
    probe: &Probe,
) -> ExposureClass {
    if matches!(probe.family, ProbeFamily::StaticUnknown) {
        return ExposureClass::StaticUnknown;
    }
    if reach.state == StageState::No {
        return ExposureClass::NoStaticPath;
    }
    if infect.state == StageState::Unknown || infect.state == StageState::Opaque {
        return ExposureClass::InfectionUnknown;
    }
    if propagate.state == StageState::Unknown || propagate.state == StageState::Opaque {
        return ExposureClass::PropagationUnknown;
    }
    if observe.state == StageState::No {
        return ExposureClass::ReachableUnrevealed;
    }
    if reach.state == StageState::Yes
        && discriminate.state == StageState::Yes
        && infect.state == StageState::Yes
        && propagate.state == StageState::Yes
    {
        ExposureClass::Exposed
    } else {
        ExposureClass::WeaklyExposed
    }
}

/// Maximum headline confidence permitted for a given per-stage `Confidence`
/// marker.  `High` and `Medium` return `1.0` so genuine all-Yes/Medium
/// exposures are never suppressed.  Only `Low` and `Unknown` stages produce a
/// real ceiling, capping an over-stated aggregate score without affecting the
/// classification at all.
fn confidence_ceiling(c: &Confidence) -> f32 {
    match c {
        Confidence::High | Confidence::Medium => 1.0,
        Confidence::Low => 0.66,
        Confidence::Unknown => 0.50,
    }
}

pub(in crate::analysis) fn confidence_score(
    reach: &StageEvidence,
    infect: &StageEvidence,
    propagate: &StageEvidence,
    observe: &StageEvidence,
    discriminate: &StageEvidence,
    class: &ExposureClass,
) -> f32 {
    let stages = [reach, infect, propagate, observe, discriminate];
    let mut score = 0.0;
    for stage in &stages {
        score += match stage.state {
            StageState::Yes => 0.2,
            StageState::Weak => 0.12,
            StageState::Unknown => 0.07,
            StageState::Opaque => 0.05,
            StageState::No => 0.02,
            StageState::NotApplicable => 0.1,
        };
    }
    if matches!(
        class,
        ExposureClass::NoStaticPath | ExposureClass::ReachableUnrevealed
    ) {
        score = (score + 0.15_f32).min(0.95_f32);
    }
    // Cap the headline score by the weakest contributing stage's per-stage
    // Confidence ceiling (RIPR-SPEC-0109).  Applied AFTER the +0.15 bump so
    // it can only lower, never raise.  High/Medium stages → cap = 1.0 (no
    // effect); Low → cap = 0.66; Unknown → cap = 0.50.
    let cap = stages
        .iter()
        .map(|s| confidence_ceiling(&s.confidence))
        .fold(1.0_f32, f32::min);
    (score.min(cap) * 100.0).round() / 100.0
}

pub(in crate::analysis) fn missing_evidence(
    probe: &Probe,
    class: &ExposureClass,
    infect: &StageEvidence,
    observe: &StageEvidence,
    discriminate: &StageEvidence,
    activation: &ActivationEvidence,
) -> Vec<String> {
    let mut missing = Vec::new();
    // A related assertion exists but is not credited: saying no assertion
    // or oracle was detected would contradict the refusal it replaces.
    let refused_context = observe.summary == super::ASSERTION_CONTEXT_UNESTABLISHED;
    if refused_context {
        missing.push(observe.summary.clone());
    }
    match class {
        ExposureClass::Exposed => {}
        ExposureClass::NoStaticPath => {
            missing.push("No static test path reaches the changed owner".to_string())
        }
        ExposureClass::ReachableUnrevealed if refused_context => {}
        ExposureClass::ReachableUnrevealed => missing.push(
            "No detected assertion observes the changed value, error, field, or effect".to_string(),
        ),
        ExposureClass::InfectionUnknown => missing.push(infect.summary.clone()),
        ExposureClass::PropagationUnknown => missing.push(
            "No clear propagation path from changed behavior to an observable sink".to_string(),
        ),
        ExposureClass::StaticUnknown => missing.push(
            "Syntax-first analysis cannot classify this change; inspect the changed behavior and its existing tests"
                .to_string(),
        ),
        ExposureClass::WeaklyExposed => {}
    }
    // #12414: a Predicate finding that already names a missing input — the
    // equality boundary or an inserted guard's domain — carries no extra
    // generic line; the typed shape, not a `==` string match, decides.
    if matches!(probe.family, ProbeFamily::Predicate)
        && infect.state != StageState::Yes
        && !activation.missing_discriminators.iter().any(|fact| {
            matches!(
                fact.shape,
                DiscriminatorShape::Boundary | DiscriminatorShape::GuardDomain
            )
        })
    {
        missing.push("No detected boundary input for the changed predicate".to_string());
    }
    if observe.state != StageState::Yes && !refused_context {
        missing.push("No relevant oracle was detected".to_string());
    }
    if discriminate.state != StageState::Yes {
        if matches!(
            probe.family,
            ProbeFamily::ErrorPath | ProbeFamily::ReturnValue
        ) && wrapper_error_seam_expression(&[probe.expression.as_str()])
        {
            // #3700 (final consolidation): for a wrapper error seam the
            // typed static limitation — not an exact-variant prescription —
            // is the honest outcome. The witnesses may already
            // downcast-and-pin the variant; what ripr cannot statically
            // establish is whether the boxed conversion carries that variant.
            missing.push(
                "Typed static limitation (wrapper_error_binding_unresolved): the wrapper error conversion's variant binding is not statically established"
                    .to_string(),
            );
        } else if matches!(probe.family, ProbeFamily::ErrorPath) {
            missing.push("No exact error variant discriminator was detected".to_string());
        } else if discriminate.summary.contains(SAME_TEST_PAIRING_MISSING)
            || discriminate.summary.contains(FIELD_PINNED_EQUAL_TO_OPERAND)
            || discriminate.summary.contains(MASK_BITS_UNEXERCISED)
        {
            missing.push(discriminate.summary.clone());
        } else {
            missing.push("No strong discriminator was detected".to_string());
        }
    }
    missing.extend(
        activation
            .missing_discriminators
            .iter()
            .map(|fact| format!("{MISSING_DISCRIMINATOR_VALUE_PREFIX}{}", fact.value)),
    );
    missing.sort();
    missing.dedup();
    missing
}

/// A changed item-level compile-time assertion: `const _: () = assert!(..)`
/// (also `assert_eq!`/`assert_ne!`/`debug_assert!` forms) or the
/// `static_assertions` `const_assert*!` macros, bare or path-qualified. The
/// match is on the written text only; anything else keeps its generic reason.
pub(super) fn is_compile_time_assertion(expression: &str) -> bool {
    let text = expression.trim_start();
    if let Some(rest) = text.strip_prefix("const _") {
        let Some((_, value)) = rest.split_once('=') else {
            return false;
        };
        let value = value.trim_start();
        let value = value.strip_prefix('{').map_or(value, str::trim_start);
        return ["assert!", "assert_eq!", "assert_ne!", "debug_assert!"]
            .iter()
            .any(|macro_call| value.starts_with(macro_call));
    }
    let name = text.strip_prefix("static_assertions::").unwrap_or(text);
    let ident_end = name
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(name.len());
    matches!(
        &name[..ident_end],
        "const_assert" | "const_assert_eq" | "const_assert_ne"
    ) && name[ident_end..].starts_with('!')
}

/// #11549 / #12174: the probe label of a changed `unsafe fn` or of an `unsafe`
/// block holding an unchecked assumption, only for `static_unknown` probes.
/// The shape extractor owns the parsing.
fn is_unsafe_obligation(probe: &Probe) -> bool {
    matches!(probe.family, ProbeFamily::StaticUnknown)
        && (probe.expression.starts_with("unsafe fn ")
            || probe.expression == crate::analysis::syntax::ra::UNCHECKED_ASSUMPTION_LABEL)
}

/// Typed spellings of the substring heuristics that mark related-test bodies or
/// probe expressions as opaque before any value on them is read as an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssertKind {
    FixtureMention,
    BuilderMention,
    ArbGeneratorMention,
    AsyncKeyword,
    SpawnKeyword,
    AwaitKeyword,
}

impl AssertKind {
    const TEST_BODY_SIGNALS: &'static [(Self, &'static str)] = &[
        (Self::FixtureMention, "fixture"),
        (Self::BuilderMention, "builder"),
        (Self::ArbGeneratorMention, "arb_"),
    ];
    const EXPRESSION_SIGNALS: &'static [(Self, &'static str)] = &[
        (Self::AsyncKeyword, "async"),
        (Self::SpawnKeyword, "spawn"),
        (Self::AwaitKeyword, "await"),
    ];

    fn matches_in_test_body(body: &str) -> bool {
        Self::TEST_BODY_SIGNALS
            .iter()
            .any(|(_, needle)| body.contains(needle))
    }

    /// The expression signals match whole name segments (#14875): an
    /// identifier is split at `_` and at lower-to-upper case steps, and a
    /// segment must equal `async`, `await` or `spawn` (or be `asyncio`).
    /// `async move`, `.await`, `spawn_blocking`, `async_trait`, `spawnSync`
    /// and `asyncio.run` count; `respawn_limit`, `awaiting_ack`,
    /// `asynchronous_mode` and a `"async"` string or comment do not.
    fn matches_in_expression(expression: &str) -> bool {
        let masked = crate::analysis::extract::mask_comments_and_strings(expression);
        masked
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .flat_map(name_segments)
            .any(|segment| {
                segment.eq_ignore_ascii_case("asyncio")
                    || Self::EXPRESSION_SIGNALS
                        .iter()
                        .any(|(_, needle)| segment.eq_ignore_ascii_case(needle))
            })
    }
}

/// The `_`- and case-separated segments of one identifier:
/// `spawn_blocking` is `spawn`, `blocking`; `readFileAsync` is `read`,
/// `File`, `Async`.
fn name_segments(token: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    for part in token.split('_').filter(|part| !part.is_empty()) {
        let chars: Vec<(usize, char)> = part.char_indices().collect();
        let mut begin = 0;
        for (at, &(index, ch)) in chars.iter().enumerate().skip(1) {
            let Some(&(_, previous)) = chars.get(at - 1) else {
                continue;
            };
            let next_lower = chars
                .get(at + 1)
                .is_some_and(|(_, next)| next.is_lowercase());
            // `readFile` splits before `F`; `HTTPSpawn` splits before `S`.
            let step = ch.is_uppercase()
                && (previous.is_lowercase()
                    || previous.is_ascii_digit()
                    || (previous.is_uppercase() && next_lower));
            if step {
                segments.extend(part.get(begin..index));
                begin = index;
            }
        }
        segments.extend(part.get(begin..));
    }
    segments
}

/// #14181: whether file line `line` sits inside a (possibly rustfmt-split)
/// inline const assertion block in `owner`'s body, whose first body line is
/// the owner's `start_line`.
fn in_owner_const_assertion(owner: &FunctionSummary, line: usize) -> bool {
    line.checked_sub(owner.start_line).is_some_and(|offset| {
        owner.body.contains("const")
            && crate::analysis::probes::inline_const_assertion_lines(&owner.body)
                .contains(&(offset + 1))
    })
}

pub(in crate::analysis) fn stop_reasons(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
) -> Vec<StopReason> {
    let mut reasons = Vec::new();
    if owner_fn.is_none() {
        if is_compile_time_assertion(&probe.expression) {
            reasons.push(StopReason::CompileTimeInvariant);
        } else {
            reasons.push(StopReason::NoChangedRustLine);
        }
    } else if probe.family == ProbeFamily::StaticUnknown
        && (crate::analysis::probes::is_inline_const_assertion_line(&probe.expression)
            || (probe.after.is_some()
                && owner_fn
                    .is_some_and(|owner| in_owner_const_assertion(owner, probe.location.line))))
    {
        // #14181: `const { assert!(N <= 249) };` inside a function is checked
        // when the function is built; a violating edit fails to compile. A
        // rustfmt-split block is found by its line in the owner's body. Only
        // an added-side probe sits on its own new-file line; a removed one's
        // location is where the deletion sits.
        reasons.push(StopReason::CompileTimeInvariant);
    }
    if is_unsafe_obligation(probe) {
        reasons.push(StopReason::UnsafeObligation);
    }
    if related_tests
        .iter()
        .any(|test| AssertKind::matches_in_test_body(&test.body))
    {
        reasons.push(StopReason::FixtureOpaque);
    }
    if AssertKind::matches_in_expression(&probe.expression) {
        reasons.push(StopReason::AsyncBoundaryOpaque);
    }
    if contains_macro_invocation(&probe.expression) {
        reasons.push(StopReason::ProcMacroOpaque);
    }
    reasons.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    reasons.dedup_by(|a, b| a.as_str() == b.as_str());
    reasons
}

fn contains_macro_invocation(expression: &str) -> bool {
    for (idx, ch) in expression.char_indices() {
        if ch != '!' || expression[idx + 1..].starts_with('=') {
            continue;
        }
        let before_bang = expression[..idx].trim_end();
        if before_bang
            .chars()
            .last()
            .is_some_and(|ch| ch == '_' || ch == ')' || ch.is_ascii_alphanumeric())
        {
            return true;
        }
    }
    false
}

/// #3700: the typed next step for a wrapper error seam. It names what static
/// analysis cannot establish instead of prescribing an assertion the suite may
/// already contain.
pub(in crate::analysis) const WRAPPER_ERROR_BINDING_NEXT_STEP: &str = "Typed static limitation (wrapper_error_binding_unresolved): ripr cannot statically establish that this boxed wrapper conversion carries the callee's error variant, so an existing exact downcast witness is not statically creditable. Verify via real mutation testing or deeper conversion modeling.";

pub(in crate::analysis) fn recommended_next_step(
    probe: &Probe,
    class: &ExposureClass,
    owner_assertion_shaped: bool,
    stop_reasons: &[StopReason],
) -> Option<String> {
    recommended_next_step_with_boundary(
        probe,
        class,
        owner_assertion_shaped,
        stop_reasons,
        false,
        false,
    )
}

/// Next-step guidance when the caller knows whether a related test already
/// supplies the changed boundary input (infection `yes`, #10622). For a weak
/// predicate that means the input exists and only the oracle is weak, so the
/// "add boundary tests" template would be misleading. Guidance only: repair
/// eligibility is decided elsewhere.
pub(in crate::analysis) fn recommended_next_step_with_boundary(
    probe: &Probe,
    class: &ExposureClass,
    owner_assertion_shaped: bool,
    stop_reasons: &[StopReason],
    boundary_input_observed: bool,
    collection_contents_check_side_effect: bool,
) -> Option<String> {
    if boundary_input_observed
        && !owner_assertion_shaped
        && *class == ExposureClass::WeaklyExposed
        && matches!(probe.family, ProbeFamily::Predicate)
    {
        return Some(crate::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP.to_string());
    }
    // RIPR-SPEC-0133: an assertion-shaped owner is the oracle, not the code
    // under test, so the standard code-under-test advice is incoherent for it.
    // The exposure class is unchanged; only the guidance is reframed.
    if owner_assertion_shaped {
        return assertion_shaped_next_step(probe, class, stop_reasons);
    }
    match class {
        ExposureClass::Exposed => None,
        ExposureClass::WeaklyExposed => Some(
            if matches!(probe.family, ProbeFamily::ErrorPath | ProbeFamily::ReturnValue)
                && wrapper_error_seam_expression(&[probe.expression.as_str()])
            {
                // #3700 (final consolidation): do not prescribe an assertion
                // the suite may already contain. The typed limitation names
                // what static analysis cannot establish; discriminating this
                // seam needs real mutation testing or deeper conversion
                // modeling (Into/From through Box).
                WRAPPER_ERROR_BINDING_NEXT_STEP
            } else if matches!(probe.family, ProbeFamily::ReturnValue)
                && super::exact_error_variant(&probe.expression).is_some()
            {
                // A changed `Err(...)` construction is not a "broad assertion"
                // gap: the related test may already assert an exact (sibling)
                // variant. The missing discriminator is an input that reaches
                // the changed error path plus an assertion pinning this exact
                // variant (RIPR-SPEC-0106).
                "Add a test input that reaches the changed error path and assert the exact error variant it returns."
            } else if matches!(probe.family, ProbeFamily::SideEffect)
                && collection_contents_check_side_effect
            {
                crate::analysis::repair_route::COLLECTION_CONTENTS_CHECK_NEXT_STEP
            } else {
                weakly_exposed_guidance_for_family(&probe.family)
            }
            .to_string(),
        ),
        ExposureClass::ReachableUnrevealed => {
            if matches!(probe.family, ProbeFamily::ErrorPath | ProbeFamily::ReturnValue)
                && super::exact_error_variant(&probe.expression).is_some()
            {
                // A changed `Err(...)` construction with no observing
                // assertion: the actionable missing discriminator is the
                // exact variant (RIPR-SPEC-0106), not generic assertion
                // advice.
                Some("Add a test input that reaches the changed error path and assert the exact error variant it returns.".to_string())
            } else {
                Some("Add a meaningful assertion that observes the changed value, branch, error, field, event, or side effect.".to_string())
            }
        }
        ExposureClass::NoStaticPath => Some(crate::domain::NO_STATIC_PATH_NEXT_STEP.to_string()),
        ExposureClass::InfectionUnknown | ExposureClass::PropagationUnknown | ExposureClass::StaticUnknown => Some(uncertainty_next_step(class, stop_reasons, false, &probe.family)),
    }
}

/// #7393: the family templates presume an assertion shape the related test
/// may not have — "instead of only is_err()" when no `is_err()` exists, or
/// "replace broad assertions" when the related test already is an
/// `assert_eq!`. When the strongest reach-establishing related oracle is
/// already exact, avoid presuming a broad assertion. Error-path advice names
/// the missing arm input; return-value advice first asks whether the change
/// can affect the return, since an exact effect assertion cannot establish it.
///
/// The caller annotates each row's miss reason first, so this selection
/// reads the same final rows the JSON/human projections render. A row is
/// eligible only when it establishes reach to the changed owner
/// ([`RelationReason::establishes_reach`]; a `SameTestFile` or
/// token-coincidence row never dictates the owner finding's advice) and its
/// annotated miss is not `missing_exact_assertion` (there the assertion
/// shape IS the gap). An observation-unconfirmed row stays eligible: the
/// suite's exact assertion exists, but it may observe a different sink. The oracle must also be
/// admitted as strong: a syntactic `assert_eq!` the classifier downgraded
/// (for example a self-computed expected value) does not establish an
/// independent exact discriminator, so "keep the exact assertion" would be
/// wrong advice for it.
///
/// Returns None when the strongest eligible related oracle is not an
/// admitted strong exact one (the family template stands), when no eligible
/// row exists, or for families whose templates already name the gap.
/// Wrapper-error seams and changed `Err(...)` constructions keep their
/// dedicated guidance at the caller.
pub(in crate::analysis) fn exact_oracle_gap_next_step(
    probe: &Probe,
    class: &ExposureClass,
    related: &[RelatedTest],
    missing: &[MissingDiscriminatorFact],
    owner_is_function: bool,
) -> Option<String> {
    if *class != ExposureClass::WeaklyExposed || related.is_empty() {
        return None;
    }
    let candidates = related.iter().filter(|test| {
        test.is_contributing()
            && test
                .relation_reason
                .is_none_or(|reason| reason.establishes_reach(owner_is_function))
            && test.miss != Some(RelatedTestMiss::MissingExactAssertion)
    });
    let strongest = candidates.max_by_key(|test| test.oracle_strength.rank())?;
    // An exact-kind oracle admitted as strong pins an independent expected
    // output; anything weaker (a broad error check, a smoke assertion, an
    // unreadable shape, or an exact-looking assertion the classifier
    // downgraded, such as a self-computed expected value) keeps the family
    // template, which is honest advice for those shapes.
    if !matches!(strongest.oracle_strength, OracleStrength::Strong) {
        return None;
    }
    if !matches!(
        strongest.oracle_kind,
        OracleKind::ExactValue
            | OracleKind::ExactErrorVariant
            | OracleKind::WholeObjectEquality
            | OracleKind::Snapshot
    ) {
        return None;
    }
    let family_gap = match probe.family {
        ProbeFamily::ErrorPath => {
            "Add a test input that executes the changed error arm and assert the exact variant or payload it produces"
        }
        ProbeFamily::ReturnValue => {
            "Check whether the change can affect the returned value. If it can, add a separating input and assert the exact returned value. If it cannot, test the changed effect separately; another input cannot distinguish unchanged returns"
        }
        _ => return None,
    };
    let missing_value = missing
        .first()
        .map(|fact| format!(" Missing discriminator: `{}`.", fact.value))
        .unwrap_or_default();
    let oracle_context = if probe.family == ProbeFamily::ReturnValue {
        "already has an exact assertion; this does not establish observation of the returned value"
    } else {
        "already asserts an exact value; the gap is not the assertion shape"
    };
    Some(format!(
        "The related test `{}` {oracle_context}. {family_gap}.{missing_value}",
        strongest.name,
    ))
}

/// Guidance for an assertion-shaped owner, phrased for oracles rather than for
/// code under test. Unknowns name an inspection, not an established test gap.
fn assertion_shaped_next_step(
    probe: &Probe,
    class: &ExposureClass,
    stop_reasons: &[StopReason],
) -> Option<String> {
    match class {
        ExposureClass::Exposed => None,
        ExposureClass::WeaklyExposed => {
            Some(crate::domain::ASSERTION_SHAPED_WEAKLY_EXPOSED_NEXT_STEP.to_string())
        }
        ExposureClass::ReachableUnrevealed => {
            Some(crate::domain::ASSERTION_SHAPED_REACHABLE_UNREVEALED_NEXT_STEP.to_string())
        }
        ExposureClass::NoStaticPath => {
            Some(crate::domain::ASSERTION_SHAPED_NO_STATIC_PATH_NEXT_STEP.to_string())
        }
        ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown
        | ExposureClass::StaticUnknown => Some(uncertainty_next_step(
            class,
            stop_reasons,
            true,
            &probe.family,
        )),
    }
}

/// #9244 item 5: the inspection a developer performs for an `infection_unknown`
/// finding. Only a predicate has "sides of a condition"; other families name
/// what the changed expression produces (a scalar return value, an error
/// variant) or which arm an input selects, so the wording never asks for the
/// "other side" of a comparison that is not there.
fn infection_unknown_inspection_step(family: &ProbeFamily) -> &'static str {
    match family {
        ProbeFamily::Predicate => {
            "Open the related test, check whether it reaches the changed code, and inspect other existing tests. Trace their inputs through the local or computed operand (including default and empty cases) to see which side of the changed condition they exercise, then name an input that would exercise the other side. ripr.toml does not support fixture/builder value mappings."
        }
        ProbeFamily::MatchArm => {
            "Open the related test, check whether it reaches the changed code, and inspect other existing tests. Trace their inputs through the local or computed operand (including default and empty cases) to see which match arm each input selects, then name an input that would select the changed arm. ripr.toml does not support fixture/builder value mappings."
        }
        ProbeFamily::CallDeletion | ProbeFamily::SideEffect => {
            "Open the related test, check whether it reaches the changed code, and inspect other existing tests. Trace their inputs through the local or computed operand (including default and empty cases) to see whether the call runs and what it does or changes that a test could observe, then name an input for which that observable effect would differ between the original and changed code. ripr.toml does not support fixture/builder value mappings."
        }
        ProbeFamily::ReturnValue
        | ProbeFamily::ErrorPath
        | ProbeFamily::FieldConstruction
        | ProbeFamily::StaticUnknown => {
            "Open the related test, check whether it reaches the changed code, and inspect other existing tests. Trace their inputs through the local or computed operand (including default and empty cases) to see what the changed expression produces for each, then name an input for which the original and changed code would produce different results. ripr.toml does not support fixture/builder value mappings."
        }
    }
}

/// The same uncertainty-resolution action feeds every Finding consumer. No
/// new evidence or classification is inferred from a suggested experiment.
fn uncertainty_next_step(
    class: &ExposureClass,
    reasons: &[StopReason],
    assertion_shaped: bool,
    family: &ProbeFamily,
) -> String {
    let mut step = if assertion_shaped {
        crate::domain::ASSERTION_SHAPED_INFECTION_UNKNOWN_NEXT_STEP.to_string()
    } else {
        match class {
            ExposureClass::InfectionUnknown => infection_unknown_inspection_step(family),
            ExposureClass::PropagationUnknown => "Trace the changed value to its output or effect, and inspect what the related test actually checks. If it has an assertion, inspect exactly what that assertion observes.",
            _ => "Inspect the changed behavior and its existing tests; ripr has not established a test gap.",
        }.to_string()
    };
    let mut added = Vec::new();
    for reason in reasons {
        let action = match reason {
            StopReason::NoChangedRustLine => {
                "Inspect the changed declaration and the behavior that consumes it; ripr could not place this line inside an analyzed Rust function."
            }
            StopReason::ProcMacroOpaque | StopReason::MacroReachUnresolved => {
                "Inspect the macro invocation and the output or effect it produces; ripr does not expand the macro."
            }
            StopReason::FixtureOpaque => {
                "A related test mentions a fixture, builder, or generator; inspect its produced input before deciding another case is needed."
            }
            StopReason::AsyncBoundaryOpaque => {
                "If a task boundary is involved, check whether related tests wait for completion and what resulting effects they check."
            }
            StopReason::FeatureUnknown => {
                "Check the intended Cargo feature/cfg settings and whether any relevant gated tests or assertions actually run."
            }
            StopReason::DynamicDispatchUnresolved => {
                "Identify the receiver and implementation the existing test uses at the unresolved call."
            }
            StopReason::ExternalCrateBoundary => {
                "Inspect the observable contract at the external-crate boundary; ripr does not follow that path."
            }
            StopReason::MaxDepthReached | StopReason::TransitiveReachUnresolved => {
                "Inspect the helper call chain to the changed owner before adding a direct test merely to satisfy static analysis."
            }
            StopReason::HelperIdentityUnresolved => {
                "Identify which same-named helper the existing test calls before changing any test."
            }
            StopReason::CompileTimeInvariant => {
                "Check the assertion by building with the target, features and cfg that include it (a gated assertion is not compiled otherwise); a violating edit fails to compile, so no test observes it."
            }
            StopReason::UnsafeObligation => {
                "Review the safety contract and each caller or assumption site by hand; a safe-code test cannot discharge an unsafe obligation."
            }
            StopReason::GapEvidenceUnresolved => {
                "Inspect the refused assertion and whether it runs; an unreadable assertion is not an established missing test."
            }
            StopReason::InfectionEvidenceUnknown
            | StopReason::PropagationEvidenceUnknown
            | StopReason::StaticProbeUnknown => continue,
        };
        if !added.contains(&action) {
            step.push(' ');
            step.push_str(action);
            added.push(action);
        }
    }
    step.push_str(" Add or tighten a test only if this finds a gap; compare old and new code with the project's configured test runner (`cargo test` for ordinary Cargo tests). The static class may remain unknown.");
    step
}

/// #15149: for an owner that declares `-> bool`, `assert!(owner(..))` is
/// `assert_eq!(owner(..), true)` and `assert!(!owner(..))` is
/// `assert_eq!(owner(..), false)`: each already checks the exact result. The
/// return-value template ("replace broad assertions with exact equality")
/// then names a rewrite that checks the same values, which cannot tell the
/// old code from the new on the inputs already tested. What is missing is an
/// input on which the two versions return different booleans.
///
/// Fires only for a weakly exposed return-value probe of a `-> bool` owner
/// when every contributing row that establishes reach to the owner asserts
/// the owner call's result directly in one of those two forms. Any other row
/// (a broad or unreadable oracle, a method receiver, a turbofish, a bound
/// result) keeps the family template. Guidance only: the exposure class,
/// stages and repair eligibility are decided elsewhere and are unchanged.
pub(in crate::analysis) fn bool_owner_exact_result_next_step(
    probe: &Probe,
    class: &ExposureClass,
    owner: Option<&FunctionSummary>,
    related: &[RelatedTest],
    asserted_call_is_owner: &dyn Fn(&RelatedTest, &[String]) -> bool,
) -> Option<String> {
    if *class != ExposureClass::WeaklyExposed || probe.family != ProbeFamily::ReturnValue {
        return None;
    }
    let owner = owner?;
    if super::owner_pin::declared_return_type(&owner.body).as_deref() != Some("bool") {
        return None;
    }
    let mut reaching = related
        .iter()
        .filter(|test| {
            test.is_contributing()
                && test
                    .relation_reason
                    .is_none_or(|reason| reason.establishes_reach(true))
        })
        .peekable();
    reaching.peek()?;
    // A row annotated `missing_exact_assertion` says the assertion shape IS
    // the gap (the same exclusion `exact_oracle_gap_next_step` makes), so it
    // keeps the family template here rather than being read as exact.
    // The row's relation reason is test-level reach: the test calls the
    // owner somewhere, not necessarily in this assertion. A same-named rival
    // asserted beside an unasserted owner call must not borrow the owner's
    // step, so the asserted call itself must settle on the owner (#15537
    // review); anything unsettled keeps the family template.
    if !reaching.all(|test| {
        test.miss != Some(RelatedTestMiss::MissingExactAssertion)
            && test
                .oracle
                .as_deref()
                .and_then(|oracle| asserted_bool_owner_call(oracle, &owner.name))
                .is_some_and(|qualifier| asserted_call_is_owner(test, &qualifier))
    }) {
        return None;
    }
    Some(format!(
        "The related tests already assert the exact boolean result of `{}`, so rewriting them with exact equality would check the same values. Add a test with an input whose boolean result differs between the old and new code, and assert that result.",
        owner.name
    ))
}

/// Whether `text` is `assert!(owner(..))` or `assert!(!owner(..))` (plain,
/// `std::` or `core::` rooted, with optional message arguments), where the
/// condition is exactly one call of `owner` through an optional plain path.
/// Returns the call's module-path qualifier in source order (empty for a
/// bare call), or `None` for any other shape. The text match is by the
/// path's last segment only; which definition the qualified call names is
/// the caller's `asserted_call_is_owner` question.
fn asserted_bool_owner_call(text: &str, owner: &str) -> Option<Vec<String>> {
    let text = text.trim().trim_end_matches(';').trim_end();
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    if masked.len() != text.len() {
        return None;
    }
    let bang = masked.find("assert!")?;
    if !matches!(
        masked[..bang].trim(),
        "" | "std::" | "core::" | "::std::" | "::core::"
    ) {
        return None;
    }
    let open = masked[bang..].find('(')? + bang;
    if !masked[bang + "assert!".len()..open].trim().is_empty()
        || matching_close(&masked, open) != Some(masked.len() - 1)
    {
        return None;
    }
    let inner = &masked[open + 1..masked.len() - 1];
    let condition = inner[..top_level_comma(inner).unwrap_or(inner.len())].trim();
    let condition = condition
        .strip_prefix('!')
        .map_or(condition, str::trim_start);
    let call_open = condition.find('(')?;
    let mut segments: Vec<String> = condition[..call_open]
        .trim_end()
        .split("::")
        .map(|segment| segment.trim().to_string())
        .collect();
    let readable = segments.pop().as_deref() == Some(owner)
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
        // The call's own parenthesis closes the condition, so
        // `owner(a) && other` or `owner(a).not()` is not this shape.
        && matching_close(condition, call_open) == Some(condition.len() - 1);
    readable.then_some(segments)
}

/// The offset of the bracket closing the one at `open` in `text`.
fn matching_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in text.bytes().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// The offset of the first comma outside every bracket in `text`.
fn top_level_comma(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in text.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return Some(offset),
            _ => {}
        }
    }
    None
}

fn weakly_exposed_guidance_for_family(family: &ProbeFamily) -> &'static str {
    match family {
        ProbeFamily::Predicate => {
            "Add boundary tests for below, equal, and above the changed threshold with exact assertions."
        }
        ProbeFamily::ErrorPath => {
            "Assert the exact error variant or payload instead of only is_err()."
        }
        ProbeFamily::SideEffect => {
            "Add a mock expectation, event receiver assertion, persisted-state check, or metric assertion for the changed effect."
        }
        ProbeFamily::ReturnValue => {
            "Replace broad assertions with exact equality or a property that constrains the changed returned value."
        }
        _ => "Strengthen the related assertion so it discriminates the changed behavior.",
    }
}

#[cfg(kani)]
mod kani_proofs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_maps_reachable_but_unobserved_probe_to_reachable_unrevealed() {
        let class = classify(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::No),
            &stage(StageState::Yes),
            &probe(ProbeFamily::ReturnValue, "value + 1"),
        );

        assert_eq!(class, ExposureClass::ReachableUnrevealed);
    }

    #[test]
    fn confidence_score_handles_opaque_no_and_not_applicable_stage_states() {
        let score = confidence_score(
            &stage(StageState::Opaque),
            &stage(StageState::No),
            &stage(StageState::NotApplicable),
            &stage(StageState::Yes),
            &stage(StageState::Weak),
            &ExposureClass::NoStaticPath,
        );

        assert!(
            (score - 0.64).abs() < f32::EPSILON,
            "confidence score should equal 0.64 (got {score})"
        );
    }

    #[test]
    fn missing_evidence_reports_reachable_unrevealed_gap() {
        let probe = probe(ProbeFamily::ReturnValue, "value + 1");
        let missing = missing_evidence(
            &probe,
            &ExposureClass::ReachableUnrevealed,
            &stage(StageState::Yes),
            &stage(StageState::No),
            &stage(StageState::Yes),
            &ActivationEvidence::default(),
        );

        assert!(
            missing.contains(
                &"No detected assertion observes the changed value, error, field, or effect"
                    .to_string()
            )
        );
    }

    // #9244 item 5: the infection_unknown next step names a concrete action
    // (open the test, find the side of the condition it exercises, name an
    // input for the other side) instead of an open-ended "trace" instruction.
    #[test]
    fn infection_unknown_next_step_names_a_concrete_action() {
        let step = recommended_next_step(
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
            &ExposureClass::InfectionUnknown,
            false,
            &[],
        )
        .unwrap_or_default();
        assert!(
            step.starts_with("Open the related test, check whether it reaches the changed code"),
            "{step}"
        );
        assert!(
            step.contains("then name an input that would exercise the other side"),
            "{step}"
        );
        assert!(
            !step.contains("to check whether the change matters"),
            "{step}"
        );
        // The assertion-shaped owner reframing is unchanged.
        let shaped = recommended_next_step(
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
            &ExposureClass::InfectionUnknown,
            true,
            &[],
        )
        .unwrap_or_default();
        assert!(
            shaped.starts_with("The changed owner is an assertion helper"),
            "{shaped}"
        );
    }

    // #9244 item 5 residual: a scalar return value has no "other side of a
    // condition"; the wording names what the changed expression produces.
    #[test]
    fn infection_unknown_next_step_only_names_condition_sides_for_predicates() {
        let step_for = |family, expression: &str| {
            recommended_next_step(
                &probe(family, expression),
                &ExposureClass::InfectionUnknown,
                false,
                &[],
            )
            .unwrap_or_default()
        };
        let scalar = step_for(ProbeFamily::ReturnValue, "base_fee * 3");
        assert!(!scalar.contains("condition"), "{scalar}");
        assert!(!scalar.contains("other side"), "{scalar}");
        assert!(
            scalar.contains("what the changed expression produces for each"),
            "{scalar}"
        );
        assert!(
            scalar.contains("ripr.toml does not support fixture/builder value mappings."),
            "{scalar}"
        );
        for family in [
            ProbeFamily::ErrorPath,
            ProbeFamily::FieldConstruction,
            ProbeFamily::StaticUnknown,
        ] {
            let step = step_for(family, "value");
            assert!(!step.contains("condition"), "{step}");
            assert!(step.contains("different results"), "{step}");
        }
        // A unit-returning call has no result to compare: name the effect.
        for (family, expression) in [
            (ProbeFamily::SideEffect, "client.send(value)"),
            (ProbeFamily::CallDeletion, "log_audit(&entry);"),
        ] {
            let step = step_for(family, expression);
            assert!(
                step.contains("what it does or changes that a test could observe"),
                "{step}"
            );
            assert!(step.contains("observable effect would differ"), "{step}");
            assert!(!step.contains("different results"), "{step}");
            assert!(!step.contains("produces"), "{step}");
            assert!(!step.contains("condition"), "{step}");
        }
        let arm = step_for(ProbeFamily::MatchArm, "Tier::Pro => 2");
        assert!(arm.contains("which match arm each input selects"), "{arm}");
        assert!(!arm.contains("condition"), "{arm}");
        let predicate = step_for(ProbeFamily::Predicate, "amount >= threshold");
        assert!(
            predicate.contains("which side of the changed condition"),
            "{predicate}"
        );
    }

    #[test]
    fn recommended_next_step_covers_side_effect_and_default_weak_guidance() {
        let side_effect = recommended_next_step(
            &probe(ProbeFamily::SideEffect, "client.send(value)"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
        );
        assert_eq!(
            side_effect.as_deref(),
            Some(
                "Add a mock expectation, event receiver assertion, persisted-state check, or metric assertion for the changed effect."
            )
        );

        let localized_collection_push = recommended_next_step_with_boundary(
            &probe(ProbeFamily::SideEffect, "buf.push(msg.to_string())"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
            false,
            true,
        );
        assert_eq!(
            localized_collection_push.as_deref(),
            Some(crate::analysis::repair_route::COLLECTION_CONTENTS_CHECK_NEXT_STEP)
        );
        assert!(
            !localized_collection_push
                .as_deref()
                .unwrap_or_default()
                .contains("mock"),
            "{localized_collection_push:?}"
        );

        let match_arm = recommended_next_step(
            &probe(ProbeFamily::MatchArm, "None => 0"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
        );
        assert_eq!(
            match_arm.as_deref(),
            Some("Strengthen the related assertion so it discriminates the changed behavior.")
        );
    }

    #[test]
    fn boundary_observed_weak_predicate_guidance_targets_the_oracle() {
        let predicate = probe(ProbeFamily::Predicate, "qty > 10");
        let observed = recommended_next_step_with_boundary(
            &predicate,
            &ExposureClass::WeaklyExposed,
            false,
            &[],
            true,
            false,
        );
        assert_eq!(
            observed.as_deref(),
            Some(crate::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP)
        );
        // Discriminating negative: boundary input not observed keeps the
        // boundary-test template.
        let unobserved = recommended_next_step_with_boundary(
            &predicate,
            &ExposureClass::WeaklyExposed,
            false,
            &[],
            false,
            false,
        );
        assert_eq!(
            unobserved.as_deref(),
            Some(
                "Add boundary tests for below, equal, and above the changed threshold with exact assertions."
            )
        );
        // The observed-boundary wording is specific to weak predicates.
        let other_family = recommended_next_step_with_boundary(
            &probe(ProbeFamily::ReturnValue, "value + 1"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
            true,
            false,
        );
        assert_ne!(
            other_family.as_deref(),
            Some(crate::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP)
        );
    }

    #[test]
    fn boundary_observed_does_not_override_assertion_shaped_owner_guidance() {
        let step = recommended_next_step_with_boundary(
            &probe(ProbeFamily::Predicate, "qty > 10"),
            &ExposureClass::WeaklyExposed,
            true,
            &[],
            true,
            false,
        );
        assert_eq!(
            step.as_deref(),
            Some(crate::domain::ASSERTION_SHAPED_WEAKLY_EXPOSED_NEXT_STEP)
        );
    }

    #[test]
    fn recommended_next_step_covers_targeted_weak_guidance_families() {
        let predicate = recommended_next_step(
            &probe(ProbeFamily::Predicate, "value >= threshold"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
        );
        assert_eq!(
            predicate.as_deref(),
            Some(
                "Add boundary tests for below, equal, and above the changed threshold with exact assertions."
            )
        );

        let error_path = recommended_next_step(
            &probe(ProbeFamily::ErrorPath, "Err(AppError::Denied)"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
        );
        assert_eq!(
            error_path.as_deref(),
            Some("Assert the exact error variant or payload instead of only is_err().")
        );

        let return_value = recommended_next_step(
            &probe(ProbeFamily::ReturnValue, "count + 1"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
        );
        assert_eq!(
            return_value.as_deref(),
            Some(
                "Replace broad assertions with exact equality or a property that constrains the changed returned value."
            )
        );
    }

    // XQFi: a return-value probe on an `Err(...)` construction is weakly
    // exposed because the changed error path is not discriminated — not
    // because the oracle is "broad". The sibling-variant fixture asserts an
    // exact `CalcError::Negative` while the change constructs `TooLarge`, so
    // the guidance must ask for a reaching input and the exact constructed
    // variant instead of repeating the broad-assertion advice.
    #[test]
    fn return_value_error_construction_guidance_names_the_missing_discriminator() {
        let error_construction = recommended_next_step(
            &probe(ProbeFamily::ReturnValue, "return Err(CalcError::TooLarge);"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
        );
        assert_eq!(
            error_construction.as_deref(),
            Some(
                "Add a test input that reaches the changed error path and assert the exact error variant it returns."
            )
        );
        // A plain (non-Err) return value keeps the exact-equality guidance.
        let plain_value = recommended_next_step(
            &probe(ProbeFamily::ReturnValue, "Ok(amount)"),
            &ExposureClass::WeaklyExposed,
            false,
            &[],
        );
        assert_eq!(
            plain_value.as_deref(),
            Some(
                "Replace broad assertions with exact equality or a property that constrains the changed returned value."
            )
        );
    }

    // RIPR-SPEC-0133: an assertion-shaped owner is the oracle. Guidance is
    // reframed for every class; the class itself is untouched (classification
    // happens in `classify`, upstream of this fn).
    #[test]
    fn recommended_next_step_reframes_guidance_for_assertion_shaped_owners() {
        let return_value = probe(ProbeFamily::ReturnValue, "count + 1");

        assert_eq!(
            recommended_next_step(&return_value, &ExposureClass::Exposed, true, &[]),
            None
        );
        assert_eq!(
            recommended_next_step(&return_value, &ExposureClass::WeaklyExposed, true, &[])
                .as_deref(),
            Some(crate::domain::ASSERTION_SHAPED_WEAKLY_EXPOSED_NEXT_STEP)
        );
        assert_eq!(
            recommended_next_step(
                &return_value,
                &ExposureClass::ReachableUnrevealed,
                true,
                &[]
            )
            .as_deref(),
            Some(crate::domain::ASSERTION_SHAPED_REACHABLE_UNREVEALED_NEXT_STEP)
        );
        assert_eq!(
            recommended_next_step(&return_value, &ExposureClass::NoStaticPath, true, &[])
                .as_deref(),
            Some(crate::domain::ASSERTION_SHAPED_NO_STATIC_PATH_NEXT_STEP)
        );
        for class in [
            ExposureClass::InfectionUnknown,
            ExposureClass::PropagationUnknown,
            ExposureClass::StaticUnknown,
        ] {
            let step = recommended_next_step(&return_value, &class, true, &[]).unwrap_or_default();
            assert!(
                step.starts_with(crate::domain::ASSERTION_SHAPED_INFECTION_UNKNOWN_NEXT_STEP),
                "{step}"
            );
            assert!(
                step.contains("old and new code") && step.contains("may remain unknown"),
                "{step}"
            );
            assert!(
                !step.contains("deep static analysis") && !step.contains("ripr.toml"),
                "{step}"
            );
        }
    }

    // RIPR-SPEC-0133: the reframed guidance must state the detection rule in
    // one sentence and must not ask for a test of the oracle itself.
    #[test]
    fn assertion_shaped_guidance_states_the_rule_and_avoids_test_of_test_advice() {
        let reason = crate::domain::ASSERTION_SHAPED_OWNER_REASON;
        for guidance in [
            crate::domain::ASSERTION_SHAPED_WEAKLY_EXPOSED_NEXT_STEP,
            crate::domain::ASSERTION_SHAPED_REACHABLE_UNREVEALED_NEXT_STEP,
            crate::domain::ASSERTION_SHAPED_NO_STATIC_PATH_NEXT_STEP,
            crate::domain::ASSERTION_SHAPED_INFECTION_UNKNOWN_NEXT_STEP,
        ] {
            assert!(
                guidance.contains(reason),
                "guidance must embed the one-sentence rule `{reason}`; got: {guidance}"
            );
            assert!(
                !guidance.contains("co-located test"),
                "guidance must not ask for a test that observes the oracle; got: {guidance}"
            );
        }
    }

    // RIPR-SPEC-0109 control (a): genuine all-Yes/Medium exposure must not be
    // suppressed — the cap is 1.0 for Medium stages.
    #[test]
    fn confidence_score_genuine_exposure_all_yes_medium_is_not_capped() {
        let score = confidence_score(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &ExposureClass::Exposed,
        );
        // 5 × Yes = 5 × 0.20 = 1.00; cap = 1.0 (all Medium) → unchanged.
        assert!(
            (score - 1.0).abs() < f32::EPSILON,
            "genuine exposure must report confidence 1.0, got {score}"
        );
    }

    // RIPR-SPEC-0109 control (b): a propagation_unknown finding with
    // propagate.confidence = Low must be capped to ≤ 0.66, and its class
    // must NOT change (class is determined by `classify`, not here).
    #[test]
    fn confidence_score_low_propagate_confidence_caps_score_to_0_66() {
        let low_propagate =
            StageEvidence::new(StageState::Unknown, Confidence::Low, "propagation unknown");
        let score = confidence_score(
            &stage(StageState::Yes),  // reach
            &stage(StageState::Yes),  // infect
            &low_propagate,           // propagate — Low confidence
            &stage(StageState::Yes),  // observe
            &stage(StageState::Weak), // discriminate
            &ExposureClass::PropagationUnknown,
        );
        // Raw: 0.20+0.20+0.07+0.20+0.12 = 0.79; Low cap = 0.66 → score = 0.66.
        assert!(
            score <= 0.66,
            "propagation_unknown with Low confidence must be capped to ≤ 0.66, got {score}"
        );
        // Confirm class is determined independently — classification is
        // asserted via the `classify` fn contract, not here.  The score must
        // be strictly below the uncapped value of 0.79.
        assert!(
            score < 0.79,
            "score must be below uncapped 0.79, got {score}"
        );
    }

    // RIPR-SPEC-0109 control (c): #1232 no-regression — infection_unknown and
    // propagation_unknown classifications must remain unchanged; only the
    // numeric confidence may drop.
    #[test]
    fn confidence_score_cap_does_not_change_classification() {
        // infection_unknown: `let _ = expr` pattern — infect.confidence = Low
        let low_infect =
            StageEvidence::new(StageState::Unknown, Confidence::Low, "infection unknown");
        let infect_class = classify(
            &stage(StageState::Yes),
            &low_infect,
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::ReturnValue, "compute()"),
        );
        assert_eq!(
            infect_class,
            ExposureClass::InfectionUnknown,
            "infection_unknown class must be unchanged after RIPR-SPEC-0109"
        );

        // propagation_unknown: `.ok()` swallow — propagate.confidence = Low
        let low_propagate =
            StageEvidence::new(StageState::Unknown, Confidence::Low, "propagation unknown");
        let prop_class = classify(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &low_propagate,
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::ReturnValue, "self.persist(amount).ok();"),
        );
        assert_eq!(
            prop_class,
            ExposureClass::PropagationUnknown,
            "propagation_unknown class must be unchanged after RIPR-SPEC-0109"
        );
    }

    // #2450: the Exposed branch is the single over-credit guard. These tests
    // pin the contract that Exposed requires all of discriminate+infect+
    // propagate == Yes, and that any stage not-Yes downgrades to WeaklyExposed
    // (never Exposed). A regression here is the cardinal sin per AGENTS.md.

    // Reach through file or name proximity alone is `Weak`; a neighbour's
    // strong assertion must not make the changed owner `Exposed`.
    #[test]
    fn classify_does_not_emit_exposed_when_reach_is_weak() {
        let class = classify(
            &stage(StageState::Weak),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::ReturnValue, "(cents + 49) / 100"),
        );
        assert_eq!(class, ExposureClass::WeaklyExposed);
    }

    #[test]
    fn classify_emits_exposed_only_when_all_discriminate_infect_propagate_are_yes() {
        // All stages Yes → Exposed (the only path to Exposed).
        let all_yes = classify(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
        );
        assert_eq!(all_yes, ExposureClass::Exposed);

        // discriminate=Weak with infect=Yes + propagate=Yes at the FINAL guard
        // → WeaklyExposed, NOT Exposed. This is the cardinal-sin guard: all
        // stages Yes except discrimination → must not credit as Exposed.
        let weak_discrim = classify(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Weak),
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
        );
        assert_eq!(
            weak_discrim,
            ExposureClass::WeaklyExposed,
            "Weak discrimination must not credit as Exposed (over-credit guard)"
        );

        // infect=No (not Unknown) at the final guard → WeaklyExposed.
        // infect=No means infection is resolved as "not happening", so the
        // all-Yes Exposed check fails on the infect arm specifically.
        let no_infect = classify(
            &stage(StageState::Yes),
            &stage(StageState::No),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
        );
        assert_eq!(
            no_infect,
            ExposureClass::WeaklyExposed,
            "infect=No at the final guard must yield WeaklyExposed, not Exposed"
        );

        // propagate=No (not Unknown) at the final guard → WeaklyExposed.
        let no_propagate = classify(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::No),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
        );
        assert_eq!(
            no_propagate,
            ExposureClass::WeaklyExposed,
            "propagate=No at the final guard must yield WeaklyExposed, not Exposed"
        );

        // infect=Unknown → InfectionUnknown (never reaches the Exposed check).
        let unknown_infect = classify(
            &stage(StageState::Yes),
            &stage(StageState::Unknown),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
        );
        assert_eq!(unknown_infect, ExposureClass::InfectionUnknown);

        // propagate=Unknown → PropagationUnknown (never reaches the Exposed check).
        let unknown_prop = classify(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Unknown),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &probe(ProbeFamily::Predicate, "amount >= threshold"),
        );
        assert_eq!(unknown_prop, ExposureClass::PropagationUnknown);
    }

    #[test]
    fn classify_strong_oracle_on_wrong_sink_does_not_credit_as_exposed() {
        // A "strong" oracle that observes a DIFFERENT sink than the changed one
        // should produce observe/discriminate stages that are not both Yes.
        // The classifier never sees "wrong sink" directly — it sees the stage
        // states the callers produce. This test pins that observe=Yes but
        // discriminate=No (the wrong-sink case) yields WeaklyExposed, not Exposed.
        let wrong_sink = classify(
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::Yes),
            &stage(StageState::No),
            &probe(ProbeFamily::ErrorPath, "Err(InvalidCurrency)"),
        );
        assert_eq!(
            wrong_sink,
            ExposureClass::WeaklyExposed,
            "observe=Yes but discriminate=No (wrong sink) must not credit as Exposed"
        );
    }

    fn stage(state: StageState) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, "stage")
    }

    fn probe(family: ProbeFamily, expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:test".to_string()),
            location: SourceLocation::new("src/lib.rs", 1, 1),
            owner: None,
            family,
            delta: DeltaKind::Value,
            before: None,
            after: None,
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn related_test(name: &str, kind: OracleKind, strength: OracleStrength) -> RelatedTest {
        related_test_with_miss(name, kind, strength, None)
    }

    fn related_test_with_miss(
        name: &str,
        kind: OracleKind,
        strength: OracleStrength,
        miss: Option<RelatedTestMiss>,
    ) -> RelatedTest {
        related_test_full(name, kind, strength, miss, RelationReason::DirectOwnerCall)
    }

    fn related_test_full(
        name: &str,
        kind: OracleKind,
        strength: OracleStrength,
        miss: Option<RelatedTestMiss>,
        relation_reason: RelationReason,
    ) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: std::path::PathBuf::from("src/lib.rs"),
            line: 40,
            oracle: None,
            oracle_kind: kind,
            oracle_strength: strength,
            relation_reason: Some(relation_reason),
            relation_confidence: Some(RelationConfidence::High),
            miss,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }
    }

    fn missing(value: &str) -> Vec<MissingDiscriminatorFact> {
        vec![MissingDiscriminatorFact {
            value: value.to_string(),
            reason: format!("no related test call uses {value}"),
            flow_sink: None,
            shape: DiscriminatorShape::Boundary,
            drafted_boundary: None,
        }]
    }

    // #7393: the error_path family template presumes an `is_err()`-only
    // test; when the strongest related oracle is already exact, the step
    // names the missing error-arm input instead, like the predicate probe.
    #[test]
    fn exact_related_oracle_names_the_missing_input_for_error_path() {
        let step = exact_oracle_gap_next_step(
            &probe(
                ProbeFamily::ErrorPath,
                "Err(\"port must be greater than zero\".to_string())",
            ),
            &ExposureClass::WeaklyExposed,
            &[related_test(
                "parse_port_ok",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
            &missing("p == 0"),
            true,
        );
        let step = step.as_deref().unwrap_or("");
        assert!(step.contains("already asserts an exact value"), "{step}");
        assert!(step.contains("executes the changed error arm"), "{step}");
        assert!(step.contains("p == 0"), "{step}");
        assert!(!step.contains("is_err"), "{step}");
    }

    // #7393: the return_value family template presumes a broad assertion;
    // an exact `assert_eq!` may observe a different sink. Establish return
    // sensitivity before recommending a separating input.
    #[test]
    fn exact_related_oracle_names_the_missing_input_for_return_value() {
        let step = exact_oracle_gap_next_step(
            &probe(ProbeFamily::ReturnValue, "Ok(p) if p > 0"),
            &ExposureClass::WeaklyExposed,
            &[related_test(
                "parse_port_ok",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
            &[],
            true,
        );
        let step = step.as_deref().unwrap_or("");
        assert!(step.contains("already has an exact assertion"), "{step}");
        assert!(
            step.contains("Check whether the change can affect the returned value"),
            "{step}"
        );
        assert!(!step.contains("broad assertions"), "{step}");
        assert!(step.contains("If it can"), "{step}");
        assert!(step.contains("If it cannot"), "{step}");
        assert!(!step.contains("gap is not the assertion shape"), "{step}");
    }

    // Controls: a broad related oracle, a stronger broad oracle outranking
    // an exact one, no related rows, and a non-weak class all keep the
    // family template (None here).
    #[test]
    fn non_exact_oracle_keeps_the_family_template() {
        assert_eq!(
            exact_oracle_gap_next_step(
                &probe(ProbeFamily::ErrorPath, "Err(BadPort)"),
                &ExposureClass::WeaklyExposed,
                &[related_test(
                    "t",
                    OracleKind::BroadError,
                    OracleStrength::Strong
                )],
                &[],
                true,
            ),
            None
        );
        assert_eq!(
            exact_oracle_gap_next_step(
                &probe(ProbeFamily::ErrorPath, "Err(BadPort)"),
                &ExposureClass::WeaklyExposed,
                &[
                    related_test("exact", OracleKind::ExactValue, OracleStrength::Weak),
                    related_test("broad", OracleKind::BroadError, OracleStrength::Strong),
                ],
                &[],
                true,
            ),
            None
        );
        assert_eq!(
            exact_oracle_gap_next_step(
                &probe(ProbeFamily::ErrorPath, "Err(BadPort)"),
                &ExposureClass::WeaklyExposed,
                &[],
                &[],
                true,
            ),
            None
        );
        assert_eq!(
            exact_oracle_gap_next_step(
                &probe(ProbeFamily::ErrorPath, "Err(BadPort)"),
                &ExposureClass::Exposed,
                &[related_test(
                    "t",
                    OracleKind::ExactValue,
                    OracleStrength::Strong
                )],
                &[],
                true,
            ),
            None
        );
    }

    // A row whose only gap is the input carries an exact observing assertion:
    // the step still fires and names the input.
    #[test]
    fn missing_input_row_keeps_the_exact_oracle_step() {
        let step = exact_oracle_gap_next_step(
            &probe(ProbeFamily::ReturnValue, "Ok(p) if p > 0"),
            &ExposureClass::WeaklyExposed,
            &[related_test_with_miss(
                "parse_port_ok",
                OracleKind::ExactValue,
                OracleStrength::Strong,
                Some(RelatedTestMiss::MissingInput),
            )],
            &[],
            true,
        );
        let step = step.as_deref().unwrap_or("");
        assert!(step.contains("already has an exact assertion"), "{step}");
        assert!(
            step.contains("Check whether the change can affect the returned value"),
            "{step}"
        );
    }

    // An observation-unconfirmed row that does establish reach stays
    // eligible: the suite's exact assertion exists and the gap is the input,
    // so naming the input is honester than presuming a broad assertion
    // (#7393's own parse_port scenario).
    #[test]
    fn unconfirmed_observation_on_a_reach_row_keeps_the_step() {
        let step = exact_oracle_gap_next_step(
            &probe(ProbeFamily::ReturnValue, "Ok(p) if p > 0"),
            &ExposureClass::WeaklyExposed,
            &[related_test_with_miss(
                "parses_valid_port",
                OracleKind::ExactValue,
                OracleStrength::Strong,
                Some(RelatedTestMiss::ObservationUnconfirmed),
            )],
            &[],
            true,
        );
        let step = step.as_deref().unwrap_or("");
        assert!(step.contains("already has an exact assertion"), "{step}");
    }

    // A syntactic assert_eq! the classifier downgraded (a self-computed
    // expected value) is not an independent exact discriminator: "keep the
    // exact assertion" would be wrong advice, so the family template stands.
    #[test]
    fn downgraded_exact_strength_keeps_the_family_template() {
        assert_eq!(
            exact_oracle_gap_next_step(
                &probe(ProbeFamily::ReturnValue, "Ok(p) if p > 0"),
                &ExposureClass::WeaklyExposed,
                &[related_test(
                    "tax_matches_reference",
                    OracleKind::ExactValue,
                    OracleStrength::Weak,
                )],
                &[],
                true,
            ),
            None
        );
    }

    // A row that does not establish reach to the changed owner (same test
    // file, token coincidence) never dictates the owner advice, even when
    // its assertion is exact and strong (#7542: the weak direct oracle's
    // gap must not be erased by an unrelated exact test).
    #[test]
    fn non_reach_rows_do_not_dictate_the_step() {
        for reason in [
            RelationReason::SameTestFile,
            RelationReason::SameModule,
            RelationReason::OwnerNamedTest,
            RelationReason::WeakTokenSubstring,
        ] {
            assert_eq!(
                exact_oracle_gap_next_step(
                    &probe(ProbeFamily::ReturnValue, "Ok(p) if p > 0"),
                    &ExposureClass::WeaklyExposed,
                    &[
                        related_test_full(
                            "unrelated_exact",
                            OracleKind::ExactValue,
                            OracleStrength::Strong,
                            None,
                            reason,
                        ),
                        related_test(
                            "owed_positive",
                            OracleKind::RelationalCheck,
                            OracleStrength::Weak
                        ),
                    ],
                    &[],
                    true,
                ),
                None,
                "relation {reason:?} must not select the exact-oracle step"
            );
        }
    }

    // A reach-establishing row whose annotated miss is
    // `missing_exact_assertion` observes the behavior but does not pin the
    // exact value the change alters: there the assertion shape IS the gap,
    // so the family template stands.
    #[test]
    fn missing_exact_assertion_row_keeps_the_family_template() {
        assert_eq!(
            exact_oracle_gap_next_step(
                &probe(ProbeFamily::ReturnValue, "Ok(p) if p > 0"),
                &ExposureClass::WeaklyExposed,
                &[related_test_with_miss(
                    "pins_sibling_field",
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                    Some(RelatedTestMiss::MissingExactAssertion),
                )],
                &[],
                true,
            ),
            None
        );
    }

    // #12174: unsafe obligation requires static_unknown family, not label alone.
    #[test]
    fn unsafe_obligation_requires_static_unknown_family_and_label() {
        use crate::analysis::syntax::ra::UNCHECKED_ASSUMPTION_LABEL;

        assert!(!is_unsafe_obligation(&probe(
            ProbeFamily::ReturnValue,
            "unsafe fn x",
        )));
        assert!(!is_unsafe_obligation(&probe(
            ProbeFamily::Predicate,
            UNCHECKED_ASSUMPTION_LABEL,
        )));
        assert!(is_unsafe_obligation(&probe(
            ProbeFamily::StaticUnknown,
            "unsafe fn x",
        )));
        assert!(is_unsafe_obligation(&probe(
            ProbeFamily::StaticUnknown,
            UNCHECKED_ASSUMPTION_LABEL,
        )));
        assert!(!is_unsafe_obligation(&probe(
            ProbeFamily::StaticUnknown,
            "pub fn pick",
        )));
        assert!(!is_unsafe_obligation(&probe(
            ProbeFamily::StaticUnknown,
            "unsafe block",
        )));
    }

    #[test]
    fn assert_kind_matches_exactly_the_former_body_predicate() {
        fn legacy_test_body(body: &str) -> bool {
            body.contains("fixture") || body.contains("builder") || body.contains("arb_")
        }
        let bodies = [
            "let input = load_fixture();",
            "Request::builder().build()",
            "let n = arb_u32();",
            "plain test",
            "async fn helper() {}",
            "fixture_builder_arb_",
        ];
        for body in bodies {
            assert_eq!(
                super::AssertKind::matches_in_test_body(body),
                legacy_test_body(body),
                "body: {body}"
            );
        }
    }

    /// #14875: an expression marks a task boundary only through a whole
    /// `async`/`await`/`spawn` name segment, never through a longer word,
    /// a string or a comment that contains it.
    #[test]
    fn async_boundary_signals_match_whole_name_segments() {
        for expression in [
            "async fn run() {}",
            "async move { work() }",
            "task::spawn(future)",
            "std::thread::spawn(|| work())",
            "tokio::task::spawn_blocking(work)",
            "spawn_local(fut)",
            "value.await",
            "load().await?",
            "#[async_trait]",
            "asyncio.run(main())",
            "proc.spawnSync(cmd)",
            "readFileAsync(path)",
            "HTTPSpawn::new()",
            "IOAsync",
            "r#async",
            "async spawn await",
        ] {
            assert!(
                super::AssertKind::matches_in_expression(expression),
                "expression: {expression}"
            );
        }
        for expression in [
            "plain expression",
            "s.respawn_limit > n",
            "self.asynchronous_mode",
            "awaiting_ack == 0",
            "spawned_count + 1",
            "spawner.len()",
            "respawnLimit > n",
            "asynchronousMode",
            "HTTPSpawner",
            "log(\"async done\")",
            "x > 1 // await later",
        ] {
            assert!(
                !super::AssertKind::matches_in_expression(expression),
                "expression: {expression}"
            );
        }
    }

    // #11547: a changed compile-time assertion names its own stop reason;
    // other item-level text keeps `no_changed_rust_line`.
    #[test]
    fn compile_time_assertions_are_recognised_by_written_shape_only() {
        for text in [
            "const _: () = assert!(N <= 16);",
            "const _: () = assert_eq!(N, 16);",
            "const _ : () = assert!(N <= 16);",
            "const _: () = { assert!(N <= 16); };",
            "const_assert!(N <= 16);",
            "static_assertions::const_assert!(N <= 16);",
            "const_assert_eq!(N, 16);",
        ] {
            assert!(is_compile_time_assertion(text), "{text}");
        }
        for text in [
            "pub const N: usize = 12;",
            "const _: () = ();",
            "assert!(N <= 16);",
            "other::const_assert!(N <= 16);",
            "let const_assert = 1;",
            "const _: () = register(\"assertion_table\");",
            "const _: u32 = assertive();",
            "const _: &str = \"assert!(x)\";",
            "const_assert_ne_helper!(x);",
            "const_assert_helper(x);",
        ] {
            assert!(!is_compile_time_assertion(text), "{text}");
        }
    }

    fn bool_owner(signature: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::is_small".to_string()),
            name: "is_small".to_string(),
            file: std::path::PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: format!("{signature} {{\n    n == 1 || n == 2\n}}").into(),
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            returns: Vec::new(),
            literals: crate::analysis::facts::SharedFactSlice::empty(),
            source_role: crate::analysis::facts::FunctionSourceRole::Production,
            attrs: crate::analysis::facts::SharedFactSlice::empty(),
            impl_attrs: Vec::new(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    /// Every asserted call settles on the owner.
    fn any_call(_: &RelatedTest, _: &[String]) -> bool {
        true
    }

    fn bool_row(name: &str, oracle: &str) -> RelatedTest {
        let mut row = related_test_full(
            name,
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
            Some(RelatedTestMiss::WeakAssertion),
            RelationReason::DirectOwnerCall,
        );
        row.oracle = Some(oracle.to_string());
        row
    }

    // #15149: `assert!(is_small(1))` and `assert!(!is_small(3))` already
    // check exact booleans, so "replace broad assertions with exact
    // equality" names a rewrite that checks the same values; the step names
    // the missing deciding input instead.
    #[test]
    fn bool_owner_exact_result_rows_name_the_deciding_input() {
        let owner = bool_owner("pub fn is_small(n: u32) -> bool");
        let rows = [
            bool_row("one_is_small", "assert!(is_small(1));"),
            bool_row("three_is_not_small", "assert!(!is_small(3), \"3, {}\", 3);"),
            bool_row("pathed", "std::assert!(crate::is_small(4));"),
            bool_row("core_rooted", "core::assert!(is_small(1));"),
            bool_row("abs_std", "::std::assert!(!is_small(3));"),
            bool_row("abs_core", "::core::assert!(super::is_small(1));"),
        ];
        let step = bool_owner_exact_result_next_step(
            &probe(ProbeFamily::ReturnValue, "n == 1 || n == 2"),
            &ExposureClass::WeaklyExposed,
            Some(&owner),
            &rows,
            &any_call,
        );
        let step = step.as_deref().unwrap_or("");
        assert!(
            step.contains("already assert the exact boolean result of `is_small`"),
            "{step}"
        );
        assert!(
            step.contains("input whose boolean result differs between the old and new code"),
            "{step}"
        );
        assert!(!step.contains("Replace broad assertions"), "{step}");
    }

    // The discriminating negatives: each keeps the family template.
    #[test]
    fn bool_owner_exact_result_step_needs_every_reaching_row_to_assert_the_owner_result() {
        let owner = bool_owner("pub fn is_small(n: u32) -> bool");
        let return_value = probe(ProbeFamily::ReturnValue, "n == 1 || n == 2");
        let exact = bool_row("one_is_small", "assert!(is_small(1));");
        for broad in [
            "assert!(is_small(1) || is_small(2));",
            "assert!(is_small(1).then_some(()).is_some());",
            "assert!(other(is_small(1)));",
            "assert!(not_is_small(1));",
            "assert!(x.is_small(1));",
            "debug_assert!(is_small(1));",
            "assert_ne!(is_small(1), false);",
            "assert!(is_small::<u32>(1));",
            "assert!(r);",
            "assert!(!!is_small(1));",
            "assert!((|| is_small(1))());",
            "assert!((|n| is_small(n))(1));",
        ] {
            let rows = [exact.clone(), bool_row("broad", broad)];
            assert_eq!(
                bool_owner_exact_result_next_step(
                    &return_value,
                    &ExposureClass::WeaklyExposed,
                    Some(&owner),
                    &rows,
                    &any_call,
                ),
                None,
                "{broad}"
            );
        }
        let mut no_oracle = exact.clone();
        no_oracle.oracle = None;
        let mut shape_gap = exact.clone();
        shape_gap.miss = Some(RelatedTestMiss::MissingExactAssertion);
        assert_eq!(
            bool_owner_exact_result_next_step(
                &return_value,
                &ExposureClass::WeaklyExposed,
                Some(&owner),
                &[exact.clone(), shape_gap],
                &any_call,
            ),
            None,
            "a missing_exact_assertion row keeps the family template"
        );
        let rows = [exact.clone()];
        for (label, step) in [
            (
                "a non-bool owner",
                bool_owner_exact_result_next_step(
                    &return_value,
                    &ExposureClass::WeaklyExposed,
                    Some(&bool_owner("pub fn is_small(n: u32) -> u8")),
                    &rows,
                    &any_call,
                ),
            ),
            (
                "an exposed class",
                bool_owner_exact_result_next_step(
                    &return_value,
                    &ExposureClass::Exposed,
                    Some(&owner),
                    &rows,
                    &any_call,
                ),
            ),
            (
                "a predicate probe",
                bool_owner_exact_result_next_step(
                    &probe(ProbeFamily::Predicate, "n == 1 || n == 2"),
                    &ExposureClass::WeaklyExposed,
                    Some(&owner),
                    &rows,
                    &any_call,
                ),
            ),
            (
                "no owner",
                bool_owner_exact_result_next_step(
                    &return_value,
                    &ExposureClass::WeaklyExposed,
                    None,
                    &rows,
                    &any_call,
                ),
            ),
            (
                "no rows",
                bool_owner_exact_result_next_step(
                    &return_value,
                    &ExposureClass::WeaklyExposed,
                    Some(&owner),
                    &[],
                    &any_call,
                ),
            ),
            (
                "an unreadable oracle",
                bool_owner_exact_result_next_step(
                    &return_value,
                    &ExposureClass::WeaklyExposed,
                    Some(&owner),
                    &[exact.clone(), no_oracle],
                    &any_call,
                ),
            ),
        ] {
            assert_eq!(step, None, "{label}");
        }
        // A proximity-only row does not reach the owner, so it neither
        // blocks nor supplies the step.
        let mut proximity = bool_row("nearby", "assert!(helper(1));");
        proximity.relation_reason = Some(RelationReason::SameTestFile);
        assert!(
            bool_owner_exact_result_next_step(
                &return_value,
                &ExposureClass::WeaklyExposed,
                Some(&owner),
                &[exact, proximity.clone()],
                &any_call,
            )
            .is_some()
        );
        assert_eq!(
            bool_owner_exact_result_next_step(
                &return_value,
                &ExposureClass::WeaklyExposed,
                Some(&owner),
                &[proximity],
                &any_call,
            ),
            None
        );
    }

    // #15537 review: `let _ = crate::a::is_small(1); assert!(crate::b::is_small(1));`
    // reaches the owner in `a` but asserts the rival in `b`. The step reads
    // the asserted call's own identity, so the rival's assertion keeps the
    // family template; the owner's own assertion is the positive control.
    #[test]
    fn bool_owner_step_needs_the_asserted_call_to_be_the_owner() {
        let owner = bool_owner("pub fn is_small(n: u32) -> bool");
        let return_value = probe(ProbeFamily::ReturnValue, "n == 1 || n == 2");
        let owner_module = |_: &RelatedTest, qualifier: &[String]| {
            qualifier == ["crate".to_string(), "a".to_string()]
        };
        let rival = [bool_row("mixed", "assert!(crate::b::is_small(1));")];
        assert_eq!(
            bool_owner_exact_result_next_step(
                &return_value,
                &ExposureClass::WeaklyExposed,
                Some(&owner),
                &rival,
                &owner_module,
            ),
            None
        );
        let own = [bool_row("mixed", "assert!(crate::a::is_small(1));")];
        assert!(
            bool_owner_exact_result_next_step(
                &return_value,
                &ExposureClass::WeaklyExposed,
                Some(&owner),
                &own,
                &owner_module,
            )
            .is_some()
        );
        // One unsettled row among owner rows keeps the template.
        let both = [own[0].clone(), rival[0].clone()];
        assert_eq!(
            bool_owner_exact_result_next_step(
                &return_value,
                &ExposureClass::WeaklyExposed,
                Some(&owner),
                &both,
                &owner_module,
            ),
            None
        );
    }
}
