use super::super::rust_index::{
    OracleFact, OracleTextShape, TestSummary, extract_identifier_tokens, has_oracle_text_shape,
    is_inequality_assertion,
};

use super::arm_selection::ArmSelector;
use super::builder_override::{
    KeyedFallback, OwnerCall, is_literal_key, literals_equal, overridden_fields_read, setter_names,
    struct_literal_fields,
};
use super::propagation_witness::{
    assertion_field_collection_reads, assertion_keyed_getter_reads,
    assertion_observes_direct_collection, direct_collection_mutation_receiver,
    field_collection_insert_key, field_collection_write_path,
};
use super::reach::{invokes_opaque_macro, is_proximity_only};
use super::rust_string_literals;
use super::text::changed_error_variant;
use crate::analysis::classifier::oracle_binds_sink_identity;
use crate::analysis::extract::mask_comments_and_strings;
use crate::domain::*;

/// Shared oracle provenance at the reveal admission boundary. The same
/// execution/macro decision also gates predicate boundary pairing; it applies
/// before token, strength, observation, or owner-pin confirmation.
pub(in crate::analysis) struct ReturnOracleAdmission<'a> {
    pub(in crate::analysis) owner_return_pin: &'a dyn Fn(&TestSummary, &OracleFact) -> bool,
    pub(in crate::analysis) assertion_admitted: &'a dyn Fn(&TestSummary, &OracleFact) -> bool,
    /// Whether the assertion's macros are the standard ones (no visible
    /// binding rebinds `assert_ne!`, `assert!` or `matches!`). Consulted
    /// only before a RIPR-SPEC-0231 sibling exclusion is credited.
    pub(in crate::analysis) sibling_exclusion_admitted:
        &'a dyn Fn(&TestSummary, &OracleFact) -> bool,
    /// Whether a test related only by file or module may run the owner
    /// (#6297). One that cannot, by any name path, does not confirm a match
    /// arm while another related test reaches the owner.
    pub(in crate::analysis) proximity_may_reach_owner: &'a dyn Fn(&TestSummary) -> bool,
    /// Names the changed owner's signature binds. A test can never hold
    /// the owner's parameter, so for a value probe a shared parameter name
    /// confirms observation only in an assertion that calls the owner
    /// (#5830: `subtotal` in `tax(subtotal)` matched an unrelated
    /// `subtotal(3, 100)` test).
    pub(in crate::analysis) owner_parameters: &'a [String],
    /// Whether a function of this name transitively calls the changed
    /// owner. An equality whose one side calls the owner and whose other
    /// side reaches it computes its expected value through the changed
    /// code, so the two sides move together (RIPR-SPEC-0035
    /// self-computed expected value, #5830). The first argument is the
    /// type a call names (`Money` in `Money::new(8)`), when it names one:
    /// such a call reaches the owner only through that type's function.
    pub(in crate::analysis) expected_reaches_owner: &'a dyn Fn(Option<&str>, &str) -> bool,
    /// RIPR-SPEC-0094 Part D: whether an effect observer can carry the state
    /// the changed effect writes (`EffectStateCarrier::admits`). Consulted
    /// only for effect families after `effect_observer_confirms`.
    pub(in crate::analysis) effect_state_carried: &'a dyn Fn(&TestSummary, &OracleFact) -> bool,
    /// RIPR-SPEC-0040 builder override (#6613): whether the owner type's
    /// method `(type, setter, field)` plainly stores its argument in the
    /// field (`builder_override::setter_plainly_writes`). Only a verified
    /// setter defeats a changed default.
    pub(in crate::analysis) setter_writes_field: &'a dyn Fn(&str, &str, &str) -> bool,
    /// RIPR-SPEC-0228 getter credit: whether the owner type's method
    /// `(type, getter, field)` returns that field path itself
    /// (`builder_override::getter_plainly_reads`). Only a verified getter
    /// lets a read through a method confirm a field-path collection write.
    pub(in crate::analysis) getter_reads_field: &'a dyn Fn(&str, &str, &str) -> bool,
    /// RIPR-SPEC-0228 keyed getter credit: whether the owner type's method
    /// `(type, getter, field)` looks its one argument up in that field path
    /// (`builder_override::getter_plainly_reads_key`).
    /// `Some` carries what the getter returns for a missing key.
    pub(in crate::analysis) getter_reads_key: &'a dyn Fn(&str, &str, &str) -> Option<KeyedFallback>,
    /// The owner's call parameters in order, receiver excluded; a
    /// destructured parameter is an empty name. A keyed read confirms a
    /// map insert only through the argument the insert's key comes from.
    pub(in crate::analysis) owner_argument_names: &'a [String],
    /// Whether the owner declares a return type. A bare call to such an
    /// owner (`s.place("A", 0);`) may have returned before its insert, so
    /// only a call whose result is consumed confirms a keyed read.
    pub(in crate::analysis) owner_returns_value: bool,
    /// RIPR-SPEC-0228 keyed reads: whether `(type, ctor, field)`, the
    /// constructor a test builds its binding with, leaves that map field
    /// empty (`builder_override::constructor_starts_empty`).
    pub(in crate::analysis) constructor_starts_empty: &'a dyn Fn(&str, &str, &str) -> bool,
    /// RIPR-SPEC-0228 keyed reads: whether `(type, method, expected)`, a
    /// call the test makes on its binding before the owner call, can never
    /// store the expected literal (`builder_override::method_cannot_store`).
    pub(in crate::analysis) call_cannot_store: &'a dyn Fn(&str, &str, &str) -> bool,
}

#[cfg(test)]
fn reveal_evidence(
    probe: &Probe,
    related_tests: &[(&TestSummary, RelationReason)],
) -> (StageEvidence, StageEvidence, Vec<RelatedTest>) {
    let (observe, discriminate, related, _) = reveal_evidence_with_expression(
        probe,
        &probe.expression,
        related_tests,
        &[],
        &|_, _| false,
        &|_, _| false,
        &ReturnOracleAdmission {
            owner_return_pin: &|_, _| false,
            assertion_admitted: &|_, _| true,
            sibling_exclusion_admitted: &|_, _| true,
            proximity_may_reach_owner: &|_| false,
            owner_parameters: &[],
            expected_reaches_owner: &|_, _| false,
            effect_state_carried: &|_, _| true,
            setter_writes_field: &|_, _, _| true,
            getter_reads_field: &|_, _, _| false,
            getter_reads_key: &|_, _, _| None,
            owner_argument_names: &[],
            owner_returns_value: true,
            constructor_starts_empty: &|_, _, _| false,
            call_cannot_store: &|_, _, _| false,
        },
        None,
    );
    (observe, discriminate, related)
}

#[cfg(test)]
#[allow(
    clippy::too_many_arguments,
    reason = "reveal's grouped inputs plus the optional RIPR-SPEC-0229 arm selector"
)]
fn reveal_evidence_with_expression(
    probe: &Probe,
    analysis_expression: &str,
    related_tests: &[(&TestSummary, RelationReason)],
    owner_local_bindings: &[String],
    same_name_import_defeats: &dyn Fn(&TestSummary, &str) -> bool,
    cross_package_name_defeats: &dyn Fn(&TestSummary, &str) -> bool,
    return_admission: &ReturnOracleAdmission<'_>,
    arm_selector: Option<&ArmSelector>,
) -> (StageEvidence, StageEvidence, Vec<RelatedTest>, usize) {
    let outcome = reveal_outcome(
        probe,
        analysis_expression,
        related_tests,
        owner_local_bindings,
        same_name_import_defeats,
        cross_package_name_defeats,
        return_admission,
        arm_selector,
    );
    (
        outcome.observe,
        outcome.discriminate,
        outcome.related,
        outcome.related_total,
    )
}

/// The reveal stages plus whether the owner pin was credited.
pub(in crate::analysis) struct RevealOutcome {
    pub(in crate::analysis) observe: StageEvidence,
    pub(in crate::analysis) discriminate: StageEvidence,
    pub(in crate::analysis) related: Vec<RelatedTest>,
    pub(in crate::analysis) related_total: usize,
    /// #6692: an assertion credited to this probe pinned the owner's
    /// return value through `ReturnOracleAdmission::owner_return_pin`
    /// after every reveal gate (name-only relations next to a
    /// reach-bearing test, foreign same-name imports, cross-package
    /// same-name definitions, the exact error variant).
    pub(in crate::analysis) owner_pin_credited: bool,
}

#[allow(
    clippy::too_many_arguments,
    reason = "reveal's grouped inputs plus the optional RIPR-SPEC-0229 arm selector"
)]
pub(in crate::analysis) fn reveal_outcome(
    probe: &Probe,
    analysis_expression: &str,
    related_tests: &[(&TestSummary, RelationReason)],
    owner_local_bindings: &[String],
    same_name_import_defeats: &dyn Fn(&TestSummary, &str) -> bool,
    cross_package_name_defeats: &dyn Fn(&TestSummary, &str) -> bool,
    return_admission: &ReturnOracleAdmission<'_>,
    arm_selector: Option<&ArmSelector>,
) -> RevealOutcome {
    if related_tests.is_empty() {
        return RevealOutcome {
            observe: StageEvidence::new(
                StageState::No,
                Confidence::Medium,
                "No reachable test oracle found",
            ),
            discriminate: StageEvidence::new(
                StageState::No,
                Confidence::Medium,
                "No assertion can discriminate the changed behavior without a reachable test",
            ),
            related: Vec::new(),
            related_total: 0,
            owner_pin_credited: false,
        };
    }

    let analysis = analyze_related_assertions(
        probe,
        analysis_expression,
        related_tests,
        owner_local_bindings,
        same_name_import_defeats,
        cross_package_name_defeats,
        return_admission,
        arm_selector,
    );
    let (related, related_tests_total) = finalize_related_tests(analysis.related);
    let observe = build_observe_evidence(analysis.matched_any, analysis.refused_context);
    let discriminate = if analysis.refused_context && !analysis.matched_any {
        StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            "No statically established assertion can discriminate the changed behavior; execution or macro binding is unestablished (rust_assertion_context_unestablished)",
        )
    } else if needs_token_confirmation(&probe.family)
        && analysis.matched_any
        && !analysis.observation_unverified
        && !analysis.strongest_observation_confirmed
    {
        StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            "Strongest oracle does not confirm observation of the changed expression; a weaker assertion cannot supply its confirmation (oracle_confirmation_mixed)",
        )
    } else if analysis.observation_unverified && analysis.override_defeated {
        StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            BUILDER_OVERRIDE_UNOBSERVED_SUMMARY,
        )
    } else if analysis.observation_unverified && analysis.proximity_confirmation_withheld {
        // The generic unconfirmed summary says no assertion text references
        // the arm, which is false when a same-file test names its variant.
        StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            if matches!(probe.family, ProbeFamily::MatchArm) {
                PROXIMITY_CONFIRMATION_WITHHELD
            } else {
                PROXIMITY_VARIANT_CONFIRMATION_WITHHELD
            },
        )
    } else {
        build_discriminate_evidence(
            &analysis.strongest,
            &analysis.strongest_kind,
            &probe.family,
            analysis.observation_unverified,
        )
    };

    RevealOutcome {
        observe,
        discriminate,
        related,
        related_total: related_tests_total,
        owner_pin_credited: analysis.owner_pin_credited,
    }
}

/// Discriminator summary for a struct literal (or one of its fields) whose
/// read field the test overwrote on the owner's result before asserting on
/// it (RIPR-SPEC-0040 builder override, #6613). Keeps the
/// `(observation_unverified)` token downstream consumers key on.
const BUILDER_OVERRIDE_UNOBSERVED_SUMMARY: &str = "Discriminator unconfirmed: the related test overwrites the changed field on the owner's result before asserting on it, so no assertion observes this probe's changed value (observation_unverified)";

const PROXIMITY_CONFIRMATION_WITHHELD: &str = "Discriminator unconfirmed: no assertion in a test that calls or otherwise reaches this function names the changed arm; a test that only shares its file or module, and calls nothing that reaches the function, cannot confirm the arm (observation_unverified)";

const PROXIMITY_VARIANT_CONFIRMATION_WITHHELD: &str = "Discriminator unconfirmed: no assertion in a test that calls or otherwise reaches this function names the changed error variant; a test that only shares its file or module pins that variant on another function's result (observation_unverified)";

struct RevealAssertionAnalysis {
    related: Vec<RelatedTest>,
    strongest: OracleStrength,
    strongest_kind: OracleKind,
    /// Confirmation belongs to the assertion supplying the selected oracle
    /// strength. A weaker token match must not upgrade an unrelated exact
    /// oracle, even when both assertions are in the same related test.
    strongest_observation_confirmed: bool,
    matched_any: bool,
    /// #6692: some credited assertion pinned the owner's return value.
    owner_pin_credited: bool,
    refused_context: bool,
    /// True when a test related only by file or module matched an assertion
    /// but could not confirm a match arm, because another related test reaches
    /// the owner (#6297).
    proximity_confirmation_withheld: bool,
    /// True when a matched assertion's only confirming tokens were struct
    /// fields the test overwrote on the owner's result before reading them
    /// (`builder_override`, #6613). Names the downgrade only; the class is
    /// decided by `observation_unverified`.
    override_defeated: bool,
    /// True when this probe's family requires a `token_match` to confirm that
    /// an assertion actually references the specific changed sub-expression, and
    /// no such match has fired yet.
    ///
    /// Applies to: `MatchArm`, `ReturnValue`, `FieldConstruction`, `SideEffect`,
    /// `CallDeletion`, `ErrorPath`. For each of these families, the broad
    /// `family_match` or `assertion_count == 1` matcher alone is insufficient:
    /// it tells us an oracle of the right shape exists in a reachable test, but
    /// cannot confirm it observes *this particular* changed expression (vs. a
    /// sibling, an unrelated field, or a different call site).
    ///
    /// Confirmation differs by family:
    /// - **Value** families (MatchArm, ReturnValue, FieldConstruction,
    ///   ErrorPath): the only static signal of specificity is a `token_match` —
    ///   an assertion whose text contains an identifier token from the probe
    ///   expression. For probes whose changed expression constructs an exact
    ///   error variant, `assertion_matches_probe_detail`'s
    ///   `ExactErrorVariant` fast-path returns `has_token_match=true` only when
    ///   the assertion text contains that specific variant token (RIPR-SPEC-0106,
    ///   Part B), so a genuine variant-pinning oracle clears this guard. A sibling
    ///   variant, a broad `is_err()`, or a non-variant exact-value oracle does not.
    /// - **Effect** families (SideEffect, CallDeletion): the canonical observer
    ///   is a mock/expectation/snapshot that kind-matches the seam without
    ///   sharing a token, so a genuine effect observer (`effect_observer_confirms`)
    ///   confirms in addition to `token_match`.
    ///
    /// Cleared as soon as a confirming assertion fires.
    observation_unverified: bool,
}

/// Returns true for families where an assertion must specifically reference the
/// changed sub-expression to confirm observation. For **value** families
/// (MatchArm, ReturnValue, FieldConstruction, ErrorPath) the only static
/// confirmation signal is a `token_match`. For probes whose changed expression
/// constructs an exact error variant, a genuine variant-pinning oracle
/// (`ExactErrorVariant` whose text contains that specific variant token) sets
/// `has_token_match=true` in `assertion_matches_probe_detail` (RIPR-SPEC-0106,
/// Part B), clearing this guard. A broad `is_err()` or an exact-value oracle on
/// a sibling result does not. For **effect** families (SideEffect,
/// CallDeletion) the legitimate observer is often a mock/expectation that
/// **kind-matches the seam** without sharing any probe token; for those, a
/// seam-kind match also confirms observation (see `effect_observer_confirms`).
fn needs_token_confirmation(family: &ProbeFamily) -> bool {
    matches!(
        family,
        ProbeFamily::MatchArm
            | ProbeFamily::ReturnValue
            | ProbeFamily::FieldConstruction
            | ProbeFamily::SideEffect
            | ProbeFamily::CallDeletion
            | ProbeFamily::ErrorPath
    )
}

/// Returns true for the **effect** families (SideEffect, CallDeletion) whose
/// changed behavior is a side effect or outbound call. For these, the canonical
/// observer is a mock/expectation that kind-matches the seam rather than a
/// value assertion that names a token from the changed expression. A genuine
/// effect observer therefore confirms observation even without a `token_match`.
fn is_effect_family(family: &ProbeFamily) -> bool {
    matches!(family, ProbeFamily::SideEffect | ProbeFamily::CallDeletion)
}

fn effect_target_tokens(expression: &str) -> Vec<String> {
    let target_end = expression.find(['(', '=']).unwrap_or(expression.len());
    extract_identifier_tokens(&expression[..target_end])
}

/// Returns true when `assertion` is a genuine **effect observer** that
/// kind-matches an effect seam: a mock/expectation, a snapshot, or a
/// whole-object equality capturing the resulting state. This is intentionally
/// narrower than `oracle_matches_family` for effect families — it excludes the
/// broad assertion-or-expectation text shape, so a plain non-observing assertion
/// (e.g. `assert!(result)`) does
/// **not** clear `observation_unverified`. Only a real expectation/snapshot
/// observer does.
fn effect_observer_confirms(assertion: &OracleFact) -> bool {
    matches!(
        assertion.kind,
        OracleKind::MockExpectation | OracleKind::Snapshot | OracleKind::WholeObjectEquality
    )
}

fn collection_observer_confirms(expression: &str, assertion: &OracleFact) -> bool {
    let Some(receiver) = direct_collection_mutation_receiver(expression) else {
        return false;
    };
    assertion_observes_direct_collection(&assertion.text, receiver)
}

/// RIPR-SPEC-0228: whether `assertion` observes the field collection `path`
/// that the owner writes (`self.log.push(..)`). One compared side must read
/// that field from the binding the test called the owner on, after that
/// call (`inv.log`, `inv.log.len()`), or read it through a method whose body
/// returns that field (`inv.history()` with
/// `fn history(&self) -> &[Event] { &self.log }`). A shared name is not a
/// read: `inv.log()` returning another field does not confirm.
fn field_collection_observer_confirms(
    path: &str,
    count_reads: bool,
    owner: Option<OwnerCall<'_>>,
    test: &TestSummary,
    assertion: &OracleFact,
    getter_reads_field: &dyn Fn(&str, &str, &str) -> bool,
) -> bool {
    let Some(owner) = owner else {
        return false;
    };
    assertion_field_collection_reads(&assertion.text, path, count_reads)
        .into_iter()
        .any(|read| {
            owner_call_precedes_read(
                &test.body,
                test.start_line,
                &read.binding,
                owner.name(),
                assertion.line,
            ) && read.getter.as_deref().is_none_or(|getter| {
                owner
                    .impl_type()
                    .is_some_and(|impl_type| getter_reads_field(impl_type, getter, path))
            })
        })
}

/// RIPR-SPEC-0228 keyed reads: whether `assertion` reads back the entry the
/// owner inserted into the field map `path` under its argument number
/// `key_index`. One compared side must be `binding.getter(key)` on the
/// binding the test called the owner on; `getter` must resolve to an
/// inherent method of the owner type that looks its argument up in `path`
/// (`getter_reads_key`); and an accepted owner call before the read must
/// pass `key` in that argument position. The `assert_eq!` must expect a
/// literal the getter cannot return for a missing key
/// ([`KeyedFallback::differs_from`]): `assert_eq!(s.count("A"), 0)` passes
/// whether or not `.unwrap_or(0)` found the insert. `inv.on_hand("A")`
/// after `inv.ship("A", 3)` reads the entry `self.stock.insert(sku, ..)`
/// wrote; `inv.on_hand("B")` does not.
fn keyed_field_read_confirms(
    path: &str,
    key_index: usize,
    owner: Option<OwnerCall<'_>>,
    test: &TestSummary,
    assertion: &OracleFact,
    return_admission: &ReturnOracleAdmission<'_>,
) -> bool {
    let Some(owner) = owner else {
        return false;
    };
    let Some(impl_type) = owner.impl_type() else {
        return false;
    };
    // A `#[should_panic]` test passes when the values differ.
    if test
        .attrs
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(test.body.as_str()))
        .any(|text| text.contains("should_panic"))
    {
        return false;
    }
    assertion_keyed_getter_reads(&assertion.text)
        .into_iter()
        .any(|read| {
            (return_admission.getter_reads_key)(impl_type, &read.getter, path)
                .is_some_and(|fallback| fallback.differs_from(&read.expected))
                && owner_call_passes_key(
                    &test.body,
                    test.start_line,
                    assertion.line,
                    &KeyedCall {
                        binding: &read.binding,
                        method: owner.name(),
                        getter: &read.getter,
                        expected: &read.expected,
                        impl_type,
                        returns_value: return_admission.owner_returns_value,
                        index: key_index,
                        key: &read.key,
                        path,
                        starts_empty: return_admission.constructor_starts_empty,
                        cannot_store: return_admission.call_cannot_store,
                    },
                )
        })
}

/// Whether `body`, whose first line is file line `start_line`, calls
/// `binding.method(..)` on a line before `read_line` without rebinding
/// `binding` (`let binding = ..`) between that call and the read. Comments
/// and string contents never count as a call.
pub(in crate::analysis) fn owner_call_precedes_read(
    body: &str,
    start_line: usize,
    binding: &str,
    method: &str,
    read_line: usize,
) -> bool {
    !owner_calls_before_read(body, start_line, binding, method, read_line).is_empty()
}

/// The offsets in `body` of the `binding.method(..)` calls that
/// [`owner_call_precedes_read`] accepts.
fn owner_calls_before_read(
    body: &str,
    start_line: usize,
    binding: &str,
    method: &str,
    read_line: usize,
) -> Vec<usize> {
    let masked = mask_comments_and_strings(body);
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let line_at = |at: usize| start_line + masked[..at].matches('\n').count();
    let bound_at = |at: usize| {
        masked[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !(ident(before) || before == '.'))
    };
    let name_ends = |at: usize| masked[at..].chars().next().is_none_or(|next| !ident(next));
    // Offsets where `binding` stops naming the value the call wrote to: a
    // `let [mut] binding` shadow or a plain `binding = ..` assignment. Offsets,
    // not lines, so a rebinding on the call's own line still counts.
    let mut rebindings: Vec<usize> = masked
        .match_indices("let ")
        .filter(|(at, _)| bound_at(*at))
        .filter_map(|(at, keyword)| {
            let rest = masked[at + keyword.len()..].trim_start();
            let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
            let after = rest.strip_prefix(binding)?;
            let end = masked.len() - after.len();
            name_ends(end).then_some(at)
        })
        .collect();
    rebindings.extend(
        masked
            .match_indices(binding)
            .filter(|(at, name)| bound_at(*at) && name_ends(at + name.len()))
            .filter(|(at, name)| {
                let rest = masked[at + name.len()..].trim_start();
                rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>")
            })
            .map(|(at, _)| at),
    );
    let call = format!("{binding}.{method}(");
    masked
        .match_indices(&call)
        .filter(|(at, _)| bound_at(*at) && runs_unconditionally(&masked, *at))
        .map(|(at, _)| at)
        .filter(|at| {
            line_at(*at) < read_line
                && !rebindings
                    .iter()
                    .any(|rebound| *rebound > *at && line_at(*rebound) <= read_line)
        })
        .collect()
}

/// One accepted owner call a keyed read is checked against.
struct KeyedCall<'a> {
    /// The test binding the owner is called on and the getter read from.
    binding: &'a str,
    method: &'a str,
    /// The verified keyed getter the assertion reads through.
    getter: &'a str,
    /// The literal the assertion expects the entry to hold.
    expected: &'a str,
    /// The owner's impl type, which `binding` must be built as.
    impl_type: &'a str,
    /// Whether the owner declares a return type.
    returns_value: bool,
    /// The insert key's argument position, receiver excluded.
    index: usize,
    /// The key the getter reads, as spelled in the assertion.
    key: &'a str,
    /// The map field the owner inserts into.
    path: &'a str,
    /// Whether `(type, ctor, field)` builds that field empty.
    starts_empty: &'a dyn Fn(&str, &str, &str) -> bool,
    /// Whether `(type, method, expected)` can never store `expected`.
    cannot_store: &'a dyn Fn(&str, &str, &str) -> bool,
}

/// RIPR-SPEC-0228 keyed reads: whether some owner call that
/// [`owner_call_precedes_read`] accepts passes `key` as its argument number
/// `index` (receiver excluded), so the entry the owner inserted under that
/// parameter is the one the test reads back. The call must be a bare
/// statement (`inv.ship("A", 3);`), optionally ending in `.unwrap()`,
/// `.expect(..)` or `?`, or bind its result only after one of those
/// (`let r = inv.ship("A", 3).unwrap();`): an owner that returned before the
/// insert would then have failed the test. A result kept or projected
/// unconsumed (`let r = inv.ship(..)`, `let _ = ..`, `.unwrap_err()`,
/// `.is_err()`, `.is_ok()`) or a call inside an assertion does not count,
/// and neither does a bound result named again before the read: a returned
/// `&mut Self` (`r.set("A", 7)`) writes the entry without naming the
/// binding. Fails closed on anything it cannot read.
fn owner_call_passes_key(
    body: &str,
    start_line: usize,
    read_line: usize,
    call: &KeyedCall,
) -> bool {
    let masked = mask_comments_and_strings(body);
    let normalize = |text: &str| text.trim().trim_start_matches('&').trim().to_string();
    let key = normalize(call.key);
    let name = format!("{}.{}", call.binding, call.method);
    let Some(lines_before) = read_line.checked_sub(start_line) else {
        return false;
    };
    let read_at: usize = masked
        .split_inclusive('\n')
        .take(lines_before)
        .map(str::len)
        .sum();
    // The read line must open with the assertion: `s.set("K", 5); assert_eq!(..)`
    // writes the entry on the read's own line, outside the checked window.
    if !masked
        .get(read_at..)
        .is_some_and(|line| line.trim_start().starts_with("assert"))
    {
        return false;
    }
    owner_calls_before_read(body, start_line, call.binding, call.method, read_line)
        .into_iter()
        .any(|at| {
            let statement_start = masked[..at].rfind([';', '{', '}']).map_or(0, |i| i + 1);
            let Some((end, consumed)) = call_statement_tail(&masked, at, name.len() + 1) else {
                return false;
            };
            let prefix = masked[statement_start..at].trim();
            let shape = if prefix.is_empty() {
                consumed || !call.returns_value
            } else {
                consumed
                    && plain_let_binding_name(prefix).is_some_and(|bound| {
                        bound_result_only_compared(&masked, end, read_at, bound)
                    })
            };
            // Cheap checks first: the constructor check may scan the index.
            shape
                && body
                    .get(at..)
                    .and_then(|text| super::activation::call_arguments(text, &name))
                    .and_then(|arguments| {
                        arguments
                            .get(call.index)
                            .map(|argument| normalize(argument))
                    })
                    .is_some_and(|argument| argument == key)
                && key_holds_between(&masked, end, read_at, &key)
                // An early `return` (a platform skip) passes without the read.
                && !masked
                    .get(end..read_at)
                    .is_none_or(|window| contains_as_whole_word(window, "return"))
                && only_getter_reads_between(&masked, end, read_at, call.binding, call.getter)
                && binding_starts_without_expected(body, &masked, statement_start, call)
        })
}

/// RIPR-SPEC-0228 rule 3 for keyed reads: between the owner call (ending
/// at `from`) and the read (at `to`) in masked `code`, `binding` may appear
/// only as `binding.getter(..)` through the verified read-only getter. Any
/// other mention (`s.set("K", 5)`, `s.m.clear()`, `&mut s`) may rewrite the
/// entry the read sees.
fn only_getter_reads_between(
    code: &str,
    from: usize,
    to: usize,
    binding: &str,
    getter: &str,
) -> bool {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let Some(window) = code.get(from..to) else {
        return false;
    };
    let read = format!(".{getter}(");
    window.match_indices(binding).all(|(at, name)| {
        let after = &window[at + name.len()..];
        window[..at]
            .chars()
            .next_back()
            .is_some_and(|ch| ident(ch) || ch == '.')
            || after.starts_with(ident)
            || after.starts_with(read.as_str())
    })
}

/// RIPR-SPEC-0228 keyed reads: whether the test builds `binding` with its
/// map field empty and, before the owner call (statement starting at
/// `call_start` in masked `code`), touches it only in ways that cannot
/// already store the expected value. The last `let [mut] binding` must be
/// `Type::ctor(..)`, or `Default::default()` annotated `: Type`, with
/// `Type` the owner's impl type and the constructor verified to build the
/// field empty ([`KeyedCall::starts_empty`]). Every later statement that
/// names `binding` must be a call `binding.m(..)` whose arguments are all
/// plain literals, none equal to the expected value, optionally ending in
/// `.unwrap()`, `.expect(..)` or `?`, to a method verified unable to store
/// the expected literal ([`KeyedCall::cannot_store`]); literal tokens are
/// read from the unmasked `body`. The binding must be declared once, at the
/// test's top level. A seeding call (`s.set("K", 5)`, `s.set("K", v)`), a
/// helper (`seed(&mut s)`), a struct literal and a seeding constructor
/// (`S::with("K", 5)`) all fail closed: the entry could already hold the
/// value the assertion expects whether or not the owner's insert ran.
fn binding_starts_without_expected(
    body: &str,
    code: &str,
    call_start: usize,
    call: &KeyedCall,
) -> bool {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let Some(before) = code.get(..call_start) else {
        return false;
    };
    // The binding must be declared once, at the top level of the test: a
    // `let` in a nested block or a second `let` may not be the value the
    // owner call sees.
    let declarations: Vec<(usize, usize)> = before
        .match_indices("let ")
        .filter_map(|(at, keyword)| {
            if before[..at].chars().next_back().is_some_and(ident) {
                return None;
            }
            let rest = before[at + keyword.len()..].trim_start();
            let rest = rest.strip_prefix("mut ").map_or(rest, str::trim_start);
            let after = rest.strip_prefix(call.binding)?;
            (!after.starts_with(ident)).then_some((at, before.len() - after.len()))
        })
        .collect();
    let [(let_at, after_name)] = declarations.as_slice() else {
        return false;
    };
    let after_name = *after_name;
    if !runs_unconditionally(code, *let_at) {
        return false;
    }
    let Some(declaration_end) = before[after_name..].find(';').map(|at| after_name + at) else {
        return false;
    };
    let Some((annotation, initializer)) = before[after_name..declaration_end].split_once('=')
    else {
        return false;
    };
    let (annotation, initializer) = (annotation.trim(), initializer.trim());
    let annotated = match annotation.strip_prefix(':').map(str::trim) {
        None => false,
        Some(ty)
            if ty == call.impl_type
                || ty
                    .strip_prefix(call.impl_type)
                    .is_some_and(|rest| rest.starts_with('<')) =>
        {
            true
        }
        _ => return false,
    };
    let constructor = if annotated && initializer == "Default::default()" {
        Some("default")
    } else {
        initializer
            .strip_prefix(call.impl_type)
            .and_then(|rest| rest.strip_prefix("::"))
            .and_then(|rest| {
                let name_len = rest.find(|ch: char| !ident(ch)).unwrap_or(rest.len());
                let (name, arguments) = rest.split_at(name_len);
                // The argument list must close the initializer.
                (!name.is_empty()
                    && arguments.starts_with('(')
                    && closing_paren(arguments).is_some_and(|close| close + 1 == arguments.len()))
                .then_some(name)
            })
    };
    if !constructor.is_some_and(|ctor| (call.starts_empty)(call.impl_type, ctor, call.path)) {
        return false;
    }
    let names_binding = |text: &str| {
        text.match_indices(call.binding).any(|(at, found)| {
            !text[..at]
                .chars()
                .next_back()
                .is_some_and(|ch| ident(ch) || ch == '.')
                && !text[at + found.len()..].starts_with(ident)
        })
    };
    let expected = call.expected.trim();
    let expected = expected
        .strip_prefix("Some(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(expected);
    let mut start = declaration_end + 1;
    while start < call_start {
        let end = before[start..]
            .find(';')
            .map_or(call_start, |at| start + at);
        let statement = &before[start..end];
        let at = start + (statement.len() - statement.trim_start().len());
        start = end + 1;
        if !names_binding(statement) {
            continue;
        }
        let Some(method) = before[at..]
            .strip_prefix(call.binding)
            .and_then(|rest| rest.strip_prefix('.'))
            .map(|rest| &rest[..rest.find(|ch: char| !ident(ch)).unwrap_or(rest.len())])
            .filter(|method| !method.is_empty())
        else {
            return false;
        };
        let name = format!("{}.{method}", call.binding);
        let ends_here = before[at + name.len()..].starts_with('(')
            && call_statement_tail(code, at, name.len() + 1)
                .is_some_and(|(tail, _)| tail == end + 1);
        let literal_arguments = body
            .get(at..end)
            .and_then(|text| super::activation::call_arguments(text, &name))
            .is_some_and(|arguments| {
                arguments
                    .iter()
                    .all(|argument| literals_equal(argument, expected) == Some(false))
            });
        if !(ends_here
            && literal_arguments
            && (call.cannot_store)(call.impl_type, method, expected))
        {
            return false;
        }
    }
    true
}

/// The offset of the `)` closing the `(` that opens `text`.
fn closing_paren(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `key` names the same value from offset `from` (just past the
/// owner call) up to `to` (the read) in masked `code`: a literal always
/// does; a plain identifier does when nothing in between names it, so no
/// reassignment, shadow or mutation can change it. Anything else fails
/// closed.
fn key_holds_between(code: &str, from: usize, to: usize, key: &str) -> bool {
    if is_literal_key(key) {
        return true;
    }
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let Some(window) = code.get(from..to) else {
        return false;
    };
    !key.is_empty()
        && key.chars().all(ident)
        && !key.starts_with(|ch: char| ch.is_ascii_digit())
        && !window.match_indices(key).any(|(at, name)| {
            !window[..at].chars().next_back().is_some_and(ident)
                && !window[at + name.len()..].starts_with(ident)
        })
}

/// Whether nothing from the first unconditional `binding.method(..)` call
/// before `read_line` up to that line may write through `binding` or copy
/// what the call left, so the read sees the owner's value and nothing else
/// holds it. Fails closed:
/// - every owner call in the window must be a bare statement
///   `binding.method(..);`, optionally ending in `.unwrap()`, `.expect(..)`
///   or `?`: a bound or chained result (`let o = order.add(..)`,
///   `order.add(..).clear()`) may write through `binding` without naming it;
/// - the read line must start with its assertion, so nothing earlier on it
///   runs between;
/// - any other statement in the window may name `binding` only as a plain
///   field read that is a whole `assert!`/`assert_eq!`/`assert_ne!`
///   argument (`assert_eq!(order.total, 4)`); any other mention
///   (`order.log.clear()`, `let before = order.log.clone()`,
///   `order.log[0] = ..`, `&mut order`, a `ref mut` pattern, a block)
///   refuses. Starting at the first call, not the last, also refuses a
///   snapshot taken between two calls, however it is spelled.
pub(in crate::analysis) fn binding_untouched_after_owner_call(
    body: &str,
    start_line: usize,
    binding: &str,
    method: &str,
    read_line: usize,
) -> bool {
    let masked = mask_comments_and_strings(body);
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let Some(lines_before) = read_line.checked_sub(start_line) else {
        return false;
    };
    let read_at: usize = masked
        .split_inclusive('\n')
        .take(lines_before)
        .map(str::len)
        .sum();
    if !masked
        .get(read_at..)
        .is_some_and(|line| line.trim_start().starts_with("assert"))
    {
        return false;
    }
    let call = format!("{binding}.{method}(");
    let Some(call_at) = masked[..read_at]
        .match_indices(&call)
        .map(|(at, _)| at)
        .find(|at| {
            !masked[..*at]
                .chars()
                .next_back()
                .is_some_and(|before| ident(before) || before == '.')
                && runs_unconditionally(&masked, *at)
        })
    else {
        return false;
    };
    let statement_start = masked[..call_at]
        .rfind([';', '{', '}'])
        .map_or(0, |index| index + 1);
    if !masked[statement_start..call_at].trim().is_empty() {
        return false;
    }
    let Some(mut window_at) = bare_call_end(&masked[..read_at], call_at, call.len()) else {
        return false;
    };
    while window_at < read_at {
        let statement_len = masked[window_at..read_at]
            .find(';')
            .map_or(read_at - window_at, |semi| semi + 1);
        let statement = &masked[window_at..window_at + statement_len];
        let leading = statement.len() - statement.trim_start().len();
        // A later owner call on the binding, itself a bare statement whose
        // arguments do not name the binding: an argument runs before the
        // call and may copy what the earlier one left
        // (`order.add((snap = order.log.clone(), 4).1)`).
        if statement.trim_start().starts_with(&call)
            && bare_call_end(&masked[..read_at], window_at + leading, call.len())
                == Some(window_at + statement_len)
            && statement.trim_start()[call.len()..]
                .match_indices(binding)
                .all(|(at, name)| {
                    let arguments = &statement.trim_start()[call.len()..];
                    arguments[..at].chars().next_back().is_some_and(ident)
                        || arguments[at + name.len()..].starts_with(ident)
                })
        {
            window_at += statement_len;
            continue;
        }
        if !plain_assertion_or_unrelated(statement.trim_end_matches(';'), binding) {
            return false;
        }
        window_at += statement_len;
    }
    true
}

/// The offset just past the `;` that ends the bare call statement whose
/// `binding.method(` (`call_len` bytes) starts at `call_at` in masked
/// `code`. Only `.unwrap()`, `.expect(..)` or `?` may follow the call: they
/// consume its result, so a returned `&mut Self` is never written.
fn bare_call_end(code: &str, call_at: usize, call_len: usize) -> Option<usize> {
    call_statement_tail(code, call_at, call_len).map(|(end, _)| end)
}

/// [`bare_call_end`]'s end offset, with whether `.unwrap()`, `.expect(..)`
/// or `?` consumed the call's result before the `;`.
fn call_statement_tail(code: &str, call_at: usize, call_len: usize) -> Option<(usize, bool)> {
    let args_at = call_at + call_len;
    let mut depth = 0usize;
    let mut close = None;
    for (offset, ch) in code.get(args_at..)?.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => {
                close = Some(args_at + offset);
                break;
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    let after_call = code.get(close? + 1..)?.trim_start();
    let mut tail = after_call;
    loop {
        if let Some(rest) = tail.strip_prefix(".unwrap()") {
            tail = rest.trim_start();
        } else if let Some(rest) = tail.strip_prefix('?') {
            tail = rest.trim_start();
        } else if let Some(rest) = tail.strip_prefix(".expect(") {
            let end = rest.find(')')?;
            if rest[..end].contains('(') {
                return None;
            }
            tail = rest[end + 1..].trim_start();
        } else {
            break;
        }
    }
    tail.starts_with(';')
        .then(|| (code.len() - tail.len() + 1, tail.len() < after_call.len()))
}

/// Whether every mention of `bound` in masked `code` between `from` and
/// `to` is a whole operand of an assertion macro, first
/// (`assert_eq!(receipt, ..)`) or last (`assert_eq!(.., receipt);`): a
/// comparison only borrows the value, while any other use
/// (`receipt.set(..)`, `let _ = receipt;`, `assert!(touch(receipt))`) may
/// write through a returned `&mut Self`.
fn bound_result_only_compared(code: &str, from: usize, to: usize, bound: &str) -> bool {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let Some(window) = code.get(from..to) else {
        return false;
    };
    window.match_indices(bound).all(|(at, name)| {
        let before = &window[..at];
        let after = &window[at + name.len()..];
        if before.chars().next_back().is_some_and(ident) || after.starts_with(ident) {
            return true;
        }
        // Split on `;` only: a struct literal operand holds braces.
        let statement = before.rfind(';').map_or(before, |i| &before[i + 1..]);
        let before = before.trim_end();
        let after = after.trim_start();
        statement.trim_start().starts_with("assert")
            && ((before.ends_with("!(") && after.starts_with(','))
                || (before.ends_with(',')
                    && after
                        .strip_prefix(')')
                        .is_some_and(|rest| rest.trim_start().starts_with(';'))))
    })
}

/// The `NAME` of `let NAME =` or `let mut NAME =` with one plain
/// identifier: the bound value is the consumed result, not a guard or a
/// projection of it.
fn plain_let_binding_name(prefix: &str) -> Option<&str> {
    let rest = prefix.strip_prefix("let ")?.strip_suffix('=')?.trim();
    let name = rest.strip_prefix("mut ").map_or(rest, str::trim);
    (name != "_"
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_')
        && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_'))
    .then_some(name)
}

/// Whether a masked `statement` (no trailing `;`) leaves `binding` alone:
/// it does not name it, or names it only as plain field reads that are
/// whole arguments of a top-level `assert!`/`assert_eq!`/`assert_ne!` with
/// no block, closure or nested macro other than `vec!`/`format!`.
fn plain_assertion_or_unrelated(statement: &str, binding: &str) -> bool {
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    let mentions: Vec<usize> = statement
        .match_indices(binding)
        .map(|(at, _)| at)
        .filter(|at| {
            !statement[..*at].chars().next_back().is_some_and(ident)
                && !statement[at + binding.len()..].starts_with(ident)
        })
        .collect();
    if mentions.is_empty() {
        return true;
    }
    let trimmed = statement.trim_start();
    let opener = ["assert!(", "assert_eq!(", "assert_ne!("]
        .iter()
        .find(|plain| trimmed.starts_with(**plain));
    // `reset![order]` and other macros may write through the binding.
    opener.is_some_and(|opener| {
        let inner = &trimmed[opener.len()..];
        inner.match_indices('!').all(|(bang, _)| {
            let invoked = inner[bang + 1..].starts_with(['(', '[', '{']);
            let name_start = inner[..bang]
                .rfind(|ch: char| !ident(ch))
                .map_or(0, |index| index + 1);
            !invoked
                || (matches!(&inner[name_start..bang], "vec" | "format")
                    && !inner[..name_start].ends_with(':'))
        })
    }) && !statement.contains(['{', '|'])
        && mentions.iter().all(|at| {
            let before = statement[..*at].trim_end();
            let mut rest = &statement[at + binding.len()..];
            while let Some(member) = rest.strip_prefix('.') {
                let len = member.find(|ch: char| !ident(ch)).unwrap_or(member.len());
                if len == 0 {
                    return false;
                }
                rest = &member[len..];
            }
            // A whole argument of the assertion itself, not of a call
            // nested in it (`assert!(reset(&mut (order)))`).
            before.matches('(').count() == before.matches(')').count() + 1
                && (before.ends_with('(') || before.ends_with(','))
                && rest.trim_start().starts_with([',', ')'])
        })
}

/// Whether the test body calls `method` on anything but `binding`: another
/// receiver (`expected.add(..)`, `o.add(..)` in a block), a path call
/// (`Order::add(&mut expected, ..)`) or a chained receiver
/// (`Order::new().add(..)`). Such a value may carry the owner's changed
/// value into the other side of the equality, which then cannot fail on it.
/// Fails closed: a call on an unrelated value also counts.
pub(in crate::analysis) fn owner_called_on_other_receiver(
    body: &str,
    binding: &str,
    method: &str,
) -> bool {
    let masked = mask_comments_and_strings(body);
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    masked.match_indices(method).any(|(at, name)| {
        let before = &masked[..at];
        if before.chars().next_back().is_some_and(ident)
            || masked[at + name.len()..].starts_with(ident)
        {
            // Part of a longer name.
            return false;
        }
        // A path (`Order::add(..)`, `let f = Order::add;`) may run the owner
        // on any value.
        if before.trim_end().ends_with("::") {
            return true;
        }
        let Some(receiver_end) = before.trim_end().strip_suffix('.').map(str::len) else {
            // A free function or local of the same name is not the method.
            return false;
        };
        let receiver_end = masked[..receiver_end].trim_end().len();
        let receiver_start = masked[..receiver_end]
            .rfind(|ch: char| !ident(ch))
            .map_or(0, |index| index + 1);
        masked[..receiver_start].trim_end().ends_with('.')
            || masked[receiver_start..receiver_end] != *binding
    })
}

/// RIPR-SPEC-0228 keyed reads: the field path `<path>` when file line
/// `line` of the owner whose source text `owner_body` starts at file line
/// `owner_start_line` begins with a `self.<path>.insert(` statement at the
/// top level of the owner body, outside any branch, loop, block or closure;
/// `None` otherwise. An insert under
/// `if qty > 100 { .. }` runs only for some inputs, so a test reading the
/// key back says nothing about it unless its inputs reach the branch.
pub(in crate::analysis) fn unconditional_owner_insert_field(
    owner_body: &str,
    owner_start_line: usize,
    line: usize,
) -> Option<String> {
    let masked = mask_comments_and_strings(owner_body);
    let lines_before = line.checked_sub(owner_start_line)?;
    let line_at: usize = masked
        .split_inclusive('\n')
        .take(lines_before)
        .map(str::len)
        .sum();
    let text = masked.get(line_at..)?;
    let at = line_at + (text.len() - text.trim_start().len());
    // The statement must be the insert itself (`self.a.b.insert(`), not a
    // call that hands it to a closure (`self.items.iter().for_each(|k| ..)`).
    text.trim_start()
        .split_once('(')
        .and_then(|(target, _)| target.strip_prefix("self."))
        .and_then(|target| target.strip_suffix(".insert"))
        .filter(|path| {
            path.split('.').all(|segment| {
                !segment.is_empty() && segment.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            })
        })
        .filter(|_| runs_unconditionally(&masked, at))
        .map(str::to_string)
}

/// Whether the call at `at` in a test body (masked, starting at its `fn`)
/// is a top-level statement of that body: not inside a nested block, branch,
/// loop or closure, any of which may never run it. Fails closed.
fn runs_unconditionally(masked: &str, at: usize) -> bool {
    let before = &masked[..at];
    let depth = before.matches('{').count() as isize - before.matches('}').count() as isize;
    let statement_start = before.rfind([';', '{', '}']).map_or(0, |index| index + 1);
    depth == 1 && !before[statement_start..].contains('|')
}

/// For a `MatchArm` probe expression, extract only the "variant" tokens —
/// the identifier segments that appear immediately after a `::` separator.
/// These are the arm-specific tokens that can confirm an assertion targets
/// this arm rather than a sibling sharing the same enum qualifier.
///
/// Example: `"Mode::Frozen => -1,"` → `["Frozen"]`.
/// Example: `"Status::Active | Status::Idle => 0,"` → `["Active", "Idle"]`.
/// Example: `"None => 0,"` → `[]` (no `::` in expression).
fn match_arm_variant_tokens(expression: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let bytes = expression.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 1 < len {
        if bytes[i] == b':' && bytes[i + 1] == b':' {
            // skip "::"
            i += 2;
            // collect the identifier that follows
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                let variant = &expression[start..i];
                if extract_identifier_tokens(variant).contains(&variant.to_string()) {
                    variants.push(variant.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    variants
}

#[allow(
    clippy::too_many_arguments,
    reason = "reveal's grouped inputs plus the optional RIPR-SPEC-0229 arm selector"
)]
fn analyze_related_assertions(
    probe: &Probe,
    analysis_expression: &str,
    related_tests: &[(&TestSummary, RelationReason)],
    owner_local_bindings: &[String],
    same_name_import_defeats: &dyn Fn(&TestSummary, &str) -> bool,
    cross_package_name_defeats: &dyn Fn(&TestSummary, &str) -> bool,
    return_admission: &ReturnOracleAdmission<'_>,
    arm_selector: Option<&ArmSelector>,
) -> RevealAssertionAnalysis {
    let probe_tokens = if is_effect_family(&probe.family) {
        // An effect target rooted at a binding the owner itself introduces
        // (`let table = ..; table.validate()?;`) names a value no test can
        // hold: a test's same-named local is a different binding, so the
        // shared name is coincidence, not observation of the effect. The
        // field and method tokens of the target still confirm.
        effect_target_tokens(analysis_expression)
            .into_iter()
            .filter(|token| !owner_local_bindings.contains(token))
            .collect()
    } else {
        extract_identifier_tokens(analysis_expression)
    };
    let effect_literals = if is_effect_family(&probe.family) {
        rust_string_literals(analysis_expression)
    } else {
        Vec::new()
    };
    // For MatchArm: collect variant-only tokens (post-`::`) for the specificity
    // check. Qualifier tokens (e.g. the type name before `::`) are excluded so
    // that a sibling-arm assertion sharing the qualifier cannot spuriously
    // confirm observation of this arm.
    let match_arm_variants = if matches!(probe.family, ProbeFamily::MatchArm) {
        match_arm_variant_tokens(analysis_expression)
    } else {
        Vec::new()
    };
    // For MatchArm: string literals in the arm PATTERN (left of `=>`) are the
    // parser-owned identity of a literal arm (`"sensor"` in
    // `"sensor" => "sensor-v2",`). Result-side literals never confirm: a
    // sibling arm returning the same literal does not select this arm.
    // A sibling arm's pattern literals never overlap this arm's pattern, so
    // literal confirmation keeps the sibling rejection that variant-only
    // matching provides. No `=>` means no statically established pattern.
    let match_arm_literals = if matches!(probe.family, ProbeFamily::MatchArm) {
        match_arm_pattern_literals(analysis_expression)
    } else {
        Vec::new()
    };
    // A literal/variant match does not establish that an arm guard evaluated
    // true. Preserve the arm as an explicit unverified observation until a
    // producer-owned guard witness exists.
    let match_arm_guarded = matches!(probe.family, ProbeFamily::MatchArm)
        && match_arm_pattern_has_guard(analysis_expression);
    // For probes whose changed expression constructs an exact error variant
    // (`Err(Type::Variant)`): collect the variant-only token (the identifier
    // after the last `::`) so that a sibling-variant assertion that pins a
    // different variant of the same error type cannot confirm this probe.
    // RIPR-SPEC-0106 (Part B). The guard keys on the changed expression, not
    // the family label: a `return_value` probe on an `Err(...)` construction
    // would otherwise credit a sibling-variant oracle through the shared enum
    // qualifier token — and a `field_construction` probe whose field is an
    // `Err(...)` construction (`outcome: Err(CalcError::TooLarge)`) has the
    // same shared-qualifier exposure.
    let error_construction_path = if matches!(
        probe.family,
        ProbeFamily::ErrorPath | ProbeFamily::ReturnValue | ProbeFamily::FieldConstruction
    ) {
        error_path_variant_path(&probe.expression)
            .or_else(|| error_path_variant_path(analysis_expression))
    } else {
        None
    };
    let error_construction_variant = error_construction_path
        .as_deref()
        .and_then(|path| path.rsplit("::").next())
        .map(str::to_string);
    // The enum segment before the variant (`PayError` in
    // `PayError::Insufficient`): the shared qualifier a sibling-variant pin
    // names. See `names_only_sibling_variants`.
    let error_construction_qualifier = error_construction_path
        .as_deref()
        .and_then(|path| path.rsplit("::").nth(1))
        .filter(|qualifier| !qualifier.is_empty())
        .map(str::to_string);
    // #3700 (final consolidation): a wrapper error seam
    // (`callee(..).map_err(..)`) whose changed expression carries no
    // parseable variant has no statically establishable variant identity —
    // whether the wrapper faithfully carries the callee's error variant
    // through the boxed conversion is not statically resolvable. Such a seam
    // therefore NEVER confirms observation from lexical matching (every
    // confirming signal is token coincidence by construction); it stays
    // below `exposed` and carries the typed
    // `wrapper_error_binding_unresolved` limitation attached by
    // `apply_wrapper_error_binding_limit` (analysis/language/rust/oracles.rs).
    let wrapper_seam = error_construction_variant.is_none()
        && matches!(
            probe.family,
            ProbeFamily::ErrorPath | ProbeFamily::ReturnValue
        )
        && wrapper_error_seam_expression(&[probe.expression.as_str(), analysis_expression]);
    // #5830: on a value probe, a token naming the owner's own parameter or
    // `let` local (`subtotal` in `subtotal * 8 / 100`, `sku` in
    // `sku.get_unchecked(start..)`) is a binding no test holds, and a
    // numeric literal (`100`) matches any test input of the same number.
    // Both confirm only in an assertion that calls the owner. Effect
    // families already drop owner locals from their tokens above and keep
    // parameters, because a test commonly passes its receiver by the same
    // name.
    // The constructed field's own name stays a confirming token even when
    // it is also a parameter (`storage` in the shorthand `HirLet { storage }`):
    // `statement.storage` names the field, not the binding. A return-value
    // probe on a field initializer of the returned literal (`storage,`) has
    // the same shape.
    // The probe's own changed text names the field; the analysis expression
    // may be the whole enclosing literal.
    let field_fragment = matches!(probe.family, ProbeFamily::FieldConstruction)
        || probe.expression.trim_end().ends_with(',');
    let constructed_field = field_fragment
        .then(|| constructed_field_name(&probe.expression))
        .flatten();
    let owner_scoped_tokens: Vec<String> = if matches!(
        probe.family,
        ProbeFamily::ReturnValue | ProbeFamily::FieldConstruction
    ) {
        probe_tokens
            .iter()
            .filter(|token| constructed_field != Some(token.as_str()))
            .filter(|token| {
                return_admission.owner_parameters.contains(token)
                    || owner_local_bindings.contains(token)
                    || token.starts_with(|ch: char| ch.is_ascii_digit())
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let match_context = RevealMatchContext {
        probe_tokens: &probe_tokens,
        effect_literals: &effect_literals,
        match_arm_variants: &match_arm_variants,
        match_arm_literals: &match_arm_literals,
        match_arm_guarded,
        error_construction_variant: error_construction_variant.as_deref(),
        error_construction_qualifier: error_construction_qualifier.as_deref(),
        family: &probe.family,
        wrapper_seam,
        // #3709: the owner's bare name is the segment after the symbol's
        // final `::` separator (`src/lib.rs::impl Discount::score` -> `score`,
        // `src/lib.rs::expect_response` -> `expect_response`).
        owner_callee: probe.owner.as_ref().and_then(|symbol| {
            let name = symbol.0.rsplit("::").next()?;
            (!name.is_empty()).then_some(name)
        }),
        arm_selector: arm_selector.filter(|_| matches!(probe.family, ProbeFamily::MatchArm)),
        arm_inputs_readable: false,
        arm_pattern_pinned: false,
        owner_scoped_tokens: &owner_scoped_tokens,
        owner_shadowed: false,
        sibling_excluded: false,
    };
    let confirm_required = needs_token_confirmation(&probe.family);
    // RIPR-SPEC-0040 builder override (#6613): the fields a changed struct
    // literal (`return_value`) or field initializer (`field_construction`)
    // writes. Such a field's token confirms nothing in an assertion that
    // reads the field only after the test overwrote it on the owner's result.
    let override_fields: Vec<String> = match probe.family {
        ProbeFamily::ReturnValue => {
            let fields = struct_literal_fields(&probe.expression);
            if !fields.is_empty() {
                fields
            } else if constructed_field.is_some() {
                constructed_field.map(str::to_string).into_iter().collect()
            } else {
                struct_literal_fields(analysis_expression)
            }
        }
        ProbeFamily::FieldConstruction => {
            constructed_field.map(str::to_string).into_iter().collect()
        }
        _ => Vec::new(),
    };
    let override_owner = probe
        .owner
        .as_ref()
        .and_then(|symbol| OwnerCall::from_symbol(&symbol.0));
    let mut override_setters = Vec::new();
    if let Some(impl_type) = override_owner.and_then(|owner| owner.impl_type()) {
        for field in &override_fields {
            for setter in setter_names(field) {
                if (return_admission.setter_writes_field)(impl_type, &setter, field) {
                    override_setters.push(setter);
                }
            }
        }
    }
    let override_owner = override_owner.map(|owner| owner.with_setters(&override_setters));
    // RIPR-SPEC-0228: a field-path collection write (`self.log.push(..)`) is
    // observed by a read of that field, never by a shared name alone.
    let field_write = field_collection_write_path(&probe.expression);
    let insert_key_index = field_collection_insert_key(&probe.expression).and_then(|key| {
        return_admission
            .owner_argument_names
            .iter()
            .position(|name| *name == key)
    });
    let mut override_defeated = false;
    let mut related = Vec::new();
    let mut strongest = OracleStrength::None;
    let mut strongest_kind = OracleKind::Unknown;
    let mut strongest_observation_confirmed = false;
    let mut matched_any = false;
    let mut owner_pin_credited = false;
    let mut refused_context = false;
    // For families that need token confirmation: start pessimistic and clear
    // once a token_match fires.
    let mut observation_unverified = false;
    // Set when a matched assertion came from a test that cannot confirm a
    // match arm (#6297), so the unconfirmed summary can say why rather than
    // claim no assertion names the arm.
    let mut proximity_confirmation_withheld = false;
    // When any related test is tied to the owner by a call, helper chain,
    // assertion affinity or seam callee, reach comes from that test.
    // Same-file and same-module relations do not count: `reach.rs` treats
    // them as proximity with no reach. A test related only because its name or
    // path contains a changed token or the owner's name, with no captured
    // call, helper chain or assertion affinity (`WeakTokenSubstring`,
    // `OwnerNamedTest`), may never run the changed code, so its
    // assertions stay visible but cannot supply the credited oracle: a test
    // named `malformedsource_variant_is_distinct` that pins
    // `ParseError::MalformedSource == ParseError::MalformedSource` observes
    // nothing the changed `try_parse` does (#4486). Same-file and same-module
    // tests keep crediting: they commonly exercise a private helper through
    // the module's own entry point, which the relation cannot see.
    let name_only = is_name_only_relation;
    let credits = oracle_crediting_relations(related_tests);
    // A seam callee call runs the seam's callee, not the owner (`reach.rs`
    // keeps it out of owner reach), so it cannot be the reaching test that
    // withholds a same-file match-arm confirmation below.
    let owner_reaching_related = related_tests.iter().any(|(_, reason)| {
        *reason != RelationReason::SeamCalleeCall
            && !name_only(*reason)
            && !is_proximity_only(*reason)
    });

    let names_shared_variant = matches!(probe.family, ProbeFamily::MatchArm)
        || (matches!(
            probe.family,
            ProbeFamily::ErrorPath | ProbeFamily::ReturnValue
        ) && (changed_error_variant(&probe.expression).is_some()
            // A change confined to the payload line of a multiline
            // `Err::<T, E>(\n E::X,\n)` leaves only `E::X,` in the probe
            // expression; the enclosing constructor is in the analysis
            // expression (#7094 review).
            || error_construction_path.is_some()));

    // #7763: a reason string also occurs in unrelated plan/gate values.
    // When another test reaches the owner, proximity alone cannot make that
    // shared literal an oracle for its predicate. Preserve possible opaque or
    // transitive owner calls, and retain the uncredited row for explanation.
    let shared_string_predicate = matches!(probe.family, ProbeFamily::Predicate)
        && !rust_string_literals(&probe.expression).is_empty();

    for (test, reason) in related_tests {
        let relation_reason = Some(*reason);
        let relation_confidence = Some(reason.confidence());
        let credits_oracle = credits(*reason)
            && !(shared_string_predicate
                && owner_reaching_related
                && is_proximity_only(*reason)
                && !invokes_opaque_macro(&test.body)
                && !(return_admission.proximity_may_reach_owner)(test));
        // #6297: a match arm's variant (`Unit::Fortnight`) names an enum value
        // that every function handling the enum shares, so unlike a
        // return-value token it does not tie an assertion to the owner (the
        // arm's string literals are already scoped to owner calls for the same
        // reason). When another related test reaches the owner by a call, a
        // test related only by proximity (same file or module) still credits
        // strength but cannot confirm the arm. A same-file
        // `matches!(Unit::from_str(..), Ok(Unit::Fortnight))` confirmed the
        // `seconds` arm, so the arm's verdict followed edits to a test that
        // never runs `seconds`.
        //
        // An exact error variant (`Err(PayError::Limit)`) is the same kind of
        // shared enum value: a same-file `matches!(refund(..),
        // Err(PayError::Limit))` must not confirm `deposit_cap`'s
        // `return Err(PayError::Limit)` while another test reaches
        // `deposit_cap` (#7063).
        let confirms_observation = !(names_shared_variant
            && owner_reaching_related
            && is_proximity_only(*reason)
            && !invokes_opaque_macro(&test.body)
            && !(return_admission.proximity_may_reach_owner)(test));
        let assertions: Vec<_> = test
            .assertions
            .iter()
            .filter(|assertion| (return_admission.assertion_admitted)(test, assertion))
            .collect();
        let refused_here = assertions.len() != test.assertions.len();
        refused_context |= refused_here;
        if assertions.is_empty() {
            related.push(RelatedTest {
                name: test.name.clone(),
                file: test.file.clone(),
                line: test.start_line,
                oracle: None,
                oracle_kind: OracleKind::Unknown,
                oracle_strength: OracleStrength::None,
                relation_reason,
                relation_confidence,
                miss: Some(if refused_here {
                    RelatedTestMiss::AssertionNotCredited
                } else {
                    RelatedTestMiss::NoAssertion
                }),
            });
            continue;
        }
        // #3731 review (F11): computed once per test — whether the test's
        // own file imports the owner callee's bare name from a FOREIGN
        // path, which makes every bare-scrutinee binding in it ambiguous.
        let import_defeats_owner = match_context
            .owner_callee
            .is_some_and(|callee| same_name_import_defeats(test, callee));
        // #3731 review (G1): computed once per test — whether the test's
        // own package defines a function with the owner callee's bare name
        // while the changed owner lives in ANOTHER package, which makes the
        // bare call ambiguous across packages the same way a foreign
        // import does.
        let cross_package_defeats_owner = match_context
            .owner_callee
            .is_some_and(|callee| cross_package_name_defeats(test, callee));
        // RIPR-SPEC-0229: an arm selection is read only in a test whose every
        // mention of the owner is a direct call this module reads. A
        // `let reason = |x| ..` closure or any other local use of the name
        // may shadow the owner, so its calls say nothing about the owner.
        // A `let` bound to an owner call may carry the arm's result to the
        // expected side, so such a test confirms nothing either. A test that
        // never names the owner (it reaches it only through a wrapper) passes
        // no input to read, so its tokens confirm as before selection (#6297).
        let arm_selector = match_context
            .arm_selector
            .filter(|selector| selector.mentioned_by(test));
        let match_context = RevealMatchContext {
            arm_selector,
            arm_inputs_readable: arm_selector.is_some_and(|selector| {
                selector.observed_inputs(test).is_some() && !selector.binds_owner_result(test)
            }),
            owner_shadowed: match_context
                .owner_callee
                .is_some_and(|owner| test_body_shadows_owner(test, owner)),
            ..match_context
        };
        // Refusing credit must not manufacture the singleton-test fallback
        // for an otherwise unrelated surviving oracle.
        let assertion_count = test.assertions.len();
        let related_before = related.len();
        for assertion in assertions {
            // RIPR-SPEC-0231 examples 19 and 20: an inequality on the owner
            // call that excludes only a sibling variant of the changed error
            // observes the changed result, but only as `relational_check` /
            // weak (rule 1), whatever the classifier read.
            // A test-visible `macro_rules! assert_ne` (or `assert`,
            // `matches`) may check nothing, so the exclusion is credited
            // only under the shared assertion-context admission.
            let sibling_excluded =
                excludes_only_sibling_variant(
                    &match_context,
                    assertion,
                    import_defeats_owner,
                    cross_package_defeats_owner,
                ) && (return_admission.sibling_exclusion_admitted)(test, assertion);
            let excluded_sibling;
            let assertion = if sibling_excluded {
                excluded_sibling = OracleFact {
                    kind: OracleKind::RelationalCheck,
                    strength: OracleStrength::Weak,
                    ..assertion.clone()
                };
                &excluded_sibling
            } else {
                assertion
            };
            // #4478: whether this `assert_eq!` pins the owner's whole return
            // value through a call that names the owner. The owner-side and
            // test-side identity gates live in `owner_pin`; the family and
            // oracle-kind gates are checked first so the closure only runs
            // for return-value and error-path exact pins. #6692: a field of a
            // hand-written `Clone::clone`'s returned literal is pinned by
            // `assert_eq!(recv.clone(), recv)` under the same gates
            // (`OwnerReturnPin::establish_clone_field`).
            //
            // A bare `assert!` pins a bool owner's return value the same way
            // (`assert!(f(x))` is `assert_eq!(f(x), true)`), which also
            // observes a predicate that is that owner's whole tail. Its kind
            // stays the classifier's `relational_check` (RIPR-SPEC-0231);
            // only its strength relative to this probe rises. #6578: the
            // same holds for an inline expect-test comparison
            // (`expect![["5"]].assert_eq(&owner(..))`), whose kind stays
            // `snapshot` (RIPR-SPEC-0231 step 6); the pin admits no other
            // snapshot spelling.
            let owner_pinned = match probe.family {
                ProbeFamily::ReturnValue => matches!(
                    assertion.kind,
                    OracleKind::ExactValue
                        | OracleKind::ExactErrorVariant
                        | OracleKind::WholeObjectEquality
                        | OracleKind::RelationalCheck
                        | OracleKind::Snapshot
                ),
                // A changed `return Err(..);` is an `error_path` probe; its
                // pin compares the owner's whole `Err(..)` result.
                ProbeFamily::ErrorPath => matches!(assertion.kind, OracleKind::ExactErrorVariant),
                ProbeFamily::Predicate => matches!(assertion.kind, OracleKind::RelationalCheck),
                ProbeFamily::FieldConstruction => matches!(
                    assertion.kind,
                    OracleKind::ExactValue | OracleKind::WholeObjectEquality
                ),
                _ => false,
            } && pin_strength_admits(assertion)
                && (return_admission.owner_return_pin)(test, assertion);
            let strength_raising_pin = owner_pinned
                && (matches!(assertion.kind, OracleKind::RelationalCheck)
                    || is_inline_expect_snapshot(assertion));
            let owner_bound = !match_context.owner_scoped_tokens.is_empty()
                && oracle_binds_sink_identity(
                    &assertion.text,
                    &test.body,
                    match_context.owner_callee,
                );
            let assertion_context = RevealMatchContext {
                arm_pattern_pinned: match_context
                    .arm_selector
                    .is_some_and(|selector| selector.pattern_pin_selects(test, assertion)),
                sibling_excluded,
                ..match_context
            };
            let (matched, mut has_token_match) = assertion_matches_probe_detail_with_literals(
                &assertion_context,
                assertion,
                assertion_count,
                import_defeats_owner,
                cross_package_defeats_owner,
                owner_pinned,
                owner_bound,
            );
            // A read of the written field collection matches the probe even
            // when it shares no name with the changed line
            // (`inv.history()` for `self.log.push(..)`).
            let field_collection_read = field_write.is_some_and(|path| {
                field_collection_observer_confirms(
                    path,
                    // Only a deleted write changes how many elements the
                    // collection holds; a changed argument does not.
                    matches!(probe.family, ProbeFamily::CallDeletion),
                    override_owner,
                    test,
                    assertion,
                    return_admission.getter_reads_field,
                )
            });
            // A keyed getter read (`inv.on_hand("A")`) of a map entry the
            // owner inserted under the same argument (`self.stock.insert(sku,
            // ..)` with `inv.ship("A", ..)`).
            let keyed_read = field_write
                .zip(insert_key_index)
                .is_some_and(|(path, index)| {
                    keyed_field_read_confirms(
                        path,
                        index,
                        override_owner,
                        test,
                        assertion,
                        return_admission,
                    )
                });
            let field_collection_read = field_collection_read || keyed_read;
            let matched = matched || field_collection_read;
            // RIPR-SPEC-0040 builder override (#6613): drop the field tokens
            // the test overwrote before this assertion reads them, and ask
            // again whether anything else confirms.
            let overridden = match override_owner {
                Some(owner) if has_token_match && !override_fields.is_empty() => {
                    overridden_fields_read(test, assertion, owner, &override_fields)
                }
                _ => Vec::new(),
            };
            if !overridden.is_empty() {
                let unoverridden: Vec<String> = probe_tokens
                    .iter()
                    .filter(|token| !overridden.contains(token))
                    .cloned()
                    .collect();
                let narrowed = RevealMatchContext {
                    probe_tokens: &unoverridden,
                    ..assertion_context
                };
                let (_, narrowed_token_match) = assertion_matches_probe_detail_with_literals(
                    &narrowed,
                    assertion,
                    assertion_count,
                    import_defeats_owner,
                    cross_package_defeats_owner,
                    owner_pinned,
                    owner_bound,
                );
                override_defeated |= !narrowed_token_match;
                has_token_match = narrowed_token_match;
            }
            // RIPR-SPEC-0035 / #5830: an equality whose expected side is
            // computed through the changed owner moves with it, so it
            // neither pins the value nor confirms observation.
            let self_computed = matched
                && match_context.owner_callee.is_some_and(|owner| {
                    expected_computed_through_owner(
                        &assertion.text,
                        owner,
                        return_admission.expected_reaches_owner,
                    )
                });
            // #6692: the owner pin is credited only through an assertion
            // that matched, from a test that may supply the oracle, with the
            // pin surviving the reveal-side defeats. The missing-field
            // cleanup in `ClassifiedProbeEvidence::gather` reads this.
            owner_pin_credited |= matched
                && credits_oracle
                && !self_computed
                && owner_return_pin_holds(
                    &match_context,
                    assertion,
                    owner_pinned,
                    import_defeats_owner,
                    cross_package_defeats_owner,
                );
            if matched && !credits_oracle {
                related.push(RelatedTest {
                    name: test.name.clone(),
                    file: test.file.clone(),
                    line: test.start_line,
                    oracle: Some(assertion.text.clone()),
                    oracle_kind: assertion.kind.clone(),
                    oracle_strength: self_computed_cap(
                        probe_relative_oracle_strength(&probe.family, assertion),
                        self_computed,
                    ),
                    relation_reason,
                    relation_confidence,
                    miss: Some(RelatedTestMiss::NoCallPath),
                });
            } else if matched {
                let observation_confirmed = !self_computed
                    && (!confirm_required
                        || (confirms_observation
                            && (collection_observer_confirms(&probe.expression, assertion)
                                || field_collection_read
                                || (direct_collection_mutation_receiver(&probe.expression)
                                    .is_none()
                                    && ((has_token_match && field_write.is_none())
                                        || (is_effect_family(&probe.family)
                                            && effect_observer_confirms(assertion)
                                            && (return_admission.effect_state_carried)(
                                                test, assertion,
                                            )))))));
                proximity_confirmation_withheld |= !confirms_observation;
                if confirm_required {
                    // Observation is confirmed when the assertion specifically
                    // references the changed sub-expression. For value families
                    // (MatchArm/ReturnValue/FieldConstruction) the only static
                    // signal is a `token_match`. For effect families
                    // (SideEffect/CallDeletion) the canonical observer is a
                    // mock/expectation/snapshot that kind-matches the seam
                    // without sharing a token, so a genuine effect observer also
                    // confirms. This prevents a real mock from being wrongly
                    // flagged `observation_unverified`, while a plain
                    // non-observing assertion (no token, no effect observer)
                    // stays unverified.
                    if !matched_any {
                        // First matching assertion: observation is unverified
                        // unless confirmed.
                        observation_unverified = !observation_confirmed;
                    } else if observation_confirmed {
                        // A later confirmed assertion clears the unverified flag.
                        observation_unverified = false;
                    }
                }
                matched_any = true;
                let mut relative_strength = if strength_raising_pin {
                    OracleStrength::Strong
                } else {
                    probe_relative_oracle_strength(&probe.family, assertion)
                };
                relative_strength = self_computed_cap(relative_strength, self_computed);
                // Keep strength, kind, and confirmation on one assertion.
                // An equally strong confirmed oracle wins over an unrelated
                // one regardless of encounter order; a weaker oracle cannot.
                if relative_strength.rank() > strongest.rank()
                    || (relative_strength.rank() == strongest.rank()
                        && observation_confirmed
                        && !strongest_observation_confirmed)
                {
                    strongest = relative_strength.clone();
                    strongest_kind = assertion.kind.clone();
                    strongest_observation_confirmed = observation_confirmed;
                }
                related.push(RelatedTest {
                    name: test.name.clone(),
                    file: test.file.clone(),
                    line: test.start_line,
                    oracle: Some(assertion.text.clone()),
                    oracle_kind: assertion.kind.clone(),
                    oracle_strength: relative_strength,
                    relation_reason,
                    relation_confidence,
                    miss: None,
                });
            }
        }
        if related.len() == related_before {
            // #5344/#5329: a test ripr examined stays listed even when none of
            // its assertions apply, so a finding that says related tests were
            // found names them and says why each one misses. The first
            // assertion is kept as the row's text so a reader can check the
            // claim; its kind is `unknown` and strength `none` because it
            // supplies no oracle.
            // Listing it changes no stage or class: those are decided above
            // from `matched_any`, `strongest` and the refusal flag.
            let first = (!refused_here).then(|| test.assertions.first()).flatten();
            related.push(RelatedTest {
                name: test.name.clone(),
                file: test.file.clone(),
                line: test.start_line,
                oracle: first.map(|assertion| assertion.text.clone()),
                // Kind stays `unknown`: a finding-level oracle summary must
                // not report the shape of an assertion that matched nothing.
                oracle_kind: OracleKind::Unknown,
                oracle_strength: OracleStrength::None,
                relation_reason,
                relation_confidence,
                miss: Some(if refused_here {
                    RelatedTestMiss::AssertionNotCredited
                } else {
                    RelatedTestMiss::AssertionNotObserving
                }),
            });
        }
    }

    RevealAssertionAnalysis {
        related,
        strongest,
        strongest_kind,
        strongest_observation_confirmed,
        matched_any,
        owner_pin_credited,
        refused_context,
        observation_unverified,
        proximity_confirmation_withheld,
        override_defeated,
    }
}

/// Extracts the variant path from a changed expression that constructs an
/// exact error variant.
///
/// For `return Err(CalcError::TooLarge);` → `Some("CalcError::TooLarge")`.
/// For `let d = digit(c).ok_or(CalcError::TooLarge)?;` → the same (#6695).
/// For `return Err(anyhow!("..."));` → `None` (no qualified variant).
///
/// Used by RIPR-SPEC-0106 (Part B) to restrict assertion matching to the
/// changed expression's specific variant (the final segment), preventing
/// sibling-variant over-credit. The identity comes from the shared
/// `changed_error_variant` owner that repo mode reads too.
fn error_path_variant_path(expression: &str) -> Option<String> {
    let variant_path = super::text::changed_error_variant(expression)?;
    // The final component after the last `::` must read as a variant.
    let last = variant_path.rsplit("::").next()?;
    last.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        .then_some(variant_path)
}

/// Whether an assertion names the changed error's enum only through
/// SIBLING variants (RIPR-SPEC-0106 Part B, PR #6786 review): it spells at
/// least one `<qualifier>::<Variant>` path, and no such path ends in the
/// changed `variant`. `assert!(matches!(e, PayError::Limit))` against a
/// changed `Err(PayError::Insufficient)` shares the `PayError` token with
/// the changed line, but that qualifier is common to every variant and pins
/// a different error; it is not an observation of the changed one.
/// String and comment contents are masked, so a message naming a path is
/// not an assertion of it. A text that never spells the qualifier as a
/// path is not decided here.
fn names_only_sibling_variants(text: &str, qualifier: &str, variant: &str) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut named_any = false;
    let mut start = 0;
    while let Some(found) = masked[start..].find(qualifier) {
        let at = start + found;
        start = at + qualifier.len();
        if at > 0 && is_ident(bytes[at - 1]) {
            continue;
        }
        let Some(rest) = masked[start..].trim_start().strip_prefix("::") else {
            continue;
        };
        let rest = rest.trim_start();
        let segment_len = rest.bytes().take_while(|byte| is_ident(*byte)).count();
        if segment_len == 0 {
            continue;
        }
        if &rest[..segment_len] == variant {
            return false;
        }
        named_any = true;
    }
    named_any
}

/// RIPR-SPEC-0231 examples 19 and 20: `assert_ne!(owner(..), Err(E::Bad))`
/// or `assert!(!matches!(owner(..), Err(E::Bad)))` against a changed
/// `Err(E::Big)`. The assertion names the changed error's enum only through
/// a sibling variant, but it excludes that sibling from the owner's own
/// result, so a change that returned the sibling would fail it: it observes
/// the result, weakly. One compared operand (or the whole `matches!`
/// scrutinee) must be a bare call to the owner, with no foreign import,
/// nested `fn` or local binding making that call ambiguous, and the negated
/// `matches!` must be the whole condition with an unguarded pattern. An
/// inequality on any other value shares only the enum qualifier and stays
/// refused.
fn excludes_only_sibling_variant(
    context: &RevealMatchContext,
    assertion: &OracleFact,
    import_defeats_owner: bool,
    cross_package_defeats_owner: bool,
) -> bool {
    let (Some(variant), Some(qualifier), Some(owner)) = (
        context.error_construction_variant,
        context.error_construction_qualifier,
        context.owner_callee,
    ) else {
        return false;
    };
    if import_defeats_owner || cross_package_defeats_owner || context.owner_shadowed {
        return false;
    }
    let masked = crate::analysis::extract::mask_comments_and_strings(&assertion.text);
    let statement = masked.trim();
    let statement = statement.strip_suffix(';').unwrap_or(statement).trim_end();
    let excluded = if let Some(rest) = statement.strip_prefix("assert_ne!") {
        // The message arguments after the two operands never decide it.
        let operands = macro_arguments(rest);
        match operands.as_slice() {
            [left, right, ..] if is_whole_owner_call(left, owner) => Some(*right),
            [left, right, ..] if is_whole_owner_call(right, owner) => Some(*left),
            _ => None,
        }
    } else if let Some(rest) = statement.strip_prefix("assert!") {
        // The negated `matches!` must be the whole condition (no `||`,
        // `^` or other operand beside it) with an unguarded pattern.
        macro_arguments(rest)
            .first()
            .and_then(|condition| condition.strip_prefix('!'))
            .and_then(|negated| negated.trim_start().strip_prefix("matches!"))
            .and_then(|rest| match macro_arguments(rest).as_slice() {
                [scrutinee, pattern]
                    if is_whole_owner_call(scrutinee, owner) && !has_word(pattern, "if") =>
                {
                    Some(*pattern)
                }
                _ => None,
            })
    } else {
        None
    };
    excluded.is_some_and(|excluded| names_only_sibling_variants(excluded, qualifier, variant))
}

/// The trimmed top-level comma-separated arguments of a macro call whose
/// parenthesised arguments are all of `rest` (`(a, f(b, c))`), or nothing
/// when `rest` is not exactly one balanced group.
fn macro_arguments(rest: &str) -> Vec<&str> {
    let rest = rest.trim();
    if !rest.starts_with('(') || group_end(rest) != Some(rest.len() - 1) {
        return Vec::new();
    }
    let inner = &rest[1..rest.len() - 1];
    let mut parts = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (at, byte) in inner.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(inner[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// The byte index of the delimiter closing the group `text` opens with.
fn group_end(text: &str) -> Option<usize> {
    let mut depth = 0_usize;
    for (at, byte) in text.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `operand` is exactly one bare call `owner(..)`: not a path
/// segment, method, nested argument or larger expression.
fn is_whole_owner_call(operand: &str, owner: &str) -> bool {
    operand.strip_prefix(owner).is_some_and(|rest| {
        let rest = rest.trim_start();
        rest.starts_with('(') && group_end(rest) == Some(rest.len() - 1)
    })
}

fn has_word(text: &str, word: &str) -> bool {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .any(|part| part == word)
}

/// Whether the test body defines a nested `fn <owner>` or binds a local
/// named `owner` anywhere, under either shadow authority. Conservative: a
/// binding after the assertion also refuses the sibling exclusion.
fn test_body_shadows_owner(test: &TestSummary, owner: &str) -> bool {
    use crate::analysis::extract::ShadowAuthority;
    let masked = crate::analysis::extract::mask_comments_and_strings(&test.body);
    ShadowAuthority::LexicalMaskedBody.body_shadows_callee_at_line(&masked, owner, usize::MAX)
        || ShadowAuthority::ParserBodyFacts {
            nested_fn_names: &test.nested_fn_names,
            let_bindings: &test.let_bindings,
        }
        .body_shadows_callee_at_line("", owner, usize::MAX)
}

/// Whether an assertion observes an error at all: a typed error oracle, a
/// guarded `Result` match, or an identifier that names an error or a panic
/// (`Err`, `ParseError`, `unwrap_err`, `is_err`, `err`, `should_panic`).
/// Deliberately lenient on trailing error tokens: it only decides whether a
/// token overlap may count as observing a changed error path, never whether
/// the oracle is strong. A leading or middle error lexeme in a compound
/// identifier (`error_count`, `nonerror`) is not an observer (#5255). Sibling ErrorPath
/// confirmation sites do not scan identifier lexemes: diagnostic stripping,
/// guarded owner-result matches, and exact-variant pins are independent of
/// this gate.
fn assertion_observes_error(assertion: &OracleFact) -> bool {
    if matches!(
        assertion.kind,
        OracleKind::ExactErrorVariant | OracleKind::BroadError | OracleKind::GuardedResultMatch
    ) {
        return true;
    }
    // Split raw identifiers here: the shared token extractor drops `Err`
    // and `is_err` as assertion noise, and they are exactly the signal.
    crate::analysis::extract::mask_comments_and_strings(&assertion.text)
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .any(identifier_names_error_observer)
}

/// Trailing `err`/`error` on a real token boundary (or a panic token) names
/// an error observer. `ends_with("error")` would credit `nonerror`;
/// any-segment matching would credit `error_count`.
fn identifier_names_error_observer(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    if lower.contains("panic") {
        return true;
    }
    let last = lower.rsplit('_').next();
    last == Some("err") || last == Some("error") || token.ends_with("Error")
}

/// Probe-side matching inputs shared by every assertion of one probe
/// (grouped so the per-assertion matcher stays under the argument limit).
struct RevealMatchContext<'a> {
    probe_tokens: &'a [String],
    effect_literals: &'a [String],
    match_arm_variants: &'a [String],
    match_arm_literals: &'a [String],
    /// True when the parser-owned arm pattern carries a guard. This slice
    /// fails closed until the observed input can be shown to satisfy it.
    match_arm_guarded: bool,
    error_construction_variant: Option<&'a str>,
    /// The enum segment of the changed error path (`PayError`), when the
    /// path is qualified. Gates sibling-variant pins of any oracle kind.
    error_construction_qualifier: Option<&'a str>,
    family: &'a ProbeFamily,
    /// `true` only for #3700 wrapper error seams: the changed expression is a
    /// `map_err` conversion with no parseable variant, so the variant
    /// binding is not statically establishable and nothing may confirm
    /// observation through lexical matching.
    wrapper_seam: bool,
    /// #3709: the changed owner's bare function name, when the probe has
    /// one. A guarded Result match whose scrutinee directly calls this
    /// callee observes the owner's returned `Result` — the exact sink for
    /// the value/error families — without any changed-line token overlap.
    owner_callee: Option<&'a str>,
    /// RIPR-SPEC-0229: the changed arm's pattern and the owner-call input
    /// position its `match` reads, when established. An assertion whose
    /// compared operand is a direct owner call passing an input that
    /// selects this arm confirms observation of the arm; once established,
    /// selection is the only confirmation (selection outranks tokens).
    arm_selector: Option<&'a ArmSelector>,
    /// RIPR-SPEC-0229: whether the current test's every owner mention is a
    /// direct call the selector reads. A `let reason = |x| ..` closure or
    /// any other local use of the name may shadow the owner.
    arm_inputs_readable: bool,
    /// RIPR-SPEC-0229 (#6616): the current assertion is an admitted exact
    /// pattern pin (a verified local `assert_matches!`) whose value
    /// operand is a direct owner call selecting the arm.
    arm_pattern_pinned: bool,
    /// Probe tokens that name a binding only the owner holds (a parameter
    /// or `let` local) on a value probe. They confirm observation only in
    /// an assertion that calls the owner; anywhere else the shared name is
    /// coincidence (#5830).
    owner_scoped_tokens: &'a [String],
    /// Whether the current test's body defines or binds a local under the
    /// owner's bare name, so a bare call to it may not reach the owner
    /// (RIPR-SPEC-0231 sibling exclusion).
    owner_shadowed: bool,
    /// Whether the current assertion is an admitted RIPR-SPEC-0231
    /// sibling exclusion on the owner call (`excludes_only_sibling_variant`
    /// under standard macro bindings), decided once per assertion.
    sibling_excluded: bool,
}

/// The field a `field_construction` expression assigns: the identifier
/// before a single `:` (`total: a + b`), or the whole shorthand
/// identifier (`storage,`).
fn constructed_field_name(expression: &str) -> Option<&str> {
    let expression = expression.trim().trim_end_matches(',').trim();
    let head = match expression.find(':') {
        Some(colon) if !expression[colon + 1..].starts_with(':') => &expression[..colon],
        Some(_) => return None,
        None => expression,
    };
    let head = head.trim();
    (!head.is_empty() && head.bytes().all(is_ident_byte)).then_some(head)
}

/// True when `text`, comments and strings masked, calls `name`: the whole
/// word followed by `(` or a turbofish, as `name(..)`, `recv.name(..)` or
/// `Path::name(..)`.
fn text_calls(text: &str, name: &str) -> bool {
    called_names(text).iter().any(|called| called == name)
}

/// Every identifier `text` calls (comments and strings masked), excluding
/// macro invocations: the word immediately before `(` or `::<`.
fn called_names(text: &str) -> Vec<String> {
    called_paths(text)
        .into_iter()
        .map(|(_, name)| name)
        .collect()
}

fn is_called_path_ident_byte(byte: u8) -> bool {
    !byte.is_ascii() || is_ident_byte(byte)
}

fn is_called_path_ident_start_byte(byte: u8) -> bool {
    !byte.is_ascii() || byte.is_ascii_alphabetic() || byte == b'_'
}

/// Last path segment of `path`, treating non-ASCII characters as identifier
/// characters so a name such as `módulo` is not cut at `ó`. `rfind` yields
/// a character start; skip that whole character rather than one byte.
fn last_called_path_segment(path: &str) -> &str {
    let path = path.trim_end();
    let begin = path
        .rfind(|ch: char| ch.is_ascii() && !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map_or(0, |at| {
            at + path[at..].chars().next().map_or(1, char::len_utf8)
        });
    path.get(begin..).unwrap_or("")
}

/// Every call in `text` as (named type, identifier): the type is the
/// upper-case path segment directly before the identifier (`Money` in
/// `Money::new(8)`), and `None` for a bare call, a method call, a module
/// path or `Self::`, whose callee the text alone does not tie to a type.
fn called_paths(text: &str) -> Vec<(Option<String>, String)> {
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_called_path_ident_start_byte(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_called_path_ident_byte(bytes[index]) {
            index += 1;
        }
        let preceded_by_ident = start > 0 && is_called_path_ident_byte(bytes[start - 1]);
        let rest = masked[index..].trim_start();
        if !preceded_by_ident && (rest.starts_with('(') || rest.starts_with("::<")) {
            let qualifier = masked[..start]
                .trim_end()
                .strip_suffix("::")
                .map(last_called_path_segment)
                .filter(|segment| {
                    *segment != "Self" && segment.starts_with(|ch: char| ch.is_ascii_uppercase())
                })
                .map(str::to_string);
            names.push((qualifier, masked[start..index].to_string()));
        }
    }
    names
}

/// A self-computed assertion's probe-relative strength is at most `weak`.
fn self_computed_cap(strength: OracleStrength, self_computed: bool) -> OracleStrength {
    if self_computed && strength.rank() > OracleStrength::Weak.rank() {
        OracleStrength::Weak
    } else {
        strength
    }
}

/// RIPR-SPEC-0035 self-computed expected value (#5830): an `assert_eq!`
/// with one operand calling `owner` and the other calling a function that
/// transitively reaches `owner`. Both sides then carry the changed
/// behavior, so a wrong owner value moves them together and the equality
/// still holds: `assert_eq!(invoice(3, 100), sub + tax(sub))` for a
/// changed `tax` that `invoice` calls. An operand that computes its
/// value without the owner (a literal, or a call that never reaches it)
/// keeps the assertion's strength.
fn expected_computed_through_owner(
    text: &str,
    owner: &str,
    reaches_owner: &dyn Fn(Option<&str>, &str) -> bool,
) -> bool {
    // RIPR-SPEC-0197: `assert!(a == b)` and its Err-return guard twin
    // compare the same operands an `assert_eq!` would.
    // Selected the way the pin selects (#7094 follow-up): a guard reads its
    // condition and a plain `assert!` its comparison, so an `assert_eq!`
    // spelled in a message, comment or guard body cannot switch the reader.
    let equality;
    let reads_equality = crate::analysis::extract::err_return_guard_twin(text).is_some()
        || super::owner_pin::is_plain_macro(text, "assert");
    let [left, right] = if !reads_equality && text.contains("assert_eq!") {
        let Some(operands) = assertion_comparison_operands(text) else {
            return false;
        };
        operands
    } else {
        equality = super::owner_pin::equality_condition_operands(text);
        let Some([left, right]) = &equality else {
            return false;
        };
        [left.as_str(), right.as_str()]
    };
    let reaches = |operand: &str| {
        called_paths(operand)
            .iter()
            .any(|(ty, called)| called != owner && reaches_owner(ty.as_deref(), called))
    };
    match (text_calls(left, owner), text_calls(right, owner)) {
        // Both sides run the owner. Unless the code around the owner calls
        // differs (`tax(250) * 2` against `tax(250) + 8` pins the owner's
        // value), the sides move together; reordering, parentheses or a
        // path prefix (`2 * tax(250)`, `crate::tax(250)`) do not count as
        // a difference (#6970 review).
        (true, true) => {
            terms_outside_owner_calls(left, owner) == terms_outside_owner_calls(right, owner)
        }
        (true, false) => reaches(right),
        (false, true) => reaches(left),
        _ => false,
    }
}

/// The identifiers and literals of `operand` outside every call of `owner`
/// (its arguments and any `path::` prefix included), sorted, with
/// comments and string contents masked. Operators and grouping are
/// dropped, so equal multisets mean the operands differ at most in order,
/// grouping or what they pass the owner.
fn terms_outside_owner_calls(operand: &str, owner: &str) -> Vec<String> {
    let masked = crate::analysis::extract::mask_comments_and_strings(operand);
    let bytes = masked.as_bytes();
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut terms = Vec::new();
    let mut path = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_word(bytes[index]) {
            if !(bytes[index] == b':' && bytes.get(index + 1) == Some(&b':')) {
                terms.append(&mut path);
            }
            index += if bytes[index] == b':' && bytes.get(index + 1) == Some(&b':') {
                2
            } else {
                1
            };
            continue;
        }
        let start = index;
        while index < bytes.len() && is_word(bytes[index]) {
            index += 1;
        }
        let word = &masked[start..index];
        let rest = masked[index..].trim_start();
        if word == owner && rest.starts_with('(') {
            // Drop the path prefix and skip the balanced argument list.
            path.clear();
            let mut depth = 0usize;
            let mut cursor = masked.len() - rest.len();
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'(' => depth += 1,
                    b')' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            cursor += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            index = cursor;
            continue;
        }
        path.push(word.to_string());
    }
    terms.append(&mut path);
    terms.sort();
    terms
}

/// The bare-scrutinee convention of the synthesized guarded-Result-match
/// oracle text (extract::oracles::scan): the scrutinee path is embedded
/// directly after `match `, so a BARE one-segment scrutinee appears as
/// `match <callee>(`. A qualified scrutinee (`match helpers::parse(..)`)
/// never contains that substring — reveal cannot resolve a qualified path
/// to the probe owner's identity, so its confirmation stays unverified
/// (#3731 review; identity resolution tracked on #3727).
fn guarded_oracle_names_bare_callee(text: &str, callee: &str) -> bool {
    text.contains(&format!("match {callee}("))
}

/// String literals in a match-arm pattern (left of the arm separator `=>`).
/// Empty when the expression has no arm separator: without a statically
/// established pattern there is no literal arm identity to confirm.
///
/// The separator is the first `=>` outside string/character literals and
/// comments (a `=>` inside literal content such as `"sensor=>legacy"` is
/// not an arm separator). Values are decoded literal values, so a raw
/// spelling and a cooked spelling with different decoded values never
/// equate (`r"sensor\n"` is not `"sensor\n"`).
fn match_arm_pattern_literals(expression: &str) -> Vec<String> {
    match find_fat_arrow(expression) {
        Some(separator) => match_arm_string_values(&expression[..separator]),
        None => Vec::new(),
    }
}

/// Whether the parser-owned match-arm pattern includes a guard before `=>`.
///
/// The `if` keyword must sit outside string/character literals and comments,
/// so `"if" =>` and comments do not create a guard. This slice deliberately
/// fails closed for guarded arms: matching the pattern literal alone does
/// not establish guard satisfaction.
fn match_arm_pattern_has_guard(expression: &str) -> bool {
    let Some(separator) = find_fat_arrow(expression) else {
        return false;
    };
    contains_guard_if(&expression[..separator])
}

/// Byte index of the `=` in the first `=>` outside string/character literals
/// and comments. A fat arrow inside literal content or a comment never
/// separates a match arm from its body.
pub(super) fn find_fat_arrow(text: &str) -> Option<usize> {
    let opaque = lex_opaque_ranges(text);
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index + 1 < bytes.len() {
        if opaque
            .iter()
            .any(|(start, end)| index >= *start && index < *end)
        {
            index += 1;
            continue;
        }
        if bytes[index] == b'=' && bytes[index + 1] == b'>' {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// True when a whole-word `if` occurs outside string/character literals and
/// comments. Raw-string contents (including `r#"if"#`) never count.
fn contains_guard_if(pattern: &str) -> bool {
    let opaque = lex_opaque_ranges(pattern);
    let bytes = pattern.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if opaque
            .iter()
            .any(|(start, end)| index >= *start && index < *end)
        {
            index += 1;
            continue;
        }
        let is_if = bytes[index] == b'i'
            && rest_at(pattern, index).starts_with("if")
            && !is_rust_word_continue_at(pattern, index + 2);
        if is_if {
            let before = if index == 0 {
                None
            } else {
                pattern
                    .get(..index)
                    .and_then(|before| before.chars().next_back())
            };
            if before.is_none_or(|ch| !is_rust_word_char(ch)) {
                return true;
            }
        }
        index += 1;
    }
    false
}

/// Suffix of `text` at byte `index`, or empty when `index` is past the end
/// or inside a multibyte character. Structural scans use this so
/// non-ASCII literal contents can never panic byte slicing.
fn rest_at(text: &str, index: usize) -> &str {
    text.get(index..).unwrap_or("")
}

fn is_rust_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_rust_word_continue_at(text: &str, index: usize) -> bool {
    text.get(index..)
        .is_some_and(|rest| rest.chars().next().is_some_and(is_rust_word_char))
}

/// Byte ranges (prefix and quotes included) of the string-literal spans in
/// `text`, covering cooked (`"..."`, `b"..."`) and raw (`r"..."`,
/// `r#"..."#`, `br...`) spellings. Comments and character literals never
/// produce spans.
pub(super) fn string_span_ranges(text: &str) -> Vec<(usize, usize)> {
    lex_strings(text)
        .into_iter()
        .map(|(start, end, _)| (start, end))
        .collect()
}

/// True when `text` holds a string literal that never terminates. The lexer
/// extends such a span to the end of the text, so structural scans fail
/// closed instead of reading past it.
fn has_unterminated_string(text: &str) -> bool {
    // Only an unterminated literal reaches the end of the text without a
    // decoded value; an invalid escape keeps its closing quote and span.
    lex_strings(text)
        .into_iter()
        .any(|(_, end, value)| value.is_none() && end == text.len() && !text.is_empty())
}

/// Decoded string-literal values in `text`, sorted and deduplicated.
/// A raw spelling contributes its verbatim content while a cooked spelling
/// contributes its unescaped value, so spellings with different decoded
/// values never equate. An undecodable or unterminated literal contributes
/// nothing (fail closed).
fn match_arm_string_values(text: &str) -> Vec<String> {
    let mut values = lex_strings(text)
        .into_iter()
        .filter_map(|(_, _, value)| value)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

/// Byte ranges of every lexically opaque region in `text`: string literals
/// (cooked and raw), character literals, line comments, and (nesting-aware)
/// block comments. Structural scans skip these ranges so literal contents
/// never read as syntax.
fn lex_opaque_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut opaque = Vec::new();
    for (start, end, _) in lex_strings(text) {
        opaque.push((start, end));
    }
    let mut index = 0usize;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        if opaque
            .iter()
            .any(|(start, end)| index >= *start && index < *end)
        {
            index += 1;
            continue;
        }
        let rest = rest_at(text, index);
        if rest.starts_with("//") {
            let end = rest.find('\n').map_or(text.len(), |offset| index + offset);
            opaque.push((index, end));
            index = end;
            continue;
        }
        if rest.starts_with("/*") {
            let end = block_comment_end(text, index);
            opaque.push((index, end));
            index = end;
            continue;
        }
        if rest.starts_with('\'')
            && let Some(end) = char_literal_end(text, index)
        {
            opaque.push((index, end));
            index = end;
            continue;
        }
        index += 1;
    }
    opaque
}

/// End byte index of the nesting-aware block comment opening at `start`.
/// An unterminated comment extends to the end of the text so structural
/// scans fail closed instead of reading past it.
fn block_comment_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut cursor = start;
    while cursor < bytes.len() {
        if rest_at(text, cursor).starts_with("/*") {
            depth += 1;
            cursor += 2;
        } else if rest_at(text, cursor).starts_with("*/") {
            depth -= 1;
            cursor += 2;
            if depth == 0 {
                break;
            }
        } else {
            cursor += 1;
        }
    }
    cursor
}

/// String literals as `(span_start, span_end, decoded_value)`. Cooked
/// literals decode standard escapes; raw literals contribute verbatim
/// content. Line and (nesting-aware) block comments are skipped, so quoted
/// comment text never contributes a value while genuine strings containing
/// comment delimiters still lex as strings. An undecodable literal keeps
/// its span with no value, and an unterminated literal extends to the end
/// of the text with no value, so downstream structural scans fail closed
/// instead of reading past it.
pub(super) fn lex_strings(text: &str) -> Vec<(usize, usize, Option<String>)> {
    let mut literals = Vec::new();
    let mut index = 0usize;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        let rest = rest_at(text, index);
        if rest.starts_with("//") {
            index = rest.find('\n').map_or(text.len(), |offset| index + offset);
            continue;
        }
        if rest.starts_with("/*") {
            index = block_comment_end(text, index);
            continue;
        }
        if let Some((end, value)) = raw_string_at(text, index) {
            literals.push((index, end, Some(value)));
            index = end;
            continue;
        }
        if rest.starts_with('\'')
            && let Some(end) = char_literal_end(text, index)
        {
            index = end;
            continue;
        }
        let cooked_start = if rest.starts_with('"') || rest.starts_with("b\"") {
            Some(index)
        } else {
            None
        };
        if let Some(start) = cooked_start {
            let quote = start + usize::from(text[start..].starts_with('b'));
            match cooked_string_at(text, quote) {
                Some((end, value)) => {
                    literals.push((start, end, value));
                    index = end;
                }
                None => {
                    literals.push((start, text.len(), None));
                    break;
                }
            }
            continue;
        }
        index += 1;
    }
    literals
}

/// Raw string at `start` (`r"..."`, `r#"..."#`, `br...`), returning the end
/// byte index and the verbatim content. `None` when no raw string opens
/// here or the terminator is missing (the caller then advances one byte).
fn raw_string_at(text: &str, start: usize) -> Option<(usize, String)> {
    let mut cursor = start;
    if rest_at(text, cursor).starts_with('b') {
        cursor += 1;
    }
    if !rest_at(text, cursor).starts_with('r') {
        return None;
    }
    cursor += 1;
    let mut hashes = 0usize;
    while rest_at(text, cursor).starts_with('#') {
        hashes += 1;
        cursor += 1;
    }
    if !rest_at(text, cursor).starts_with('"') {
        return None;
    }
    cursor += 1;
    let content_start = cursor;
    let bytes = text.as_bytes();
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' {
            let mut tail = cursor + 1;
            let mut seen = 0usize;
            while seen < hashes && tail < bytes.len() && bytes[tail] == b'#' {
                seen += 1;
                tail += 1;
            }
            if seen == hashes {
                return Some((tail, text[content_start..cursor].to_string()));
            }
        }
        cursor += 1;
    }
    None
}

/// Cooked string starting at the opening `"` byte index `quote`, returning
/// the end byte index and the decoded value (`None` value on an invalid
/// escape). `None` when the string never terminates.
fn cooked_string_at(text: &str, quote: usize) -> Option<(usize, Option<String>)> {
    let mut value = String::new();
    let mut valid = true;
    let mut index = quote + 1;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        let ch = rest_at(text, index).chars().next()?;
        if ch == '"' {
            return Some((index + 1, valid.then(|| value.clone())));
        }
        if ch != '\\' {
            value.push(ch);
            index += ch.len_utf8();
            continue;
        }
        match decode_cooked_escape(text, index) {
            Some((consumed, decoded)) => {
                value.push_str(&decoded);
                index += consumed;
            }
            None => {
                valid = false;
                index += 1;
            }
        }
    }
    None
}

/// Decode one cooked escape at the `\` byte index `start`, returning the
/// consumed byte count and the decoded text. Covers `\\`, `\"`, `\'`,
/// `\n`, `\r`, `\t`, `\0`, `\xNN`, `\u{...}`, and the
/// backslash-newline line continuation.
fn decode_cooked_escape(text: &str, start: usize) -> Option<(usize, String)> {
    let rest = &text[start..];
    let mut chars = rest.chars();
    if chars.next() != Some('\\') {
        return None;
    }
    match chars.next()? {
        '\\' => Some((2, "\\".to_string())),
        '"' => Some((2, "\"".to_string())),
        '\'' => Some((2, "'".to_string())),
        'n' => Some((2, "\n".to_string())),
        'r' => Some((2, "\r".to_string())),
        't' => Some((2, "\t".to_string())),
        '0' => Some((2, "\0".to_string())),
        'x' => {
            let digits = rest.get(2..4)?;
            let byte = u8::from_str_radix(digits, 16).ok()?;
            Some((4, (byte as char).to_string()))
        }
        'u' => {
            let braced = rest.strip_prefix("\\u{")?;
            let end = braced.find('}')?;
            let scalar = u32::from_str_radix(&braced[..end], 16).ok()?;
            let decoded = char::from_u32(scalar)?;
            Some((3 + end + 1, decoded.to_string()))
        }
        '\n' => {
            let mut consumed = 2usize;
            for ch in rest[2..].chars() {
                if ch.is_whitespace() {
                    consumed += ch.len_utf8();
                } else {
                    break;
                }
            }
            Some((consumed, String::new()))
        }
        _ => None,
    }
}

/// End byte index of the character literal opening at `start`, or `None`
/// when the `'` begins a lifetime rather than a character literal.
fn char_literal_end(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start + 1..];
    let mut chars = rest.char_indices().peekable();
    let (_, first) = chars.next()?;
    if first == '\n' {
        return None;
    }
    if first == '\\' {
        let mut cursor = 1usize;
        let mut found = false;
        for (offset, ch) in chars {
            cursor = offset + ch.len_utf8();
            if ch == '\'' {
                found = true;
                break;
            }
            if ch == '\n' || cursor > 10 {
                return None;
            }
        }
        if !found {
            return None;
        }
        return Some(start + 1 + cursor);
    }
    let (offset, _) = chars.next()?;
    if rest[offset..].starts_with('\'') {
        return Some(start + 1 + offset + 1);
    }
    None
}

/// Direct string-literal inputs supplied by the observed owner call.
///
/// Match-arm confirmation is intentionally narrower than arbitrary literal
/// occurrence. Only the two compared operands of `assert_eq!` / `assert_ne!`
/// are observed. Diagnostic arguments are ignored. Within a compared operand,
/// the complete expression must be a syntactically bare owner call with one
/// direct string-literal argument. Qualified paths, methods, wrappers,
/// conditionals, blocks, variables, and transformed/nested inputs fail closed.
pub(super) fn split_top_level_arguments(text: &str) -> Option<Vec<&str>> {
    let opaque = lex_opaque_ranges(text);
    let mut arguments = Vec::new();
    let mut start = 0usize;
    let mut stack = Vec::new();

    for (index, ch) in text.char_indices() {
        if opaque
            .iter()
            .any(|(range_start, range_end)| index >= *range_start && index < *range_end)
        {
            continue;
        }

        match ch {
            '(' | '[' | '{' => stack.push(ch),
            ')' if stack.pop() != Some('(') => return None,
            ']' if stack.pop() != Some('[') => return None,
            '}' if stack.pop() != Some('{') => return None,
            ',' if stack.is_empty() => {
                arguments.push(text[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    if !stack.is_empty() {
        return None;
    }
    if has_unterminated_string(text) {
        return None;
    }
    arguments.push(text[start..].trim());
    Some(arguments)
}

pub(super) fn matching_parenthesis(text: &str, opening: usize) -> Option<usize> {
    let opaque = lex_opaque_ranges(text);
    let mut depth = 0usize;

    for (relative, ch) in text[opening..].char_indices() {
        let index = opening + relative;
        if opaque
            .iter()
            .any(|(range_start, range_end)| index >= *range_start && index < *range_end)
        {
            continue;
        }

        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn assertion_comparison_operands(text: &str) -> Option<[&str; 2]> {
    let spans = string_span_ranges(text);
    for macro_name in ["assert_eq!", "assert_ne!"] {
        let mut search_from = 0usize;
        while let Some(relative) = text[search_from..].find(macro_name) {
            let start = search_from + relative;
            if spans
                .iter()
                .any(|(span_start, span_end)| start >= *span_start && start < *span_end)
            {
                search_from = start + macro_name.len();
                continue;
            }

            let mut opening = start + macro_name.len();
            while text[opening..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                opening += text[opening..].chars().next()?.len_utf8();
            }
            if text[opening..].chars().next()? != '(' {
                search_from = start + macro_name.len();
                continue;
            }
            let closing = matching_parenthesis(text, opening)?;
            let arguments = split_top_level_arguments(&text[opening + 1..closing])?;
            if arguments.len() < 2 {
                return None;
            }
            return Some([arguments[0], arguments[1]]);
        }
    }
    None
}

fn direct_owner_string_input(expression: &str, owner: &str) -> Option<String> {
    let expression = expression.trim();
    let rest = expression.strip_prefix(owner)?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }

    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let opening = expression.len() - rest.len();
    let closing = matching_parenthesis(expression, opening)?;
    if !expression[closing + 1..].trim().is_empty() {
        return None;
    }

    let arguments = split_top_level_arguments(&expression[opening + 1..closing])?;
    if arguments.len() != 1 {
        return None;
    }
    let argument = arguments[0].trim();
    let spans = string_span_ranges(argument);
    if spans.len() != 1 || spans[0] != (0, argument.len()) {
        return None;
    }
    let mut literals = match_arm_string_values(argument);
    (literals.len() == 1).then(|| literals.remove(0))
}

fn owner_call_literals(text: &str, owner: &str) -> Vec<String> {
    let Some(operands) = assertion_comparison_operands(text) else {
        return Vec::new();
    };
    let mut literals = operands
        .into_iter()
        .filter_map(|operand| direct_owner_string_input(operand, owner))
        .collect::<Vec<_>>();
    literals.sort();
    literals.dedup();
    literals
}

/// Returns `(matched, has_token_match)`.
///
/// `matched` is true when the assertion should be associated with this probe
/// (via token text, family kind, or single-assertion escape hatch).
/// `has_token_match` is true when the assertion text contains an identifier
/// token from the probe expression that is specific enough to confirm this
/// particular sub-expression is being observed.
///
/// For `MatchArm` probes, `has_token_match` uses only the **variant** tokens
/// (`match_arm_variants`, the identifiers immediately after `::` in the probe
/// expression). This prevents a sibling-arm assertion like `Mode::Warm` from
/// clearing `observation_unverified` for a probe on `Mode::Frozen`, because
/// the shared qualifier token `Mode` is excluded from the confirmation set.
/// When the expression contains no `::` (e.g. `None => 0,`), the variant
/// token list is empty and variant matching contributes nothing. String
/// literals in the arm PATTERN additionally confirm, but only when supplied
/// as inputs to the changed owner (`route("sensor")`): result-side literals
/// never confirm (a sibling returning the same literal does not select this
/// arm), and diagnostic/message literals never confirm. The owner's bare
/// callee name scopes the match; without a known owner nothing confirms.
///
/// For probes whose changed expression constructs an exact error variant,
/// with `ExactErrorVariant` assertions (RIPR-SPEC-0106, Part B): when
/// `error_construction_variant` is `Some`, the oracle is gated on the changed
/// expression's specific variant token:
/// - `ErrorPath` probes: the assertion must pin that variant to match at all.
///   A sibling-variant assertion (`CalcError::Negative`) does not associate
///   with a `CalcError::TooLarge` probe — both share the `CalcError`
///   qualifier token, but only the variant token (`TooLarge`) is specific.
/// - Other direct families (e.g. a `return_value` or `field_construction`
///   probe on an `Err(...)` construction): the assertion stays associated
///   through the standard match rules, but only a variant-pinned text sets
///   `has_token_match`, so a sibling-variant oracle leaves observation
///   unverified instead of crediting discrimination for an unrelated seam.
///
/// Without `error_construction_variant` (probe has no parseable variant),
/// falls back to the standard `token_match` behavior — except for #3700
/// wrapper error seams (`context.wrapper_seam` is `true`), where lexical
/// matching can never confirm observation: the variant binding of a
/// `map_err` conversion is not statically establishable, so the seam stays
/// below `exposed` and carries the typed
/// `wrapper_error_binding_unresolved` limitation.
///
/// A `GuardedResultMatch` assertion confirms a probe only through the
/// producer-owned binding, under five #3731 fail-closed gates: the
/// synthesized text must embed a BARE one-segment scrutinee
/// (`match <owner>(..)` — a qualified path's identity is unresolvable
/// here, #3727), when the changed expression constructs an exact
/// error variant the guarded pin must name that exact variant, the
/// related test's file must not import the owner callee's bare name from
/// a FOREIGN path (a same-name import makes the bare binding ambiguous —
/// see `use_statements_import_foreign_callee_name`), the test's own package
/// must not define a same-named function while the changed owner lives in
/// another package (a bare call may bind the test package's own function —
/// the cross-package ambiguity gate), and — for a return-value probe whose
/// changed value is the SUCCESS payload — the match's Ok arm must observe
/// the unwrapped value (RIPR-SPEC-0175; the fact's extraction-time
/// `ok_value_observed` decision): a routing form with no Ok arm and a
/// payload-ignoring `Ok(_) => ..` arm never observe a changed Ok value, so
/// those confirmations are refused (fail closed, under-credit). For a
/// variant-carrying probe
/// the qualifier token is not a specificity signal, so the guarded
/// oracle's `has_token_match` is exactly the variant-gated owner binding.
fn assertion_matches_probe_detail_with_literals(
    context: &RevealMatchContext,
    assertion: &OracleFact,
    assertion_count: usize,
    import_defeats_owner: bool,
    cross_package_defeats_owner: bool,
    owner_pinned: bool,
    owner_bound: bool,
) -> (bool, bool) {
    let RevealMatchContext {
        probe_tokens,
        effect_literals,
        match_arm_variants,
        match_arm_literals,
        match_arm_guarded,
        error_construction_variant,
        error_construction_qualifier,
        family,
        wrapper_seam,
        owner_callee,
        arm_selector,
        arm_inputs_readable,
        arm_pattern_pinned,
        owner_scoped_tokens,
        owner_shadowed: _,
        sibling_excluded,
    } = *context;
    // #4748: use the same operand boundary as extraction, including token and
    // exact-variant matching. A genuine error oracle cannot borrow its changed
    // reader/variant identity from a diagnostic argument either. Preserve the
    // original OracleFact for public rendering and producer-owned guarded facts.
    let scoped_assertion = (matches!(family, ProbeFamily::ErrorPath)
        && !matches!(assertion.kind, OracleKind::GuardedResultMatch))
    .then(|| crate::analysis::extract::assertion_oracle_text(&assertion.text))
    .flatten()
    .map(|text| OracleFact {
        text,
        ..assertion.clone()
    });
    let assertion = scoped_assertion.as_ref().unwrap_or(assertion);
    let token_match = probe_tokens
        .iter()
        .any(|token| contains_as_whole_word(&assertion.text, token));
    // #5830: the subset of `token_match` that can confirm observation. An
    // owner-scoped token counts only when the assertion is bound to the
    // owner's result: it calls the owner, or names a test `let` bound from
    // an owner call (`let value = score(1); assert!(matches!(value, 2))`).
    let confirming_token_match = probe_tokens.iter().any(|token| {
        contains_as_whole_word(&assertion.text, token)
            && (owner_bound || !owner_scoped_tokens.contains(token))
    });
    let effect_literal_match = !effect_literals.is_empty()
        && rust_string_literals(&assertion.text)
            .iter()
            .any(|literal| effect_literals.contains(literal));
    // #3709: a guarded Result match whose scrutinee directly calls the
    // probe's owner observes the owner's returned `Result` — the sink every
    // value/error behavior in the owner flows through — regardless of the
    // changed line's tokens. The oracle's text embeds the scrutinee callee,
    // so the binding is same-entity by name, not token coincidence; a
    // shadowed callee never produces the oracle (extraction-side defeat),
    // and only the result-defined families (ErrorPath, ReturnValue) credit:
    // a changed effect or call inside the owner need not flow through the
    // matched result, so those families keep their existing observers.
    //
    // #3731 review, four fail-closed gates on the owner shortcut:
    // - BARE scrutinee only. The synthesized text embeds the scrutinee path
    //   after `match `, so a one-segment scrutinee reads `match <callee>(`;
    //   a qualified path (`match helpers::parse(..)`) names an entity this
    //   lexical view cannot resolve to the probe owner (an imported or
    //   re-exported same-named callee is exactly the token-coincidence
    //   family), so its observation stays unverified. Qualified-path
    //   identity resolution is the #3727 follow-up.
    // - Exact variant when the changed expression constructs one. A probe
    //   with an `error_construction_variant` is confirmed only when the
    //   guarded oracle's pin names that exact variant; a sibling-variant
    //   (or type-only) guard does not observe the changed error
    //   (RIPR-SPEC-0106 Part B, mirrored from the ExactErrorVariant gate).
    // - No foreign same-name import in the related test's file (F11). An
    //   import of the callee's bare name from a path outside the analyzed
    //   crate makes the bare binding ambiguous between the owner and the
    //   imported callee, so the confirmation is refused (fail-closed
    //   under-credit). An own-crate import (`use this_crate::callee;` — the
    //   normal integration-test binding) is the owner's own export and does
    //   not defeat.
    // - No same-named function in the test's OWN package when the owner
    //   lives in another package (G1). The RustIndex knows the test
    //   package's own functions; a bare `match <callee>(..)` scrutinee in
    //   that test may bind the local definition instead of the owner, so
    //   the confirmation is refused (fail-closed under-credit).
    // - Ok-arm observation for success-payload return-value probes
    //   (RIPR-SPEC-0175). A return-value probe whose changed expression
    //   does NOT construct an exact error variant is a probe on the
    //   owner's returned success value: it is discriminated only when the
    //   match's Ok arm observes the unwrapped value. The routing form (no
    //   Ok arm — the success value flows into a trivial catch-all) and a
    //   payload-ignoring `Ok(_) => ..` arm never observe a changed Ok
    //   value, so the confirmation is refused (fail closed, under-credit;
    //   the extraction-time `ok_value_observed` decision rides the fact).
    //   A return-value probe on an exact Err construction keeps the
    //   Err-guard discriminator — the pin gate above names the changed
    //   variant — the same principle that leaves ErrorPath probes
    //   unchanged here.
    let producer_owned_result = owner_callee.is_some_and(|owner| {
        matches!(assertion.kind, OracleKind::GuardedResultMatch)
            && matches!(family, ProbeFamily::ErrorPath | ProbeFamily::ReturnValue)
            && !import_defeats_owner
            && !cross_package_defeats_owner
            && guarded_oracle_names_bare_callee(&assertion.text, owner)
            && error_construction_variant
                .is_none_or(|variant| contains_as_whole_word(&assertion.text, variant))
            && (matches!(family, ProbeFamily::ErrorPath)
                || error_construction_variant.is_some()
                || assertion.ok_value_observed == Some(true))
    });
    // #4478: an `assert_eq!` whose operand is a call naming the owner pins
    // the owner's whole return value, which a changed `return_value`
    // expression flows into when the owner's return paths make it the
    // value's source (`owner_pin` decides that and the call's identity).
    // Same defeats as the guarded-match shortcut above: no foreign
    // same-name import, no same-named function in the test's own package,
    // and the exact variant when the changed expression constructs one.
    let owner_return_pinned = owner_return_pin_holds(
        context,
        assertion,
        owner_pinned,
        import_defeats_owner,
        cross_package_defeats_owner,
    );
    // For MatchArm probes, restrict the confirmation check to variant-only
    // tokens (post-`::`). The qualifier ("Mode" in "Mode::Frozen") is shared
    // across all arms and therefore cannot confirm this specific arm.
    // For #3700 wrapper error seams there is no confirmation signal at all:
    // every lexical overlap between the seam expression and a witness text
    // (parameter names, the callee name, `Into::into`, a message string) is
    // token coincidence by construction, so observation stays unverified and
    // the seam cannot read `exposed` from lexical heuristics.
    let has_token_match = if let Some(selector) = arm_selector
        && matches!(family, ProbeFamily::MatchArm)
    {
        // RIPR-SPEC-0229 selection outranks tokens: once the arm's
        // scrutinee is a direct owner input, a variant token or argument
        // literal anywhere in the assertion confirms nothing; only a
        // readable owner call whose input selects the arm does. Same owner
        // ambiguity defeats as the literal rule below.
        arm_inputs_readable
            && !import_defeats_owner
            && !cross_package_defeats_owner
            && (selector.assertion_selects(&assertion.text) || arm_pattern_pinned)
    } else if matches!(family, ProbeFamily::MatchArm) {
        !match_arm_guarded
            && (match_arm_variants
                .iter()
                .any(|v| contains_as_whole_word(&assertion.text, v))
                || !match_arm_literals.is_empty()
                    && !import_defeats_owner
                    && !cross_package_defeats_owner
                    && owner_callee.is_some_and(|owner| {
                        owner_call_literals(&assertion.text, owner)
                            .iter()
                            .any(|literal| match_arm_literals.contains(literal))
                    }))
    } else if wrapper_seam {
        // A #3700 wrapper error seam stays unconfirmable: see above.
        false
    } else if matches!(family, ProbeFamily::ErrorPath) && !assertion_observes_error(assertion) {
        // A changed error path (`reader.read(buf)?`, `return Err(..)`) is
        // visible to a test only as an error. An assertion that never
        // touches an error (`assert_eq!(reader.len(), 10)`) cannot observe
        // it, however many identifiers it shares with the changed line.
        false
    } else if matches!(assertion.kind, OracleKind::GuardedResultMatch)
        && error_construction_variant.is_some()
    {
        // Mirror of the ExactErrorVariant gate (#3731 review): for a
        // variant-carrying probe, a guarded Result match confirms only
        // through the variant-gated owner binding above. The shared
        // enum-qualifier token (`ParseError` in `Err(ParseError::..)`) is
        // not a specificity signal — a guard pinning a sibling variant of
        // the same error type would otherwise clear the unverified flag.
        producer_owned_result
    } else {
        confirming_token_match
            || effect_literal_match
            || producer_owned_result
            || owner_return_pinned
    };
    // PR #6786 review: an assertion of any other kind (an `exact_value`
    // `assert!(matches!(e, PayError::Limit))` inside a match arm) that names
    // the changed error's enum only through sibling variants shares just
    // the enum qualifier with the changed line. Same outcome as the
    // ExactErrorVariant gate below with a non-matching variant: an
    // ErrorPath probe does not match it at all, and no family confirms
    // observation through it. Guarded matches keep their own variant gate
    // (`producer_owned_result`) above.
    if !matches!(
        assertion.kind,
        OracleKind::ExactErrorVariant | OracleKind::GuardedResultMatch
    ) && let (Some(variant), Some(qualifier)) =
        (error_construction_variant, error_construction_qualifier)
        && names_only_sibling_variants(&assertion.text, qualifier, variant)
    {
        if sibling_excluded {
            return (true, true);
        }
        if matches!(family, ProbeFamily::ErrorPath) {
            return (false, false);
        }
        return (token_match || effect_literal_match, false);
    }
    // Fail-closed: if error_construction_variant is None (no parseable variant
    // in the probe), fall through to the standard token_match + family_match
    // check below.
    if matches!(assertion.kind, OracleKind::ExactErrorVariant)
        && let Some(variant) = error_construction_variant
    {
        let variant_matches = contains_as_whole_word(&assertion.text, variant);
        if matches!(family, ProbeFamily::ErrorPath) {
            return (variant_matches, variant_matches);
        }
        return (token_match || effect_literal_match, variant_matches);
    }
    let family_match = oracle_matches_family(family, assertion);
    let matched = token_match
        || effect_literal_match
        || family_match
        || producer_owned_result
        || owner_return_pinned
        || assertion_count == 1;
    (matched, has_token_match)
}

/// #4478: whether an owner-pinned assertion keeps its pin after the
/// reveal-side defeats: no foreign same-name import, no same-named
/// function in the test's own package, and the exact variant when the
/// changed expression constructs one. The one authority for both the
/// match decision and the credited owner-pin outcome (#6692).
fn owner_return_pin_holds(
    context: &RevealMatchContext,
    assertion: &OracleFact,
    owner_pinned: bool,
    import_defeats_owner: bool,
    cross_package_defeats_owner: bool,
) -> bool {
    owner_pinned
        && !import_defeats_owner
        && !cross_package_defeats_owner
        && context
            .error_construction_variant
            .is_none_or(|variant| contains_as_whole_word(&assertion.text, variant))
}

#[cfg(test)]
#[allow(
    clippy::too_many_arguments,
    reason = "test-only mirror of the grouped RevealMatchContext inputs"
)]
fn assertion_matches_probe_detail(
    probe_tokens: &[String],
    match_arm_variants: &[String],
    match_arm_literals: &[String],
    error_construction_variant: Option<&str>,
    family: &ProbeFamily,
    assertion: &OracleFact,
    assertion_count: usize,
    owner_callee: Option<&str>,
) -> (bool, bool) {
    assertion_matches_probe_detail_with_literals(
        &RevealMatchContext {
            probe_tokens,
            effect_literals: &[],
            match_arm_variants,
            match_arm_literals,
            match_arm_guarded: false,
            error_construction_variant,
            error_construction_qualifier: None,
            family,
            wrapper_seam: false,
            owner_callee,
            arm_selector: None,
            arm_inputs_readable: false,
            arm_pattern_pinned: false,
            owner_scoped_tokens: &[],
            owner_shadowed: false,
            sibling_excluded: false,
        },
        assertion,
        assertion_count,
        false,
        false,
        false,
        false,
    )
}

/// Every `use` declaration of a file source, masked, trimmed, and without
/// its terminating `;` — the callee-independent half of
/// `use_statements_import_foreign_callee_name`, so one scan of a file serves every
/// callee and every probe (see `FileUseStatements`).
pub(in crate::analysis::classify) fn file_use_statements(source: &str) -> Vec<String> {
    let masked = crate::analysis::extract::mask_comments_and_strings(source);
    all_use_statements(&masked)
        .iter()
        .map(|statement| {
            let statement = statement.trim();
            statement
                .strip_suffix(';')
                .unwrap_or(statement)
                .trim_end()
                .to_string()
        })
        .collect()
}

/// One imported item of a `use` declaration: its full path with
/// whitespace removed (`a::b::C`, `a::E::*`) and the `as` rename, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UsePath {
    pub(super) path: String,
    pub(super) alias: Option<String>,
}

/// Every `use` declaration of `source` flattened into one path per item,
/// with brace lists expanded (`use a::{b, c::*};` -> `a::b`, `a::c::*`).
pub(super) fn flattened_use_paths(source: &str) -> Vec<UsePath> {
    let mut out = Vec::new();
    for statement in file_use_statements(source) {
        if let Some(rest) = statement.trim_start().strip_prefix("use") {
            flatten_use_items("", rest, &mut out);
        }
    }
    out
}

fn flatten_use_items(prefix: &str, items: &str, out: &mut Vec<UsePath>) {
    for item in split_top_level_commas(items) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        match item.find('{') {
            None => {
                let words = item.split_whitespace().collect::<Vec<_>>();
                let (path, alias) = match words.iter().position(|word| *word == "as") {
                    Some(at) => (
                        words[..at].concat(),
                        words.get(at + 1).map(|w| w.to_string()),
                    ),
                    None => (words.concat(), None),
                };
                out.push(UsePath {
                    path: format!("{prefix}{path}"),
                    alias,
                });
            }
            Some(open) => {
                if let Some(close) = matching_brace_close(item, open) {
                    let head = item[..open].split_whitespace().collect::<String>();
                    flatten_use_items(&format!("{prefix}{head}"), &item[open + 1..close], out);
                }
            }
        }
    }
}

/// #3731 review (F11, F22): whether the related test's file imports the
/// owner callee's bare name FROM A FOREIGN PATH — a `use` binding whose
/// first path segment is neither `crate`/`self`/`super` nor one of the
/// analyzed workspace's own package names (`crate_names`). Such an import
/// makes a bare `match <callee>(..)` scrutinee ambiguous between the
/// changed owner and the imported same-named callee, so the reveal-side
/// owner binding must not confirm (fail closed, under-credit). An
/// own-crate import (`use this_crate::callee;` — the normal
/// integration-test binding of the changed owner) binds the owner itself
/// and does not defeat.
///
/// Bounded lexical scan over the masked file source covering ALL `use`
/// declarations (#3731 review F22): file-level items, module-nested `use`s
/// (`mod tests { use other::expect_response; .. }` — the historical
/// harness shape), and function-local imports. A binding is the terminal
/// `::` segment of an import item — a simple path or a (nested) brace-list
/// item; a `callee as alias` rename binds the alias, not the name. Glob
/// (`use p::*;`) imports prove nothing and are not detected, and a
/// brace-rooted `use {..};` with no path prefix counts as foreign (its
/// binding target is not statically the owner's own export) — bounded-scan
/// residuals; parser-backed import resolution is #3727. Scanning past
/// module boundaries can defeat a confirmation for a test the nested
/// import is not visible to — a documented under-credit residual, since
/// lexical scope resolution is exactly what this scan cannot do.
///
/// This is the callee-dependent half; `file_use_statements` produces its
/// statements and `FileUseStatements` memoizes them per file.
fn use_statements_import_foreign_callee_name(
    statements: &[String],
    callee: &str,
    crate_names: &std::collections::BTreeSet<String>,
) -> bool {
    if callee.is_empty() {
        return false;
    }
    for statement in statements {
        let statement = statement.as_str();
        let Some(first_segment) = use_statement_first_segment(statement) else {
            continue;
        };
        // Both sides compare in crate-identifier form (#3731 review F23):
        // a hyphenated package name (`foo-bar`) is imported through its
        // underscore identifier (`foo_bar`), so every stored crate name
        // admits both spellings.
        let own = crate_names
            .iter()
            .any(|name| name == first_segment || crate_identifier(name) == first_segment);
        let foreign =
            first_segment != "crate" && first_segment != "self" && first_segment != "super" && !own;
        if foreign && use_statement_binds_name(statement, callee) {
            return true;
        }
    }
    false
}

/// Scans `source` and applies the gate in one call. The per-probe defeat
/// reads the same two halves through `FileUseStatements`; the owner-return
/// pin (#4478) calls this directly for a receiver type or trait name.
pub(in crate::analysis) fn file_imports_foreign_callee_name(
    source: &str,
    callee: &str,
    crate_names: &std::collections::BTreeSet<String>,
) -> bool {
    use_statements_import_foreign_callee_name(&file_use_statements(source), callee, crate_names)
}

/// The crate-identifier form of a manifest name: hyphens normalize to
/// underscores in crate identifiers, so a package named `foo-bar` is
/// imported as `foo_bar` and an import gate must treat the two spellings
/// as the same crate (#3731 review F23).
fn crate_identifier(name: &str) -> String {
    name.replace('-', "_")
}

/// Per-file `use` declarations for the reveal-side same-name-import gate,
/// scanned at most once per file for as long as the memo lives.
///
/// The gate depends only on the test file's source, the owner callee, and
/// the index's package names, never on the probe, so a classification run
/// shares one memo across all its probes (it rides the run-scoped
/// `RelatedTestCandidateIndex`). Re-masking every related test file for
/// every probe was about a third of the sampled stacks of a warm
/// `ripr check` on this repository. The memo must not outlive the index it was filled from.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct FileUseStatements {
    by_file: std::cell::RefCell<std::collections::BTreeMap<std::path::PathBuf, Vec<String>>>,
}

impl FileUseStatements {
    /// `use_statements_import_foreign_callee_name` over `source`'s statements,
    /// scanning `source` only on the first query for `file`. Every caller
    /// must pass the indexed source that `file` resolves to.
    pub(in crate::analysis) fn imports_foreign_callee_name(
        &self,
        file: &std::path::Path,
        source: &str,
        callee: &str,
        crate_names: &std::collections::BTreeSet<String>,
    ) -> bool {
        if callee.is_empty() {
            return false;
        }
        if let Some(statements) = self.by_file.borrow().get(file) {
            return use_statements_import_foreign_callee_name(statements, callee, crate_names);
        }
        let statements = file_use_statements(source);
        let imports = use_statements_import_foreign_callee_name(&statements, callee, crate_names);
        self.by_file
            .borrow_mut()
            .insert(file.to_path_buf(), statements);
        imports
    }
}

/// Every `use` declaration in a (masked) source, at any brace depth: the
/// statement runs from a whole-word `use` keyword to its terminating `;`
/// (a `use` path or brace list cannot contain `;`, and comments/strings
/// are already masked, so a masked-out `use` inside a string or comment
/// never appears). Consuming each statement whole keeps a later
/// same-tuned text inside one import from re-matching.
fn all_use_statements(masked: &str) -> Vec<String> {
    let bytes = masked.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let use_starts = bytes[index] == b'u'
            && bytes.get(index + 1) == Some(&b's')
            && bytes.get(index + 2) == Some(&b'e')
            && (index == 0 || !is_ident_byte(bytes[index - 1]))
            && bytes[index + 3..]
                .first()
                .is_some_and(|byte| byte.is_ascii_whitespace());
        if use_starts {
            let mut end = index;
            while end < bytes.len() && bytes[end] != b';' {
                end += 1;
            }
            if end < bytes.len() {
                out.push(masked[index..=end].to_string());
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }
    out
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The first path segment of a `use` statement (the keyword is still
/// present): `use crate::x::y;` -> `crate`, `use a::b::{c};` -> `a`. An
/// empty segment (a brace-rooted `use {..};`) signals no path prefix.
pub(in crate::analysis::classify) fn use_statement_first_segment(statement: &str) -> Option<&str> {
    let rest = statement.trim_start().strip_prefix("use")?;
    // `use ::name::..` roots the path at the extern crate `name`, the same
    // crate `use name::..` names.
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("::").map_or(rest, str::trim_start);
    let end = rest
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Whether the `use` statement binds `callee` as an imported item: the
/// terminal `::` segment of a brace-less path, or any brace-list item
/// (nested brace lists recurse). `as` renames bind the alias; `*` and
/// `self` bind nothing nameable here.
pub(in crate::analysis::classify) fn use_statement_binds_name(
    statement: &str,
    callee: &str,
) -> bool {
    let Some(rest) = statement.trim_start().strip_prefix("use") else {
        return false;
    };
    use_items_bind(rest.trim_start(), callee, ImportMatch::Binding)
}

/// What a `use` item is matched on: the name it binds in the importing
/// scope, or the item it imports (`use p::Buf as _;` imports `Buf` without
/// binding the name, which still brings a trait's methods into scope).
#[derive(Clone, Copy)]
enum ImportMatch {
    Binding,
    Item,
}

/// Whether `source` imports the item `name` through a `use` path rooted in
/// this workspace (`crate`, `self`, `super`, or a workspace package), at
/// any depth (#4478). Glob imports are not read: they prove nothing about
/// which items they bring in.
pub(in crate::analysis) fn file_imports_own_item(
    source: &str,
    name: &str,
    crate_names: &std::collections::BTreeSet<String>,
) -> bool {
    if name.is_empty() {
        return false;
    }
    let masked = crate::analysis::extract::mask_comments_and_strings(source);
    all_use_statements(&masked).iter().any(|statement| {
        let statement = statement.trim();
        let statement = statement.strip_suffix(';').unwrap_or(statement).trim_end();
        let Some(first_segment) = use_statement_first_segment(statement) else {
            return false;
        };
        let own = first_segment == "crate"
            || first_segment == "self"
            || first_segment == "super"
            || crate_names.iter().any(|crate_name| {
                crate_name == first_segment || crate_identifier(crate_name) == first_segment
            });
        own && statement
            .trim_start()
            .strip_prefix("use")
            .is_some_and(|rest| use_items_bind(rest.trim_start(), name, ImportMatch::Item))
    })
}

/// Whether one comma-separated `use` item group binds `callee`. An item is
/// a `::`-separated path that may end in a brace list; the binding of a
/// brace-less item is its terminal segment (respecting `as` renames), and
/// brace-list items recurse.
fn use_items_bind(items: &str, callee: &str, mode: ImportMatch) -> bool {
    for item in split_top_level_commas(items) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        match item.find('{') {
            None => {
                if braceless_item_binds(item, callee, mode) {
                    return true;
                }
            }
            Some(open) => {
                if let Some(close) = matching_brace_close(item, open)
                    && use_items_bind(&item[open + 1..close], callee, mode)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// The binding name of a brace-less import item: its terminal `::`
/// segment, with `callee as alias` renames resolving to the alias.
fn braceless_item_binds(item: &str, callee: &str, mode: ImportMatch) -> bool {
    let terminal = item.rsplit("::").next().unwrap_or(item).trim();
    let mut parts = terminal.split_whitespace();
    let name = parts.next().unwrap_or("");
    if matches!(mode, ImportMatch::Binding) && parts.next() == Some("as") {
        return parts.next() == Some(callee);
    }
    name == callee && name != "*" && name != "self"
}

/// The comma-separated top-level (depth-0) slices of an item list.
fn split_top_level_commas(items: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, character) in items.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&items[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(&items[start..]);
    out
}

/// The byte offset of the `}` closing the `{` at `open`, or `None`.
fn matching_brace_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (index, character) in text[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the changed expression is a wrapper error seam: a `map_err`
/// conversion over a callee result whose error identity the seam expression
/// itself does not spell out (no `Err(..)` construction). This is the #3700
/// boxed-wrapper shape — `try_parse_summary(raw).map_err(Into::into)` — where
/// the propagated variant lives in the callee, not in the changed line.
pub(in crate::analysis) fn wrapper_error_seam_expression(expressions: &[&str]) -> bool {
    expressions
        .iter()
        .any(|expression| last_top_level_map_err_dot(expression).is_some())
}

/// Strips grouping that wraps the ENTIRE expression — balanced `(..)` and
/// `{..}` pairs whose opener is the first character and whose closer is the
/// last — repeatedly (`(try_x(raw).map_err(Into::into));` -> the inner
/// conversion, #3714 round-2 review, devin hGdAZ). Grouping that does NOT
/// span the whole expression (`(a) + (b.map_err(f))`) is left in place:
/// those shapes fail closed instead of crediting an ambiguous conversion.
pub(in crate::analysis) fn without_harmless_outer_groups(expression: &str) -> &str {
    // A trailing statement semicolon must not defeat the group-span check.
    let mut working = expression.trim();
    if let Some(stripped) = working.strip_suffix(';') {
        working = stripped.trim_end();
    }
    loop {
        let bytes = working.as_bytes();
        if bytes.first() != Some(&b'(') && bytes.first() != Some(&b'{') {
            return working;
        }
        let opener = bytes[0];
        let closer = if opener == b'(' { b')' } else { b'}' };
        let mut depth = 0isize;
        let mut matched = false;
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == opener {
                depth += 1;
            } else if *byte == closer {
                depth -= 1;
                if depth == 0 {
                    matched = index == bytes.len() - 1;
                    break;
                }
            }
        }
        if !matched {
            return working;
        }
        working = &working[1..working.len() - 1];
        working = working.trim();
    }
}

/// The byte index of the `.` opening the LAST top-level `.map_err(..)`
/// conversion in `expression`. Top-level means bracket depth zero, with
/// string literals, char literals, and lifetimes skipped (a `'` that does
/// not close as a character literal is a lifetime or loop label, not a
/// literal — #3714 round-2 review, devin hDRNH). Single shared authority
/// for wrapper-seam detection (decision.rs, the limitation limiter, and
/// the #3714 related-test attribution) so syntax fixes cannot make the
/// paths disagree (#3714 round-2 review, devin hDRP-). `None` when no
/// `.map_err(..)` conversion opens at depth zero.
pub(in crate::analysis) fn last_top_level_map_err_dot(expression: &str) -> Option<usize> {
    let expression = without_harmless_outer_groups(expression);
    let bytes = expression.as_bytes();
    let mut depth = 0isize;
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;
    let mut last = None;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if in_char {
            if escaped {
                escaped = false;
            } else if byte == b'\'' {
                in_char = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'\'' => {
                // A quote opens a char literal only in the `'x'` and
                // `'\x'` forms; lifetimes (`'_`, `'a`, `'ctx`) and loop
                // labels have no closing quote and are skipped.
                let tail = &bytes[index + 1..];
                let opens_char = (tail.first() == Some(&b'\\') && tail.get(2) == Some(&b'\''))
                    || tail.first() == Some(&b'\'')
                    || (tail
                        .first()
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                        && tail.get(1) == Some(&b'\''));
                if opens_char {
                    in_char = true;
                }
                index += 1;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'.' if depth == 0 => {
                let rest = &expression[index + 1..];
                let name = rest.trim_start();
                if let Some(after_name) = name.strip_prefix("map_err") {
                    let token_is_whole = after_name
                        .chars()
                        .next()
                        .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'));
                    let followed_by_call = after_name.trim_start().starts_with('(');
                    if token_is_whole && followed_by_call {
                        last = Some(index);
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    last
}

/// Establish the wrapper-to-variant binding for a wrapper error seam.
///
/// The seam's changed expression carries no parseable variant, so the
/// propagated variant identity must come from a witness whose exact-variant
/// pin demonstrably constrains the callee's own result: a related test that
/// calls the seam's callee, where the `matches!` scrutinee either calls the
/// callee directly or names a variable bound from a callee call in the test
/// body. The stored identity is the qualified `Enum::Variant` path, so equal
/// terminal variant names from different enums cannot align. A witness that
/// only calls the wrapper, only names a variant in message text, or pins a
/// variant against another call establishes nothing (#3700).
/// Check whether `text` contains `token` as a whole word — delimited by
/// non-identifier characters (or string boundaries) on both sides. This
/// replaces the old `token.len() > 3` gate, which filtered out short tokens
/// like `id`, `key`, `sum` entirely. With word-boundary matching, a short
/// token matches `result.id` or `id == 42` but NOT `provider` or `middle`
/// (#2397).
///
/// Uses `is_ident_char` (alphanumeric + underscore) for boundary checks,
/// matching Rust identifier rules: `_` is part of an identifier, so `err`
/// does NOT match inside `is_err`.
pub(in crate::analysis) fn contains_as_whole_word(text: &str, token: &str) -> bool {
    fn is_ident_char(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }
    if token.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = text[start..].find(token) {
        let abs_pos = start + pos;
        let end_pos = abs_pos + token.len();
        let before_ok = abs_pos == 0 || !is_ident_char(text.as_bytes()[abs_pos - 1]);
        let after_ok = end_pos >= text.len() || !is_ident_char(text.as_bytes()[end_pos]);
        if before_ok && after_ok {
            return true;
        }
        // Step past the match's first char, not one byte: a token that starts
        // with a multibyte char would otherwise leave `start` inside it.
        start = abs_pos + token.chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn finalize_related_tests(mut related: Vec<RelatedTest>) -> (Vec<RelatedTest>, usize) {
    related.sort_by(|a, b| a.name.cmp(&b.name).then(a.line.cmp(&b.line)));
    related.dedup_by(|a, b| a.name == b.name && a.oracle == b.oracle);
    // Renderers present the first entry as the primary related test, so the
    // strongest relation leads. The sort is stable: name and line order holds
    // within one confidence tier, and the dedup above is unchanged.
    related.sort_by_key(|test| std::cmp::Reverse(related_test_rank(test)));
    // #5344: tests listed only as examined misses are packed separately and
    // only into slots the oracle rows leave free, so the oracle rows, their
    // order and their packing are exactly what they were before misses were
    // listed. Downstream selection (exact-oracle alignment, fix sites, repair
    // readiness) reads this window.
    let (mut oracle_rows, unmatched): (Vec<_>, Vec<_>) =
        related.into_iter().partition(|test| !test.is_unmatched());
    // Same post-dedup row unit as before; retain the total separately without
    // exposing discarded rows to downstream evidence/target selection.
    let matched_total = oracle_rows.len() + unmatched.len();
    // JSON/human renderers cap at eight rows. When one test's assertions would
    // fill that window, unique tests go first (#4760). Under the cap, keep
    // per-assertion rows so existing goldens and #1728 witnesses stay intact.
    if oracle_rows.len() > RELATED_TESTS_RENDER_CAP {
        oracle_rows = pack_unique_tests_first(oracle_rows, RELATED_TESTS_RENDER_CAP);
    }
    let free = RELATED_TESTS_RENDER_CAP.saturating_sub(oracle_rows.len());
    oracle_rows.extend(unmatched.into_iter().take(free));
    (oracle_rows, matched_total)
}

const RELATED_TESTS_RENDER_CAP: usize = 8;

fn pack_unique_tests_first(related: Vec<RelatedTest>, cap: usize) -> Vec<RelatedTest> {
    let mut packed = Vec::with_capacity(cap.min(related.len()));
    let mut seen = std::collections::BTreeSet::new();
    for test in &related {
        if packed.len() >= cap {
            break;
        }
        let key = (test.name.as_str(), test.file.as_path(), test.line);
        if seen.insert(key) {
            packed.push(test.clone());
        }
    }
    for test in &related {
        if packed.len() >= cap {
            break;
        }
        if packed.iter().any(|kept| {
            kept.name == test.name
                && kept.file == test.file
                && kept.line == test.line
                && kept.oracle == test.oracle
        }) {
            continue;
        }
        packed.push(test.clone());
    }
    packed
}

/// Sort rank of an emitted related test: higher relation confidence ranks
/// first; an unknown relation origin ranks with `Opaque`.
fn related_test_rank(test: &RelatedTest) -> u8 {
    match test.relation_confidence {
        Some(RelationConfidence::High) => 3,
        Some(RelationConfidence::Medium) => 2,
        Some(RelationConfidence::Low) => 1,
        Some(RelationConfidence::Opaque) | None => 0,
    }
}

/// Which relations may supply a credited oracle: a name-only relation
/// (`WeakTokenSubstring`, `OwnerNamedTest`) may not while any related test
/// bears reach. Shared with the RIPR-SPEC-0240 refusal scope, which must judge
/// exactly the tests that could have credited the refused assertions.
pub(in crate::analysis) fn oracle_crediting_relations(
    related_tests: &[(&TestSummary, RelationReason)],
) -> impl Fn(RelationReason) -> bool + use<> {
    let reach_bearing_related = related_tests
        .iter()
        .any(|(_, reason)| !is_name_only_relation(*reason) && !is_proximity_only(*reason));
    move |reason| !(reach_bearing_related && is_name_only_relation(reason))
}

/// A relation made only by the test's name or path (a changed token, the
/// owner's name), with no captured call, helper chain or assertion affinity.
fn is_name_only_relation(reason: RelationReason) -> bool {
    matches!(
        reason,
        RelationReason::WeakTokenSubstring | RelationReason::OwnerNamedTest
    )
}

pub(in crate::analysis) const ASSERTION_CONTEXT_UNESTABLISHED: &str =
    crate::domain::ASSERTION_CONTEXT_UNESTABLISHED;

fn build_observe_evidence(matched_any: bool, refused_context: bool) -> StageEvidence {
    if matched_any {
        StageEvidence::new(
            StageState::Yes,
            Confidence::Medium,
            "A related test observes a value or effect near the changed behavior",
        )
    } else {
        StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            if refused_context {
                ASSERTION_CONTEXT_UNESTABLISHED
            } else {
                "Related tests were found, but no assertion appears to observe the changed value, error, field, or effect"
            },
        )
    }
}

fn build_discriminate_evidence(
    strongest: &OracleStrength,
    strongest_kind: &OracleKind,
    family: &ProbeFamily,
    observation_unverified: bool,
) -> StageEvidence {
    // For families that require token confirmation (MatchArm, ReturnValue,
    // FieldConstruction, SideEffect, CallDeletion), a family_match or
    // assertion_count==1 alone cannot confirm that an assertion observes *this*
    // specific changed sub-expression. Without a token_match, downgrade to Weak
    // so classify() emits weakly_exposed (observation_unverified). A probe
    // with a token_match stays exposed.
    if observation_unverified {
        return StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            "Discriminator unconfirmed: no assertion text references this probe's changed expression (observation_unverified)",
        );
    }
    match strongest {
        OracleStrength::Strong => StageEvidence::new(
            StageState::Yes,
            Confidence::Medium,
            match strongest_kind {
                OracleKind::ExactErrorVariant => {
                    "Strong oracle found: exact error variant assertion"
                }
                OracleKind::GuardedResultMatch => {
                    "Strong oracle found: guarded Result match over the changed owner's result"
                }
                OracleKind::WholeObjectEquality => {
                    "Strong oracle found: whole-object equality assertion"
                }
                _ => "Strong oracle found: exact value or pattern assertion",
            },
        ),
        OracleStrength::Medium => StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            match strongest_kind {
                OracleKind::Snapshot => {
                    "Medium oracle found: snapshot assertion observes the changed behavior"
                }
                OracleKind::MockExpectation => {
                    "Medium oracle found: mock or expectation observes the changed behavior"
                }
                OracleKind::GuardedResultMatch => {
                    "Medium oracle found: guarded Result match pins the error type, not an exact variant"
                }
                _ => "Medium oracle found: property or partial structural assertion",
            },
        ),
        OracleStrength::Weak => StageEvidence::new(
            StageState::Weak,
            Confidence::High,
            match (strongest_kind, family) {
                (OracleKind::BroadError, ProbeFamily::ErrorPath) => {
                    "Only broad error oracle found; is_err() does not discriminate exact error variants"
                }
                (OracleKind::BroadError, _) => {
                    "Only broad error oracle found; it may not discriminate the changed behavior exactly"
                }
                (OracleKind::RelationalCheck, _) => {
                    "Only relational oracle found; it may not discriminate the changed value exactly"
                }
                _ => {
                    "Only weak oracle found, such as a broad relational assertion or non-empty check"
                }
            },
        ),
        OracleStrength::Smoke => StageEvidence::new(
            StageState::Weak,
            Confidence::High,
            "Only smoke oracle found, such as unwrap/expect or execution without a discriminator",
        ),
        OracleStrength::None => StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            "No assertion found on related tests",
        ),
        OracleStrength::Unknown => StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            "Assertions exist, but oracle strength is unknown",
        ),
    }
}

/// Typed family/kind relationships are kept in data so the supported oracle
/// shapes can be audited without following nested family and kind matches.
/// Text heuristics remain below because they are intentionally conservative
/// fallback signals, not typed oracle classifications. `ProbeFamily::StaticUnknown`
/// has no typed relationships.
const ORACLE_FAMILY_MATCHES: &[(ProbeFamily, OracleKind)] = &[
    (ProbeFamily::ErrorPath, OracleKind::ExactErrorVariant),
    (ProbeFamily::ErrorPath, OracleKind::BroadError),
    (ProbeFamily::SideEffect, OracleKind::MockExpectation),
    (ProbeFamily::FieldConstruction, OracleKind::ExactValue),
    (
        ProbeFamily::FieldConstruction,
        OracleKind::WholeObjectEquality,
    ),
    (ProbeFamily::FieldConstruction, OracleKind::RelationalCheck),
    (ProbeFamily::FieldConstruction, OracleKind::Snapshot),
    (ProbeFamily::Predicate, OracleKind::ExactValue),
    (ProbeFamily::Predicate, OracleKind::RelationalCheck),
    (ProbeFamily::Predicate, OracleKind::ExactErrorVariant),
    (ProbeFamily::Predicate, OracleKind::Snapshot),
    (ProbeFamily::ReturnValue, OracleKind::ExactValue),
    (ProbeFamily::ReturnValue, OracleKind::WholeObjectEquality),
    (ProbeFamily::ReturnValue, OracleKind::RelationalCheck),
    (ProbeFamily::ReturnValue, OracleKind::Snapshot),
    (ProbeFamily::ReturnValue, OracleKind::SmokeOnly),
    (ProbeFamily::CallDeletion, OracleKind::MockExpectation),
    (ProbeFamily::CallDeletion, OracleKind::ExactValue),
    (ProbeFamily::CallDeletion, OracleKind::RelationalCheck),
    (ProbeFamily::CallDeletion, OracleKind::SmokeOnly),
    (ProbeFamily::MatchArm, OracleKind::ExactErrorVariant),
    (ProbeFamily::MatchArm, OracleKind::ExactValue),
    (ProbeFamily::MatchArm, OracleKind::RelationalCheck),
    (ProbeFamily::MatchArm, OracleKind::Snapshot),
];

/// Family-specific text fallbacks are typed separately from parsed
/// [`OracleKind`] relationships. They preserve support for conservative custom
/// assertion helpers while keeping text recognition in the assertion-pattern
/// owner rather than in reveal classification.
const ORACLE_FAMILY_TEXT_SHAPES: &[(ProbeFamily, OracleTextShape)] = &[
    (ProbeFamily::ErrorPath, OracleTextShape::ErrorPath),
    (ProbeFamily::SideEffect, OracleTextShape::SideEffect),
    (
        ProbeFamily::FieldConstruction,
        OracleTextShape::MemberAccess,
    ),
    (
        ProbeFamily::CallDeletion,
        OracleTextShape::AssertionOrExpectation,
    ),
];

/// Family-specific strength overrides. A missing entry preserves the
/// classifier's parsed assertion strength; entries are only for kinds whose
/// relative strength is fixed by the probe family.
const ORACLE_FAMILY_STRENGTH_OVERRIDES: &[(ProbeFamily, OracleKind, OracleStrength)] = &[
    (
        ProbeFamily::ErrorPath,
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::ErrorPath,
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
    ),
    (
        ProbeFamily::ReturnValue,
        OracleKind::ExactValue,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::ReturnValue,
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::ReturnValue,
        OracleKind::WholeObjectEquality,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::ReturnValue,
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
    ),
    (
        ProbeFamily::ReturnValue,
        OracleKind::Unknown,
        OracleStrength::Unknown,
    ),
    (
        ProbeFamily::Predicate,
        OracleKind::ExactValue,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::Predicate,
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::Predicate,
        OracleKind::WholeObjectEquality,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::Predicate,
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
    ),
    (
        ProbeFamily::Predicate,
        OracleKind::Unknown,
        OracleStrength::Unknown,
    ),
    (
        ProbeFamily::FieldConstruction,
        OracleKind::ExactValue,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::FieldConstruction,
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::FieldConstruction,
        OracleKind::WholeObjectEquality,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::FieldConstruction,
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
    ),
    (
        ProbeFamily::FieldConstruction,
        OracleKind::Unknown,
        OracleStrength::Unknown,
    ),
    (
        ProbeFamily::MatchArm,
        OracleKind::ExactValue,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::MatchArm,
        OracleKind::ExactErrorVariant,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::MatchArm,
        OracleKind::WholeObjectEquality,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::MatchArm,
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
    ),
    (
        ProbeFamily::MatchArm,
        OracleKind::Unknown,
        OracleStrength::Unknown,
    ),
    (
        ProbeFamily::SideEffect,
        OracleKind::ExactValue,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::SideEffect,
        OracleKind::WholeObjectEquality,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::SideEffect,
        OracleKind::ExactErrorVariant,
        OracleStrength::Medium,
    ),
    (
        ProbeFamily::SideEffect,
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
    ),
    (
        ProbeFamily::SideEffect,
        OracleKind::Unknown,
        OracleStrength::Unknown,
    ),
    (
        ProbeFamily::CallDeletion,
        OracleKind::ExactValue,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::CallDeletion,
        OracleKind::WholeObjectEquality,
        OracleStrength::Strong,
    ),
    (
        ProbeFamily::CallDeletion,
        OracleKind::ExactErrorVariant,
        OracleStrength::Medium,
    ),
    (
        ProbeFamily::CallDeletion,
        OracleKind::SmokeOnly,
        OracleStrength::Smoke,
    ),
    (
        ProbeFamily::CallDeletion,
        OracleKind::Unknown,
        OracleStrength::Unknown,
    ),
    (
        ProbeFamily::StaticUnknown,
        OracleKind::Unknown,
        OracleStrength::Unknown,
    ),
];

fn oracle_matches_family(family: &ProbeFamily, assertion: &OracleFact) -> bool {
    let typed_match = ORACLE_FAMILY_MATCHES
        .iter()
        .any(|(rule_family, rule_kind)| rule_family == family && rule_kind == &assertion.kind);
    let text_shape_match = ORACLE_FAMILY_TEXT_SHAPES
        .iter()
        .any(|(rule_family, shape)| {
            rule_family == family && has_oracle_text_shape(&assertion.text, *shape)
        });
    typed_match || text_shape_match
}

fn probe_relative_oracle_strength(family: &ProbeFamily, assertion: &OracleFact) -> OracleStrength {
    if matches!(family, ProbeFamily::StaticUnknown) {
        return OracleStrength::Unknown;
    }
    let overridden = ORACLE_FAMILY_STRENGTH_OVERRIDES
        .iter()
        .find(|(rule_family, rule_kind, _)| rule_family == family && rule_kind == &assertion.kind)
        .map_or_else(
            || assertion.strength.clone(),
            |(_, _, strength)| strength.clone(),
        );
    // RIPR-SPEC-0231: no rule raises a strength. A family override may lower
    // the classifier's reading, but an assertion the classifier weakened
    // (`assert_ne!` against a struct literal keeps `whole_object_equality`
    // at weak) never comes back as strong because of its kind. Rule 1: an
    // inequality is never lifted above its classified strength, whatever
    // that strength is.
    if (matches!(
        assertion.strength,
        OracleStrength::Medium | OracleStrength::Weak | OracleStrength::Smoke
    ) || is_inequality_assertion(&assertion.text))
        && overridden.rank() > assertion.strength.rank()
    {
        assertion.strength.clone()
    } else {
        overridden
    }
}

/// RIPR-SPEC-0231 decision 4: an exact owner pin (`assert_eq!`) needs the
/// classifier's strong reading. The one exception is RIPR-SPEC-0197's
/// bool-owner pin: `assert!(owner(..))` / `assert!(!owner(..))` stays the
/// classifier's weak `relational_check`, but on a `-> bool` owner it fixes
/// the whole two-valued result, and `owner_pin` alone admits that shape.
fn pin_strength_admits(assertion: &OracleFact) -> bool {
    matches!(assertion.kind, OracleKind::RelationalCheck)
        || is_inline_expect_snapshot(assertion)
        || assertion.strength == OracleStrength::Strong
}

/// #6578: a `snapshot` oracle the owner pin may raise is only the inline
/// `expect![[..]].assert_eq(..)` comparison it reads like `assert_eq!`; an
/// `insta` macro, `expect_file!`, `assert_debug_eq` or any other text the
/// classifier grades `snapshot` keeps its medium strength.
fn is_inline_expect_snapshot(assertion: &OracleFact) -> bool {
    matches!(assertion.kind, OracleKind::Snapshot)
        && super::owner_pin::inline_expect_operands(&assertion.text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RIPR-SPEC-0228 rule 2: the field collection is read after the owner
    /// call on the same, unrebound binding; comments and strings never call.
    #[test]
    fn binding_untouched_after_owner_call_refuses_writes_between_call_and_read() {
        // The body starts at file line 10; the read sits on line 14.
        let body = |lines: &str| format!("fn case() {{\n{lines}\n}}");
        let untouched =
            |lines: &str| binding_untouched_after_owner_call(&body(lines), 10, "order", "add", 14);
        for kept in [
            "    let mut order = Order::new();\n    order.add(4);\n\n    assert_eq!(order.log, vec![4]);",
            // Plain field reads in an assertion between are not writes.
            "    let mut order = Order::new();\n    order.add(4);\n    assert_eq!(order.total, 4);\n    assert_eq!(order.log, vec![4]);",
            "    let mut order = Order::new();\n    order.add(4);\n    assert_eq!(order.tags, vec![]);\n    assert_eq!(order.log, vec![4]);",
            // Statements that do not name the binding.
            "    let mut order = Order::new();\n    order.add(4);\n    let expected = vec![4];\n    assert_eq!(order.log, expected);",
            // The result consumed by `unwrap`, `expect` or `?`.
            "    let mut order = Order::new();\n    order.add(4).unwrap();\n    order.add(5).expect(\"added\")?;\n    assert_eq!(order.log, vec![4, 5]);",
            // Further bare owner calls in the window.
            "    let mut order = Order::new();\n    order.add(1);\n    order.add(4);\n    assert_eq!(order.log, vec![1, 4]);",
        ] {
            assert!(untouched(kept), "{kept}");
        }
        for refused in [
            "    order.add(4);\n    order.log.clear();\n    order.log.push(4);\n    assert_eq!(order.log, vec![4]);",
            // A snapshot between two calls, however it is spelled.
            "    order.add(4);\n    let before = order.log.clone();\n    order.add(4);\n    assert_eq!(order.log, before);",
            "    order.add(4);\n    let before = if true { order.log.clone() } else { vec![] };\n    order.add(4);\n    assert_eq!(order.log, before);",
            "    order.add(4);\n    before.extend_from_slice(&order.log);\n    order.add(4);\n    assert_eq!(order.log, before);",
            "    order.add(1);\n    order.log.clear();\n    order.add(4);\n    assert_eq!(order.log, vec![4]);",
            // A later owner call whose arguments copy the binding.
            "    let mut order = Order::new();\n    order.add(4);\n    order.add((snap = order.log.clone(), 4).1);\n    assert_eq!(order.log, [snap.clone(), snap].concat());",
            // A later owner call that is not a bare statement.
            "    let mut order = Order::new();\n    order.add(1);\n    order.add(4).clear();\n    assert_eq!(order.log, vec![]);",
            "    let mut order = Order::new();\n    order.add(4);\n    order.log = vec![4];\n    assert_eq!(order.log, vec![4]);",
            "    let mut order = Order::new();\n    order.add(4);\n    let log = &mut order.log;\n    assert_eq!(order.log, vec![4]);",
            "    let mut order = Order::new();\n    order.add(4);\n    assert_eq!(order.log.pop(), Some(4));\n    assert_eq!(order.log, vec![4]);",
            "    let mut order = Order::new();\n    order.add(4);\n    normalize(&mut order);\n    assert_eq!(order.log, vec![4]);",
            // No call before the read, or only one that may not run.
            "    let mut order = Order::new();\n\n\n    assert_eq!(order.log, vec![4]);",
            "    let mut order = Order::new();\n    if false { order.add(4); }\n\n    assert_eq!(order.log, vec![4]);",
            // A chained or bound call result may write without naming `order`.
            "    let mut order = Order::new();\n    order.add(4).clear();\n\n    assert_eq!(order.log, vec![]);",
            "    let mut order = Order::new();\n    order.add(4).unwrap().clear();\n\n    assert_eq!(order.log, vec![]);",
            "    let mut order = Order::new();\n    let o = order.add(4);\n    o.log.clear();\n    assert_eq!(order.log, vec![]);",
            // A write earlier on the read line.
            "    let mut order = Order::new();\n    order.add(4);\n\n    order.log.clear(); assert_eq!(order.log, vec![]);",
            // Assertions that write, or are not plain assertions.
            "    let mut order = Order::new();\n    order.add(4);\n    assert!({ order.log[0] = 1; true });\n    assert_eq!(order.log, vec![1]);",
            "    let mut order = Order::new();\n    order.add(4);\n    assert_eq!(order.log[0], 4);\n    assert_eq!(order.log, vec![4]);",
            "    order.add(4);\n    assert!(match order { Order { ref mut log, .. } => { log.clear(); true } });\n\n    assert_eq!(order.log, vec![]);",
            "    let mut order = Order::new();\n    order.add(4);\n    assert!(reset(&mut (order)));\n    assert_eq!(order.log, vec![]);",
            "    let mut order = Order::new();\n    order.add(4);\n    assert_reset!(order);\n    assert_eq!(order.log, vec![]);",
            "    let mut order = Order::new();\n    order.add(4);\n    assert!(reset![1, order, 2], \"m\");\n    assert_eq!(order.log, vec![]);",
            "    let mut order = Order::new();\n    order.add(4);\n    assert!(my::vec![1, order, 2].is_empty());\n    assert_eq!(order.log, vec![]);",
        ] {
            assert!(!untouched(refused), "{refused}");
        }
    }

    /// #7688 review: a value built through the owner on another receiver
    /// may carry the changed value into the other side of the equality.
    #[test]
    fn owner_called_on_other_receiver_sees_every_other_owner_call() {
        let called = |lines: &str| {
            owner_called_on_other_receiver(&format!("fn case() {{\n{lines}\n}}"), "order", "add")
        };
        for kept in [
            "    order.add(1);\n    order.add(2);",
            "    order.add(1);\n    readd(3);\n    order.address();",
            "    order.add(1);\n    // other.add(2);\n    let s = \"x.add(1)\";",
        ] {
            assert!(!called(kept), "{kept}");
        }
        for refused in [
            "    other.add(1);\n    order.add(1);",
            "    let exp = { let mut o = Order::new(); o.add(4); o };\n    order.add(4);",
            "    Order::add(&mut exp, 4);\n    order.add(4);",
            "    let exp = Order::new().add(4).clone();\n    order.add(4);",
            "    let exp = self.order.add(4);\n    order.add(4);",
            "    let f = Order::add;\n    f(&mut exp, 4);\n    order.add(4);",
            "    exp. add(4);\n    order.add(4);",
            "    Order::new()\n        .add(4);\n    order.add(4);",
        ] {
            assert!(called(refused), "{refused}");
        }
    }

    /// RIPR-SPEC-0228 keyed reads: only an insert at the owner's top level
    /// runs on every call.
    #[test]
    fn unconditional_owner_insert_field_only_at_the_top_level() {
        let owner = "pub fn put(&mut self, sku: &str, qty: u32) {\n    if qty > 100 {\n        self.m.insert(sku.to_string(), 100);\n    } else {\n        self.m.insert(sku.to_string(), qty);\n    }\n    self.m.insert(sku.to_string(), qty);\n    self.items.iter().for_each(|k| self.m.insert(k.clone(), 1));\n    // self.m.insert(sku.to_string(), 0);\n}";
        let runs = |line: usize| unconditional_owner_insert_field(owner, 20, line).is_some();
        assert_eq!(
            unconditional_owner_insert_field(owner, 20, 26).as_deref(),
            Some("m")
        );
        assert!(!runs(22));
        assert!(!runs(24));
        assert!(runs(26));
        assert!(!runs(27));
        assert!(!runs(28));
        assert!(!runs(19));
        assert!(!runs(99));
    }

    /// RIPR-SPEC-0228 keyed reads: a `#[should_panic]` test passes when the
    /// compared values differ, so it never confirms a keyed read.
    #[test]
    fn keyed_field_read_refuses_should_panic_tests() {
        let assertion_text = "assert_eq!(inv.on_hand(\"A\"), 7);";
        let test = TestSummary {
            start_line: 1,
            ..test_with_body_assertions(
                "ships",
                "fn ships() {\n    let mut inv = Inventory::new(2);\n    inv.ship(\"A\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"A\"), 7);\n}",
                vec![OracleFact {
                    line: 4,
                    ..oracle(
                        assertion_text,
                        OracleKind::ExactValue,
                        OracleStrength::Strong,
                    )
                }],
            )
        };
        let admission = ReturnOracleAdmission {
            owner_return_pin: &|_, _| false,
            assertion_admitted: &|_, _| true,
            sibling_exclusion_admitted: &|_, _| true,
            proximity_may_reach_owner: &|_| false,
            owner_parameters: &[],
            expected_reaches_owner: &|_, _| false,
            effect_state_carried: &|_, _| true,
            setter_writes_field: &|_, _, _| false,
            getter_reads_field: &|_, _, _| false,
            getter_reads_key: &|_, getter, field| {
                (getter == "on_hand" && field == "stock")
                    .then(|| KeyedFallback::Literal("0".to_string()))
            },
            owner_argument_names: &[],
            owner_returns_value: true,
            constructor_starts_empty: &|_, _, _| true,
            call_cannot_store: &|_, _, _| true,
        };
        let confirms = |test: &TestSummary| {
            keyed_field_read_confirms(
                "stock",
                0,
                OwnerCall::from_symbol("src/inv.rs::impl Inventory::ship"),
                test,
                &test.assertions[0],
                &admission,
            )
        };
        assert!(confirms(&test));
        let panicking = TestSummary {
            attrs: vec!["#[should_panic]".to_string()],
            ..test.clone()
        };
        assert!(!confirms(&panicking));
    }

    /// RIPR-SPEC-0228 keyed reads: the owner call must pass the read key in
    /// the insert key's argument position and must not end on an error path.
    #[test]
    fn owner_call_passes_key_matches_the_key_argument_position() {
        // The body starts at file line 10; the read is its last line.
        let body = |lines: &str| format!("fn case() {{\n{lines}\n}}");
        let check = |lines: &str, returns_value: bool, index: usize, key: &str| {
            owner_call_passes_key(
                &body(lines),
                10,
                10 + lines.lines().count(),
                &KeyedCall {
                    binding: "inv",
                    method: "ship",
                    getter: "on_hand",
                    expected: "7",
                    impl_type: "Inventory",
                    returns_value,
                    index,
                    key,
                    path: "stock",
                    // `Inventory::seeded(..)` stands for a constructor that
                    // fills the map.
                    starts_empty: &|impl_type, ctor, field| {
                        impl_type == "Inventory" && field == "stock" && ctor != "seeded"
                    },
                    // `inv.load_defaults()` stands for a method that stores
                    // the expected value.
                    cannot_store: &|impl_type, method, expected| {
                        impl_type == "Inventory" && expected == "7" && method != "load_defaults"
                    },
                },
            )
        };
        let passes = |lines: &str, index: usize, key: &str| check(lines, true, index, key);
        let shipped = "    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);";
        assert!(passes(shipped, 0, "\"BOLT-M8\""));
        // A `&` on either side is the same key.
        assert!(passes(shipped, 0, "&\"BOLT-M8\""));
        assert!(passes(
            "    let mut inv = Inventory::new(2);\n    let receipt = inv.ship(sku, 3).unwrap();\n    assert_eq!(inv.on_hand(&sku), 7);",
            0,
            "&sku"
        ));
        // A bound result named again before the read may be a `&mut Self`
        // that rewrites the entry.
        for compared in [
            "    assert_eq!(receipt, Receipt { remaining: 7 });\n",
            "    assert_eq!(Receipt { remaining: 7 }, receipt);\n",
        ] {
            assert!(
                passes(
                    &format!(
                        "    let mut inv = Inventory::new(2);\n    let receipt = inv.ship(sku, 3).unwrap();\n{compared}    assert_eq!(inv.on_hand(&sku), 7);"
                    ),
                    0,
                    "&sku"
                ),
                "{compared}"
            );
        }
        for reused in [
            "    let _ = receipt;\n",
            "    receipt.set(sku, 7);\n",
            "    assert_eq!(receipt.set(sku, 7), ());\n",
            "    assert!(touch(receipt));\n",
            "    assert_eq!(x, touch(1, receipt));\n",
        ] {
            assert!(!passes(
                &format!(
                    "    let mut inv = Inventory::new(2);\n    let receipt = inv.ship(sku, 3).unwrap();\n{reused}    assert_eq!(inv.on_hand(&sku), 7);"
                ),
                0,
                "&sku"
            ));
        }
        // `?` and `.expect(..)` run the owner to completion; so does a bare
        // call when the owner returns nothing.
        for consumed in [
            "    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3)?;\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv = Inventory::new(2);\n    let mut r = inv.ship(\"BOLT-M8\", 3).expect(\"ships\");\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv: Inventory = Default::default();\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv = Inventory::default();\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
        ] {
            assert!(passes(consumed, 0, "\"BOLT-M8\""), "{consumed}");
        }
        let bare = "    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3);\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);";
        assert!(check(bare, false, 0, "\"BOLT-M8\""));
        // A bare call to an owner that returns a value may have returned
        // `Err` before the insert.
        assert!(!check(bare, true, 0, "\"BOLT-M8\""));
        // Another key, another position, a key past the argument list.
        assert!(!passes(shipped, 0, "\"NUT-M8\""));
        assert!(!passes(shipped, 1, "\"BOLT-M8\""));
        assert!(!passes(shipped, 2, "\"BOLT-M8\""));
        for refused in [
            // The owner may have returned before the insert.
            "    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 30).unwrap_err();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            "    let mut inv = Inventory::new(2);\n    let e = inv.ship(\"BOLT-M8\", 30).expect_err(\"refused\");\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            "    let mut inv = Inventory::new(2);\n    assert!(inv.ship(\"BOLT-M8\", 30).is_err());\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            // A result kept, discarded or projected without being consumed.
            "    let mut inv = Inventory::new(2);\n    let r = inv.ship(\"BOLT-M8\", 30);\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            "    let mut inv = Inventory::new(2);\n    let _ = inv.ship(\"BOLT-M8\", 30).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            "    let mut inv = Inventory::new(2);\n    let ok = inv.ship(\"BOLT-M8\", 30).is_ok();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            "    let mut inv = Inventory::new(2);\n    assert!(!inv.ship(\"BOLT-M8\", 30).is_ok());\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            "    let mut inv = Inventory::new(2);\n    let (a, b) = inv.ship(\"BOLT-M8\", 30).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            // A call that may never run, or no call before the read.
            "    let mut inv = Inventory::new(2);\n    if false { inv.ship(\"BOLT-M8\", 3).unwrap(); }\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);",
            "    let mut inv = Inventory::new(2);\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 10);\n    inv.ship(\"BOLT-M8\", 3).unwrap();",
            // A receiver not built as the owner type, or not declared here.
            "    let mut inv = Other::new(2);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv = inventory();\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv: InventoryView = make();\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv = InventoryView::new();\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            // A struct literal or an unannotated `Default::default()` is not
            // a constructor verified to leave the map empty.
            "    let mut inv = Inventory { stock: BTreeMap::new() };\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv = Default::default();\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv = Inventory::new(2).with(\"BOLT-M8\", 7);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            // A second or nested declaration of the binding.
            "    let mut inv = Inventory::seeded(&[(\"BOLT-M8\", 7)]);\n    { let inv = Inventory::new(2); inv.len(); }\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    let mut inv = Inventory::seeded(&[(\"BOLT-M8\", 7)]);\n    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            "    {\n    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);\n    }",
            // An early return between the call and the read.
            "    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    if skip() { return; }\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            // A write on the read's own line, before the assertion.
            "    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    inv.set(\"BOLT-M8\", 7); assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
        ] {
            assert!(!passes(refused, 0, "\"BOLT-M8\""), "{refused}");
        }
        // A key variable must hold the same value at the call and the read.
        let keyed = |between: &str| {
            format!(
                "    let mut inv = Inventory::new(2);\n    let mut sku = \"A\";\n    inv.ship(sku, 3).unwrap();\n{between}    assert_eq!(inv.on_hand(sku), 7);"
            )
        };
        assert!(passes(&keyed(""), 0, "sku"));
        assert!(passes(&keyed("    let n = 1;\n"), 0, "sku"));
        for between in [
            "    sku = \"B\";\n",
            "    let sku = \"B\";\n",
            "    change(&mut sku);\n",
        ] {
            assert!(!passes(&keyed(between), 0, "sku"), "{between}");
        }
        // Rule 3: between the call and the read the binding may only be
        // read through the getter.
        let rewritten = |between: &str| {
            format!(
                "    let mut inv = Inventory::new(2);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n{between}    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);"
            )
        };
        assert!(passes(
            &rewritten("    let before = inv.on_hand(\"BOLT-M8\");\n"),
            0,
            "\"BOLT-M8\""
        ));
        assert!(passes(
            &rewritten("    let invoice = 3;\n"),
            0,
            "\"BOLT-M8\""
        ));
        // A second owner call is itself the accepted call before the read.
        assert!(passes(
            &rewritten("    inv.ship(\"BOLT-M8\", 1).unwrap();\n"),
            0,
            "\"BOLT-M8\""
        ));
        for between in [
            "    inv.set(\"BOLT-M8\", 7);\n",
            "    inv.stock.clear();\n",
            "    reset(&mut inv);\n",
        ] {
            assert!(!passes(&rewritten(between), 0, "\"BOLT-M8\""), "{between}");
        }
        // An earlier statement that already stores the expected value under
        // the key: the read passes whether or not the insert runs.
        let seeded = |before: &str| {
            format!(
                "    let mut inv = Inventory::new(2);\n{before}    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);"
            )
        };
        assert!(passes(
            &seeded("    inv.receive(\"BOLT-M8\", 10);\n"),
            0,
            "\"BOLT-M8\""
        ));
        assert!(passes(
            &seeded(
                "    inv.receive(\"NUT-M8\", 10).unwrap();\n    inv.receive(\"BOLT-M8\", 10)?;\n"
            ),
            0,
            "\"BOLT-M8\""
        ));
        assert!(passes(
            &seeded("    other.set(\"BOLT-M8\", 7);\n"),
            0,
            "\"BOLT-M8\""
        ));
        // Any earlier call on the binding that could store the expected
        // value: a literal equal to it under any spelling or suffix, a
        // computed or variable argument, a helper taking the binding, or
        // anything that is not a plain call statement.
        for before in [
            "    inv.set(\"BOLT-M8\", 7);\n",
            "    inv.receive(\"BOLT-M8\", 7).unwrap();\n",
            "    inv.receive(\"NUT-M8\", 7);\n",
            "    inv.set(\"BOLT-M8\", 7u32);\n",
            "    inv.set(\"BOLT-M8\", 7_u32);\n",
            "    inv.set(\"BOLT-M8\", 3 + 4);\n",
            "    let v = 7;\n    inv.set(\"BOLT-M8\", v);\n",
            "    seed(&mut inv);\n",
            "    inv.stock.insert(\"BOLT-M8\".into(), 7);\n",
            "    inv.set(\"BOLT-M8\", 10).set(\"BOLT-M8\", 7);\n",
            "    if ready { inv.set(\"BOLT-M8\", 10); }\n",
            "    inv.load_defaults();\n",
        ] {
            assert!(!passes(&seeded(before), 0, "\"BOLT-M8\""), "{before}");
        }
        assert!(!passes(
            "    let mut inv = Inventory::seeded(&[(\"BOLT-M8\", 7)]);\n    inv.ship(\"BOLT-M8\", 3).unwrap();\n    assert_eq!(inv.on_hand(\"BOLT-M8\"), 7);",
            0,
            "\"BOLT-M8\""
        ));
        // A computed key is never matched by spelling alone.
        assert!(!passes(
            "    let mut inv = Inventory::new(2);\n    inv.ship(next(), 3).unwrap();\n    assert_eq!(inv.on_hand(next()), 7);",
            0,
            "next()"
        ));
    }

    #[test]
    fn owner_call_precedes_read_needs_an_earlier_call_on_the_same_binding() {
        // The body starts at file line 10; the read sits on line 13.
        let body = |lines: &str| format!("fn case() {{\n{lines}\n}}");
        let precedes = |lines: &str| owner_call_precedes_read(&body(lines), 10, "order", "add", 13);
        assert!(precedes(
            "    let mut order = Order::new();\n    order.add(4);\n    assert_eq!(order.entries(), &[4]);"
        ));
        for refused in [
            // Read before the call.
            "    let mut order = Order::new();\n    assert_eq!(order.entries(), &[] as &[u32]);\n    order.add(4);",
            // Rebound between the call and the read.
            "    order.add(4);\n    let order = Order::new();\n    assert!(order.entries().is_empty());",
            "    order.add(4);\n    let mut order = Order::new();\n    assert!(order.entries().is_empty());",
            // A call in a comment or string, or on another binding.
            "    let mut order = Order::new();\n    // order.add(4);\n    assert_eq!(order.entries(), &[] as &[u32]);",
            "    let mut order = Order::new();\n    let s = \"order.add(4)\";\n    assert_eq!(order.entries(), &[] as &[u32]);",
            "    let mut order = Order::new();\n    my_order.add(4);\n    assert_eq!(order.entries(), &[] as &[u32]);",
            // Rebound by a plain assignment, or by a shadow on the call's line.
            "    order.add(4);\n    order = Order::new();\n    assert!(order.entries().is_empty());",
            "    order.add(4); let order = Order::new();\n\n    assert!(order.entries().is_empty());",
            // A call that may never run: in a closure, a branch or a loop.
            "    let later = || order.add(4);\n\n    assert!(order.entries().is_empty());",
            "    if false { order.add(4); }\n\n    assert!(order.entries().is_empty());",
            "    for _ in 0..0 {\n        order.add(4);\n    assert!(order.entries().is_empty());",
        ] {
            assert!(!precedes(refused), "{refused}");
        }
        // A comparison of the binding is not an assignment.
        assert!(precedes(
            "    order.add(4);\n    let same = order == Order::new();\n    assert_eq!(order.entries(), &[4]);"
        ));
        // Rebinding another name between the call and the read is fine.
        assert!(precedes(
            "    order.add(4);\n    let orders = Order::new();\n    assert_eq!(order.entries(), &[4]);"
        ));
    }
    use std::path::PathBuf;

    fn related(name: &str, line: usize, reason: Option<RelationReason>) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/lib.rs"),
            line,
            oracle: Some(format!("assert_eq!({name}, 1);")),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: reason,
            relation_confidence: reason.map(RelationReason::confidence),
            miss: None,
        }
    }

    /// Renderers show the first related test as the primary one, so the
    /// finalized order must lead with the strongest relation rather than
    /// the alphabetically first name.
    #[test]
    fn finalized_related_tests_lead_with_the_strongest_relation() {
        let (finalized, _) = finalize_related_tests(vec![
            related(
                "a_same_file_neighbor",
                3,
                Some(RelationReason::SameTestFile),
            ),
            related("b_unknown_origin", 5, None),
            related(
                "c_direct_owner_call",
                9,
                Some(RelationReason::DirectOwnerCall),
            ),
            related(
                "c_direct_owner_call",
                9,
                Some(RelationReason::DirectOwnerCall),
            ),
            related(
                "d_direct_owner_call",
                1,
                Some(RelationReason::DirectOwnerCall),
            ),
        ]);
        let names: Vec<&str> = finalized.iter().map(|test| test.name.as_str()).collect();
        let ranks: Vec<u8> = finalized.iter().map(related_test_rank).collect();
        let mut descending = ranks.clone();
        descending.sort_by(|a, b| b.cmp(a));
        assert_eq!(ranks, descending, "ranks must not increase: {names:?}");
        assert_eq!(names.first(), Some(&"c_direct_owner_call"));
        assert_eq!(names.last(), Some(&"b_unknown_origin"));
        assert_eq!(names.len(), 4, "duplicate entries still dedup: {names:?}");
    }

    #[test]
    fn related_test_total_retains_six_eight_nine_matches_without_changing_packed_prefix() {
        for count in [6, 8, 9] {
            let input: Vec<_> = (0..count)
                .map(|index| {
                    related(
                        &format!("case_{index:02}"),
                        index + 1,
                        Some(RelationReason::DirectOwnerCall),
                    )
                })
                .collect();
            let (packed, total) = finalize_related_tests(input.clone());
            assert_eq!(total, count);
            assert_eq!(packed.len(), count.min(RELATED_TESTS_RENDER_CAP));
            let previous_prefix = if count > RELATED_TESTS_RENDER_CAP {
                pack_unique_tests_first(input, RELATED_TESTS_RENDER_CAP)
            } else {
                input
            };
            assert_eq!(packed, previous_prefix);
        }
    }

    #[test]
    fn related_test_total_uses_existing_post_dedup_oracle_row_unit() {
        let mut input: Vec<_> = (0..9)
            .map(|index| {
                related(
                    &format!("case_{index:02}"),
                    index + 1,
                    Some(RelationReason::DirectOwnerCall),
                )
            })
            .collect();
        let first = related("case_00", 1, Some(RelationReason::DirectOwnerCall));
        input.push(first.clone()); // Existing name/oracle dedup removes this.
        let mut second_oracle = first;
        second_oracle.oracle = Some("assert_eq!(different_observer, 2);".to_string());
        input.push(second_oracle); // A distinct oracle row remains the count unit.
        let (packed, total) = finalize_related_tests(input);
        assert_eq!(total, 10);
        assert_eq!(packed.len(), 8);
        assert_eq!(
            packed
                .iter()
                .map(|test| test.name.as_str())
                .collect::<Vec<_>>(),
            (0..8)
                .map(|index| format!("case_{index:02}"))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn strongest_oracle_cannot_borrow_weaker_assertion_confirmation() -> Result<(), String> {
        for family in [ProbeFamily::ReturnValue, ProbeFamily::CallDeletion] {
            let probe = probe(family, "compute_score(input)");
            let exact = oracle(
                "assert_eq!(unrelated, 42);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            );
            let weak = oracle(
                "assert!(compute_score > 0);",
                OracleKind::RelationalCheck,
                OracleStrength::Weak,
            );
            for assertions in [
                vec![exact.clone(), weak.clone()],
                vec![weak.clone(), exact.clone()],
            ] {
                let test = test_with_assertions("mixed_oracles", assertions);
                let (_, discriminate, _) =
                    reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
                if discriminate.state != StageState::Weak {
                    return Err(format!(
                        "unrelated exact oracle borrowed weak confirmation: {discriminate:?}"
                    ));
                }
            }
            let strong_test = test_with_assertions("unrelated_exact", vec![exact]);
            let weak_test = test_with_assertions("weak_owner_observer", vec![weak]);
            for related in [
                vec![
                    (&strong_test, RelationReason::SameTestFile),
                    (&weak_test, RelationReason::DirectOwnerCall),
                ],
                vec![
                    (&weak_test, RelationReason::DirectOwnerCall),
                    (&strong_test, RelationReason::SameTestFile),
                ],
            ] {
                let (_, discriminate, _) = reveal_evidence(&probe, &related);
                if discriminate.state != StageState::Weak {
                    return Err(format!(
                        "unrelated test supplied exact discrimination: {discriminate:?}"
                    ));
                }
            }
        }
        Ok(())
    }

    /// A changed `?` propagation is visible only as an error. A success-value
    /// assertion that shares the reader's name (ripgrep `line_buffer.rs`
    /// `rdr.read(..)?` vs a test's `rdr.bstr()`) must not confirm it; an
    /// error-observing assertion over the same name still does.
    #[test]
    fn error_path_is_confirmed_only_by_an_error_observing_assertion() -> Result<(), String> {
        let probe = probe(ProbeFamily::ErrorPath, "let n = rdr.read(&mut buf)?;");
        let success = test_with_assertions(
            "reads_everything",
            vec![oracle(
                "assert_eq!(rdr.len(), 10);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&success, RelationReason::DirectOwnerCall)]);
        if discriminate.state != StageState::Weak {
            return Err(format!(
                "success-value assertion confirmed an error path: {discriminate:?}"
            ));
        }
        for text in [
            "assert!(matches!(rdr.read(&mut buf), Err(_)));",
            "assert_eq!(read_all(rdr).unwrap_err().kind(), Other);",
            "assert_eq!(rdr.last_error(), Some(ReadError::Closed));",
        ] {
            let failing = test_with_assertions(
                "reports_read_error",
                vec![oracle(text, OracleKind::ExactValue, OracleStrength::Strong)],
            );
            let (_, discriminate, _) =
                reveal_evidence(&probe, &[(&failing, RelationReason::DirectOwnerCall)]);
            if discriminate.state != StageState::Yes {
                return Err(format!(
                    "error-observing assertion `{text}` lost confirmation: {discriminate:?}"
                ));
            }
        }
        Ok(())
    }

    /// Diagnostic operands cannot turn a success check into an error observer,
    /// including diagnostic expressions that the producer used to type as errors.
    #[test]
    fn error_path_diagnostics_do_not_confirm_observation() -> Result<(), String> {
        let probe = probe(ProbeFamily::ErrorPath, "let n = rdr.read(&mut buf)?;");
        for text in [
            r#"assert_eq!(rdr.len(), 10);"#,
            r#"assert_eq!(rdr.len(), 10, "read mismatch");"#,
            r#"assert_eq!(rdr.len(), 10, "read error");"#,
            r##"assert_eq!(rdr.len(), 10, r#"read "error", (Err(_))"#);"##,
            r#"assert_eq!(rdr.len(), 10, "read \"error\", (panic)");"#,
            r#"assert_eq!(rdr.len(), 10, "{}", read_error);"#,
            r#"assert_eq!(rdr.len(), 10, "{:?}", Err::<(), _>(ReadError::Closed));"#,
            r#"assert_eq!(rdr.len(), 10, "assert!(matches!(rdr, Err(ReadError::Closed)))");"#,
            r#"assert_eq![rdr.len(), 10, "read error"];"#,
            r#"assert!{rdr.len() == 10, "read error"};"#,
            r#"assert_eq!(rdr.len() /* read error */, 10);"#,
        ] {
            let classification = crate::analysis::extract::classify_assertion(text);
            let test = test_with_assertions(
                "reads_successfully",
                vec![oracle(text, classification.kind, classification.strength)],
            );
            let (_, discriminate, _) =
                reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
            if discriminate.state != StageState::Weak {
                return Err(format!(
                    "diagnostic `{text}` confirmed an error path: {discriminate:?}"
                ));
            }
        }
        Ok(())
    }

    /// A test-local identifier that merely contains an error lexeme
    /// (`error_count`, `nonerror`) is not an error observer. #4748 excluded
    /// diagnostics; this residual is operand position (#5255).
    #[test]
    fn error_path_operand_error_lexeme_does_not_confirm_observation() -> Result<(), String> {
        let probe = probe(ProbeFamily::ErrorPath, "let n = rdr.read(&mut buf)?;");
        for text in [
            "assert_eq!((rdr.len(), error_count), (10, 0));",
            "assert_eq!(rdr.len(), error_count);",
            "assert_eq!(rdr.len(), err_count);",
            "assert_eq!((rdr.len(), nonerror), (10, 0));",
        ] {
            let classification = crate::analysis::extract::classify_assertion(text);
            let test = test_with_assertions(
                "reads_successfully",
                vec![oracle(text, classification.kind, classification.strength)],
            );
            let (_, discriminate, _) =
                reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
            if discriminate.state != StageState::Weak {
                return Err(format!(
                    "operand error lexeme `{text}` confirmed an error path: {discriminate:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn error_path_diagnostic_tokens_cannot_pin_the_changed_error() -> Result<(), String> {
        // The operand observes an error, but only the message names the changed reader.
        let read = probe(ProbeFamily::ErrorPath, "let n = rdr.read(&mut buf)?;");
        let text = r#"assert_eq!(unrelated.unwrap_err().kind(), Other, "rdr read buf");"#;
        let test = test_with_assertions(
            "checks_another_error",
            vec![oracle(text, OracleKind::ExactValue, OracleStrength::Strong)],
        );
        let (_, discriminate, _) =
            reveal_evidence(&read, &[(&test, RelationReason::DirectOwnerCall)]);
        if discriminate.state != StageState::Weak {
            return Err(format!(
                "diagnostic tokens pinned changed reader: {discriminate:?}"
            ));
        }
        let variant = probe(ProbeFamily::ErrorPath, "return Err(ReadError::Closed);");
        let text = r#"assert_eq!(rdr, Err(ReadError::Busy), "Closed");"#;
        let test = test_with_assertions(
            "checks_sibling_variant",
            vec![oracle(
                text,
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&variant, &[(&test, RelationReason::DirectOwnerCall)]);
        if discriminate.state == StageState::Yes {
            return Err(format!(
                "diagnostic pinned sibling variant: {discriminate:?}"
            ));
        }
        Ok(())
    }

    /// A deleted call on a binding the owner introduces (`let table = ..;
    /// table.validate()?;`, regex `dfa.accels.validate()?`) is not observed by
    /// a test's same-named local. A parameter receiver (`cache.insert(..)` on
    /// `cache: &mut Cache`) and the target's method token still confirm.
    #[test]
    fn effect_target_rooted_at_an_owner_local_needs_more_than_the_local_name() -> Result<(), String>
    {
        let owner_locals = vec!["table".to_string()];
        let reveal = |probe: &Probe, test: &TestSummary| {
            reveal_evidence_with_expression(
                probe,
                &probe.expression,
                &[(test, RelationReason::DirectOwnerCall)],
                &owner_locals,
                &|_, _| false,
                &|_, _| false,
                &ReturnOracleAdmission {
                    owner_return_pin: &|_, _| false,
                    assertion_admitted: &|_, _| true,
                    sibling_exclusion_admitted: &|_, _| true,
                    proximity_may_reach_owner: &|_| false,
                    owner_parameters: &[],
                    expected_reaches_owner: &|_, _| false,
                    effect_state_carried: &|_, _| true,
                    setter_writes_field: &|_, _, _| true,
                    getter_reads_field: &|_, _, _| false,
                    getter_reads_key: &|_, _, _| None,
                    owner_argument_names: &[],
                    owner_returns_value: true,
                    constructor_starts_empty: &|_, _, _| false,
                    call_cannot_store: &|_, _, _| false,
                },
                None,
            )
            .1
        };
        let deletion = probe(ProbeFamily::CallDeletion, "table.validate()?;");
        let same_name = test_with_assertions(
            "keeps_rows",
            vec![oracle(
                "assert_eq!(table.total(), 3);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let discriminate = reveal(&deletion, &same_name);
        if discriminate.state != StageState::Weak {
            return Err(format!(
                "owner-local name confirmed a deleted call: {discriminate:?}"
            ));
        }
        let names_method = test_with_assertions(
            "validates",
            vec![oracle(
                "assert_eq!(table.validate(), Ok(()));",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        if reveal(&deletion, &names_method).state != StageState::Yes {
            return Err("the target's method token must still confirm".to_string());
        }
        let parameter = probe(ProbeFamily::CallDeletion, "cache.insert(\"k\", value);");
        let parameter_observer = test_with_assertions(
            "stores",
            vec![oracle(
                "assert_eq!(cache.inserted, vec![1]);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        if reveal(&parameter, &parameter_observer).state != StageState::Yes {
            return Err("a receiver that is not an owner local must still confirm".to_string());
        }
        Ok(())
    }

    /// #6773 review: the clone-field missing-field cleanup reads reveal's
    /// credited owner-pin outcome. A name-only test that holds
    /// `assert_eq!(w.clone(), w)` next to a reach-bearing test supplies no
    /// oracle, so the pin is not credited; neither is it through a foreign
    /// same-name import or a cross-package same-name definition. A
    /// reach-bearing test holding the same assertion credits it.
    #[test]
    fn a_clone_field_owner_pin_is_credited_only_through_reveals_gates() -> Result<(), String> {
        let mut probe = probe(ProbeFamily::FieldConstruction, "start: self.start,");
        probe.owner = Some(SymbolId(
            "src/lib.rs::impl Clone for Window::clone".to_string(),
        ));
        let pinned_text = "assert_eq!(w.clone(), w);";
        let pinned = test_with_assertions(
            "clone_round_trip",
            vec![oracle(
                pinned_text,
                OracleKind::WholeObjectEquality,
                OracleStrength::Strong,
            )],
        );
        let reach = test_with_assertions(
            "builds_a_window",
            vec![oracle(
                "assert!(w.is_open());",
                OracleKind::RelationalCheck,
                OracleStrength::Weak,
            )],
        );
        let pin = |_: &TestSummary, assertion: &OracleFact| assertion.text == pinned_text;
        let credited = |related: &[(&TestSummary, RelationReason)],
                        import: &dyn Fn(&TestSummary, &str) -> bool,
                        package: &dyn Fn(&TestSummary, &str) -> bool| {
            reveal_outcome(
                &probe,
                &probe.expression,
                related,
                &[],
                import,
                package,
                &ReturnOracleAdmission {
                    owner_return_pin: &pin,
                    assertion_admitted: &|_, _| true,
                    sibling_exclusion_admitted: &|_, _| true,
                    proximity_may_reach_owner: &|_| false,
                    owner_parameters: &[],
                    expected_reaches_owner: &|_, _| false,
                    effect_state_carried: &|_, _| true,
                    setter_writes_field: &|_, _, _| true,
                    getter_reads_field: &|_, _, _| false,
                    getter_reads_key: &|_, _, _| None,
                    owner_argument_names: &[],
                    owner_returns_value: true,
                    constructor_starts_empty: &|_, _, _| false,
                    call_cannot_store: &|_, _, _| false,
                },
                None,
            )
            .owner_pin_credited
        };
        let never = |_: &TestSummary, _: &str| false;
        let always = |_: &TestSummary, _: &str| true;
        if !credited(
            &[(&pinned, RelationReason::DirectOwnerCall)],
            &never,
            &never,
        ) {
            return Err("a reach-bearing test's owner pin must be credited".to_string());
        }
        for name_only in [
            RelationReason::WeakTokenSubstring,
            RelationReason::OwnerNamedTest,
        ] {
            if credited(
                &[
                    (&pinned, name_only),
                    (&reach, RelationReason::DirectOwnerCall),
                ],
                &never,
                &never,
            ) {
                return Err(format!(
                    "{name_only:?} test credited the owner pin next to a reach-bearing test"
                ));
            }
        }
        if credited(
            &[(&pinned, RelationReason::DirectOwnerCall)],
            &always,
            &never,
        ) || credited(
            &[(&pinned, RelationReason::DirectOwnerCall)],
            &never,
            &always,
        ) {
            return Err("a same-name import or package definition must defeat the pin".to_string());
        }
        Ok(())
    }

    #[test]
    fn name_only_test_cannot_supply_the_oracle_for_reach_from_another_test() -> Result<(), String> {
        let probe = probe(ProbeFamily::ReturnValue, "compute_score(input)");
        // Token-confirmed and strong on its own: #4404's single-assertion
        // binding does not stop it, only its relation can.
        let confirmed_exact = oracle(
            "assert_eq!(compute_score, 42);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let weak = oracle(
            "assert!(compute_score(input).is_ok());",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        );
        let proximity_test = test_with_assertions("compute_score_named", vec![confirmed_exact]);
        let owner_test = test_with_assertions("calls_owner", vec![weak]);
        for name_only in [
            RelationReason::WeakTokenSubstring,
            RelationReason::OwnerNamedTest,
        ] {
            for related in [
                vec![
                    (&proximity_test, name_only),
                    (&owner_test, RelationReason::DirectOwnerCall),
                ],
                vec![
                    (&owner_test, RelationReason::DirectOwnerCall),
                    (&proximity_test, name_only),
                ],
            ] {
                let (_, discriminate, _) = reveal_evidence(&probe, &related);
                if discriminate.state == StageState::Yes {
                    return Err(format!(
                        "{name_only:?} test supplied the oracle for reach from another test: {discriminate:?}"
                    ));
                }
            }
        }
        // A same-file or same-module test commonly reaches a private helper
        // through the module's entry point, so it keeps crediting.
        for proximity in [RelationReason::SameTestFile, RelationReason::SameModule] {
            let (_, discriminate, _) = reveal_evidence(
                &probe,
                &[
                    (&proximity_test, proximity),
                    (&owner_test, RelationReason::DirectOwnerCall),
                ],
            );
            if discriminate.state != StageState::Yes {
                return Err(format!(
                    "{proximity:?} test lost its oracle credit: {discriminate:?}"
                ));
            }
        }
        // With no reach-bearing relation, reach itself stays weak or absent
        // (reach.rs), so the proximity oracle keeps its old reading here.
        // An assertionless same-file or same-module neighbour supplies no
        // reach either, so it must not switch the rule on.
        let bystander = test_with_assertions("same_file_bystander", Vec::new());
        for related in [
            vec![(&proximity_test, RelationReason::WeakTokenSubstring)],
            vec![
                (&bystander, RelationReason::SameTestFile),
                (&proximity_test, RelationReason::WeakTokenSubstring),
            ],
            vec![
                (&bystander, RelationReason::SameModule),
                (&proximity_test, RelationReason::OwnerNamedTest),
            ],
        ] {
            let (_, discriminate, _) = reveal_evidence(&probe, &related);
            if discriminate.state != StageState::Yes {
                return Err(format!(
                    "relation with no reach-bearing test lost its oracle reading: {discriminate:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn equally_strong_confirmed_oracle_preserves_discrimination_in_either_order()
    -> Result<(), String> {
        let probe = probe(ProbeFamily::ReturnValue, "compute_score(input)");
        let unrelated = oracle(
            "assert_eq!(unrelated, 42);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let aligned = oracle(
            "assert_eq!(compute_score(input), 42);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        for assertions in [
            vec![unrelated.clone(), aligned.clone()],
            vec![aligned, unrelated],
        ] {
            let test = test_with_assertions("exact_owner_observer", assertions);
            let (_, discriminate, _) =
                reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
            if discriminate.state != StageState::Yes {
                return Err(format!(
                    "confirmed exact oracle lost discrimination: {discriminate:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn reveal_evidence_keeps_assertionless_related_test_without_observe_signal() {
        let probe = probe(ProbeFamily::ReturnValue, "score");
        let test = test_with_assertions("score_returns_value", Vec::new());
        let (observe, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::No);
        assert_eq!(discriminate.state, StageState::No);
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].name, "score_returns_value");
        assert_eq!(related[0].oracle, None);
    }

    #[test]
    fn reveal_evidence_records_matching_assertions_and_sorts_related_tests() {
        let probe = probe(
            ProbeFamily::ErrorPath,
            "return Err(AuthError::RevokedToken);",
        );
        let late = test_with_assertions(
            "z_error_path",
            vec![oracle(
                "assert!(score(\"\").is_err());",
                OracleKind::BroadError,
                OracleStrength::Weak,
            )],
        );
        let early = test_with_assertions(
            "a_error_path",
            vec![oracle(
                "assert_matches!(score(\"\"), Err(AuthError::RevokedToken));",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (observe, discriminate, related) = reveal_evidence(
            &probe,
            &[
                (&late, RelationReason::DirectOwnerCall),
                (&early, RelationReason::DirectOwnerCall),
            ],
        );

        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(discriminate.state, StageState::Yes);
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].name, "a_error_path");
        assert_eq!(related[0].oracle_strength, OracleStrength::Strong);
        assert_eq!(related[1].name, "z_error_path");
    }

    /// #4760: per-assertion rows stay under the render cap. When one test has
    /// more matching oracles than the cap, unique tests still occupy a slot.
    #[test]
    fn related_tests_cap_keeps_a_second_test_when_one_test_has_many_oracles() {
        let probe = probe(ProbeFamily::ReturnValue, "(0, self.iter.size_hint().1)");
        let multi = test_with_assertions(
            "combinations_inexact_size_hints",
            (0..9)
                .map(|index| {
                    oracle(
                        &format!("assert_eq!(it.size_hint().1, Some({index}));"),
                        OracleKind::ExactValue,
                        OracleStrength::Strong,
                    )
                })
                .collect(),
        );
        let other = test_with_assertions(
            "while_some_is_untested",
            vec![oracle(
                "assert_eq!(1, 1);",
                OracleKind::Unknown,
                OracleStrength::Unknown,
            )],
        );
        let (_, _, related) = reveal_evidence(
            &probe,
            &[
                (&multi, RelationReason::DirectOwnerCall),
                (&other, RelationReason::SameTestFile),
            ],
        );

        let named: Vec<&str> = related.iter().map(|test| test.name.as_str()).collect();
        assert_eq!(
            related.len(),
            RELATED_TESTS_RENDER_CAP,
            "packed to the render cap: {named:?}"
        );
        assert!(
            named.contains(&"while_some_is_untested"),
            "a second test must survive the 8-row cap: {named:?}"
        );
    }

    #[test]
    fn reveal_evidence_lists_a_test_whose_assertions_do_not_match_as_a_miss() {
        let probe = probe(ProbeFamily::StaticUnknown, "opaque_changed_expr");
        let test = test_with_assertions(
            "opaque_behavior",
            vec![
                oracle(
                    "assert_eq!(unrelated, 3);",
                    OracleKind::Unknown,
                    OracleStrength::Unknown,
                ),
                oracle(
                    "assert!(other_value);",
                    OracleKind::Unknown,
                    OracleStrength::Unknown,
                ),
            ],
        );
        let (observe, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::WeakTokenSubstring)]);

        // Unmatched assertions still supply no oracle (#5344): the stages
        // stay `no`. The examined test is listed, marked as a miss, with its
        // first assertion as the checked text and no oracle strength.
        assert_eq!(observe.state, StageState::No);
        assert_eq!(discriminate.state, StageState::No);
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].name, "opaque_behavior");
        assert_eq!(
            related[0].miss,
            Some(RelatedTestMiss::AssertionNotObserving)
        );
        assert!(related[0].is_unmatched());
        assert_eq!(
            related[0].oracle.as_deref(),
            Some("assert_eq!(unrelated, 3);")
        );
        assert_eq!(related[0].oracle_strength, OracleStrength::None);
        assert_eq!(related[0].oracle_kind, OracleKind::Unknown);
    }

    /// #5344 review: examined misses must not enter the unique-test packing.
    /// One test with two matched rows (weak, then strong) plus seven examined
    /// misses: both oracle rows survive, the misses fill the slots left.
    #[test]
    fn examined_misses_never_push_a_second_oracle_row_out_of_the_window() {
        let probe = probe(ProbeFamily::StaticUnknown, "score");
        let exact = test_with_assertions(
            "check_exact",
            vec![
                oracle(
                    "assert!(score > 0);",
                    OracleKind::Unknown,
                    OracleStrength::Weak,
                ),
                oracle(
                    "assert_eq!(score, 3);",
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                ),
            ],
        );
        let misses: Vec<_> = (0..7)
            .map(|idx| {
                test_with_assertions(
                    &format!("aaa_miss_{idx}"),
                    vec![
                        oracle(
                            "assert!(other);",
                            OracleKind::Unknown,
                            OracleStrength::Unknown,
                        ),
                        oracle(
                            "assert!(more);",
                            OracleKind::Unknown,
                            OracleStrength::Unknown,
                        ),
                    ],
                )
            })
            .collect();
        let mut related_input = vec![(&exact, RelationReason::DirectOwnerCall)];
        related_input.extend(misses.iter().map(|test| (test, RelationReason::SameModule)));
        let (_, _, related, total) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &related_input,
            &[],
            &|_, _| false,
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(total, 9);
        assert_eq!(related.len(), 8);
        let exact_rows: Vec<_> = related
            .iter()
            .filter(|test| test.name == "check_exact")
            .filter_map(|test| test.oracle.as_deref())
            .collect();
        assert_eq!(
            exact_rows,
            vec!["assert!(score > 0);", "assert_eq!(score, 3);"],
            "{related:?}"
        );
        assert_eq!(related.iter().filter(|test| test.is_unmatched()).count(), 6);
    }

    #[test]
    fn a_matched_test_ranks_ahead_of_an_examined_miss_and_keeps_its_window_slot() {
        // Eight matched tests fill the render window; a ninth test whose
        // assertions do not match must not displace any of them.
        let probe = probe(ProbeFamily::StaticUnknown, "score");
        let matched: Vec<_> = (0..8)
            .map(|idx| {
                test_with_assertions(
                    &format!("matched_{idx}"),
                    vec![oracle(
                        "assert_eq!(score, 3);",
                        OracleKind::ExactValue,
                        OracleStrength::Strong,
                    )],
                )
            })
            .collect();
        let miss = test_with_assertions(
            "aaa_examined_miss",
            vec![
                oracle(
                    "assert!(other);",
                    OracleKind::Unknown,
                    OracleStrength::Unknown,
                ),
                oracle(
                    "assert!(more);",
                    OracleKind::Unknown,
                    OracleStrength::Unknown,
                ),
            ],
        );
        let mut related_input: Vec<_> = matched
            .iter()
            .map(|test| (test, RelationReason::DirectOwnerCall))
            .collect();
        related_input.insert(0, (&miss, RelationReason::DirectOwnerCall));
        let (_, _, related, total) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &related_input,
            &[],
            &|_, _| false,
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(total, 9, "every examined test is counted");
        assert_eq!(related.len(), 8);
        assert!(
            related.iter().all(|test| !test.is_unmatched()),
            "the miss is outside the window: {related:?}"
        );
    }

    #[test]
    fn assertion_matching_accepts_token_family_and_single_assertion_fallbacks() {
        let token_assertion = oracle(
            "assert_eq!(score, 3);",
            OracleKind::Unknown,
            OracleStrength::Unknown,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &["score".to_string()],
            &[],
            &[],
            None,
            &ProbeFamily::StaticUnknown,
            &token_assertion,
            2,
            None,
        );
        assert!(matched, "token match must fire");
        assert!(has_token, "token match must set has_token_match");

        let family_assertion = oracle(
            "assert!(result.is_err());",
            OracleKind::BroadError,
            OracleStrength::Weak,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &["err".to_string()],
            &[],
            &[],
            None,
            &ProbeFamily::ErrorPath,
            &family_assertion,
            2,
            None,
        );
        assert!(matched, "family match must fire");
        assert!(!has_token, "family-only match must not set has_token_match");

        let fallback_assertion = oracle(
            "assert!(ran);",
            OracleKind::Unknown,
            OracleStrength::Unknown,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &["run".to_string()],
            &[],
            &[],
            None,
            &ProbeFamily::StaticUnknown,
            &fallback_assertion,
            1,
            None,
        );
        assert!(matched, "single-assertion fallback must fire");
        assert!(
            !has_token,
            "escape-hatch-only match must not set has_token_match"
        );

        let (matched, _) = assertion_matches_probe_detail(
            &["run".to_string()],
            &[],
            &[],
            None,
            &ProbeFamily::StaticUnknown,
            &fallback_assertion,
            2,
            None,
        );
        assert!(!matched, "fallback must not fire for assertion_count > 1");
    }

    // Literal-arm identity (#1714): an assertion supplying the arm's string
    // literal confirms a MatchArm probe with no `::` variant tokens.
    #[test]
    fn literal_assertion_confirms_literal_match_arm() {
        let aligned = oracle(
            "assert_eq!(route(\"sensor\"), \"sensor-v2\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &[],
            &[],
            &["sensor".to_string()],
            None,
            &ProbeFamily::MatchArm,
            &aligned,
            2,
            Some("route"),
        );
        assert!(matched, "literal overlap must associate");
        assert!(has_token, "owner-supplied pattern literal must confirm");
    }

    // Sibling rejection (#1714): a sibling arm's literals never overlap this
    // arm's expression, so the sibling oracle leaves it unverified.
    #[test]
    fn sibling_literal_assertion_does_not_confirm_changed_arm() {
        let sibling = oracle(
            "assert_eq!(route(\"focused-test\"), \"proof\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &[],
            &[],
            &["sensor".to_string()],
            None,
            &ProbeFamily::MatchArm,
            &sibling,
            2,
            Some("route"),
        );
        assert!(
            !has_token,
            "sibling literals must not confirm the changed arm"
        );
        let _ = matched;
    }

    // Shared result (#1714): a sibling returning the same result literal does
    // not select the changed arm, even though the literal overlaps.
    #[test]
    fn shared_result_literal_does_not_confirm_sibling_arm() {
        let shared = oracle(
            "assert_eq!(route(\"focused-test\"), \"sensor-v2\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let (_, has_token) = assertion_matches_probe_detail(
            &[],
            &[],
            &["sensor".to_string()],
            None,
            &ProbeFamily::MatchArm,
            &shared,
            2,
            Some("route"),
        );
        assert!(
            !has_token,
            "result-side overlap must not confirm the changed arm"
        );
    }

    // Only a direct bare owner call in one of the two compared operands
    // supplies an observable literal input. Every other shape fails closed.
    #[test]
    fn owner_call_literal_scope_rejects_longer_callee_names() {
        for assertion in [
            "assert_eq!(reroute(\"sensor\"), 1);",
            "assert_eq!(other::route(\"sensor\"), 1);",
            "assert_eq!(other :: route(\"sensor\"), 1);",
            "assert_eq!(router.route(\"sensor\"), 1);",
            "assert_eq!(router . route(\"sensor\"), 1);",
            "assert_eq!(route(wrap(\"sensor\")), 1);",
            "assert_eq!(route(if false { \"sensor\" } else { \"focused-test\" }), 1);",
        ] {
            assert!(
                owner_call_literals(assertion, "route").is_empty(),
                "unsupported owner-call shape unexpectedly supplied an input: {assertion}"
            );
        }
        assert_eq!(
            owner_call_literals(
                "assert_eq!(route(\"focused-test\"), \"proof\", \"{}\", route(\"sensor\"));",
                "route"
            ),
            vec!["focused-test".to_string()]
        );
        assert_eq!(
            owner_call_literals("assert_eq!(route(\"sensor\"), \"v\");", "route"),
            vec!["sensor".to_string()]
        );
        assert_eq!(
            owner_call_literals("assert_eq!(\"v\", route(\"sensor\"));", "route"),
            vec!["sensor".to_string()]
        );
    }

    #[test]
    fn match_arm_guard_detection_masks_literals_and_comments() {
        assert!(match_arm_pattern_has_guard(
            "\"sensor\" if kind.len() > 10 => \"sensor-v2\""
        ));
        assert!(!match_arm_pattern_has_guard("\"if\" => \"literal\""));
        assert!(!match_arm_pattern_has_guard(
            "/* if */ \"sensor\" => \"sensor-v2\""
        ));
        assert!(!match_arm_pattern_has_guard("Mode::If => 1"));
    }

    #[test]
    fn match_arm_pattern_literals_ignore_comment_text() {
        assert_eq!(
            match_arm_pattern_literals("\"sensor\" /* \"focused-test\" */ => \"sensor-v2\""),
            vec!["sensor".to_string()]
        );
        assert_eq!(
            match_arm_pattern_literals(
                "\"sensor\" /* outer /* r#\"focused-test\"# */ tail */ => \"sensor-v2\""
            ),
            vec!["sensor".to_string()]
        );
        assert_eq!(
            match_arm_pattern_literals("\"sensor\" // \"focused-test\"\n => \"sensor-v2\""),
            vec!["sensor".to_string()]
        );
        assert_eq!(
            match_arm_pattern_literals("/* \"focused-test\" */ \"sensor\" => \"sensor-v2\""),
            vec!["sensor".to_string()]
        );
        assert_eq!(
            match_arm_pattern_literals("// \"focused-test\"\n\"sensor\" => \"sensor-v2\""),
            vec!["sensor".to_string()]
        );
        assert_eq!(
            match_arm_pattern_literals("\"/*\" => \"comment-like\""),
            vec!["/*".to_string()]
        );
        assert_eq!(
            match_arm_pattern_literals("r#\"//\"# => \"comment-like\""),
            vec!["//".to_string()]
        );
        // An unterminated comment swallows the separator, so no arm
        // identity is established at all (fail closed, not a partial
        // literal).
        assert_eq!(
            match_arm_pattern_literals("\"sensor\" /* unterminated => \"sensor-v2\""),
            Vec::<String>::new()
        );
    }

    #[test]
    fn match_arm_literal_confirmation_respects_owner_ambiguity_and_guards() {
        let aligned = oracle(
            "assert_eq!(route(\"sensor\"), \"sensor-v2\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let empty = Vec::<String>::new();
        let pattern_literals = vec!["sensor".to_string()];
        let family = ProbeFamily::MatchArm;

        for (guarded, import_defeats_owner, cross_package_defeats_owner, expected) in [
            (false, false, false, true),
            (true, false, false, false),
            (false, true, false, false),
            (false, false, true, false),
            (false, true, true, false),
        ] {
            let context = RevealMatchContext {
                probe_tokens: &empty,
                effect_literals: &empty,
                match_arm_variants: &empty,
                match_arm_literals: &pattern_literals,
                match_arm_guarded: guarded,
                error_construction_variant: None,
                error_construction_qualifier: None,
                family: &family,
                wrapper_seam: false,
                owner_callee: Some("route"),
                arm_selector: None,
                arm_inputs_readable: false,
                arm_pattern_pinned: false,
                owner_scoped_tokens: &empty,
                owner_shadowed: false,
                sibling_excluded: false,
            };
            let (_, has_token) = assertion_matches_probe_detail_with_literals(
                &context,
                &aligned,
                2,
                import_defeats_owner,
                cross_package_defeats_owner,
                false,
                false,
            );
            assert_eq!(
                has_token, expected,
                "wrong confirmation: guarded={guarded} import={import_defeats_owner} cross_package={cross_package_defeats_owner}"
            );
        }
    }

    // A diagnostic message that spells the owner call binds no input: the
    // callee occurrence inside the message string is not a call.
    #[test]
    fn owner_call_literal_scope_rejects_call_text_inside_message() {
        assert!(
            owner_call_literals(
                "assert!(value == 1, \"route(\\\"sensor\\\") should hold\");",
                "route"
            )
            .is_empty()
        );
    }

    // Variable-bound inputs are not statically visible: `route(input)` with
    // no literal argument supplies nothing, so confirmation fails closed.
    #[test]
    fn owner_call_literal_scope_rejects_variable_bound_inputs() {
        assert!(
            owner_call_literals("assert_eq!(route(input), \"sensor-v2\");", "route").is_empty()
        );
    }

    // Diagnostic text (#1714): a pattern literal appearing only as assertion
    // message text is not an owner-supplied input.
    #[test]
    fn diagnostic_literal_does_not_confirm_changed_arm() {
        let diagnostic = oracle(
            "assert_eq!(route(\"focused-test\"), \"proof\", \"sensor\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        );
        let (_, has_token) = assertion_matches_probe_detail(
            &[],
            &[],
            &["sensor".to_string()],
            None,
            &ProbeFamily::MatchArm,
            &diagnostic,
            2,
            Some("route"),
        );
        assert!(
            !has_token,
            "message-only literal must not confirm the changed arm"
        );
    }

    // RIPR-SPEC-0106 Control 2 (SIBLING-VARIANT): a Negative-pinning assertion
    // must not match a TooLarge error_path probe.
    #[test]
    fn sibling_variant_assertion_does_not_match_too_large_probe() {
        let sibling_assertion = oracle(
            "assert_eq!(err, CalcError::Negative);",
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
        );
        // Probe expression names TooLarge; error_path_variant = Some("TooLarge").
        let (matched, has_token) = assertion_matches_probe_detail(
            &["CalcError".to_string(), "TooLarge".to_string()],
            &[],
            &[],
            Some("TooLarge"),
            &ProbeFamily::ErrorPath,
            &sibling_assertion,
            2,
            None,
        );
        assert!(
            !matched,
            "sibling-variant Negative assertion must not match TooLarge probe"
        );
        assert!(
            !has_token,
            "sibling-variant assertion must not set has_token_match for TooLarge probe"
        );
    }

    // #3700 (final consolidation): a wrapper error seam never confirms
    // observation from lexical matching — not from a callee-side exact Err
    // pin, not from a wrapper-invoking downcast pin, not from message text —
    // so the seam stays `weakly_exposed` (unverified observation) and the
    // typed `wrapper_error_binding_unresolved` limitation (attached by
    // `apply_wrapper_error_binding_limit`) is the honest outcome instead of
    // an escalating lexical binding heuristic.
    #[test]
    fn wrapper_seam_never_confirms_and_stays_weak_under_strongest_witnesses() {
        let wrapper_probe = probe(
            ProbeFamily::ErrorPath,
            "try_parse_summary(raw).map_err(Into::into)",
        );
        let binder = test_with_body_assertions(
            "try_parse_summary_pins_malformed_source",
            "let result = try_parse_summary(\"@bad;\");",
            vec![oracle(
                "if !matches!(result, Err(ParseSummaryError::MalformedSource)) {
return Err(\"callee pin\".into());
}",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let observer = test_with_body_assertions(
            "parse_summary_boxed_variant_propagates_malformed_source",
            "let error = parse_summary(\"@bad;\").err().ok_or(\"expected error\")?;",
            vec![oracle(
                "if !matches!(error.downcast_ref::<ParseSummaryError>(), Some(ParseSummaryError::MalformedSource)) {
return Err(\"boxed identity should survive\".into());
}",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (observe, discriminate, _) = reveal_evidence(
            &wrapper_probe,
            &[
                (&binder, RelationReason::OwnerNamedTest),
                (&observer, RelationReason::DirectOwnerCall),
            ],
        );
        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "a wrapper seam without a parseable variant must never read exposed from lexical heuristics"
        );
    }

    // #3700 removal-fails control for the typed path: the parseable-variant
    // site (`return Err(ParseSummaryError::MalformedSource);`) credits only
    // while the witness pins the exact variant against the callee's own
    // result. Removing the variant pin (broad is_err) drops the seam to
    // weakly_exposed — the pre-existing main behavior, no wrapper heuristics
    // involved.
    #[test]
    fn typed_variant_site_loses_credit_when_witness_pin_removed() {
        let typed_probe = probe(
            ProbeFamily::ErrorPath,
            "return Err(ParseSummaryError::MalformedSource);",
        );
        let exact_witness = test_with_body_assertions(
            "try_parse_summary_fails_closed_on_malformed_source",
            "let result = try_parse_summary(\"@bad;\");",
            vec![oracle(
                "if !matches!(result, Err(ParseSummaryError::MalformedSource)) {
return Err(\"typed pin\".into());
}",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (observe, discriminate, _) = reveal_evidence(
            &typed_probe,
            &[(&exact_witness, RelationReason::OwnerNamedTest)],
        );
        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "the parseable-variant path keeps the pre-existing variant-bound credit"
        );

        let broad_witness = test_with_body_assertions(
            "try_parse_summary_fails_closed_on_malformed_source",
            "let result = try_parse_summary(\"@bad;\");",
            vec![oracle(
                "assert!(result.is_err());",
                OracleKind::BroadError,
                OracleStrength::Weak,
            )],
        );
        let (_, degraded, _) = reveal_evidence(
            &typed_probe,
            &[(&broad_witness, RelationReason::OwnerNamedTest)],
        );
        assert_eq!(
            degraded.state,
            StageState::Weak,
            "removing the variant pin must fail the typed path's credit"
        );
    }

    // RIPR-SPEC-0106 Control 1 (POSITIVE): exact variant assertion DOES match
    // the probe when the specific variant token is present.
    #[test]
    fn exact_variant_assertion_matches_matching_probe() {
        let exact_assertion = oracle(
            "assert_eq!(err, CalcError::Negative);",
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &["CalcError".to_string(), "Negative".to_string()],
            &[],
            &[],
            Some("Negative"),
            &ProbeFamily::ErrorPath,
            &exact_assertion,
            2,
            None,
        );
        assert!(
            matched,
            "exact variant assertion must match the probe when variant token matches"
        );
        assert!(
            has_token,
            "matching variant assertion must set has_token_match"
        );
    }

    // #3700 callee-extraction pins: the converted callee is the final
    // top-level call segment before `.map_err(..)` — receiver qualification,
    // chaining, and turbofish must not misattribute it, and non-call shapes
    // fail closed to `None`.

    #[test]
    fn sibling_variant_assertion_does_not_confirm_return_value_error_construction() {
        let sibling_assertion = oracle(
            "assert_eq!(err, CalcError::Negative);",
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &[
                "return".to_string(),
                "Err".to_string(),
                "CalcError".to_string(),
                "TooLarge".to_string(),
            ],
            &[],
            &[],
            Some("TooLarge"),
            &ProbeFamily::ReturnValue,
            &sibling_assertion,
            2,
            None,
        );
        assert!(
            matched,
            "the sibling test still observes near the changed behavior and stays associated"
        );
        assert!(
            !has_token,
            "a Negative pin must not confirm observation of a TooLarge construction"
        );
    }

    #[test]
    fn aligned_variant_assertion_confirms_return_value_error_construction() {
        let aligned_assertion = oracle(
            "assert_eq!(result.unwrap_err(), CalcError::TooLarge);",
            OracleKind::ExactErrorVariant,
            OracleStrength::Strong,
        );
        let (matched, has_token) = assertion_matches_probe_detail(
            &[
                "return".to_string(),
                "Err".to_string(),
                "CalcError".to_string(),
                "TooLarge".to_string(),
            ],
            &[],
            &[],
            Some("TooLarge"),
            &ProbeFamily::ReturnValue,
            &aligned_assertion,
            2,
            None,
        );
        assert!(matched);
        assert!(
            has_token,
            "an oracle pinning the constructed variant confirms observation"
        );
    }

    // XQFf: the error-construction variant guard must also cover
    // `FieldConstruction` probes — a probe whose field is an `Err(...)`
    // construction (`outcome: Err(CalcError::TooLarge)`) has no separate
    // `error_construction_variant` source, so without the guard a
    // sibling-variant oracle (`CalcError::Negative`) confirmed observation
    // through the shared `CalcError` qualifier token.
    #[test]
    fn sibling_variant_assertion_does_not_confirm_field_construction_error_construction() {
        let probe = probe(
            ProbeFamily::FieldConstruction,
            "outcome: Err(CalcError::TooLarge)",
        );
        let test = test_with_assertions(
            "negative_outcome_asserted",
            vec![oracle(
                "assert_eq!(err, CalcError::Negative);",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (observe, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "the sibling Negative pin must not confirm the TooLarge field construction"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "sibling-variant oracle must leave field construction observation unverified: got `{}`",
            discriminate.summary
        );
        assert_eq!(related.len(), 1);
    }

    #[test]
    fn aligned_variant_assertion_confirms_field_construction_error_construction() {
        let probe = probe(
            ProbeFamily::FieldConstruction,
            "outcome: Err(CalcError::TooLarge)",
        );
        let test = test_with_assertions(
            "too_large_outcome_asserted",
            vec![oracle(
                "assert_eq!(err, CalcError::TooLarge);",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "an oracle pinning the constructed variant must keep the field seam exposed"
        );
    }

    // End-to-end: the sibling fixture shape (RIPR-SPEC-0106) keeps the
    // return_value finding's discrimination unconfirmed instead of crediting
    // the sibling oracle as a strong discriminator.
    #[test]
    fn sibling_variant_oracle_leaves_return_value_discrimination_unconfirmed() {
        let probe = probe(ProbeFamily::ReturnValue, "return Err(CalcError::TooLarge);");
        let test = test_with_assertions(
            "negative_input_rejects_with_negative_error",
            vec![oracle(
                "assert_eq!(err, CalcError::Negative);",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (observe, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(discriminate.state, StageState::Weak);
        assert!(
            discriminate
                .summary
                .contains("Discriminator unconfirmed: no assertion text references this probe's changed expression (observation_unverified)"),
            "sibling oracle must not credit strong discrimination; got: {}",
            discriminate.summary
        );
        assert_eq!(related.len(), 1);
    }

    #[test]
    fn discriminate_evidence_names_strength_and_oracle_kind() {
        let cases = [
            (
                OracleStrength::Strong,
                OracleKind::ExactErrorVariant,
                ProbeFamily::ErrorPath,
                StageState::Yes,
                "Strong oracle found: exact error variant assertion",
            ),
            (
                OracleStrength::Strong,
                OracleKind::WholeObjectEquality,
                ProbeFamily::ReturnValue,
                StageState::Yes,
                "Strong oracle found: whole-object equality assertion",
            ),
            (
                OracleStrength::Strong,
                OracleKind::ExactValue,
                ProbeFamily::ReturnValue,
                StageState::Yes,
                "Strong oracle found: exact value or pattern assertion",
            ),
            (
                OracleStrength::Medium,
                OracleKind::Snapshot,
                ProbeFamily::ReturnValue,
                StageState::Weak,
                "Medium oracle found: snapshot assertion observes the changed behavior",
            ),
            (
                OracleStrength::Medium,
                OracleKind::MockExpectation,
                ProbeFamily::SideEffect,
                StageState::Weak,
                "Medium oracle found: mock or expectation observes the changed behavior",
            ),
            (
                OracleStrength::Medium,
                OracleKind::ExactValue,
                ProbeFamily::ReturnValue,
                StageState::Weak,
                "Medium oracle found: property or partial structural assertion",
            ),
            (
                OracleStrength::Weak,
                OracleKind::BroadError,
                ProbeFamily::ErrorPath,
                StageState::Weak,
                "Only broad error oracle found; is_err() does not discriminate exact error variants",
            ),
            (
                OracleStrength::Weak,
                OracleKind::BroadError,
                ProbeFamily::ReturnValue,
                StageState::Weak,
                "Only broad error oracle found; it may not discriminate the changed behavior exactly",
            ),
            (
                OracleStrength::Weak,
                OracleKind::RelationalCheck,
                ProbeFamily::Predicate,
                StageState::Weak,
                "Only relational oracle found; it may not discriminate the changed value exactly",
            ),
            (
                OracleStrength::Weak,
                OracleKind::ExactValue,
                ProbeFamily::ReturnValue,
                StageState::Weak,
                "Only weak oracle found, such as a broad relational assertion or non-empty check",
            ),
            (
                OracleStrength::Smoke,
                OracleKind::SmokeOnly,
                ProbeFamily::ReturnValue,
                StageState::Weak,
                "Only smoke oracle found, such as unwrap/expect or execution without a discriminator",
            ),
            (
                OracleStrength::None,
                OracleKind::Unknown,
                ProbeFamily::ReturnValue,
                StageState::No,
                "No assertion found on related tests",
            ),
            (
                OracleStrength::Unknown,
                OracleKind::Unknown,
                ProbeFamily::ReturnValue,
                StageState::Unknown,
                "Assertions exist, but oracle strength is unknown",
            ),
        ];

        for (strength, kind, family, state, summary) in cases {
            let evidence = build_discriminate_evidence(&strength, &kind, &family, false);
            assert_eq!(evidence.state, state);
            assert_eq!(evidence.summary, summary);
        }
    }

    #[test]
    fn oracle_family_matching_covers_family_specific_shapes() {
        assert!(oracle_matches_family(
            &ProbeFamily::ErrorPath,
            &oracle(
                "assert_matches!(result, Err(AuthError::RevokedToken));",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::ErrorPath,
            &oracle(
                "assert!(result.is_err());",
                OracleKind::BroadError,
                OracleStrength::Weak
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::ErrorPath,
            &oracle(
                "assert!(matches!(result, Err(_)));",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::ErrorPath,
            &oracle(
                "assert_eq!(kind, Error::Denied);",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::SideEffect,
            &oracle(
                "mock.expect_send();",
                OracleKind::MockExpectation,
                OracleStrength::Medium
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::SideEffect,
            &oracle(
                "assert!(event.saved);",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::SideEffect,
            &oracle(
                "assert!(event.published);",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::FieldConstruction,
            &oracle(
                "assert_eq!(item.id, 3);",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(!oracle_matches_family(
            &ProbeFamily::FieldConstruction,
            &oracle(
                "assert!(3.14_f64 > 0.0_f64);",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        // #2904: a dot inside a string literal is not a field access.
        assert!(!oracle_matches_family(
            &ProbeFamily::FieldConstruction,
            &oracle(
                "assert_eq!(msg, \"error.timeout\");",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::FieldConstruction,
            &oracle(
                "assert_debug_snapshot!(item);",
                OracleKind::Snapshot,
                OracleStrength::Medium
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::Predicate,
            &oracle(
                "assert!(value >= 3);",
                OracleKind::RelationalCheck,
                OracleStrength::Weak
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::ReturnValue,
            &oracle(
                "assert_eq!(score(), 3);",
                OracleKind::ExactValue,
                OracleStrength::Strong
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::ReturnValue,
            &oracle(
                "score().unwrap();",
                OracleKind::SmokeOnly,
                OracleStrength::Smoke
            )
        ));
        assert!(!oracle_matches_family(
            &ProbeFamily::ReturnValue,
            &oracle(
                "assert_custom(score());",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::CallDeletion,
            &oracle(
                "assert!(sent);",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::CallDeletion,
            &oracle(
                "expect_send_called();",
                OracleKind::Unknown,
                OracleStrength::Unknown
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::CallDeletion,
            &oracle(
                "mock.expect_send();",
                OracleKind::MockExpectation,
                OracleStrength::Medium
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::MatchArm,
            &oracle(
                "assert_matches!(kind, Ready);",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong
            )
        ));
        assert!(oracle_matches_family(
            &ProbeFamily::MatchArm,
            &oracle(
                "assert_eq!(kind, Ready);",
                OracleKind::ExactValue,
                OracleStrength::Strong
            )
        ));
        assert!(!oracle_matches_family(
            &ProbeFamily::StaticUnknown,
            &oracle(
                "assert_eq!(value, 3);",
                OracleKind::ExactValue,
                OracleStrength::Strong
            )
        ));
    }

    #[test]
    fn pin_strength_gate_keeps_exact_pins_strong_and_defers_bool_pins_to_owner_pin() {
        // RIPR-SPEC-0231 decision 4: a weak exact oracle never pins.
        assert!(!pin_strength_admits(&oracle(
            "assert_ne!(f(), Foo { a: 1 })",
            OracleKind::WholeObjectEquality,
            OracleStrength::Weak,
        )));
        assert!(!pin_strength_admits(&oracle(
            "assert_eq!(f(), 1)",
            OracleKind::ExactValue,
            OracleStrength::Medium,
        )));
        assert!(pin_strength_admits(&oracle(
            "assert_eq!(f(), 1)",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )));
        // RIPR-SPEC-0197: the bool-owner `assert!` pin keeps its weak
        // relational kind; `owner_pin` decides whether it pins.
        assert!(pin_strength_admits(&oracle(
            "assert!(!gate(9))",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        )));
        // #6578: of the medium snapshots, only the inline expect-test
        // comparison the owner pin reads like `assert_eq!`.
        assert!(pin_strength_admits(&oracle(
            "expect![[\"w12\"]].assert_eq(&render(4));",
            OracleKind::Snapshot,
            OracleStrength::Medium,
        )));
        for text in [
            "insta::assert_snapshot!(render(4));",
            "expect![[\"w12\"]].assert_debug_eq(&render(4));",
            "expect_file![\"w.txt\"].assert_eq(&render(4));",
        ] {
            assert!(
                !pin_strength_admits(&oracle(text, OracleKind::Snapshot, OracleStrength::Medium)),
                "{text}"
            );
        }
    }

    #[test]
    fn probe_relative_oracle_strength_preserves_family_overrides() {
        let cases = [
            (
                ProbeFamily::ErrorPath,
                oracle("exact", OracleKind::ExactErrorVariant, OracleStrength::Weak),
                OracleStrength::Weak,
            ),
            (
                ProbeFamily::ErrorPath,
                oracle(
                    "exact",
                    OracleKind::ExactErrorVariant,
                    OracleStrength::Medium,
                ),
                OracleStrength::Medium,
            ),
            (
                ProbeFamily::ErrorPath,
                oracle("broad", OracleKind::BroadError, OracleStrength::Weak),
                OracleStrength::Weak,
            ),
            (
                ProbeFamily::ErrorPath,
                oracle("smoke", OracleKind::SmokeOnly, OracleStrength::Strong),
                OracleStrength::Smoke,
            ),
            (
                ProbeFamily::ErrorPath,
                oracle("snapshot", OracleKind::Snapshot, OracleStrength::Medium),
                OracleStrength::Medium,
            ),
            (
                ProbeFamily::ReturnValue,
                oracle("exact", OracleKind::ExactValue, OracleStrength::Weak),
                OracleStrength::Weak,
            ),
            (
                ProbeFamily::ReturnValue,
                oracle("exact", OracleKind::ExactValue, OracleStrength::Medium),
                OracleStrength::Medium,
            ),
            (
                ProbeFamily::Predicate,
                oracle("snapshot", OracleKind::Snapshot, OracleStrength::Medium),
                OracleStrength::Medium,
            ),
            (
                ProbeFamily::FieldConstruction,
                oracle("smoke", OracleKind::SmokeOnly, OracleStrength::Strong),
                OracleStrength::Smoke,
            ),
            (
                ProbeFamily::MatchArm,
                oracle("unknown", OracleKind::Unknown, OracleStrength::Strong),
                OracleStrength::Unknown,
            ),
            (
                ProbeFamily::SideEffect,
                oracle("mock", OracleKind::MockExpectation, OracleStrength::Medium),
                OracleStrength::Medium,
            ),
            (
                ProbeFamily::SideEffect,
                oracle(
                    "exact",
                    OracleKind::WholeObjectEquality,
                    OracleStrength::Weak,
                ),
                OracleStrength::Weak,
            ),
            (
                ProbeFamily::CallDeletion,
                oracle("rel", OracleKind::RelationalCheck, OracleStrength::Weak),
                OracleStrength::Weak,
            ),
            (
                ProbeFamily::CallDeletion,
                oracle("smoke", OracleKind::SmokeOnly, OracleStrength::Strong),
                OracleStrength::Smoke,
            ),
            (
                ProbeFamily::SideEffect,
                oracle(
                    "exact_error",
                    OracleKind::ExactErrorVariant,
                    OracleStrength::Strong,
                ),
                OracleStrength::Medium,
            ),
            (
                ProbeFamily::SideEffect,
                oracle("snapshot", OracleKind::Snapshot, OracleStrength::Weak),
                OracleStrength::Weak,
            ),
            (
                ProbeFamily::SideEffect,
                oracle("unknown", OracleKind::Unknown, OracleStrength::Strong),
                OracleStrength::Unknown,
            ),
            (
                ProbeFamily::StaticUnknown,
                oracle("exact", OracleKind::ExactValue, OracleStrength::Strong),
                OracleStrength::Unknown,
            ),
        ];

        for (family, assertion, expected) in cases {
            assert_eq!(
                probe_relative_oracle_strength(&family, &assertion),
                expected
            );
        }
    }

    /// RIPR-SPEC-0231 rule 1 (#6670 review): the classifier keeps
    /// `assert_ne!(build(), Config { retries: 9 })` at
    /// `whole_object_equality` / weak, and the family override that gives
    /// that kind strong credit for equality must not lift the inequality.
    #[test]
    fn probe_relative_oracle_strength_never_promotes_an_inequality() {
        let fact = |text: &str| {
            let classification = crate::analysis::rust_index::classify_assertion(text);
            oracle(text, classification.kind, classification.strength)
        };
        let inequality = fact("assert_ne!(build(), Config { retries: 9 });");
        let debug_inequality = fact("debug_assert_ne!(build(), Config { retries: 9 });");
        let equality = fact("assert_eq!(build(), Config { retries: 9 });");
        assert_eq!(inequality.kind, OracleKind::WholeObjectEquality);
        assert_eq!(inequality.strength, OracleStrength::Weak);
        assert_eq!(debug_inequality.kind, OracleKind::WholeObjectEquality);
        assert_eq!(debug_inequality.strength, OracleStrength::Weak);
        assert_eq!(equality.kind, OracleKind::WholeObjectEquality);
        assert_eq!(equality.strength, OracleStrength::Strong);
        for family in [
            ProbeFamily::ReturnValue,
            ProbeFamily::Predicate,
            ProbeFamily::FieldConstruction,
            ProbeFamily::SideEffect,
        ] {
            assert_eq!(
                probe_relative_oracle_strength(&family, &inequality),
                OracleStrength::Weak,
                "{family:?} lifted an inequality"
            );
            assert_eq!(
                probe_relative_oracle_strength(&family, &debug_inequality),
                OracleStrength::Weak,
                "{family:?} lifted a debug inequality"
            );
            assert_eq!(
                probe_relative_oracle_strength(&family, &equality),
                OracleStrength::Strong,
                "{family:?} lost equality's strong credit"
            );
        }
        // A family override may still lower an inequality.
        let smoke = oracle(
            "assert_ne!(run(), Config { retries: 9 });",
            OracleKind::SmokeOnly,
            OracleStrength::Weak,
        );
        assert_eq!(
            probe_relative_oracle_strength(&ProbeFamily::ReturnValue, &smoke),
            OracleStrength::Smoke
        );
    }

    fn owned_probe(family: ProbeFamily, expression: &str, owner: &str) -> Probe {
        Probe {
            owner: Some(SymbolId(format!("src/lib.rs::{owner}"))),
            ..probe(family, expression)
        }
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

    fn test_with_assertions(name: &str, assertions: Vec<OracleFact>) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/value.rs"),
            start_line: 1,
            end_line: 3,
            body: "score();".into(),
            calls: Vec::new(),
            assertions,
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    /// #3700: a witness whose body binds the seam's callee (the shape the
    /// wrapper-establishment rule keys on), with no captured call facts — the
    /// lexical fallback in `body_contains_owner_call` must carry the binding.
    fn test_with_body_assertions(
        name: &str,
        body: &str,
        assertions: Vec<OracleFact>,
    ) -> TestSummary {
        TestSummary {
            body: body.into(),
            ..test_with_assertions(name, assertions)
        }
    }

    fn oracle(text: &str, kind: OracleKind, strength: OracleStrength) -> OracleFact {
        OracleFact {
            line: 2,
            text: text.to_string(),
            kind,
            strength,
            observed_tokens: extract_identifier_tokens(text),
            ok_value_observed: None,
        }
    }

    /// A guarded Result-match oracle fact with an explicit Ok-arm
    /// observation decision, mirroring what the extraction-side scanner
    /// threads into the fact (#3731 observation authority).
    fn guarded_oracle(text: &str, strength: OracleStrength, ok_value_observed: bool) -> OracleFact {
        OracleFact {
            line: 2,
            text: text.to_string(),
            kind: OracleKind::GuardedResultMatch,
            strength,
            observed_tokens: extract_identifier_tokens(text),
            ok_value_observed: Some(ok_value_observed),
        }
    }

    // --- #3709 producer-owned guarded Result match ---

    /// A guarded Result match whose scrutinee directly calls the probe's
    /// owner confirms observation even with zero changed-line token overlap.
    #[test]
    fn guarded_result_match_on_owner_confirms_observation_without_token_overlap() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "validates_ready_response",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (observe, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(discriminate.state, StageState::Yes, "{discriminate:?}");
        assert!(
            discriminate
                .summary
                .contains("guarded Result match over the changed owner's result"),
            "{discriminate:?}"
        );
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].oracle_kind, OracleKind::GuardedResultMatch);
    }

    /// The same oracle against a probe owned by a different function never
    /// confirms observation: the binding is same-entity by name.
    #[test]
    fn guarded_result_match_on_wrong_owner_stays_unverified() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_a_different_helper",
            vec![guarded_oracle(
                "match other_helper(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (_, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::SameTestFile)]);

        assert_eq!(discriminate.state, StageState::Weak, "{discriminate:?}");
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "wrong owner must not confirm: {discriminate:?}"
        );
        assert_eq!(related.len(), 1, "association is via single-assertion only");
    }

    /// A medium (type-pin) guarded match confirms observation but keeps the
    /// seam below exposed: the error type is pinned, the variant is not.
    #[test]
    fn guarded_result_match_type_pin_keeps_discriminate_weak() {
        let probe = owned_probe(
            ProbeFamily::ErrorPath,
            "return Err(Box::new(ParseError::InvalidData { .. }));",
            "expect_response",
        );
        let test = test_with_assertions(
            "checks_error_type",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => .downcast_ref::<ParseError> }",
                OracleStrength::Medium,
                true,
            )],
        );
        let (observe, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(discriminate.state, StageState::Weak, "{discriminate:?}");
        assert!(
            !matches!(discriminate.state, StageState::Yes),
            "a type-only pin must not read exposed"
        );
    }

    /// Effect families never take the producer-owned path: a changed call or
    /// effect inside the owner need not flow through the matched result.
    #[test]
    fn guarded_result_match_does_not_credit_effect_families() {
        let probe = owned_probe(
            ProbeFamily::SideEffect,
            "audit_log.append(event);",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_result",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_ne!(
            discriminate.state,
            StageState::Yes,
            "effect families keep their own observers: {discriminate:?}"
        );
    }

    /// #3731 review: a guarded match pinning a SIBLING variant does not
    /// confirm a probe whose changed expression constructs the exact
    /// variant — the shared enum-qualifier token is not a specificity
    /// signal, so the seam stays weakly exposed with an unverified
    /// observation.
    #[test]
    fn guarded_result_match_sibling_variant_does_not_confirm() {
        let probe = owned_probe(
            ProbeFamily::ErrorPath,
            "return Err(ParseError::InvalidData);",
            "expect_response",
        );
        let test = test_with_assertions(
            "pins_a_sibling_variant",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => ParseError::UnexpectedEof }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (observe, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::Yes, "the guard still observes");
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "a sibling-variant guard must not read exposed: {discriminate:?}"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "the sibling variant leaves observation unverified: {discriminate:?}"
        );
        assert_eq!(
            related.len(),
            1,
            "association survives; confirmation does not"
        );
    }

    /// Positive control for the sibling gate: the SAME guarded match
    /// pinning the exact changed variant does confirm, through the
    /// variant-gated owner binding.
    #[test]
    fn guarded_result_match_exact_variant_on_owner_confirms() {
        let probe = owned_probe(
            ProbeFamily::ErrorPath,
            "return Err(ParseError::InvalidData);",
            "expect_response",
        );
        let test = test_with_assertions(
            "pins_the_exact_variant",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => ParseError::InvalidData }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "the exact-variant guard must confirm: {discriminate:?}"
        );
    }

    /// #3731 review: a qualified scrutinee sharing the owner's bare name
    /// (`other_crate::expect_response`) never confirms the local owner —
    /// reveal cannot resolve the qualified path's identity, so the
    /// observation stays unverified.
    #[test]
    fn guarded_result_match_on_qualified_scrutinee_stays_unverified() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_a_qualified_same_named_callee",
            vec![guarded_oracle(
                "match other_crate::expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::SameTestFile)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "a qualified scrutinee must not bind the owner: {discriminate:?}"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "the qualified-path observation stays unverified: {discriminate:?}"
        );
    }

    // --- #3731 observation authority (RIPR-SPEC-0175): Ok-arm observation ---

    /// A return-value probe whose changed value is the SUCCESS payload is
    /// not confirmed by a guarded-routing match with no Ok arm: the success
    /// value flows into a trivial catch-all, so the test never observes a
    /// change to it and the fact reports `ok_value_observed: Some(false)`.
    #[test]
    fn return_value_probe_routing_form_without_ok_arm_stays_unconfirmed() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "Ok(build_response(expected_id, trimmed.trim()))",
            "expect_response",
        );
        let test = test_with_assertions(
            "routes_the_result",
            vec![guarded_oracle(
                "match expect_response(..) { Err(..) => ParseError::InvalidData, _ => .. }",
                OracleStrength::Strong,
                false,
            )],
        );
        let (_, discriminate, related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "a routing form never observes the Ok payload: {discriminate:?}"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "the unobserved success value stays unverified: {discriminate:?}"
        );
        assert_eq!(
            related.len(),
            1,
            "association survives; confirmation does not"
        );
    }

    /// A payload-ignoring Ok arm (`Ok(_) => {}`) does not confirm a
    /// success-payload return-value probe either: the synthesized text
    /// keeps its `Ok(..) => ..` template (output-contract stability), so
    /// the unobserving decision rides the fact's `ok_value_observed`.
    #[test]
    fn return_value_probe_payload_ignoring_ok_arm_stays_unconfirmed() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "Ok(build_response(expected_id, trimmed.trim()))",
            "expect_response",
        );
        let test = test_with_assertions(
            "ignores_the_payload",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => ParseError::InvalidData }",
                OracleStrength::Strong,
                false,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "an Ok arm that ignores the payload never discriminates it: {discriminate:?}"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "the ignored payload stays unverified: {discriminate:?}"
        );
    }

    /// Positive control: the same probe against an OBSERVING Ok arm
    /// (`Ok(v) => assert_eq!(v, 3)`) confirms through the producer-owned
    /// binding — the fact reports `ok_value_observed: Some(true)`.
    #[test]
    fn return_value_probe_observing_ok_arm_confirms() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "Ok(build_response(expected_id, trimmed.trim()))",
            "expect_response",
        );
        let test = test_with_assertions(
            "asserts_the_payload",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => ParseError::InvalidData }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "an observing Ok arm discriminates the success payload: {discriminate:?}"
        );
    }

    /// ErrorPath probes keep the existing behavior: the Err guard is the
    /// discriminator there, so a non-observing Ok arm never blocks the
    /// variant-gated confirmation.
    #[test]
    fn error_path_probe_is_independent_of_ok_arm_observation() {
        let probe = owned_probe(
            ProbeFamily::ErrorPath,
            "return Err(ParseError::InvalidData);",
            "expect_response",
        );
        let test = test_with_assertions(
            "pins_the_error_variant",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => ParseError::InvalidData }",
                OracleStrength::Strong,
                false,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "the Err guard discriminates regardless of the Ok arm: {discriminate:?}"
        );
    }

    /// A return-value probe on an exact Err CONSTRUCTION also keeps the
    /// Err-guard discriminator (the pin names the changed variant): the
    /// Ok-arm observation requirement applies only to success-payload
    /// probes — the shape the guarded-routing positive fixture pins.
    #[test]
    fn return_value_probe_on_err_construction_is_independent_of_ok_arm_observation() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "return Err(ParseError::InvalidData);",
            "expect_response",
        );
        let test = test_with_assertions(
            "routes_the_error",
            vec![guarded_oracle(
                "match expect_response(..) { Err(..) => ParseError::InvalidData, _ => .. }",
                OracleStrength::Strong,
                false,
            )],
        );
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "the exact-variant pin discriminates the Err construction: {discriminate:?}"
        );
    }

    // --- #3731 review round 4 (F11): foreign same-name import defeat ---

    /// The related test's file importing the owner callee's bare name from
    /// a FOREIGN path (`use other_crate::expect_response;`) makes the bare
    /// scrutinee binding ambiguous — the owner confirmation is refused and
    /// the observation stays unverified.
    #[test]
    fn foreign_same_name_import_defeats_owner_confirmation() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_an_imported_same_named_callee",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let test_source = "use other_crate::expect_response;\n";
        let crate_names = std::collections::BTreeSet::new();
        let (_, discriminate, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_test, callee| file_imports_foreign_callee_name(test_source, callee, &crate_names),
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "a foreign same-name import must defeat the owner binding: {discriminate:?}"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "the ambiguous binding leaves observation unverified: {discriminate:?}"
        );
    }

    /// Positive control: the same harness WITHOUT the import confirms —
    /// and an OWN-CRATE import (the normal integration-test binding) does
    /// not defeat, because it binds the owner's own export.
    #[test]
    fn no_import_or_own_crate_import_keeps_owner_confirmation() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_the_owner_directly",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let without_import = "";
        let own_crate_import = "use guarded_result_match::{ParseError, expect_response};\n";
        let own_crate_names: std::collections::BTreeSet<String> = ["guarded_result_match"]
            .into_iter()
            .map(str::to_string)
            .collect();

        let (_, discriminate, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_, callee| file_imports_foreign_callee_name(without_import, callee, &own_crate_names),
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "no import means no ambiguity: {discriminate:?}"
        );

        let (_, own_crate, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_, callee| {
                file_imports_foreign_callee_name(own_crate_import, callee, &own_crate_names)
            },
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(
            own_crate.state,
            StageState::Yes,
            "an own-crate import binds the owner's own export and must not defeat: {own_crate:?}"
        );
    }

    /// An aliased foreign import (`use other_crate::expect_response as
    /// respond;`) binds the ALIAS, not the bare name, so the bare
    /// scrutinee is unambiguous and the confirmation stands.
    #[test]
    fn aliased_foreign_import_does_not_defeat_owner_confirmation() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_the_owner_with_an_unrelated_alias_import",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let aliased_import = "use other_crate::expect_response as respond;\n";
        let crate_names = std::collections::BTreeSet::new();
        let (_, discriminate, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_, callee| file_imports_foreign_callee_name(aliased_import, callee, &crate_names),
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "an aliased import binds the alias, not the bare name: {discriminate:?}"
        );
    }

    /// F22 (#3731 review): a foreign import NESTED inside a test module —
    /// the historical `mod tests { use other_crate::expect_response; .. }`
    /// harness shape — defeats the owner confirmation too. Pre-fix the
    /// scan read file-level `use` statements only, so the nested import
    /// bypassed the defeat.
    #[test]
    fn nested_module_foreign_import_defeats_owner_confirmation() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_an_imported_same_named_callee_from_a_test_module",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let test_source = "mod tests {\n    use other_crate::expect_response;\n\n    #[test]\n    fn guards_the_result() {}\n}\n";
        let crate_names = std::collections::BTreeSet::new();
        let (_, discriminate, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_test, callee| file_imports_foreign_callee_name(test_source, callee, &crate_names),
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "a module-nested foreign same-name import must defeat the owner binding: {discriminate:?}"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "the ambiguous binding leaves observation unverified: {discriminate:?}"
        );
    }

    /// F22 direct-scan controls on the same function: a nested import is
    /// found at any depth, a function-local import counts, and text
    /// without the keyword does not.
    #[test]
    fn foreign_callee_import_scan_covers_nested_and_local_use_declarations() {
        let crate_names = std::collections::BTreeSet::new();
        let nested = "mod tests {\n    use other_crate::expect_response;\n}\n";
        assert!(
            file_imports_foreign_callee_name(nested, "expect_response", &crate_names),
            "a module-nested foreign import must defeat"
        );
        let brace_list = "mod tests {\n    use other_crate::{setup, expect_response};\n}\n";
        assert!(
            file_imports_foreign_callee_name(brace_list, "expect_response", &crate_names),
            "a module-nested brace-list import must defeat"
        );
        let function_local = "fn t() {\n    use other_crate::expect_response;\n}\n";
        assert!(
            file_imports_foreign_callee_name(function_local, "expect_response", &crate_names),
            "a function-local foreign import must defeat"
        );
        let own_path = "mod tests {\n    use crate::expect_response;\n}\n";
        assert!(
            !file_imports_foreign_callee_name(own_path, "expect_response", &crate_names),
            "an own-crate nested import binds the owner and must not defeat"
        );
    }

    /// The run-scoped memo answers every (file, callee) query exactly as a
    /// fresh scan of that file does, on the first (scanning) query and on
    /// every later (cached) one, so sharing it across probes cannot change
    /// a defeat.
    #[test]
    fn shared_file_use_statements_match_a_fresh_scan_for_every_callee() {
        let crate_names: std::collections::BTreeSet<String> =
            ["own_crate"].into_iter().map(str::to_string).collect();
        let files = [
            (
                "tests/foreign.rs",
                "// use comment_crate::setup;\nuse other_crate::{setup, expect_response};\n",
            ),
            (
                "tests/own.rs",
                "use own_crate::expect_response;\nfn t() {\n    use other_crate::teardown;\n}\n",
            ),
            (
                "tests/none.rs",
                "fn t() {\n    let text = \"use other_crate::expect_response;\";\n}\n",
            ),
        ];
        let callees = ["expect_response", "setup", "teardown", "absent", ""];
        let memo = FileUseStatements::default();
        for round in 0..2 {
            for (file, source) in files {
                for callee in callees {
                    assert_eq!(
                        memo.imports_foreign_callee_name(
                            std::path::Path::new(file),
                            source,
                            callee,
                            &crate_names,
                        ),
                        file_imports_foreign_callee_name(source, callee, &crate_names),
                        "round {round}: {file} / {callee:?}"
                    );
                }
            }
        }
        // The fixture exercises both answers, so agreement is not vacuous.
        assert!(memo.imports_foreign_callee_name(
            std::path::Path::new("tests/foreign.rs"),
            files[0].1,
            "setup",
            &crate_names,
        ));
        assert!(!memo.imports_foreign_callee_name(
            std::path::Path::new("tests/own.rs"),
            files[1].1,
            "expect_response",
            &crate_names,
        ));
    }

    /// F23 (#3731 review): the analyzed crate's own names include the
    /// `[lib]` target name, and hyphenated package names normalize to
    /// underscores in crate identifiers — an import through the
    /// underscore form of a hyphenated package name binds the owner's own
    /// export and must NOT defeat, while a foreign first segment still
    /// does.
    #[test]
    fn own_lib_target_and_hyphen_normalized_names_do_not_defeat() {
        // Package `foo-bar` (hyphenated) whose lib target is `foo_bar`:
        // the integration-test binding `use foo_bar::expect_response;` is
        // the owner's own export on both spellings.
        let hyphenated_and_lib_names: std::collections::BTreeSet<String> = ["foo-bar", "foo_bar"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let import = "use foo_bar::expect_response;\n";
        assert!(
            !file_imports_foreign_callee_name(import, "expect_response", &hyphenated_and_lib_names),
            "an import through the crate's own lib-target name must not defeat"
        );
        // The normalization direction too: a raw hyphenated manifest name
        // admits its underscore crate identifier.
        let hyphenated_only: std::collections::BTreeSet<String> =
            ["foo-bar"].into_iter().map(str::to_string).collect();
        assert!(
            !file_imports_foreign_callee_name(import, "expect_response", &hyphenated_only),
            "a hyphenated own package name must admit its underscore identifier"
        );
        // A foreign first segment still defeats.
        let foreign_names: std::collections::BTreeSet<String> = ["unrelated_crate"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert!(
            file_imports_foreign_callee_name(import, "expect_response", &foreign_names),
            "a foreign first segment must still defeat"
        );
    }

    /// #3731 review (G1): the cross-package same-name defeat threads
    /// through the same per-test path as the import defeat — when the
    /// test's own package defines the callee's name while the changed
    /// owner lives in another package, the bare binding is ambiguous and
    /// the confirmation is refused; without the defeat it stands.
    #[test]
    fn cross_package_same_name_defeat_blocks_owner_confirmation() {
        let probe = owned_probe(
            ProbeFamily::ReturnValue,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            "expect_response",
        );
        let test = test_with_assertions(
            "guards_a_same_named_local_function",
            vec![guarded_oracle(
                "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }",
                OracleStrength::Strong,
                true,
            )],
        );
        let (_, defeated, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_, _| false,
            &|_, _| true,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(
            defeated.state,
            StageState::Weak,
            "a same-named function in the test's own package must defeat the \
             bare binding: {defeated:?}"
        );
        assert!(
            defeated.summary.contains("observation_unverified"),
            "the ambiguous binding leaves observation unverified: {defeated:?}"
        );

        let (_, confirmed, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_, _| false,
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(
            confirmed.state,
            StageState::Yes,
            "without a same-named local definition the confirmation stands: {confirmed:?}"
        );
    }

    // --- RIPR-SPEC-0093 arm-blind downgrade ---

    /// A MatchArm probe whose expression has no extractable tokens (e.g. `None`
    /// is filtered) and whose single related test has an ExactValue oracle for a
    /// DIFFERENT arm must emit weakly_exposed with observation_unverified.
    #[test]
    fn match_arm_probe_without_token_match_downgrades_discriminate_to_weak() {
        // probe expression "None => 0," — None is filtered, 0 is not alpha
        let probe = probe(ProbeFamily::MatchArm, "None => 0,");
        let test = test_with_assertions(
            "some_arm_returns_incremented_value",
            vec![oracle(
                "assert_eq!(reason(Some(5)), 6);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::Yes, "observe must still fire");
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "discriminate must be downgraded to Weak (observation_unverified)"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "summary must name the reason: got `{}`",
            discriminate.summary
        );
    }

    /// RIPR-SPEC-0040 builder override (#6613): the related test and its
    /// assertion for the retry builder shapes, with the assertion on body
    /// line 2 (file line 3).
    fn builder_override_test(statement: &str, assertion: &str) -> TestSummary {
        test_with_body_assertions(
            "override_sets_attempts",
            &format!("fn override_sets_attempts() {{\n    {statement}\n    {assertion}\n}}"),
            vec![OracleFact {
                line: 3,
                ..oracle(assertion, OracleKind::ExactValue, OracleStrength::Strong)
            }],
        )
    }

    fn builder_override_probe(family: ProbeFamily, expression: &str) -> Probe {
        Probe {
            owner: Some(SymbolId(
                "src/settings.rs::impl RetryBuilder::new".to_string(),
            )),
            ..probe(family, expression)
        }
    }

    /// RIPR-SPEC-0040 builder override (#6613): a struct literal, or one of
    /// its field initializers, whose only confirming token is a field the
    /// test overwrites on the owner's result (`.attempts(5)`) before reading
    /// it does not reach the assertion, so the exact-value oracle cannot
    /// read `exposed`.
    #[test]
    fn struct_literal_field_overridden_by_test_is_unverified() {
        let test = builder_override_test(
            "let retry = RetryBuilder::new().attempts(5).build();",
            "assert_eq!(retry.attempts, 5);",
        );
        for probe in [
            builder_override_probe(
                ProbeFamily::ReturnValue,
                "RetryBuilder { attempts: 4, delay_ms: 100 }",
            ),
            builder_override_probe(ProbeFamily::FieldConstruction, "attempts: 4"),
        ] {
            let (observe, discriminate, _related) =
                reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
            assert_eq!(observe.state, StageState::Yes, "{}", probe.expression);
            assert_eq!(discriminate.state, StageState::Weak, "{}", probe.expression);
            assert_eq!(discriminate.summary, BUILDER_OVERRIDE_UNOBSERVED_SUMMARY);
            assert!(discriminate.summary.contains("(observation_unverified)"));
        }
    }

    /// Controls for the builder override: reading the default with no
    /// setter, a setter on another field, a discarded setter result, and a
    /// setter inside an argument all still observe the changed literal.
    #[test]
    fn struct_literal_default_field_read_stays_exposed() {
        for (statement, assertion) in [
            (
                "let retry = RetryBuilder::new().build();",
                "assert_eq!(retry.attempts, 3);",
            ),
            (
                "let retry = RetryBuilder::new().delay_ms(5).build();",
                "assert_eq!(retry.attempts, 3);",
            ),
            (
                "let builder = RetryBuilder::new(); builder.with_attempts(5); let retry = builder.build();",
                "assert_eq!(retry.attempts, 3);",
            ),
            (
                "let retry = RetryBuilder::new().with_other(other.attempts(5)).build();",
                "assert_eq!(retry.attempts, 3);",
            ),
        ] {
            let test = builder_override_test(statement, assertion);
            for probe in [
                builder_override_probe(
                    ProbeFamily::ReturnValue,
                    "RetryBuilder { attempts: 4, delay_ms: 100 }",
                ),
                builder_override_probe(ProbeFamily::FieldConstruction, "attempts: 4"),
            ] {
                let (_observe, discriminate, _related) =
                    reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
                assert_eq!(
                    discriminate.state,
                    StageState::Yes,
                    "{statement} / {}: {}",
                    probe.expression,
                    discriminate.summary
                );
            }
        }
    }

    /// A MatchArm probe whose expression DOES have a token that appears in the
    /// assertion text (token_match) must stay exposed (StageState::Yes).
    #[test]
    fn match_arm_probe_with_token_match_keeps_discriminate_yes() {
        // probe expression "Status::Idle => 0," — Idle and Status are extractable
        let probe = probe(ProbeFamily::MatchArm, "Status::Idle => 0,");
        let test = test_with_assertions(
            "idle_arm_returns_zero",
            vec![oracle(
                "assert_eq!(classify(Status::Idle), 0);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(observe.state, StageState::Yes);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "token_match on Idle must keep discriminate Yes (no over-correction)"
        );
    }

    /// A ReturnValue probe whose single related test has a family-matching assertion
    /// but NO token referencing the changed expression must downgrade to Weak.
    /// This inverts the former bug-locking test
    /// `non_match_arm_probe_family_match_only_keeps_discriminate_yes`.
    #[test]
    fn return_value_family_match_only_without_token_downgrades_discriminate_to_weak() {
        // "value + 1" — tokens: ["value"] (len 5). Assertion "assert_eq!(compute(), 42);"
        // has no occurrence of "value", so has_token_match=false.
        let probe = probe(ProbeFamily::ReturnValue, "value + 1");
        let test = test_with_assertions(
            "returns_incremented",
            vec![oracle(
                "assert_eq!(compute(), 42);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "ReturnValue probe with no token_match must downgrade to Weak (observation_unverified)"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "summary must name the reason: got `{}`",
            discriminate.summary
        );
    }

    /// A ReturnValue probe whose assertion text CONTAINS a token from the probe
    /// expression must stay exposed (StageState::Yes) — no over-correction.
    #[test]
    fn return_value_with_token_match_keeps_discriminate_yes() {
        // "score + 1" — tokens: ["score"] (len 5). Assertion contains "score".
        let probe = probe(ProbeFamily::ReturnValue, "score + 1");
        let test = test_with_assertions(
            "score_incremented",
            vec![oracle(
                "assert_eq!(score(), 6);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "ReturnValue probe with token_match must stay Yes (no over-correction)"
        );
    }

    /// A FieldConstruction probe whose single assertion has a dot (family_match)
    /// but no token referencing the specific changed field must downgrade to Weak.
    #[test]
    fn field_construction_family_match_only_without_token_downgrades_to_weak() {
        // "priority: 3" — tokens: ["priority"] (len 8). Assertion "assert_eq!(item.id, 3);"
        // does not contain "priority".
        let probe = probe(ProbeFamily::FieldConstruction, "priority: 3");
        let test = test_with_assertions(
            "item_has_id",
            vec![oracle(
                "assert_eq!(item.id, 3);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "FieldConstruction probe with no token_match must downgrade to Weak"
        );
    }

    /// A FieldConstruction probe whose assertion references the exact changed field
    /// must stay exposed (StageState::Yes).
    #[test]
    fn field_construction_with_token_match_keeps_discriminate_yes() {
        // "priority: 3" — tokens: ["priority"]. Assertion "assert_eq!(item.priority, 3);"
        // contains "priority".
        let probe = probe(ProbeFamily::FieldConstruction, "priority: 3");
        let test = test_with_assertions(
            "item_has_priority",
            vec![oracle(
                "assert_eq!(item.priority, 3);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "FieldConstruction probe with token_match on priority must stay Yes"
        );
    }

    /// A SideEffect probe whose single PLAIN assertion has neither a token
    /// referencing the changed effect NOR an effect-observer kind (no mock,
    /// snapshot, or whole-object) must emit observation_unverified. This is the
    /// genuinely-blind case: the assertion only fired via the single-assertion
    /// escape hatch.
    #[test]
    fn side_effect_plain_assertion_without_token_or_observer_emits_observation_unverified() {
        // "send_notification(user_id)" — tokens: ["send", "notification", "user"].
        // Assertion "assert!(ran);" contains none of those, and is not a mock,
        // snapshot, or whole-object observer.
        let probe = probe(ProbeFamily::SideEffect, "send_notification(user_id)");
        let test = test_with_assertions(
            "ran",
            vec![oracle(
                "assert!(ran);",
                OracleKind::Unknown,
                OracleStrength::Unknown,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "SideEffect probe with no token and no effect observer must downgrade to Weak"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "plain non-observing assertion must emit observation_unverified: got `{}`",
            discriminate.summary
        );
    }

    /// REGRESSION LOCK (#1216 second-pass): a SideEffect probe whose single
    /// matched assertion is a genuine MOCK EXPECTATION that kind-matches the seam
    /// but shares NO token with the probe expression must NOT emit
    /// observation_unverified. The mock observes the effect; downgrading it to
    /// observation_unverified would be a false weakening. (It may still be Weak
    /// via the Medium-strength path, but never via observation_unverified.)
    #[test]
    fn side_effect_mock_observer_without_token_clears_observation_unverified() {
        // "send_notification(user_id)" — tokens: ["send", "notification", "user"].
        // Assertion "mock.verify();" shares no token but is a MockExpectation,
        // i.e. a genuine effect observer.
        let probe = probe(ProbeFamily::SideEffect, "send_notification(user_id)");
        let test = test_with_assertions(
            "notification_checked",
            vec![oracle(
                "mock.verify();",
                OracleKind::MockExpectation,
                OracleStrength::Medium,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "a genuine mock observer must clear observation_unverified even without a token: got `{}`",
            discriminate.summary
        );
    }

    /// REGRESSION LOCK (#1216 second-pass): a CallDeletion probe whose single
    /// matched assertion is a whole-object equality (a genuine effect observer)
    /// sharing no token must NOT emit observation_unverified. Whole-object
    /// equality captures the resulting persisted state, so it observes the
    /// effect even without naming the changed call token.
    #[test]
    fn call_deletion_whole_object_observer_without_token_clears_observation_unverified() {
        // "persist_audit(record)" — tokens: ["persist", "audit", "record"].
        // Assertion "assert_eq!(store, expected);" shares no token but is a
        // WholeObjectEquality effect observer (Strong).
        let probe = probe(ProbeFamily::CallDeletion, "persist_audit(record)");
        let test = test_with_assertions(
            "store_matches_expected",
            vec![oracle(
                "assert_eq!(store, expected);",
                OracleKind::WholeObjectEquality,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "a whole-object effect observer must clear observation_unverified even without a token: got `{}`",
            discriminate.summary
        );
    }

    /// RIPR-SPEC-0094 Part D (#7046 review): when the effect carrier refuses
    /// a whole-object equality, the effect-observer branch withholds the
    /// confirmation; a token match still confirms on its own.
    #[test]
    fn a_refused_effect_carrier_withholds_only_the_whole_object_observer() {
        let probe = probe(ProbeFamily::CallDeletion, "persist_audit(record)");
        let reveal = |kind: OracleKind, text: &str, carried: bool| {
            let test = test_with_assertions(
                "store_matches_expected",
                vec![oracle(text, kind, OracleStrength::Strong)],
            );
            let carried = move |_: &TestSummary, _: &OracleFact| carried;
            reveal_evidence_with_expression(
                &probe,
                &probe.expression,
                &[(&test, RelationReason::DirectOwnerCall)],
                &[],
                &|_, _| false,
                &|_, _| false,
                &ReturnOracleAdmission {
                    owner_return_pin: &|_, _| false,
                    assertion_admitted: &|_, _| true,
                    sibling_exclusion_admitted: &|_, _| true,
                    proximity_may_reach_owner: &|_| false,
                    owner_parameters: &[],
                    expected_reaches_owner: &|_, _| false,
                    effect_state_carried: &carried,
                    setter_writes_field: &|_, _, _| true,
                    getter_reads_field: &|_, _, _| false,
                    getter_reads_key: &|_, _, _| None,
                    owner_argument_names: &[],
                    owner_returns_value: true,
                    constructor_starts_empty: &|_, _, _| false,
                    call_cannot_store: &|_, _, _| false,
                },
                None,
            )
            .1
            .summary
        };
        let whole = "assert_eq!(store, expected);";
        assert!(
            !reveal(OracleKind::WholeObjectEquality, whole, true)
                .contains("observation_unverified")
        );
        assert!(
            reveal(OracleKind::WholeObjectEquality, whole, false)
                .contains("observation_unverified")
        );
        // A token match confirms without the effect-observer branch.
        assert!(
            !reveal(
                OracleKind::WholeObjectEquality,
                "assert_eq!(persist_audit(record), expected);",
                false,
            )
            .contains("observation_unverified")
        );
    }

    /// A VALUE family (ReturnValue) must NOT treat a mock/whole-object as an
    /// observation confirmation — only a token_match confirms value families.
    /// This guards against the effect-family relaxation leaking into value
    /// families (the point of #1200/#1216: an ExactValue/whole-object oracle
    /// does not kind-match a value seam's specific sub-expression).
    #[test]
    fn return_value_whole_object_without_token_still_emits_observation_unverified() {
        // "base * SCALE" — tokens: ["base", "SCALE"]. Assertion
        // "assert_eq!(result, expected);" is WholeObjectEquality but shares no
        // token; for a VALUE family this must NOT clear observation_unverified.
        let probe = probe(ProbeFamily::ReturnValue, "base * SCALE");
        let test = test_with_assertions(
            "result_matches_expected",
            vec![oracle(
                "assert_eq!(result, expected);",
                OracleKind::WholeObjectEquality,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "ReturnValue (value family) with no token must stay observation_unverified"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "value family must not be cleared by an effect-observer kind: got `{}`",
            discriminate.summary
        );
    }

    /// A SideEffect probe whose assertion references a specific token from the
    /// changed expression must not be downgraded by observation_unverified.
    #[test]
    fn side_effect_with_token_match_keeps_discriminate_not_unverified() {
        // "emit_payment_event(tx)" — tokens: ["emit_payment_event", "tx"] but
        // only "emit_payment_event" (len > 3, well, len 17) would match.
        // Assertion "mock.expect_emit_payment_event();" contains "emit_payment_event".
        let probe = probe(ProbeFamily::SideEffect, "emit_payment_event(tx)");
        let test = test_with_assertions(
            "payment_event_emitted",
            vec![oracle(
                "mock.expect_emit_payment_event();",
                OracleKind::MockExpectation,
                OracleStrength::Medium,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        // MockExpectation yields OracleStrength::Medium → StageState::Weak via
        // the existing Medium oracle path, NOT via observation_unverified.
        // The key invariant: observation_unverified must NOT fire when there IS
        // a token_match — so the summary must not contain "observation_unverified".
        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "token-matched SideEffect must not emit observation_unverified: got `{}`",
            discriminate.summary
        );
    }

    /// A CallDeletion probe whose single assertion fires family_match via
    /// `text.contains("assert")` but contains no token from the changed call
    /// expression must downgrade to Weak.
    #[test]
    fn call_deletion_family_match_only_without_token_downgrades_to_weak() {
        // "log_audit_event(record)" — tokens: ["audit", "event", "record"] (all len > 3).
        // Assertion "assert!(result.is_ok());" does not contain any of those.
        let probe = probe(ProbeFamily::CallDeletion, "log_audit_event(record)");
        let test = test_with_assertions(
            "result_ok",
            vec![oracle(
                "assert!(result.is_ok());",
                OracleKind::BroadError,
                OracleStrength::Weak,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "CallDeletion probe with no token_match must downgrade to Weak"
        );
    }

    /// A CallDeletion probe whose assertion text contains the full call token
    /// must not be downgraded by observation_unverified.
    #[test]
    fn call_deletion_with_token_match_does_not_emit_observation_unverified() {
        // "log_audit_event(record)" — tokens: ["log_audit_event", "record"].
        // Assertion "assert!(log_audit_event());" contains
        // "log_audit_event" as a whole-word match (#2397: the word-boundary
        // check correctly distinguishes `log_audit_event` from a different
        // identifier like `log_audit_event_was_called`).
        let probe = probe(ProbeFamily::CallDeletion, "log_audit_event(record)");
        let test = test_with_assertions(
            "audit_event_logged",
            vec![oracle(
                "assert!(log_audit_event());",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "token-matched CallDeletion must not emit observation_unverified: got `{}`",
            discriminate.summary
        );
    }

    /// MatchArm: type-blind token match — `Mode::Warm` assertion must NOT clear
    /// observation_unverified for a `Mode::Frozen` probe (Part B regression lock).
    #[test]
    fn match_arm_sibling_qualifier_does_not_clear_observation_unverified() {
        // probe expression "Mode::Frozen => -1," — tokens: ["Mode", "Frozen"].
        // Assertion "assert_eq!(classify(Mode::Warm), 1);" contains "Mode" (len 4)
        // but NOT "Frozen" (the variant token). With variant-scoped token_match,
        // "Mode" alone must not clear observation_unverified.
        let probe = probe(ProbeFamily::MatchArm, "Mode::Frozen => -1,");
        let test = test_with_assertions(
            "warm_arm_returns_one",
            vec![oracle(
                "assert_eq!(classify(Mode::Warm), 1);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "Mode::Warm assertion must not confirm Mode::Frozen arm (sibling-qualifier hole)"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "summary must name the reason: got `{}`",
            discriminate.summary
        );
    }

    /// #6297: a same-file test that names the arm's variant through another
    /// function cannot confirm the arm while a test that calls the owner is
    /// related, so editing that same-file test cannot move the verdict.
    #[test]
    fn match_arm_proximity_test_cannot_confirm_beside_reaching_test() {
        let probe = probe(ProbeFamily::MatchArm, "Unit::Fortnight => 1_209_600,");
        let reaching = test_with_assertions(
            "seconds_total",
            vec![oracle(
                "assert_eq!(total, 1_814_400);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let token_matches = |assertion: &str| {
            test_with_assertions(
                "from_str_fortnight",
                vec![oracle(
                    assertion,
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                )],
            )
        };
        let naming = token_matches(
            r#"assert!(matches!(Unit::from_str("fortnight"), Ok(Unit::Fortnight)));"#,
        );
        let not_naming = token_matches(
            r#"assert_eq!(Unit::from_str("fortnight").map(|u| u == Unit::Week), Ok(false));"#,
        );

        let (_, with_token, _) = reveal_evidence(
            &probe,
            &[
                (&reaching, RelationReason::DirectOwnerCall),
                (&naming, RelationReason::SameTestFile),
            ],
        );
        let (_, without_token, _) = reveal_evidence(
            &probe,
            &[
                (&reaching, RelationReason::DirectOwnerCall),
                (&not_naming, RelationReason::SameTestFile),
            ],
        );

        assert_eq!(with_token.state, StageState::Weak, "{}", with_token.summary);
        assert_eq!(with_token.state, without_token.state);
        assert_eq!(with_token.summary, without_token.summary);
        // The summary must not claim that no assertion names the arm while the
        // same-file test names `Unit::Fortnight`; it says why that test cannot
        // confirm. Without a proximity test the generic summary stays.
        assert_eq!(with_token.summary, PROXIMITY_CONFIRMATION_WITHHELD);
        let (_, reaching_only, _) =
            reveal_evidence(&probe, &[(&reaching, RelationReason::DirectOwnerCall)]);
        assert_eq!(reaching_only.state, StageState::Weak);
        assert_ne!(reaching_only.summary, PROXIMITY_CONFIRMATION_WITHHELD);

        // Alone, the same-file test still confirms: proximity keeps crediting
        // when no related test reaches the owner by a call.
        let (_, alone, _) = reveal_evidence(&probe, &[(&naming, RelationReason::SameTestFile)]);
        assert_eq!(alone.state, StageState::Yes, "{}", alone.summary);

        // A seam callee call runs the callee, not the owner, so it does not
        // withhold the same-file confirmation either.
        let (_, beside_seam, _) = reveal_evidence(
            &probe,
            &[
                (&reaching, RelationReason::SeamCalleeCall),
                (&naming, RelationReason::SameTestFile),
            ],
        );
        assert_eq!(
            beside_seam.state,
            StageState::Yes,
            "{}",
            beside_seam.summary
        );

        // A same-file test that may run the owner (it calls a public wrapper
        // of `seconds`) keeps confirming beside the reaching test.
        let (_, may_reach, _, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[
                (&reaching, RelationReason::DirectOwnerCall),
                (&naming, RelationReason::SameTestFile),
            ],
            &[],
            &|_, _| false,
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|test| test.name == "from_str_fortnight",
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_eq!(may_reach.state, StageState::Yes, "{}", may_reach.summary);
    }

    /// #7063: an exact error variant is shared by every function returning
    /// that enum, so a same-file pin of another function's
    /// `Err(PayError::Limit)` cannot confirm this owner's
    /// `return Err(PayError::Limit)` while a test that calls the owner is
    /// related. Alone, the same-file test still confirms, as for #6297.
    #[test]
    fn error_variant_proximity_test_cannot_confirm_beside_reaching_test() {
        let reaching = test_with_assertions(
            "a_small_deposit_is_accepted",
            vec![oracle(
                "assert_eq!(deposit_cap(100), Ok(100));",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let other_owner = test_with_assertions(
            "a_large_refund_hits_the_limit",
            vec![oracle(
                "assert!(matches!(refund(20_000), Err(PayError::Limit)));",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        for (family, expression) in [
            (ProbeFamily::ErrorPath, "return Err(PayError::Limit);"),
            (
                ProbeFamily::ErrorPath,
                "return Err::<i64, PayError>(PayError::Limit);",
            ),
            (
                ProbeFamily::ReturnValue,
                "return Err::<i64, PayError>(PayError::Limit);",
            ),
            (
                ProbeFamily::ErrorPath,
                "let v = amount.checked_mul(2).ok_or(PayError::Limit)?;",
            ),
        ] {
            let probe = probe(family, expression);
            let (_, beside, _) = reveal_evidence(
                &probe,
                &[
                    (&reaching, RelationReason::DirectOwnerCall),
                    (&other_owner, RelationReason::SameTestFile),
                ],
            );
            assert_ne!(
                beside.state,
                StageState::Yes,
                "{expression}: {}",
                beside.summary
            );
            assert_eq!(
                beside.summary, PROXIMITY_VARIANT_CONFIRMATION_WITHHELD,
                "{expression}"
            );
            let (_, alone, _) =
                reveal_evidence(&probe, &[(&other_owner, RelationReason::SameTestFile)]);
            assert_eq!(
                alone.state,
                StageState::Yes,
                "{expression}: {}",
                alone.summary
            );

            // A same-file test that may run the owner keeps confirming.
            let (_, may_reach, _, _) = reveal_evidence_with_expression(
                &probe,
                &probe.expression,
                &[
                    (&reaching, RelationReason::DirectOwnerCall),
                    (&other_owner, RelationReason::SameTestFile),
                ],
                &[],
                &|_, _| false,
                &|_, _| false,
                &ReturnOracleAdmission {
                    owner_return_pin: &|_, _| false,
                    assertion_admitted: &|_, _| true,
                    sibling_exclusion_admitted: &|_, _| true,
                    proximity_may_reach_owner: &|test| test.name == "a_large_refund_hits_the_limit",
                    owner_parameters: &[],
                    expected_reaches_owner: &|_, _| false,
                    effect_state_carried: &|_, _| true,
                    setter_writes_field: &|_, _, _| true,
                    getter_reads_field: &|_, _, _| false,
                    getter_reads_key: &|_, _, _| None,
                    owner_argument_names: &[],
                    owner_returns_value: true,
                    constructor_starts_empty: &|_, _, _| false,
                    call_cannot_store: &|_, _, _| false,
                },
                None,
            );
            assert_eq!(
                may_reach.state,
                StageState::Yes,
                "{expression}: {}",
                may_reach.summary
            );
        }

        // A change confined to the payload line of a multiline turbofish
        // leaves only `PayError::Limit,` in the probe expression; the guard
        // reads the enclosing constructor from the analysis expression.
        let payload_probe = probe(ProbeFamily::ErrorPath, "PayError::Limit,");
        let (_, payload_beside, _, _) = reveal_evidence_with_expression(
            &payload_probe,
            "return Err::<i64, PayError>(\n    PayError::Limit,\n);",
            &[
                (&reaching, RelationReason::DirectOwnerCall),
                (&other_owner, RelationReason::SameTestFile),
            ],
            &[],
            &|_, _| false,
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        assert_ne!(
            payload_beside.state,
            StageState::Yes,
            "{}",
            payload_beside.summary
        );
        assert_eq!(
            payload_beside.summary,
            PROXIMITY_VARIANT_CONFIRMATION_WITHHELD
        );

        // A custom `MyErr::<E>(E::X)` constructor names no `Result::Err`
        // variant, so the guard does not apply to it.
        let custom_probe = probe(
            ProbeFamily::ReturnValue,
            "return MyErr::<PayError>(PayError::Limit);",
        );
        let (_, custom_beside, _) = reveal_evidence(
            &custom_probe,
            &[
                (&reaching, RelationReason::DirectOwnerCall),
                (&other_owner, RelationReason::SameTestFile),
            ],
        );
        assert_ne!(
            custom_beside.summary,
            PROXIMITY_VARIANT_CONFIRMATION_WITHHELD
        );

        // A return value that names no error variant keeps the #4486
        // same-file credit beside a reaching test.
        let ok_probe = probe(ProbeFamily::ReturnValue, "return Ok(amount + 1);");
        let same_file_ok = test_with_assertions(
            "refund_adds_one",
            vec![oracle(
                "assert_eq!(refund(1), Ok(amount + 1));",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_, ok_beside, _) = reveal_evidence(
            &ok_probe,
            &[
                (&reaching, RelationReason::DirectOwnerCall),
                (&same_file_ok, RelationReason::SameTestFile),
            ],
        );
        assert_ne!(ok_beside.summary, PROXIMITY_VARIANT_CONFIRMATION_WITHHELD);
        assert_eq!(ok_beside.state, StageState::Yes, "{}", ok_beside.summary);
    }

    /// MatchArm: assertion containing the specific VARIANT token confirms the arm.
    #[test]
    fn match_arm_variant_token_match_keeps_discriminate_yes() {
        // probe expression "Mode::Frozen => -1," — variant "Frozen" appears in assertion.
        let probe = probe(ProbeFamily::MatchArm, "Mode::Frozen => -1,");
        let test = test_with_assertions(
            "frozen_arm_returns_minus_one",
            vec![oracle(
                "assert_eq!(classify(Mode::Frozen), -1);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "Frozen variant in assertion must confirm observation (no over-correction)"
        );
    }

    // --- Predicate must NOT be affected; ErrorPath now requires token/variant confirmation ---

    /// Predicate probes do not require token confirmation and must not be
    /// affected by the observation_unverified logic.
    #[test]
    fn predicate_probe_family_match_only_keeps_discriminate_yes() {
        let probe = probe(ProbeFamily::Predicate, "x > 0");
        let test = test_with_assertions(
            "check_positive",
            vec![oracle(
                "assert!(value >= 3);",
                OracleKind::RelationalCheck,
                OracleStrength::Weak,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        // RelationalCheck → OracleStrength::Weak → StageState::Weak, but NOT
        // via observation_unverified.
        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "Predicate probe must not emit observation_unverified: got `{}`",
            discriminate.summary
        );
    }

    // --- RIPR-SPEC-0107: ErrorPath now requires variant/token confirmation ---

    /// RIPR-SPEC-0107 Control A (REPRO): an ErrorPath probe with ONLY a broad
    /// `is_err()` oracle and a sibling `ExactValue` result (no variant-pinning
    /// oracle) must downgrade to `weakly_exposed` via `observation_unverified`.
    /// This is the fake-clean being fixed: the sibling oracle cannot confirm the
    /// changed error variant is specifically observed.
    #[test]
    fn error_path_broad_oracle_only_downgrades_discriminate_to_weak() {
        let probe = probe(ProbeFamily::ErrorPath, "Err(AuthError::RevokedToken)");
        let test = test_with_assertions(
            "revoked_token_fails",
            vec![oracle(
                "assert!(result.is_err());",
                OracleKind::BroadError,
                OracleStrength::Weak,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "ErrorPath probe with only a broad is_err() oracle must downgrade to Weak (observation_unverified)"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "broad is_err() must emit observation_unverified for ErrorPath: got `{}`",
            discriminate.summary
        );
    }

    /// RIPR-SPEC-0107 Control A continued: an ErrorPath probe with a sibling
    /// `ExactValue` oracle (no variant token in assertion) must also downgrade.
    /// An `assert_eq!(validate_or_default(""), "guest")` oracle credits the
    /// happy-path return value, not the error variant — it must NOT promote
    /// the error_path seam to `exposed`.
    #[test]
    fn error_path_sibling_exact_value_oracle_downgrades_discriminate_to_weak() {
        let probe = probe(ProbeFamily::ErrorPath, "Err(ParseError::TooLong(len))");
        let test = test_with_assertions(
            "default_value_returned",
            vec![oracle(
                "assert_eq!(validate_or_default(\"\"), \"guest\");",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "ErrorPath probe with a sibling ExactValue oracle (no variant token) must downgrade to Weak"
        );
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "sibling ExactValue oracle must emit observation_unverified for ErrorPath: got `{}`",
            discriminate.summary
        );
    }

    /// RIPR-SPEC-0107 Control B (MUST-NOT-OVER-CORRECT): an ErrorPath probe
    /// backed by a real variant-pinning oracle (`assert_eq!(err, ParseError::TooLong(12))`)
    /// must STAY `exposed`. The RIPR-SPEC-0106/#1252 variant-credit path sets
    /// `has_token_match=true` for a genuine `ExactErrorVariant` oracle whose
    /// text contains the probe's specific variant token, clearing
    /// `observation_unverified`.
    #[test]
    fn error_path_exact_variant_oracle_keeps_discriminate_yes() {
        // Probe: Err(ParseError::TooLong(len)) — variant token "TooLong".
        // Assertion: assert_eq!(err, ParseError::TooLong(12)) — contains "TooLong".
        let probe = probe(ProbeFamily::ErrorPath, "Err(ParseError::TooLong(len))");
        let test = test_with_assertions(
            "too_long_error_pinned",
            vec![oracle(
                "assert_eq!(err, ParseError::TooLong(12));",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "ErrorPath probe with a genuine variant-pinning ExactErrorVariant oracle must stay exposed (no over-correction)"
        );
        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "variant-confirmed oracle must NOT emit observation_unverified: got `{}`",
            discriminate.summary
        );
    }

    /// PR #6786 review blocker: an `exact_value` `assert!(matches!(e,
    /// Sibling))` (the line-level fact inside a match arm) shares only the
    /// enum qualifier with a changed `Err(PayError::Insufficient)`. It must
    /// not confirm the changed error path for any variant-carrying family;
    /// the exact-variant spelling of the same assertion still does.
    #[test]
    fn sibling_variant_exact_value_pin_cannot_confirm_a_changed_error_variant() {
        let sibling = test_with_assertions(
            "limit_is_pinned",
            vec![oracle(
                "assert!(matches!(e, PayError::Limit))",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let exact = test_with_assertions(
            "insufficient_is_pinned",
            vec![oracle(
                "assert!(matches!(e, PayError::Insufficient))",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        for family in [
            ProbeFamily::ErrorPath,
            ProbeFamily::ReturnValue,
            ProbeFamily::FieldConstruction,
        ] {
            let probe = probe(family.clone(), "return Err(PayError::Insufficient);");
            let (_observe, discriminate, _related) =
                reveal_evidence(&probe, &[(&sibling, RelationReason::DirectOwnerCall)]);
            assert_ne!(
                discriminate.state,
                StageState::Yes,
                "{family:?}: a PayError::Limit pin must not discriminate PayError::Insufficient: {}",
                discriminate.summary
            );
            let (_observe, discriminate, _related) =
                reveal_evidence(&probe, &[(&exact, RelationReason::DirectOwnerCall)]);
            assert_eq!(
                discriminate.state,
                StageState::Yes,
                "{family:?}: the exact-variant pin stays the positive control: {}",
                discriminate.summary
            );
        }
    }

    /// #6695 fallback (`error_path_variant_path` → `question_mark_error_variant`
    /// through `changed_error_variant`): an `ok_or_else(|| Variant)?` line has
    /// no `Err(` construction, so without the fallback it carried no variant
    /// identity and a sibling `ExactErrorVariant` pin confirmed it through
    /// the shared `CodeError` qualifier.
    #[test]
    fn ok_or_question_mark_line_carries_its_variant_into_the_sibling_gate() {
        const LINE: &str = "let d = digit(c).ok_or_else(|| CodeError::NotDigit)?;";
        assert_eq!(
            error_path_variant_path(LINE).as_deref(),
            Some("CodeError::NotDigit")
        );
        assert_eq!(
            error_path_variant_path("let d = digit(c).ok_or(CodeError::NotDigit)?;").as_deref(),
            Some("CodeError::NotDigit")
        );
        assert_eq!(
            error_path_variant_path("let d = digit(c).ok_or(CodeError::NotDigit);"),
            None
        );
        let probe = probe(ProbeFamily::ErrorPath, LINE);
        let sibling = test_with_assertions(
            "too_long_is_pinned",
            vec![oracle(
                "assert_eq!(err, CodeError::TooLong);",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&sibling, RelationReason::DirectOwnerCall)]);
        assert_ne!(
            discriminate.state,
            StageState::Yes,
            "a CodeError::TooLong pin must not discriminate the ok_or NotDigit line: {}",
            discriminate.summary
        );
        let exact = test_with_assertions(
            "not_digit_is_pinned",
            vec![oracle(
                "assert_eq!(err, CodeError::NotDigit);",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&exact, RelationReason::DirectOwnerCall)]);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "the exact NotDigit pin stays the positive control: {}",
            discriminate.summary
        );
    }

    #[test]
    fn names_only_sibling_variants_requires_a_sibling_path_and_no_changed_path() {
        let check = |text| names_only_sibling_variants(text, "PayError", "Insufficient");
        assert!(check("assert!(matches!(e, PayError::Limit))"));
        assert!(check("assert_eq!(e, crate::PayError :: Limit)"));
        // The changed variant named through the qualifier is never a sibling pin.
        assert!(!check("assert!(matches!(e, PayError::Insufficient))"));
        assert!(!check(
            "assert!(matches!(e, PayError::Limit | PayError::Insufficient))"
        ));
        // No qualified path: not decided by this gate.
        assert!(!check("assert_eq!(withdraw(1, 2).is_err(), true)"));
        assert!(!check("assert!(matches!(e, Limit))"));
        // A longer identifier sharing the qualifier's suffix is not the enum.
        assert!(!check("assert!(matches!(e, OtherPayError::Limit))"));
        // Message text is masked.
        assert!(!check("assert!(e.is_err(), \"PayError::Limit expected\")"));
    }

    #[test]
    fn error_path_parser_context_cannot_replace_emitted_probe_variant() {
        let probe = probe(ProbeFamily::ErrorPath, "Err(ParseError::TooLong(len))");
        let test = test_with_assertions(
            "too_long_is_exact",
            vec![oracle(
                "assert_eq!(err, ParseError::TooLong(12));",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );

        let (_observe, discriminate, _related, _) = reveal_evidence_with_expression(
            &probe,
            "Err(ParseError::SiblingVariant)",
            &[(&test, RelationReason::DirectOwnerCall)],
            &[],
            &|_, _| false,
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters: &[],
                expected_reaches_owner: &|_, _| false,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "the emitted exact variant remains authoritative when parser context resolves a sibling"
        );
        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "variant fallback must preserve exact-error confirmation: got `{}`",
            discriminate.summary
        );
    }

    /// RIPR-SPEC-0107 Control B continued: `matches!(err, ParseError::TooLong(_))`
    /// also pins the variant token and must keep the seam `exposed`.
    #[test]
    fn error_path_matches_variant_oracle_keeps_discriminate_yes() {
        let probe = probe(ProbeFamily::ErrorPath, "Err(ParseError::TooLong(len))");
        let test = test_with_assertions(
            "too_long_error_matches",
            vec![oracle(
                "assert!(matches!(err, ParseError::TooLong(_)));",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);

        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "ErrorPath probe with a matches! variant oracle must stay exposed"
        );
    }

    /// RIPR-SPEC-0107 Control C (CROSS-SURFACE / IS-EFFECT-FAMILY guard):
    /// `is_effect_family` must return false for `ErrorPath`, ensuring that a
    /// mock/snapshot cannot clear `observation_unverified` for an error seam.
    /// Only a genuine variant-pinning oracle may confirm it.
    #[test]
    fn error_path_is_not_effect_family() {
        assert!(
            !is_effect_family(&ProbeFamily::ErrorPath),
            "ErrorPath must not be classified as an effect family (mocks must not clear observation_unverified)"
        );
    }

    // ── Short-token word-boundary matching (#2397) ───────────────────────────

    #[test]
    fn whole_word_match_accepts_short_token_in_field_access() {
        assert!(contains_as_whole_word("assert_eq!(result.id, 42)", "id"));
    }

    #[test]
    fn whole_word_match_accepts_short_token_in_equality() {
        assert!(contains_as_whole_word("assert!(id == 42)", "id"));
    }

    #[test]
    fn whole_word_match_rejects_short_token_as_substring() {
        // The anti-substring property: `id` must NOT match inside `provider`
        // or `middle`. This is what the old `> 3` gate tried to protect
        // against — word-boundary matching achieves it without filtering by
        // length.
        assert!(!contains_as_whole_word("provider", "id"));
        assert!(!contains_as_whole_word("middle", "id"));
        assert!(!contains_as_whole_word("candidate", "id"));
    }

    #[test]
    fn whole_word_match_accepts_long_token() {
        // Long tokens that matched under the old gate still match.
        assert!(contains_as_whole_word(
            "assert_eq!(priority, high)",
            "priority"
        ));
    }

    #[test]
    fn whole_word_match_rejects_empty_token() {
        assert!(!contains_as_whole_word("anything", ""));
    }

    #[test]
    fn whole_word_match_steps_past_non_ascii_token_without_panicking() {
        // A rejected first occurrence of a token that starts with a
        // multibyte char (`new_заказ`) must advance by that char's width,
        // not one byte, or the next `find` slices inside the char.
        assert!(!contains_as_whole_word(
            "assert_eq!(total(new_заказ), 3);",
            "заказ"
        ));
        assert!(contains_as_whole_word("new_заказ + заказ", "заказ"));
    }

    #[test]
    fn mutating_collection_a_while_asserting_b_stays_unverified() {
        let probe = probe(ProbeFamily::SideEffect, "items.push(5)");
        for (name, assertions) in [
            (
                "wrong_collection",
                vec![oracle(
                    "assert_eq!(other, expected);",
                    OracleKind::WholeObjectEquality,
                    OracleStrength::Strong,
                )],
            ),
            (
                "expected_side_token",
                vec![oracle(
                    "assert_eq!(other, items);",
                    OracleKind::WholeObjectEquality,
                    OracleStrength::Strong,
                )],
            ),
            (
                "return_only",
                vec![oracle(
                    "assert_eq!(result, 5);",
                    OracleKind::ExactValue,
                    OracleStrength::Strong,
                )],
            ),
            (
                "callee_name_string",
                vec![oracle(
                    "assert!(label.contains(\"record_items\"));",
                    OracleKind::RelationalCheck,
                    OracleStrength::Weak,
                )],
            ),
            (
                "unrelated_mock",
                vec![oracle(
                    "mock.verify();",
                    OracleKind::MockExpectation,
                    OracleStrength::Medium,
                )],
            ),
        ] {
            let test = test_with_assertions(name, assertions);
            let (_observe, discriminate, _related) =
                reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
            assert_eq!(
                discriminate.state,
                StageState::Weak,
                "{name} must stay weakly discriminating"
            );
            assert!(
                discriminate.summary.contains("observation_unverified"),
                "{name} must not confirm a different observer: got `{}`",
                discriminate.summary
            );
        }
    }

    #[test]
    fn asserting_affected_collection_retains_confirmation_in_either_order() {
        let probe = probe(ProbeFamily::SideEffect, "items.push(5)");
        let actual = oracle(
            "assert_eq!(items, expected);",
            OracleKind::WholeObjectEquality,
            OracleStrength::Strong,
        );
        let wrong = oracle(
            "assert_eq!(other, expected);",
            OracleKind::WholeObjectEquality,
            OracleStrength::Strong,
        );
        for (name, assertions) in [
            ("actual_only", vec![actual.clone()]),
            ("wrong_then_actual", vec![wrong.clone(), actual.clone()]),
            ("actual_then_wrong", vec![actual.clone(), wrong.clone()]),
        ] {
            let test = test_with_assertions(name, assertions);
            let (_observe, discriminate, _related) =
                reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
            assert!(
                !discriminate.summary.contains("observation_unverified"),
                "{name} must retain the actual collection observer: got `{}`",
                discriminate.summary
            );
            assert_eq!(
                discriminate.state,
                StageState::Yes,
                "{name} must keep strong discrimination"
            );
        }

        let removed = test_with_assertions("removed_actual", vec![wrong]);
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&removed, RelationReason::DirectOwnerCall)]);
        assert!(
            discriminate.summary.contains("observation_unverified"),
            "removing the actual observer must fail closed: got `{}`",
            discriminate.summary
        );
    }

    #[test]
    fn sibling_effect_whole_object_without_collection_identity_still_confirms() {
        // Preserve delivered CallDeletion whole-object observer behavior.
        let cache = probe(
            ProbeFamily::CallDeletion,
            "cache.insert(\"result_key\", result)",
        );
        let cache_test = test_with_assertions(
            "store_result_inserts_result_key_with_value",
            vec![oracle(
                "assert_eq!(cache.inserted, vec![\"result_key=42\".to_string()]);",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&cache, &[(&cache_test, RelationReason::DirectOwnerCall)]);
        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "delivered cache.insert confirmation must stay on Part C: got `{}`",
            discriminate.summary
        );

        let probe = probe(ProbeFamily::CallDeletion, "persist_audit(record)");
        let test = test_with_assertions(
            "store_matches_expected",
            vec![oracle(
                "assert_eq!(store, expected);",
                OracleKind::WholeObjectEquality,
                OracleStrength::Strong,
            )],
        );
        let (_observe, discriminate, _related) =
            reveal_evidence(&probe, &[(&test, RelationReason::DirectOwnerCall)]);
        assert!(
            !discriminate.summary.contains("observation_unverified"),
            "non-collection effect observers stay on the existing Part C path: got `{}`",
            discriminate.summary
        );
    }

    /// RIPR-SPEC-0240: a name-only relation stops counting once a
    /// reach-bearing relation exists, so its refused assertion cannot decide
    /// whether a gap is withheld; alone, it still counts.
    #[test]
    fn name_only_relations_stop_crediting_beside_a_reach_bearing_one() {
        let direct = test_with_assertions("direct", Vec::new());
        let named = test_with_assertions("named", Vec::new());
        let mixed = [
            (&direct, RelationReason::DirectOwnerCall),
            (&named, RelationReason::OwnerNamedTest),
        ];
        let credits = oracle_crediting_relations(&mixed);
        assert!(credits(RelationReason::DirectOwnerCall));
        assert!(!credits(RelationReason::OwnerNamedTest));
        assert!(!credits(RelationReason::WeakTokenSubstring));

        let name_only = [(&named, RelationReason::OwnerNamedTest)];
        let credits = oracle_crediting_relations(&name_only);
        assert!(credits(RelationReason::OwnerNamedTest));

        let proximity_only = [
            (&direct, RelationReason::SameTestFile),
            (&named, RelationReason::OwnerNamedTest),
        ];
        let credits = oracle_crediting_relations(&proximity_only);
        assert!(credits(RelationReason::OwnerNamedTest));
    }

    /// Runs reveal for a `tax(subtotal)` return-value probe with the owner's
    /// parameter list and a caller set, and returns the discriminate stage
    /// plus the first related row's strength.
    fn tax_reveal(
        expression: &str,
        test: &TestSummary,
        owner_parameters: &[String],
        reaches: &dyn Fn(Option<&str>, &str) -> bool,
    ) -> (StageEvidence, OracleStrength) {
        let probe = owned_probe(ProbeFamily::ReturnValue, expression, "tax");
        let (_, discriminate, related, _) = reveal_evidence_with_expression(
            &probe,
            &probe.expression,
            &[(test, RelationReason::DirectOwnerCall)],
            &[],
            &|_, _| false,
            &|_, _| false,
            &ReturnOracleAdmission {
                owner_return_pin: &|_, _| false,
                assertion_admitted: &|_, _| true,
                sibling_exclusion_admitted: &|_, _| true,
                proximity_may_reach_owner: &|_| false,
                owner_parameters,
                expected_reaches_owner: reaches,
                effect_state_carried: &|_, _| true,
                setter_writes_field: &|_, _, _| true,
                getter_reads_field: &|_, _, _| false,
                getter_reads_key: &|_, _, _| None,
                owner_argument_names: &[],
                owner_returns_value: true,
                constructor_starts_empty: &|_, _, _| false,
                call_cannot_store: &|_, _, _| false,
            },
            None,
        );
        let strength = related
            .first()
            .map_or(OracleStrength::None, |row| row.oracle_strength.clone());
        (discriminate, strength)
    }

    fn exact(text: &str) -> OracleFact {
        oracle(text, OracleKind::ExactValue, OracleStrength::Strong)
    }

    /// #5830: `subtotal` names `tax`'s parameter. A test of the unrelated
    /// `subtotal` function shares the word, not the binding, so it cannot
    /// confirm that it observes `tax`'s return value.
    #[test]
    fn owner_parameter_name_confirms_only_in_an_assertion_calling_the_owner() {
        let params = vec!["subtotal".to_string()];
        let unrelated = test_with_assertions(
            "subtotal_multiplies",
            vec![exact("assert_eq!(subtotal(3, 5), 15);")],
        );
        let (discriminate, _) =
            tax_reveal("subtotal * 8 / 1000", &unrelated, &params, &|_, _| false);
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "{}",
            discriminate.summary
        );

        let calling = test_with_assertions(
            "tax_of_subtotal",
            vec![exact("assert_eq!(tax(subtotal), 24);")],
        );
        let (discriminate, _) = tax_reveal("subtotal * 8 / 1000", &calling, &params, &|_, _| false);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "{}",
            discriminate.summary
        );

        // A test `let` bound from an owner call holds the owner's result,
        // so naming it confirms whatever the binding is called.
        let bound = test_with_body_assertions(
            "bound_tax",
            "let subtotal = tax(300);\nassert_eq!(subtotal, 24);",
            vec![exact("assert_eq!(subtotal, 24);")],
        );
        let (discriminate, _) = tax_reveal("subtotal * 8 / 1000", &bound, &params, &|_, _| false);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "{}",
            discriminate.summary
        );

        // Without the parameter list the old token rule still confirms, which
        // is what the first case relied on before #5830.
        let (discriminate, _) = tax_reveal("subtotal * 8 / 1000", &unrelated, &[], &|_, _| false);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "{}",
            discriminate.summary
        );
    }

    /// #5830: a numeric literal in the changed expression matches any test
    /// input of the same number; only an assertion calling the owner counts.
    #[test]
    fn numeric_literal_token_confirms_only_in_an_assertion_calling_the_owner() {
        let unrelated = test_with_assertions(
            "subtotal_multiplies",
            vec![exact("assert_eq!(subtotal(3, 100), 300);")],
        );
        let (discriminate, _) = tax_reveal("amount * 8 / 100", &unrelated, &[], &|_, _| false);
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "{}",
            discriminate.summary
        );

        let calling = test_with_assertions("tax_of_100", vec![exact("assert_eq!(tax(100), 8);")]);
        let (discriminate, _) = tax_reveal("amount * 8 / 100", &calling, &[], &|_, _| false);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "{}",
            discriminate.summary
        );
    }

    /// RIPR-SPEC-0035 / #5830: the expected side `subtotal + tax(subtotal)`
    /// calls the owner and the actual side `invoice(..)` reaches it, so both
    /// sides move together and the equality pins nothing about `tax`.
    #[test]
    fn self_computed_expected_value_is_weak_and_unconfirmed() {
        let params = vec!["subtotal".to_string()];
        let test = test_with_assertions(
            "invoice_adds_tax",
            vec![exact(
                "assert_eq!(invoice(3, 100), subtotal + tax(subtotal));",
            )],
        );
        let (discriminate, strength) =
            tax_reveal("subtotal * 8 / 1000", &test, &params, &|_, name| {
                name == "invoice"
            });
        assert_eq!(strength, OracleStrength::Weak);
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "{}",
            discriminate.summary
        );

        // The same assertion against an `invoice` that never calls `tax`
        // compares tax with an independent value and keeps its credit.
        let (discriminate, strength) =
            tax_reveal("subtotal * 8 / 1000", &test, &params, &|_, _| false);
        assert_eq!(strength, OracleStrength::Strong);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "{}",
            discriminate.summary
        );
    }

    #[test]
    fn expected_computed_through_owner_reads_either_side_and_masks_strings() {
        let reaches = |_: Option<&str>, name: &str| name == "invoice";
        assert!(expected_computed_through_owner(
            "assert_eq!(sub + tax(sub), invoice(3, 100));",
            "tax",
            &reaches
        ));
        assert!(expected_computed_through_owner(
            "assert_eq!(cart.invoice(3), tax(300) + 300);",
            "tax",
            &reaches
        ));
        // A literal expected value is independent of the owner.
        assert!(!expected_computed_through_owner(
            "assert_eq!(tax(300), 24);",
            "tax",
            &reaches
        ));
        // `tax(` inside a message string is not a call.
        assert!(!expected_computed_through_owner(
            "assert_eq!(invoice(3, 100), 324, \"tax(sub)\");",
            "tax",
            &reaches
        ));
        // The same owner call on both sides holds whatever it returns.
        assert!(expected_computed_through_owner(
            "assert_eq!(tax(250), tax(250));",
            "tax",
            &|_: Option<&str>, _: &str| false
        ));
        // Different owner-dependent expressions can pin the owner's value:
        // this passes only when `tax(250)` is 8.
        assert!(!expected_computed_through_owner(
            "assert_eq!(tax(250) * 2, tax(250) + 8);",
            "tax",
            &|_: Option<&str>, _: &str| false
        ));
        // Reordering, grouping and a path prefix leave the sides equal.
        for text in [
            "assert_eq!(tax(250) * 2, 2 * tax(250));",
            "assert_eq!((tax(250)), tax(250));",
            "assert_eq!(crate::tax(250), tax(250u64));",
            "assert_eq!(tax(250) /* same */, tax(250));",
        ] {
            assert!(
                expected_computed_through_owner(text, "tax", &|_: Option<&str>, _: &str| false),
                "{text}"
            );
        }
        // Different code around the owner keeps strength.
        assert!(!expected_computed_through_owner(
            "assert_eq!(tax(250) + base, tax(250) + other);",
            "tax",
            &|_: Option<&str>, _: &str| false
        ));
        // Inequality is never strong in the first place.
        assert!(!expected_computed_through_owner(
            "assert_ne!(invoice(3, 100), tax(300));",
            "tax",
            &reaches
        ));
        // RIPR-SPEC-0197: `assert!(a == b)` and an Err-return guard twin
        // compare the same operands an `assert_eq!` would.
        for text in [
            "assert!(sub + tax(sub) == invoice(3, 100));",
            "if invoice(3, 100) != sub + tax(sub) { return Err(..) }",
            // An `assert_eq!` spelled in a message, comment or guard body
            // does not switch the reader away from the compared operands.
            "assert!(tax(300) == invoice(3, 100) - 300, \"assert_eq! equivalent\");",
            "if tax(300) != invoice(3, 100) - 300 {\n    // was assert_eq!(tax(300), 24)\n    return Err(..)\n}",
        ] {
            assert!(
                expected_computed_through_owner(text, "tax", &reaches),
                "{text}"
            );
        }
        for text in [
            "assert!(tax(300) == 24);",
            "if tax(300) != 24 { return Err(..) }",
            "assert!(tax(300) >= invoice(3, 100));",
        ] {
            assert!(
                !expected_computed_through_owner(text, "tax", &reaches),
                "{text}"
            );
        }
        assert_eq!(constructed_field_name("storage,"), Some("storage"));
        assert_eq!(
            constructed_field_name("total_cents: shipping + subtotal,"),
            Some("total_cents")
        );
        assert_eq!(constructed_field_name("Storage::Local"), None);
        assert_eq!(
            called_names("a.tax(1) + vec![x] + Tax::new::<u8>(2) + taxes (3)"),
            vec!["tax", "new", "taxes"]
        );
    }

    /// #5830 review: a type-qualified expected-side call (`Money::new(8)`)
    /// reaches the owner only through that type's function. A same-named
    /// `Invoice::new` that calls the owner leaves a genuine pin strong.
    #[test]
    fn a_type_qualified_expected_call_reaches_only_through_its_own_type() {
        let reaches =
            |ty: Option<&str>, name: &str| name == "new" && ty.is_none_or(|ty| ty == "Invoice");
        assert!(!expected_computed_through_owner(
            "assert_eq!(tax(100), Money::new(8));",
            "tax",
            &reaches
        ));
        assert!(expected_computed_through_owner(
            "assert_eq!(tax(100), Invoice::new(100).tax);",
            "tax",
            &reaches
        ));
        // A method or `Self::` call names no type, so it matches by name.
        assert!(expected_computed_through_owner(
            "assert_eq!(tax(100), cart.new(100));",
            "tax",
            &reaches
        ));
        assert_eq!(
            called_paths("Money::new(8) + crate::fees::Invoice::new(1) + Self::new(2) + new(3)"),
            vec![
                (Some("Money".to_string()), "new".to_string()),
                (Some("Invoice".to_string()), "new".to_string()),
                (None, "new".to_string()),
                (None, "new".to_string()),
            ]
        );
    }

    /// #7062: a non-ASCII module or function name used to abort
    /// `called_paths` (`rfind(..) + 1` inside `ó`) or be read as the
    /// ASCII tail (`dulo`). ASCII paths are the no-change control.
    #[test]
    fn called_paths_reads_non_ascii_identifiers_on_char_boundaries() {
        assert_eq!(
            called_paths("crate::módulo::render(2)"),
            vec![(None, "render".to_string())]
        );
        assert_eq!(
            called_names("módulo(2) + función(1)"),
            vec!["módulo".to_string(), "función".to_string()]
        );
        assert_eq!(
            called_paths("Módulo::new(8)"),
            vec![(Some("Módulo".to_string()), "new".to_string())]
        );
        assert_eq!(
            called_paths("crate::a::render(2) + Money::new(8)"),
            vec![
                (None, "render".to_string()),
                (Some("Money".to_string()), "new".to_string()),
            ]
        );
    }
}
