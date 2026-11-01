//! Task-first repair selection services (#6305).
//!
//! The `ripr repair` / `ripr continue` façade decides here; the CLI adapter
//! only parses arguments and renders the outcome. Item discovery reuses the
//! classified-seam inventory and the repair-packet eligibility flip, the
//! start/stop decision goes through the shared #6304 selector, and attempt
//! selection reuses the attempt inventory. Nothing here mints lifecycle,
//! currentness, cage, or receipt facts: the attempt authority keeps those.

use std::path::Path;

use crate::agent::artifact::current_git_head;
use crate::agent::command_specs::repair_start_command_spec;
use crate::agent::loop_commands::{bound_root, shell_arg};
use crate::analysis::canonical_gap::{
    canonical_gap_identity, probe_id_names_seam, producer_gap_id_names_seam,
};
use crate::analysis::repair_route::{
    RepairPacketIneligibility, humanize_evidence, repair_packet_eligibility,
};
use crate::app::agent_brief::AgentBriefPolicy;
use crate::app::agent_status::{AgentAttemptStatusReport, build_agent_attempt_status};
use crate::app::repair_attempt::{
    RepairAttemptId, RepairAttemptInventoryEntry, RepairAttemptManifest, RepairAttemptState,
    find_manifest_artifact_by_role, inventory_repair_attempts_from,
    load_repair_attempt_manifest_from,
};
use crate::app::repair_card::command_effect_label;
use crate::config::load_for_root;
use crate::domain::{
    CanonicalNextActionV1, CommandRole, CommandSpec, NextActionClass, NextActionCurrentness,
    NextActionDiffSource, NextActionInput, NextActionProducer, NextActionStop,
    current_command_platform, select_canonical_next_action,
};

/// What `ripr repair` without an item resolved to: the single seam to
/// start, the canonical action naming what to do instead, or the honest
/// no-eligible-seam outcome. The last case is producer-owned prose rather
/// than a canonical action: with no suitable subject the selector's
/// anti-invention law leaves nothing to bind.
pub(crate) enum RepairStartDecision {
    Start {
        seam_id: String,
    },
    /// The canonical action, plus one plain description per listed candidate
    /// (file:line, the changed code and what a test must check) when the
    /// action is a selection, so the stop never shows bare ids (#10978) or two
    /// candidates that read the same (#11111).
    Action(Box<CanonicalNextActionV1>, Vec<RepairCandidateNote>),
    NoEligible {
        total_seams: usize,
        /// What the outcome says about the change itself, so the closing
        /// next step agrees with `ripr check`.
        change: NoEligibleChange,
        /// Distinct ineligibility reasons rendered in the bounded list.
        reasons: Vec<String>,
        /// The total count of distinct ineligibility reasons across the
        /// visible seams (#8728). Greater than `reasons.len()` when the
        /// bounded list was truncated; the renderer discloses the
        /// remainder instead of presenting three as the whole story.
        reasons_total: usize,
    },
}

/// What a no-eligible outcome says about the change a bare `ripr check` would
/// read, so the closing next step agrees with check instead of one generic
/// route (#11111 burn-down).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NoEligibleChange {
    /// Every spot on the changed lines is already checked by existing tests:
    /// check reports nothing to repair.
    AlreadyChecked,
    /// The changed Rust lines touch no code a repair can start on.
    NoRepairableCode,
    /// An open spot on the change cannot start, or no diff was read.
    Other,
}

/// Producer-owned facts for one repair-start decision. [`decide_repair_start`]
/// takes these directly so the decision law is unit-testable without a
/// checkout; [`resolve_repair_start`] gathers them from the live authorities.
pub(crate) struct RepairStartFacts<'a> {
    /// The caller-bound root display; the selector copies it verbatim.
    pub(crate) root_display: &'a str,
    /// Eligible seam IDs in inventory order.
    pub(crate) eligible_ids: &'a [String],
    /// Visible seams overall, eligible or not.
    pub(crate) total_seams: usize,
    /// Distinct ineligibility reasons across the visible seams, bounded by
    /// the gatherer. Empty when every visible seam is eligible.
    pub(crate) ineligible_reasons: &'a [String],
    /// The total count of distinct ineligibility reasons across the
    /// visible seams, tracked alongside the bounded list so a truncated
    /// list can disclose its remainder (#8728).
    pub(crate) ineligible_reasons_total: usize,
    /// The current HEAD, when it could be read.
    pub(crate) head: Option<&'a str>,
    /// One plain description per eligible seam, aligned with `eligible_ids`
    /// (empty when the gatherer has none): where the change is and what it is.
    pub(crate) candidate_notes: &'a [RepairCandidateNote],
    /// The before-phase command for the single candidate, if any. Ignored
    /// unless exactly one seam is eligible.
    pub(crate) start_spec: Option<&'a CommandSpec>,
    /// What a no-eligible outcome says about the change.
    pub(crate) change: NoEligibleChange,
}

/// Decide whether `ripr repair` without an item may start. Exactly one
/// eligible seam with a fresh HEAD offers the start command; several yield
/// a bounded selection through the shared selector, which owns the class
/// in both cases; zero yields the honest producer-owned outcome below.
pub(crate) fn decide_repair_start(
    facts: &RepairStartFacts<'_>,
) -> Result<RepairStartDecision, String> {
    if facts.eligible_ids.is_empty() {
        return Ok(RepairStartDecision::NoEligible {
            total_seams: facts.total_seams,
            change: facts.change,
            reasons: facts.ineligible_reasons.to_vec(),
            reasons_total: facts.ineligible_reasons_total,
        });
    }
    let single = facts.eligible_ids.len() == 1;
    let (item_id, candidates, offered) = if single {
        (facts.eligible_ids[0].clone(), Vec::new(), facts.start_spec)
    } else {
        (String::new(), facts.eligible_ids.to_vec(), None)
    };
    let detail_route = format!("ripr pilot --root {}", shell_arg(facts.root_display));
    let restart_route = format!("ripr repair --root {}", shell_arg(facts.root_display));
    let transition_from = if single {
        "one_eligible_seam"
    } else {
        "several_eligible_seams"
    };
    let head = facts.head.map(str::to_string);
    let action = select_canonical_next_action(&NextActionInput {
        producer: NextActionProducer::RepairStart,
        root: facts.root_display.to_string(),
        // The inventory walks the live tree, so the decision's evidence
        // source is the working tree at the observed HEAD.
        diff_source: NextActionDiffSource::WorkingTree { head: head.clone() },
        item_id,
        check_item: None,
        card_item: None,
        item_candidates: candidates,
        attempts: Vec::new(),
        // The evidence was just gathered at this HEAD: expected and observed
        // bind together, like the card's snapshot. An unreadable HEAD stays
        // unbound and the executable tail refuses honestly.
        currentness: NextActionCurrentness {
            head_expected: head.clone(),
            head_observed: head,
            config_expected: None,
            config_observed: None,
        },
        offered_command: offered,
        // The eligibility flip admitted the single candidate; the before
        // driver adjudicates finally at run time.
        route_admitted: true,
        route_refusal: None,
        missing_input: None,
        platform: current_command_platform(),
        limitation: None,
        limitation_route: None,
        detail_route,
        transition_from: transition_from.to_string(),
        transition_to: offered.map(|_| command_effect_label(CommandRole::RepairStart).to_string()),
        restart_route,
        check_case: None,
        doctor_recovery: None,
        pilot_delegation: None,
        alternatives: Vec::new(),
        limitations: Vec::new(),
    })?;
    if action.action_class() == NextActionClass::RunCommand {
        let seam_id = action
            .subject()
            .item
            .clone()
            .ok_or_else(|| "repair-start decision bound no subject; this is a ripr bug, report it with the command you ran".to_string())?;
        return Ok(RepairStartDecision::Start { seam_id });
    }
    // Describe exactly the candidates the stop lists, in the stop's order; its
    // own total and remainder disclosure covers the rest.
    let stop_ids: &[String] = match action.stop() {
        Some(NextActionStop::SelectItem { candidates, .. }) => candidates,
        _ => &[],
    };
    let described = notes_for_stop_ids(stop_ids, facts.eligible_ids, facts.candidate_notes);
    Ok(RepairStartDecision::Action(Box::new(action), described))
}

/// The description of each id the stop lists, looked up by id in the eligible
/// inventory (`notes` is aligned with `eligible_ids`) and kept in the stop's
/// order, so a description can never belong to a different candidate than the
/// id beside it. An id with no description is skipped.
fn notes_for_stop_ids<T: Clone>(
    stop_ids: &[String],
    eligible_ids: &[String],
    notes: &[T],
) -> Vec<T> {
    stop_ids
        .iter()
        .filter_map(|id| {
            let position = eligible_ids.iter().position(|eligible| eligible == id)?;
            notes.get(position).cloned()
        })
        .collect()
}

/// What kind of refusal the repair gate gave, so a caller never explains an
/// already-gripped seam with advice meant for a weak one (#8766).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RefusalKind {
    /// The seam's existing tests already check it on current static evidence.
    AlreadyGripped,
    /// The repair route is not ready for an automated start; the remaining
    /// work is strengthening the related test by hand.
    RouteNotReady,
    /// Any other refusal (policy omission, cross-language limits, a class
    /// that is not a headline target, no editable test target, ...): the
    /// reason is stated as is, with no claim about what to do by hand.
    Other,
    /// The gate could not decide.
    Unverified,
}

/// The workspace a refusal is worded for: where its sources live and whether
/// an emitted next command names it with `--root`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RefusalRoot<'a> {
    source: Option<&'a Path>,
    name_in_commands: bool,
}

impl<'a> RefusalRoot<'a> {
    /// The CLI: read sources under `root` and name it in next commands.
    pub(crate) fn named(root: &'a Path) -> Self {
        Self {
            source: Some(root),
            name_in_commands: true,
        }
    }

    /// The MCP server: read sources under `root`, but next commands already
    /// run relative to it, so no `--root` is worded into them.
    pub(crate) fn implicit(root: Option<&'a Path>) -> Self {
        Self {
            source: root,
            name_in_commands: false,
        }
    }

    fn command_root(self) -> Option<&'a Path> {
        self.source.filter(|_| self.name_in_commands)
    }
}

/// The one repair-admission gate, shared by the auto selector and the
/// explicit-subject resolver: a seam may start only when the producer-owned
/// packet flip is eligible and the brief policy does not omit its class.
/// Returns the refusal kind and the reason naming why the seam may not start
/// (the eligibility label, or the policy omission when the flip is eligible),
/// with the eligibility reason taking precedence over the omission.
fn repair_refusal(
    entry: &crate::analysis::ClassifiedSeam,
    policy: AgentBriefPolicy<'_>,
    root: RefusalRoot<'_>,
) -> Option<(RefusalKind, String)> {
    let eligibility = repair_packet_eligibility(entry);
    if !eligibility.eligible() {
        return eligibility.ineligibility.map(|reason| {
            let label = ineligibility_label(reason);
            // #8766: an already-gripped seam has nothing left to repair;
            // say so instead of the bare "not a headline repair target".
            if reason == RepairPacketIneligibility::ClassNotHeadlineEligible
                && entry.class == crate::analysis::seams::SeamGripClass::StronglyGripped
            {
                (RefusalKind::AlreadyGripped, GRIPPED_SPOT_REASON.to_string())
            } else if reason == RepairPacketIneligibility::ClassNotHeadlineEligible
                && let Some(guidance) = opaque_reach_guidance(entry)
            {
                (RefusalKind::Other, guidance)
            } else if reason == RepairPacketIneligibility::RouteNotReady {
                // Name the evidence the route still needs, not just the gate
                // verdict: "repair route not ready" alone sends the reader
                // hunting for what to strengthen (#9604). The parenthetical
                // matches the agent card's readiness line so both surfaces
                // read the same way.
                // #10683: when a related test already reaches the boundary and
                // only its assertion is weak, reuse `check`'s wording and point
                // at that test: no internal evidence jargon, no new test.
                if let [only] = eligibility.readiness.missing_evidence.as_slice()
                    && is_boundary_value_discriminator(only)
                    && let Some(guidance) = weak_oracle_guidance(entry)
                {
                    return (RefusalKind::RouteNotReady, format!("{label}: {guidance}"));
                }
                // #10644: the refusal reads on a human surface, so name each
                // missing fact in plain words; the machine readiness strings
                // behind it stay internal.
                let label = route_not_ready_reason(
                    entry,
                    label,
                    &eligibility.readiness.missing_evidence,
                    root,
                );
                (RefusalKind::RouteNotReady, label)
            } else if reason == RepairPacketIneligibility::ClassNotHeadlineEligible
                && entry.class == crate::analysis::seams::SeamGripClass::Opaque
            {
                // An opaque seam: name what ripr could not establish instead
                // of "this kind of finding". The `*_unknown` classes are
                // headline classes, so they never reach this branch.
                (RefusalKind::Other, OPAQUE_SPOT_REASON.to_string())
            } else {
                (RefusalKind::Other, label.to_string())
            }
        });
    }
    policy
        .omission_reason_for_class(entry.class)
        .map(|reason| (RefusalKind::Other, reason))
}

/// The refusal for an opaque seam with no candidate test to name: ripr could
/// not see how any test reaches or observes the changed code, so it cannot say
/// what a test should check. `opaque_reach_guidance` opens with the same words
/// when it can name a candidate, so both opaque refusals read as one sentence.
const OPAQUE_SPOT_REASON: &str = "ripr does not start an automatic repair here because it could not establish how any test reaches or observes this code, so it cannot say what a test should check; write the test by hand";

/// What to do by hand when the class is opaque because ripr cannot confirm
/// that any test reaches the changed code: name the candidate test it found,
/// so the refusal is a next step and not a bare "no automatic repair". Wording
/// only; the class and the eligibility flip are unchanged.
fn opaque_reach_guidance(entry: &crate::analysis::ClassifiedSeam) -> Option<String> {
    use crate::domain::StageState;
    if entry.class != crate::analysis::seams::SeamGripClass::Opaque
        || !matches!(
            entry.evidence.reach.state,
            StageState::Opaque | StageState::Weak
        )
    {
        return None;
    }
    let summary = entry.evidence.reach.summary.as_str();
    // Only the transitive-reach sentence ("`t` (f:l) calls `x`, which may lead
    // to `o` through a call path ripr does not fully trace; no gap is
    // reported") reads as a candidate once its trailer is cut; macro and
    // trait-dispatch summaries fall back to a related test instead of leaking
    // their own wording.
    let transitive = summary
        .strip_prefix("Reach unresolved (")
        .and_then(|rest| rest.split_once("): "))
        .map(|(_, rest)| rest.split_once("; no gap").map_or(rest, |(head, _)| head))
        .filter(|rest| rest.starts_with('`') && rest.contains(", which may lead to `"));
    let candidate = if let Some(rest) = transitive {
        format!("ripr found {rest}")
    } else if let Some(test) = entry.evidence.related_tests.first() {
        format!(
            "a related test is `{}` ({}:{}), but ripr cannot show that it runs this code",
            test.test_name,
            crate::analysis::stable_path_text(&test.file),
            test.line
        )
    } else {
        // No related test to name (#11466): trait-dispatch and macro summaries
        // still say why the path is unresolved and which test mentions the
        // type, so quote that detail, cut before ripr's own "no gap" tail.
        let detail = summary
            .strip_prefix("Reach unresolved (")
            .and_then(|rest| rest.split_once("): "))
            .map(|(_, rest)| rest)?;
        let detail = ["; no gap", ". ripr does not", "; ripr does not"]
            .iter()
            .filter_map(|tail| detail.find(tail))
            .min()
            .map_or(detail, |end| &detail[..end]);
        format!("ripr found that {detail}")
    };
    Some(format!(
        "ripr does not start an automatic repair here because it could not establish which test runs this code: {candidate}. Check by hand that a test runs the changed code and asserts on its result, and add a test only if none does"
    ))
}

/// The route-not-ready refusal wording: the plain-language missing evidence
/// plus any concrete manual route (predicate or return-value stub).
fn route_not_ready_reason(
    entry: &crate::analysis::ClassifiedSeam,
    label: &str,
    missing_evidence: &[String],
    root: RefusalRoot<'_>,
) -> String {
    let missing = missing_evidence
        .iter()
        .map(|evidence| humanize_evidence(evidence))
        .collect::<Vec<_>>()
        .join("; ");
    let mut label = if missing.is_empty() {
        label.to_string()
    } else {
        format!("{label} (missing evidence: {missing})")
    };
    // A file with no test module blocks the producer's own new-test proposal,
    // but `ripr agent stub` adds one, so the target facts do not keep the
    // manual routes below from applying: only the discriminator fact does.
    let routable = stub_routable_missing_evidence(entry, missing_evidence);
    let adds_module = routable.is_some();
    let missing_evidence = routable.as_deref().unwrap_or(missing_evidence);
    // The skeleton's call to the owner is the missing test input when no test
    // calls the owner at all, so that gap does not keep the stub route back.
    let without_owner_gap = owner_call_gap_covered_by_stub(entry, missing_evidence);
    // Nothing else missing once the module blocker or the owner-call gap is
    // routed means only the test home or the call is absent, and the stub
    // supplies it (#11466, #11895).
    let stub_supplies_rest = adds_module || without_owner_gap.is_some();
    let missing_evidence = without_owner_gap.as_deref().unwrap_or(missing_evidence);
    // #10622: a Rust predicate boundary whose only missing fact
    // is the boundary discriminator has a concrete manual route —
    // the predicate stub writes a fill-in skeleton for the changed
    // comparison, prefilling the boundary input when it derives
    // (a bare parameter against a literal; compound operands
    // still leave todo!() values). Name the route instead of
    // leaving the reader at the jargon dead end; when anything
    // else is also missing the stub alone would not ready the
    // route, so the bare label stays. The seam inventory this
    // gate reads is Rust-only, so the kind check is also the
    // language check.
    // No missing fact left once the module blocker or the owner-call gap is
    // routed means the seam already has its boundary input (#11466, #11895):
    // only the test home or the call is absent, and the stub supplies it, so
    // the predicate route applies there too.
    let boundary_route = match missing_evidence {
        [missing] => {
            missing.starts_with("producer-owned missing discriminator fact: boundary_value")
        }
        [] => stub_supplies_rest,
        _ => false,
    };
    if boundary_route && entry.seam.kind() == crate::analysis::seams::SeamKind::PredicateBoundary {
        // The stub command refuses some owners: name the stub's own refusal
        // instead of offering a command that would refuse (#11111 burn-down).
        if let Some(why) = stub_refusal_reason(entry, root) {
            label.push_str(&format!(
                ": write the boundary test by hand; ripr cannot write a fill-in skeleton here because {why}"
            ));
            return label;
        }
        let file = crate::analysis::stable_path_text(entry.seam.file());
        let at = shell_arg(&format!("{file}:{}", entry.seam.display_line()));
        let root_flag = root.command_root().map_or_else(String::new, |root| {
            format!(
                " --root {}",
                shell_arg(&bound_root(&root.to_string_lossy()))
            )
        });
        let module_note = if adds_module {
            NO_TEST_MODULE_STUB_NOTE
        } else {
            ""
        };
        label.push_str(&format!(
            ": write the boundary test by hand — `ripr agent stub{root_flag} --at {at} --kind predicate` writes a fill-in skeleton for the changed comparison{module_note}; the boundary input is prefilled when it derives, otherwise fill the todo!() values, then rerun `ripr repair`"
        ));
    }
    // #10701: a changed return value or error with no test ripr can
    // safely strengthen has a working manual route too.
    // Once the module blocker is routed and nothing else is missing (#11466),
    // only the test home is absent and the stub adds it, so the seam kind
    // alone picks the stub kind.
    let manual_stub_kind = match missing_evidence {
        [missing] => crate::analysis::repair_route::stub_kind_for_missing_evidence(missing),
        [] if stub_supplies_rest => match entry.seam.kind() {
            crate::analysis::seams::SeamKind::ReturnValue => Some("return_value"),
            crate::analysis::seams::SeamKind::ErrorVariant => Some("error_path"),
            _ => None,
        },
        _ => None,
    };
    if let Some(kind) = manual_stub_kind
        && matches!(
            (entry.seam.kind(), kind),
            (
                crate::analysis::seams::SeamKind::ReturnValue,
                "return_value"
            ) | (crate::analysis::seams::SeamKind::ErrorVariant, "error_path")
        )
    {
        // The stub command refuses some owners (async, generic, ...): name
        // the stub's own refusal reason instead of offering a command that
        // would refuse (#11111 burn-down).
        if let Some(why) = stub_refusal_reason(entry, root) {
            label.push_str(&format!(
                ": write the test by hand; ripr cannot write a fill-in skeleton here because {why}"
            ));
            return label;
        }
        let file = crate::analysis::stable_path_text(entry.seam.file());
        let at = shell_arg(&format!("{file}:{}", entry.seam.display_line()));
        let root_flag = root.command_root().map_or_else(String::new, |root| {
            format!(
                " --root {}",
                shell_arg(&bound_root(&root.to_string_lossy()))
            )
        });
        let module_note = if adds_module {
            NO_TEST_MODULE_STUB_NOTE
        } else {
            ""
        };
        label.push_str(&format!(
            ": write the test by hand, or run `ripr agent stub{root_flag} --at {at} --kind {kind}` for a fill-in skeleton{module_note}, then rerun `ripr repair`"
        ));
    }
    // A named stub route already answers "no test calls this"; a second
    // suffix would read as competing advice.
    if !label.contains("ripr agent stub")
        && let Some(guidance) =
            owner_call_absent_guidance(entry, root).or_else(|| reaching_test_guidance(entry, root))
    {
        label.push_str(&format!(": {guidance}"));
    }
    label
}

/// A test already reaches the changed code but ripr cannot follow the changed
/// value to an assertion on it (`*_unknown` class past activation): name that
/// test so the reader knows where to strengthen, instead of listing missing
/// evidence alone (#11466). An activation-unknown seam is named too, but only
/// through a direct owner call: it reaches the owner, yet ripr cannot show its
/// input reaches the changed branch (#11895). Refusal text only; class and
/// eligibility are untouched.
fn reaching_test_guidance(
    entry: &crate::analysis::ClassifiedSeam,
    root: RefusalRoot<'_>,
) -> Option<String> {
    use crate::analysis::seams::SeamGripClass;
    use crate::domain::RelationReason;
    // Activation unknown with a test that calls the owner directly: the test
    // reaches the function but ripr cannot show its input reaches the changed
    // branch (#11466 follow-up). Only a direct owner call counts there, since
    // activation is exactly what a weaker relation fails to establish.
    if entry.class == SeamGripClass::ActivationUnknown {
        let test = entry
            .evidence
            .related_tests
            .iter()
            .find(|test| test.relation_reason == RelationReason::DirectOwnerCall)?;
        let file = root
            .source
            .and_then(|root| test.file.strip_prefix(root).ok())
            .unwrap_or(&test.file);
        return Some(format!(
            "`{}` ({}:{}) calls this code, but ripr cannot show that its input reaches the changed branch. Check by hand that it does and asserts on the result there, and add a test input only if the existing one does not",
            test.test_name,
            crate::analysis::stable_path_text(file),
            test.line
        ));
    }
    if !matches!(
        entry.class,
        SeamGripClass::PropagationUnknown
            | SeamGripClass::ObservationUnknown
            | SeamGripClass::DiscriminationUnknown
    ) {
        return None;
    }
    // Same pick as `RepairStartRefusal::related_test` (a direct owner call
    // first), so one refusal never names two different tests.
    let related = &entry.evidence.related_tests;
    let test = related
        .iter()
        .find(|test| test.relation_reason == RelationReason::DirectOwnerCall)
        .or_else(|| related.first())?;
    if !matches!(
        test.relation_reason,
        RelationReason::DirectOwnerCall
            | RelationReason::HelperOwnerCall
            | RelationReason::ReExportChainFollowed
    ) {
        return None;
    }
    let file = root
        .source
        .and_then(|root| test.file.strip_prefix(root).ok())
        .unwrap_or(&test.file);
    Some(format!(
        "`{}` ({}:{}) calls this code, but ripr cannot follow the changed value to an assertion on it. Check by hand that it exercises the changed branch and asserts on its result, and add the assertion there",
        test.test_name,
        crate::analysis::stable_path_text(file),
        test.line
    ))
}

/// Why `ripr agent stub` would refuse this seam, read through the stub's own
/// owner, or `None` when it would write one or the source cannot be read
/// (the existing route text then stands).
fn stub_refusal_reason(
    entry: &crate::analysis::ClassifiedSeam,
    root: RefusalRoot<'_>,
) -> Option<&'static str> {
    let root = root.source?;
    let source = std::fs::read_to_string(root.join(entry.seam.file())).ok()?;
    crate::analysis::test_stub::rust_test_stub_for_classified_seam(root, entry, &source)
        .err()
        .map(|refusal| refusal.reason())
}

/// The most common refusal on real crates: no test calls the changed owner
/// directly (a private or internal owner reached through a public call), so
/// activation is unknown. Name the closest related test so the reader has a
/// place to look; ripr cannot show that test runs the code, so the wording
/// stays a by-hand check. Refusal text only; eligibility is unchanged.
fn owner_call_absent_guidance(
    entry: &crate::analysis::ClassifiedSeam,
    root: RefusalRoot<'_>,
) -> Option<String> {
    if entry.class != crate::analysis::seams::SeamGripClass::ActivationUnknown
        || !entry
            .evidence
            .activate
            .summary
            .starts_with("No direct owner call observed")
    {
        return None;
    }
    let related = &entry.evidence.related_tests;
    // Same pick as `RepairStartRefusal::related_test` (no direct owner call
    // exists here, so that is the first related test), so one refusal never
    // names two different tests.
    let test = related.first()?;
    let file = root
        .source
        .and_then(|root| test.file.strip_prefix(root).ok())
        .unwrap_or(&test.file);
    Some(format!(
        "ripr observed no direct test call of this code; the closest related test is `{}` ({}:{}). If it runs the changed code through a public call, add the assertion there, otherwise write a new test",
        test.test_name,
        crate::analysis::stable_path_text(file),
        test.line
    ))
}

/// The `activation` stage fact ("a test input that reaches the changed
/// code") is what a stub skeleton supplies when no related test calls the
/// owner directly: its first line is that call. Drops it from the list so the
/// manual stub route is named; `None` when it does not apply. Route text only:
/// the readiness list and the eligibility flip are untouched.
fn owner_call_gap_covered_by_stub(
    entry: &crate::analysis::ClassifiedSeam,
    missing_evidence: &[String],
) -> Option<Vec<String>> {
    const ACTIVATION: &str = "incomplete evidence stage: activation";
    if entry
        .evidence
        .related_tests
        .iter()
        .any(|test| test.relation_reason == crate::domain::RelationReason::DirectOwnerCall)
    {
        return None;
    }
    let kept = missing_evidence
        .iter()
        .filter(|item| item.as_str() != ACTIVATION)
        .cloned()
        .collect::<Vec<_>>();
    (kept.len() < missing_evidence.len()).then_some(kept)
}

const NO_TEST_MODULE_STUB_NOTE: &str =
    " (the file has no test module yet; for an owner at the top of the file the stub adds one)";

/// The missing evidence left once the only target blocker is "the owner file
/// has no governed inline test module": `agent stub` places a new inline
/// module there, so the safe-target items no longer stand between the reader
/// and the stub route. `None` when that is not the blocker, so every other
/// target refusal keeps the bare label.
fn stub_routable_missing_evidence(
    entry: &crate::analysis::ClassifiedSeam,
    missing_evidence: &[String],
) -> Option<Vec<String>> {
    use crate::analysis::new_test_target::NewTestProposalBlocker;
    let admission = entry.evidence.new_test_target.as_ref()?;
    if admission.blocker != Some(NewTestProposalBlocker::NoTestModule) {
        return None;
    }
    let blocked = admission.missing_reason()?;
    let kept = missing_evidence
        .iter()
        .filter(|item| {
            item.as_str() != crate::analysis::repair_route::SAFE_TEST_TARGET_EVIDENCE
                && **item != blocked
        })
        .cloned()
        .collect::<Vec<_>>();
    (kept.len() < missing_evidence.len()).then_some(kept)
}

/// Whether a readiness missing-evidence item is the boundary-value
/// discriminator the producer could not find. Recognised through the
/// producer's own format (`analysis/repair_route.rs`), not by parsing text here.
pub(super) fn is_boundary_value_discriminator(item: &str) -> bool {
    crate::analysis::repair_route::is_boundary_value_discriminator_evidence(item)
}

/// `check`'s boundary-observed weak-oracle wording (the one domain constant)
/// followed by the existing related test, when the seam is in that case
/// (#10683). Shared by the repair refusal and `agent stub`.
pub(crate) fn weak_oracle_guidance(entry: &crate::analysis::ClassifiedSeam) -> Option<String> {
    let test = crate::analysis::repair_route::boundary_observed_weak_oracle_test(entry)?;
    Some(format!(
        "{} Related test: {}:{} {}",
        crate::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP,
        crate::analysis::stable_path_text(&test.file),
        test.line,
        test.test_name
    ))
}

/// Plain-language rendering of one route missing-evidence string for human
/// screens (#10644). The readiness list behind it keeps the stable internal
/// codes for machine consumers; this translation is render-boundary only.
pub(super) fn humanize_route_evidence(evidence: &str) -> String {
    humanize_evidence(evidence)
}

fn ineligibility_label(reason: RepairPacketIneligibility) -> &'static str {
    // Sentence-style labels: these render directly in "none is ready for an
    // automatic repair (...)" lists and refusal notes, so an internal-enum spelling reads as a
    // bug to the user (#9604).
    match reason {
        RepairPacketIneligibility::ClassNotHeadlineEligible => {
            "ripr does not start an automatic repair for this kind of finding"
        }
        RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved => {
            "ripr cannot resolve which external-language observer would notice this seam"
        }
        RepairPacketIneligibility::CrossLanguageTestTargetUnresolved => {
            "ripr cannot resolve a safe external-language test target for this seam"
        }
        RepairPacketIneligibility::RouteNotReady => "repair route not ready",
        // #10703: the same limitation `ripr check` names for this seam.
        RepairPacketIneligibility::NamedStaticLimitation => {
            "ripr does not report a gap here: a `#[should_panic]` test passes only if something panics, and ripr cannot tell whether the changed code causes that panic (`ripr check` reports the same limitation). Check by hand that the test's input reaches the changed check; add a test only if it does not"
        }
    }
}

/// Bound the displayed no-eligible reason list while keeping an honest
/// distinct-reason total. The list renders at most `MAX_LISTED_REASONS`
/// entries so the message stays bounded; the total keeps tracking every
/// distinct reason across the whole inventory so the renderer can disclose
/// the remainder instead of presenting the bound as the whole story (#8728).
/// The distinctness set is intentionally separate from the displayed list:
/// once the list fills, a repeated omitted reason must not inflate the
/// count of distinct omitted reasons (a review finding on the first #8728
/// pass: A, B, C, D, D reads as "two more" although only D is omitted).
fn collect_no_eligible_reasons<'a>(
    refusal_reasons: impl Iterator<Item = &'a str>,
) -> (Vec<String>, usize) {
    const MAX_LISTED_REASONS: usize = 3;
    let mut reasons: Vec<String> = Vec::new();
    let mut distinct: Vec<String> = Vec::new();
    for reason in refusal_reasons {
        if distinct.iter().any(|seen| seen == reason) {
            continue;
        }
        distinct.push(reason.to_string());
        if reasons.len() < MAX_LISTED_REASONS {
            reasons.push(reason.to_string());
        }
    }
    (reasons, distinct.len())
}

/// How to widen the seam budget, named once so every truncation refusal gives
/// the same actionable recovery. The cap is an environment variable, not a flag.
pub(crate) const WIDEN_SEAM_BUDGET: &str = "rerun with a wider seam budget (set RIPR_REPO_EXPOSURE_SEAM_LIMIT to a larger count, or 0 to remove the cap)";

/// Largest repository a repair start widens the built-in seam cap to cover.
/// Past it the refusal stands, so the cap still bounds pathological monorepos.
const REPAIR_SEAM_CEILING: usize = 2 * crate::analysis::DEFAULT_REPO_EXPOSURE_SEAM_LIMIT;

/// Seam inventory for a repair resolver. A repository only just past the
/// built-in cap (tokio: 12,754 seams) would otherwise refuse every repair, so
/// a default-source cut within [`REPAIR_SEAM_CEILING`] reruns once with the cap
/// raised to the exact total. The rerun is cut by nothing, and one that
/// declined a Rust file is discarded, so the fail-closed truncation gate below
/// is unchanged: an explicit
/// `RIPR_REPO_EXPOSURE_SEAM_LIMIT` or a larger repository still refuses.
///
/// The CLI resolvers, the repair-start gate behind `ripr agent status` and the
/// MCP completeness check all call this, so every surface agrees on whether a
/// 10k to 20k seam repository can start a repair. Widening holds the
/// process-wide cap exclusively, so concurrent MCP worker threads serialize
/// their reruns instead of stranding the cap.
pub(crate) fn inventory_for_repair(
    root: &Path,
    config: &crate::config::RiprConfig,
) -> Result<
    (
        Vec<crate::analysis::ClassifiedSeam>,
        Option<crate::analysis::SeamLimitInfo>,
    ),
    String,
> {
    let first = crate::analysis::inventory_classified_seams_at_with_config(root, config)?;
    if let Some(cap) = first.1.as_ref().and_then(repair_widened_cap) {
        let _widened = crate::analysis::PilotInventoryDefault::widen_to(cap);
        let rerun =
            crate::analysis::inventory_classified_seams_report_at_with_config(root, config)?;
        // The repair gates read the cap only. A file the parse budget declined
        // has no seams in the inventory, so a rerun that declined any is not
        // known to be complete and the original truncation refusal stands.
        if rerun.declined_rust_files.is_empty() {
            return Ok((rerun.classified, rerun.limit_info));
        }
    }
    Ok(first)
}

/// The cap a repair resolver reruns with, or `None` when the cut stands: only
/// a built-in default cut of a repository within [`REPAIR_SEAM_CEILING`]
/// widens, to its exact total. An explicit limit is the operator's choice.
pub(crate) fn repair_widened_cap(info: &crate::analysis::SeamLimitInfo) -> Option<usize> {
    (info.is_truncated()
        && info.source == crate::analysis::SeamLimitSource::Default
        && info.total <= REPAIR_SEAM_CEILING)
        .then_some(info.total)
}

/// Gather the live authorities and decide one repair start. A truncated seam
/// inventory fails closed: selecting from a partial list could hide
/// candidates and silently pick first.
///
/// `base` is an explicit `--base`; `None` infers the diff base exactly as a
/// bare `ripr check` does (#11246).
pub(crate) fn resolve_repair_start(
    root: &Path,
    base: Option<&str>,
) -> Result<RepairStartDecision, String> {
    let config = load_for_root(root)?;
    let (classified, limit_info) = inventory_for_repair(root, &config)?;
    if let Some(info) = &limit_info
        && info.is_truncated()
    {
        return Err(format!(
            "repair start refused: the seam inventory is truncated ({} of {} seams); next step: {WIDEN_SEAM_BUDGET}",
            info.analyzed, info.total
        ));
    }
    // The before-phase packet producer refuses policy-omitted seams
    // (#4332), so the auto selector must apply the same omission: a
    // severity-off seam is never a start candidate, and the omission is
    // reported among the no-eligible reasons. The policy only fires for
    // classes this selector already excludes, or for configured-off
    // classes, so no eligible seam is lost.
    let policy = AgentBriefPolicy::from_config(&config);
    let bound = bound_root(&root.to_string_lossy());
    let mut eligible_ids = Vec::new();
    let mut candidate_notes: Vec<RepairCandidateNote> = Vec::new();
    // #11111: the gate judges whole seams, not the change. When the diff a
    // bare `ripr check` would read is available, a seam the diff does not
    // touch says nothing about the change: an already-gripped one is counted,
    // not listed as if it were the verdict, and an eligible one is not offered
    // as the change's repair start (check offers none for a line with no
    // seam, so repair must not start somewhere else). A diff that changes no
    // Rust production line leaves the whole-repository selection unscoped.
    let diff = live_check_diff_files(root, base);
    let scope = diff.as_ref().and_then(|files| {
        if classified
            .iter()
            .any(|entry| seam_touches_change(entry, files))
        {
            Some(ChangeScope::TouchesSeams(files))
        } else {
            changed_rust_production_lines(files).map(ChangeScope::TouchesNoSeam)
        }
    });
    let off_change = |entry: &crate::analysis::ClassifiedSeam| match &scope {
        Some(ChangeScope::TouchesSeams(files)) => !seam_touches_change(entry, files),
        Some(ChangeScope::TouchesNoSeam(_)) => true,
        None => false,
    };
    // The shared gate runs once per seam; the wrapper match and the loop below
    // both read these results.
    let refusals: Vec<_> = classified
        .iter()
        .map(|entry| {
            (
                entry,
                repair_refusal(entry, policy, RefusalRoot::named(root)),
            )
        })
        .collect();
    let gripped_siblings: Vec<&crate::analysis::ClassifiedSeam> = refusals
        .iter()
        .filter(|(_, refusal)| matches!(refusal, Some((RefusalKind::AlreadyGripped, _))))
        .map(|(entry, _)| *entry)
        .collect();
    // For each gripped refusal: whether the spot is outside the change, and why.
    let mut gripped_reasons: Vec<(bool, String)> = Vec::new();
    let mut eligible_off_change = 0usize;
    let mut blocked_off_change = 0usize;
    let mut open_on_change = 0usize;
    // Distinct refusal reasons in first-seen order, each with the first seam
    // that has it and how many seams share it.
    let mut blocked_groups: Vec<(String, String, usize)> = Vec::new();
    for (entry, refusal) in &refusals {
        let off = off_change(entry);
        match refusal {
            None if off => eligible_off_change += 1,
            None => {
                eligible_ids.push(entry.seam.id().as_str().to_string());
                candidate_notes.push(describe_candidate(entry, &bound));
            }
            // An already-gripped reason names its own spot, so it is never
            // read as the verdict on a different changed line.
            Some((RefusalKind::AlreadyGripped, reason)) => {
                gripped_reasons.push((off, format!("{}: {reason}", seam_label(entry))));
            }
            // A `return Err(V)` wrapper of an error variant that is already
            // gripped is the same behavior, not an open check of its own. It
            // is classified before any off-change blocked counting.
            Some(_) if wraps_gripped_error_variant(entry, &gripped_siblings) => {
                gripped_reasons.push((
                    off,
                    format!("{}: {GRIPPED_WRAPPER_REASON}", seam_label(entry)),
                ));
            }
            // A seam the diff does not touch says nothing about the change
            // (#11111 group 4): its "not ready" or "no gap here" reason is
            // counted, not listed beside the verdict on the changed seam.
            Some(_) if off => blocked_off_change += 1,
            // Name the shared gate's own reason among the no-eligible
            // reasons so the limitation is diagnosable, with the seam it
            // is about, so two reasons about two seams never read as two
            // verdicts on one change.
            Some((_, reason)) => {
                open_on_change += 1;
                match blocked_groups.iter_mut().find(|group| group.0 == *reason) {
                    Some(group) => group.2 += 1,
                    None => blocked_groups.push((reason.clone(), seam_label(entry), 1)),
                }
            }
        }
    }
    let gripped_on_change = gripped_reasons.iter().filter(|(off, _)| !off).count();
    let mut refusal_reasons: Vec<String> = Vec::new();
    let all_changed_checked = matches!(scope, Some(ChangeScope::TouchesSeams(_)))
        && gripped_on_change > 0
        && open_on_change == 0;
    let change = if all_changed_checked {
        NoEligibleChange::AlreadyChecked
    } else if matches!(scope, Some(ChangeScope::TouchesNoSeam(_))) {
        NoEligibleChange::NoRepairableCode
    } else {
        NoEligibleChange::Other
    };
    if all_changed_checked {
        // Every spot on the changed lines is already gripped: say only that.
        // A missing-evidence or test-layout demand about code the change
        // never touched would contradict `ripr check`'s "nothing to repair".
        refusal_reasons.extend(
            gripped_reasons
                .into_iter()
                .filter(|(off, _)| !off)
                .map(|(_, reason)| reason),
        );
    } else {
        for (reason, label, count) in blocked_groups {
            let mut text = format!("{label}: {reason}");
            if count > 1 {
                text.push_str(&format!(
                    " (and {} other seams in the same state)",
                    count - 1
                ));
            }
            refusal_reasons.push(text);
        }
        let mut gripped_off_change = 0usize;
        let mut gripped_other_check = 0usize;
        for (off, reason) in gripped_reasons {
            if scope.is_none() {
                // Without the diff, the reason names its own spot only.
                refusal_reasons.push(reason);
            } else if off {
                gripped_off_change += 1;
            } else {
                // The change still has an open finding: a gripped spot on the
                // same lines is a different check, never the verdict on it.
                gripped_other_check += 1;
            }
        }
        if eligible_off_change > 0 {
            let (noun, verb) = if eligible_off_change == 1 {
                ("spot", "is")
            } else {
                ("spots", "are")
            };
            let lead = match &scope {
                Some(ChangeScope::TouchesNoSeam(lines)) => format!(
                    "your changed lines ({lines}) touch no code ripr can start a repair on, the same as `ripr check`; "
                ),
                _ => String::new(),
            };
            refusal_reasons.insert(
                0,
                format!(
                    "{lead}{eligible_off_change} repairable {noun} outside your changed lines {verb} not offered, since repairing {} would not address your change (to repair one anyway, pass its id: `ripr repair <seam-id>`)",
                    if eligible_off_change == 1 { "it" } else { "them" }
                ),
            );
        }
        if blocked_off_change > 0 {
            let noun = if blocked_off_change == 1 {
                "spot outside your changed lines cannot start a repair either and is"
            } else {
                "spots outside your changed lines cannot start a repair either and are"
            };
            refusal_reasons.push(format!(
                "{blocked_off_change} {noun} not listed, since that says nothing about your change"
            ));
        }
        if gripped_off_change > 0 {
            let noun = if gripped_off_change == 1 {
                "spot outside your changed lines is"
            } else {
                "spots outside your changed lines are"
            };
            refusal_reasons.push(format!(
                "{gripped_off_change} {noun} already noticed by existing tests and not listed, since that says nothing about your change"
            ));
        }
        if gripped_other_check > 0 {
            let noun = if gripped_other_check == 1 {
                "part of your changed lines is"
            } else {
                "parts of your changed lines are"
            };
            refusal_reasons.push(format!(
                "{gripped_other_check} other {noun} already noticed by existing tests, but that is a different check than the one still open, so it does not mean the open one is covered"
            ));
        }
        // A change on lines with no repairable code says so first, even when
        // no repairable spot elsewhere carries that lead (#11111 burn-down).
        if eligible_off_change == 0
            && let Some(ChangeScope::TouchesNoSeam(lines)) = &scope
        {
            refusal_reasons.insert(
                0,
                format!(
                    "your changed lines ({lines}) touch no code ripr can start a repair on, the same as `ripr check`"
                ),
            );
        }
    }
    // #11246: inference only narrows. When no changed line could be read, the
    // spots above are not scoped to the change, and the text says so first
    // (the reason list is capped, so it is placed where it survives).
    if let Some(note) = unscoped_diff_note(diff.as_deref(), base) {
        refusal_reasons.insert(0, note);
    }
    let (reasons, reasons_total) =
        collect_no_eligible_reasons(refusal_reasons.iter().map(String::as_str));
    let head = current_git_head(root).ok();
    let spec = if eligible_ids.len() == 1 {
        Some(repair_start_command_spec(&bound, &eligible_ids[0]))
    } else {
        None
    };
    decide_repair_start(&RepairStartFacts {
        root_display: &bound,
        eligible_ids: &eligible_ids,
        total_seams: classified.len(),
        ineligible_reasons: &reasons,
        ineligible_reasons_total: reasons_total,
        head: head.as_deref(),
        candidate_notes: &candidate_notes,
        start_spec: spec.as_ref(),
        change,
    })
}

/// Why an already-gripped spot needs no repair, naming its limits.
const GRIPPED_SPOT_REASON: &str = "existing tests already check it, so there is no gap to repair at this spot; this speaks only to this spot, not to what you changed (`ripr check` judges your changed lines)";

/// Why a wrapper that restates a gripped error variant needs no repair. It
/// is inferred from the same-line match, so it does not claim the engine
/// classified the wrapper itself as gripped.
const GRIPPED_WRAPPER_REASON: &str = "this line restates behavior the existing tests already check (its error value), so it adds no separate gap to repair; this speaks only to this spot, not to what you changed (`ripr check` judges your changed lines)";

/// The facts the wrapper match compares for one seam.
struct SeamKey<'a> {
    kind: crate::analysis::seams::SeamKind,
    file: &'a Path,
    line: usize,
    expression: &'a str,
}

fn seam_key(entry: &crate::analysis::ClassifiedSeam) -> SeamKey<'_> {
    SeamKey {
        kind: entry.seam.kind(),
        file: entry.seam.file(),
        line: entry.seam.display_line(),
        expression: entry.seam.expression(),
    }
}

/// Whether `entry` is the return-value seam of `return <expression>;` where
/// `<expression>` is an error-variant seam in `gripped` on the same line: the
/// wrapper restates that gripped behavior, so it adds no open check. Same file,
/// same line and an exact expression match; nothing looser.
fn wraps_gripped_error_variant(
    entry: &crate::analysis::ClassifiedSeam,
    gripped: &[&crate::analysis::ClassifiedSeam],
) -> bool {
    let entry = seam_key(entry);
    gripped
        .iter()
        .any(|other| restates_error_variant(&entry, &seam_key(other)))
}

fn restates_error_variant(entry: &SeamKey<'_>, other: &SeamKey<'_>) -> bool {
    use crate::analysis::seams::SeamKind;
    let normalize = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    entry.kind == SeamKind::ReturnValue
        && other.kind == SeamKind::ErrorVariant
        && other.file == entry.file
        && other.line == entry.line
        && normalize(entry.expression) == format!("return {}", normalize(other.expression))
}

/// How the diff a bare `ripr check` reads relates to the seam inventory.
enum ChangeScope<'a> {
    /// The diff touches at least one seam: only those seams are the change.
    TouchesSeams(&'a [crate::analysis::ChangedFile]),
    /// The diff changes Rust production lines, none inside a seam; the
    /// string names up to three of them for the refusal.
    TouchesNoSeam(String),
}

/// `src/lib.rs:1, src/lib.rs:12` for the Rust production lines (outside any
/// `tests/` directory) a diff changes, or `None` when it changes none.
fn changed_rust_production_lines(files: &[crate::analysis::ChangedFile]) -> Option<String> {
    const LISTED: usize = 3;
    let mut lines: Vec<String> = Vec::new();
    for file in files {
        let path = normalized_relative_path(&file.path);
        let is_production = path.ends_with(".rs")
            && !path
                .split('/')
                .any(|component| component == "tests" || component == "benches");
        if !is_production {
            continue;
        }
        for line in file.added_lines.iter().chain(file.removed_lines.iter()) {
            let label = format!("{path}:{}", line.new_side_line);
            if !lines.contains(&label) {
                lines.push(label);
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let more = lines.len().saturating_sub(LISTED);
    lines.truncate(LISTED);
    let mut text = lines.join(", ");
    if more > 0 {
        text.push_str(&format!(", and {more} more"));
    }
    Some(text)
}

/// Whether `files` (a parsed diff) adds, or deletes next to, a line inside
/// the seam's span (its display line when no span was recorded).
fn seam_touches_change(
    entry: &crate::analysis::ClassifiedSeam,
    files: &[crate::analysis::ChangedFile],
) -> bool {
    let seam_file = normalized_relative_path(entry.seam.file());
    let (start, end) = entry.seam.span().map_or(
        (entry.seam.display_line(), entry.seam.display_line()),
        |span| (span.start_line, span.end_line),
    );
    files
        .iter()
        .filter(|file| normalized_relative_path(&file.path) == seam_file)
        .any(|file| {
            file.added_lines
                .iter()
                .chain(file.removed_lines.iter())
                .any(|line| (start..=end).contains(&line.new_side_line))
        })
}

fn normalized_relative_path(path: &Path) -> String {
    let text = crate::analysis::stable_path_text(path);
    text.strip_prefix("./").unwrap_or(&text).to_string()
}

/// `the code at src/lib.rs:6 `acc``: where a refused seam is and its own code, so a
/// whole-seam reason is never read as the verdict on another line.
fn seam_label(entry: &crate::analysis::ClassifiedSeam) -> String {
    let file = crate::analysis::stable_path_text(entry.seam.file());
    format!(
        "the code at {file}:{} `{}`",
        entry.seam.display_line(),
        collapsed_code(entry.seam.expression())
    )
}

/// The two-sentence refusal for a seam whose repair has no test file ripr can
/// edit (#10978): where the change is, why ripr cannot pick a place for the
/// test, and what to do next, with the stub command pre-filled. `source` is
/// the seam's file text, read only to name its inline test modules.
pub(crate) fn no_test_route_refusal(
    root: &Path,
    seam_id: &str,
    seam: Option<&crate::analysis::ClassifiedSeam>,
    source: Option<&str>,
) -> String {
    let root_arg = shell_arg(&bound_root(&root.to_string_lossy()));
    let Some(entry) = seam else {
        return format!(
            "ripr did not start a repair for `{seam_id}`: it found no test file it can safely edit, and no seam with that id exists in the current code. Run `ripr check` to list the current findings, then write the test by hand or run `ripr agent stub --root {root_arg} --at FILE:LINE --kind KIND` with the location and kind it prints."
        );
    };
    let file = crate::analysis::stable_path_text(entry.seam.file());
    let line = entry.seam.display_line();
    let family = crate::analysis::probe_family_for_seam_kind(entry.seam.kind());
    let at = shell_arg(&format!("{file}:{line}"));
    let stub = format!("ripr agent stub --root {root_arg} --at {at} --kind {family}");
    let inline: Vec<String> = source
        .and_then(crate::analysis::governed_cfg_test_modules)
        .map(|modules| {
            modules
                .into_iter()
                .filter(|module| module.is_inline)
                .map(|module| module.name)
                .collect()
        })
        .unwrap_or_default();
    let is_production_rust = file.ends_with(".rs") && !crate::analysis::is_test_surface_path(&file);
    let (why, place) = match inline.as_slice() {
        [] => (
            format!(
                "it found no test in `tests/` that it can safely add to and `{file}` has no `#[cfg(test)]` test module"
            ),
            "a file in `tests/`".to_string(),
        ),
        [only] if is_production_rust => (
            format!(
                "it could not confirm that `mod {only}` in `{file}` is a safe place to add the test"
            ),
            format!("`mod {only}` in `{file}`"),
        ),
        names if is_production_rust => {
            let listed = names
                .iter()
                .map(|name| format!("`mod {name}`"))
                .collect::<Vec<_>>()
                .join(", ");
            (
                format!(
                    "`{file}` has {} test modules ({listed}) and ripr cannot tell which one to edit",
                    names.len()
                ),
                "one of those modules or in a file in `tests/`".to_string(),
            )
        }
        _ => (
            "it found no test file it can safely edit".to_string(),
            "a file in `tests/`".to_string(),
        ),
    };
    format!(
        "ripr did not start a repair for the change at `{file}:{line}`: {why}. Write the test by hand in {place}, or run `{stub}` to preview a starter test; no repair attempt was started."
    )
}

/// Whether an edit-cage policy rejection means the packet names no editable
/// test target (the case [`no_test_route_refusal`] explains), as opposed to a
/// malformed packet or an invalid edit surface, which keep their own
/// diagnostics. Pinned to the producer's wording by unit tests.
pub(crate) fn is_missing_edit_target_error(error: &str) -> bool {
    error.contains("has no selected edit target")
        || error.contains("is missing allowed edit target")
        || (error.contains("selected edit target `") && error.contains("is not a test surface"))
}

/// Longest changed-code excerpt shown beside a candidate.
const CANDIDATE_CODE_CHARS: usize = 70;

/// A seam's own expression collapsed to one line and bounded.
fn collapsed_code(expression: &str) -> String {
    let collapsed = expression.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > CANDIDATE_CODE_CHARS {
        let cut: String = collapsed.chars().take(CANDIDATE_CODE_CHARS).collect();
        format!("{cut}...")
    } else {
        collapsed
    }
}

/// One selectable repair candidate in plain words (#10978, #11111): where the
/// change is, the changed code, what a test must check, and the exact command
/// that starts it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RepairCandidateNote {
    /// `file:line` of the change.
    pub(crate) location: String,
    /// The changed code, collapsed to one bounded line.
    pub(crate) code: String,
    /// What a test must check to notice this change, from the seam's kind.
    pub(crate) test_need: &'static str,
    /// The exact command that starts this candidate.
    pub(crate) command: String,
    /// The change's 1-based start column, when the parser gave a span. Shown
    /// only to tell apart choices that would otherwise read the same.
    pub(crate) column: Option<usize>,
}

/// The plain-language choice `ripr repair` prints when several changes could
/// start a repair (#11111). When every listed candidate is the same changed
/// code, the header names that change once and each choice is told apart by
/// what a test must check; otherwise each choice also names its location.
/// `total` is the selection's full count of choices (one change can offer
/// several); candidates beyond the listed ones are disclosed with the route
/// that lists them all. Each command is followed by its PowerShell spelling
/// when PowerShell needs a different one (a root with an apostrophe), so a
/// PowerShell reader is not left with silent Bash. The closing line points
/// at the command `ripr check` printed, which applies to the finding check
/// selected; this selector has no diff base, so it does not guess which
/// candidate that was.
pub(crate) fn render_repair_choice(
    candidates: &[RepairCandidateNote],
    total: usize,
    root_display: &str,
) -> String {
    let mut out = String::new();
    let same_change = candidates.len() > 1
        && total == candidates.len()
        && candidates.split_first().is_some_and(|(first, rest)| {
            rest.iter()
                .all(|other| other.location == first.location && other.code == first.code)
        });
    match candidates.first() {
        Some(first) if same_change => {
            out.push_str(&format!(
                "ripr can start a repair for the change at {} `{}` in {} ways; each asks a test to check something different. Pick one and run its command:\n",
                first.location,
                first.code,
                candidates.len()
            ));
            for (index, candidate) in candidates.iter().enumerate() {
                out.push_str(&format!(
                    "  {}. {}{}\n",
                    index + 1,
                    candidate.test_need,
                    column_suffix(candidates, candidate)
                ));
                push_candidate_command(&mut out, &candidate.command);
            }
        }
        _ => {
            out.push_str(&format!(
                "ripr can start {total} different repairs. Pick one and run its command:\n"
            ));
            for (index, candidate) in candidates.iter().enumerate() {
                out.push_str(&format!(
                    "  {}. {}  `{}`: {}{}\n",
                    index + 1,
                    candidate.location,
                    candidate.code,
                    candidate.test_need,
                    column_suffix(candidates, candidate)
                ));
                push_candidate_command(&mut out, &candidate.command);
            }
            if total > candidates.len() {
                out.push_str(&format!(
                    "  ({} more not shown; `ripr pilot --root {}` lists them all)\n",
                    total - candidates.len(),
                    shell_arg(root_display)
                ));
            }
        }
    }
    out.push_str(
        "If `ripr check` printed a `ripr repair` command for this change, run that one: it is the choice check recommends.\n",
    );
    out
}

/// One candidate's start command, then its PowerShell spelling only when
/// PowerShell needs a different one (or has none), from the shared translator.
fn push_candidate_command(out: &mut String, command: &str) {
    out.push_str(&format!("     {command}\n"));
    if let Some(form) = crate::output::markdown::powershell_text_variant(command) {
        out.push_str(&format!("     (PowerShell) {form}\n"));
    }
}

/// ` (column N)` when another listed choice has the same location, code and
/// test need, so the two would otherwise read the same; empty otherwise.
fn column_suffix(candidates: &[RepairCandidateNote], candidate: &RepairCandidateNote) -> String {
    let twins = candidates
        .iter()
        .filter(|other| {
            other.location == candidate.location
                && other.code == candidate.code
                && other.test_need == candidate.test_need
        })
        .count();
    match candidate.column {
        Some(column) if twins > 1 => format!(" (column {column})"),
        _ => String::new(),
    }
}

/// Describe a selectable seam: where the change is and the changed code (the
/// seam's own expression collapsed to one line and bounded), what a test must
/// check for it, and the exact command that picks it.
fn describe_candidate(
    entry: &crate::analysis::ClassifiedSeam,
    bound_root_display: &str,
) -> RepairCandidateNote {
    let file = crate::analysis::stable_path_text(entry.seam.file());
    let code = collapsed_code(entry.seam.expression());
    RepairCandidateNote {
        location: format!("{file}:{}", entry.seam.display_line()),
        code,
        test_need: entry.seam.kind().plain_test_need(),
        command: format!(
            "ripr repair {} --root {}",
            entry.seam.id().as_str(),
            shell_arg(bound_root_display)
        ),
        column: entry.seam.span().map(|span| span.start_column),
    }
}

/// What an explicit `ripr repair` item resolved to: the seam to start,
/// or the deliberate message naming why no seam was selected.
pub(crate) enum RepairSubject {
    Seam(String),
    /// The deliberate no-selection message with its typed reason code.
    Decision(RepairRefusalCode, String),
}

/// The structured outcome of resolving one explicit repair item, before
/// either repair surface renders its refusal (#8166 residual). `ripr repair`
/// and `ripr agent repair` share this resolution — an exact seam ID, a
/// canonical gap ID, a `probe:`/`repo-probe:` finding ID, or a `file:line`
/// location naming exactly one seam — and each renders the refusal in its
/// own command's terms.
pub(crate) enum RepairSubjectOutcome {
    /// Exactly one seam, admitted by the shared repair gate.
    Seam(String),
    /// Exactly one seam, refused by the shared repair gate. `kind` is the
    /// gate's own refusal kind, so a projection can name an already-gripped
    /// seam without parsing the reason prose.
    Ineligible {
        seam_id: String,
        reason: String,
        kind: RefusalKind,
    },
    /// Several seams name the item: the seam IDs to choose from, and a
    /// parallel description of each (kind, expression, owner) so the reader
    /// can tell the candidates apart (#11466).
    Ambiguous {
        seam_ids: Vec<String>,
        descriptions: Vec<String>,
    },
    /// No current seam names the item (stale or unmapped).
    Unknown,
}

/// Resolve an explicit `ripr repair` item to its seam, rendering the
/// structured [`resolve_repair_subject_outcome`] in this command's terms:
/// an exact seam ID, or a canonical gap ID, `probe:` finding ID, or
/// `file:line` location naming exactly one seam. An ineligible subject is a
/// deliberate no-selection decision naming the reason, never a start.
/// Anything else is a deliberate no-selection message, never an implicit
/// pick.
pub(crate) fn resolve_repair_subject(root: &Path, item: &str) -> Result<RepairSubject, String> {
    Ok(match resolve_repair_subject_outcome(root, item)? {
        RepairSubjectOutcome::Seam(seam_id) => RepairSubject::Seam(seam_id),
        RepairSubjectOutcome::Ineligible {
            seam_id, reason, ..
        } => RepairSubject::Decision(
            RepairRefusalCode::SeamNotRepairEligible,
            ineligible_repair_item(root, item, &seam_id, &reason),
        ),
        RepairSubjectOutcome::Ambiguous { descriptions, .. } => {
            let listed: Vec<&str> = descriptions.iter().map(String::as_str).collect();
            RepairSubject::Decision(
                RepairRefusalCode::ItemNamesSeveralSeams,
                ambiguous_gap_item(root, item, &listed),
            )
        }
        RepairSubjectOutcome::Unknown => RepairSubject::Decision(
            RepairRefusalCode::ItemNamesNoSeam,
            unknown_repair_item(root, item),
        ),
    })
}

/// Resolve an explicit repair item to its structured outcome: an exact seam
/// ID, or a cross-namespace selector naming exactly one seam (#8166
/// residual). The cross-namespace selectors are the canonical gap IDs
/// (seam-derived `gap:<hash>` and the Rust producer `gap:rust:...` `ripr
/// check` prints), the `probe:`/`repo-probe:` finding ID `ripr check`
/// prints (re-minted from the seam by [`probe_id_names_seam`]), and a
/// `file:line` location. Every selector joins on seam identity — file,
/// owner, kind, and the seam's own expression — never on a substring or a
/// nearby line alone. The resolved seam must pass the same repair-admission
/// gate as the auto selector ([`repair_refusal`]); zero mappings are
/// `Unknown` (stale), several are `Ambiguous`, and neither starts an attempt.
pub(crate) fn resolve_repair_subject_outcome(
    root: &Path,
    item: &str,
) -> Result<RepairSubjectOutcome, String> {
    let config = load_for_root(root)?;
    let (classified, limit_info) = inventory_for_repair(root, &config)?;
    if let Some(info) = &limit_info
        && info.is_truncated()
    {
        return Err(format!(
            "repair item refused: the seam inventory is truncated ({} of {} seams); next step: {WIDEN_SEAM_BUDGET}",
            info.analyzed, info.total
        ));
    }
    Ok(resolve_repair_subject_in(
        &classified,
        AgentBriefPolicy::from_config(&config),
        item,
        RefusalRoot::named(root),
    ))
}

/// The inventory-taking core of [`resolve_repair_subject_outcome`]: resolve
/// one explicit repair item against an already-classified seam inventory and
/// apply the shared repair-admission gate to the one seam it names. The MCP
/// refresh runs this over its own diff-scoped inventory so a session item's
/// repair readiness is this exact decision, not a parallel one. `root` names
/// where the sources live (read to tell whether `ripr agent stub` would
/// refuse the seam) and whether an emitted next command carries `--root`;
/// the MCP server passes [`RefusalRoot::implicit`] because its verdicts are
/// already relative to the server's workspace root.
pub(crate) fn resolve_repair_subject_in(
    classified: &[crate::analysis::ClassifiedSeam],
    policy: AgentBriefPolicy<'_>,
    item: &str,
    root: RefusalRoot<'_>,
) -> RepairSubjectOutcome {
    let outcome_for = |entry: &crate::analysis::ClassifiedSeam| {
        let seam_id = entry.seam.id().as_str().to_string();
        match repair_refusal(entry, policy, root) {
            None => RepairSubjectOutcome::Seam(seam_id),
            Some((kind, reason)) => RepairSubjectOutcome::Ineligible {
                seam_id,
                reason,
                kind,
            },
        }
    };
    if let Some(entry) = classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == item)
    {
        return outcome_for(entry);
    }
    let matches: Vec<&crate::analysis::ClassifiedSeam> = classified
        .iter()
        .filter(|entry| {
            canonical_gap_identity(entry).is_some_and(|identity| identity.id == item)
                || producer_gap_id_names_seam(item, entry)
                || probe_id_names_seam(item, entry, classified)
                || super::selector::selector_matches_file_line(
                    crate::analysis::stable_path_text(entry.seam.file()).as_ref(),
                    entry.seam.display_line(),
                    item,
                )
        })
        .collect();
    match matches.len() {
        1 => outcome_for(matches[0]),
        0 => RepairSubjectOutcome::Unknown,
        _ => RepairSubjectOutcome::Ambiguous {
            seam_ids: matches
                .iter()
                .map(|entry| entry.seam.id().as_str().to_string())
                .collect(),
            descriptions: matches
                .iter()
                .map(|entry| describe_ambiguous_seam(entry))
                .collect(),
        },
    }
}

/// Why one named seam may not start a repair attempt right now (#8766), as
/// computed by the shared admission gate ([`repair_refusal`]).
pub(crate) struct RepairStartRefusal {
    /// The gate's own label, for example "repair route not ready".
    pub(crate) reason: String,
    /// The related test the remaining by-hand work targets, rendered as
    /// `name` (file:line), when the seam's evidence names one.
    pub(crate) related_test: Option<String>,
    /// What kind of refusal this is; `Unverified` when the gate could not
    /// decide and the start is withheld because it could not be verified.
    pub(crate) kind: RefusalKind,
}

impl RepairStartRefusal {
    fn unverified(reason: String) -> Self {
        Self {
            reason,
            related_test: None,
            kind: RefusalKind::Unverified,
        }
    }
}

/// The repair-admission gate behind every repair-start command `status` and
/// `continue` print (#8766). It loads the seam inventory once, lazily, so a
/// status read that checks several seams or attempts pays one load, and a
/// read that never needs a start pays none. The verdict for one seam is the
/// same eligibility `ripr repair` and `ripr agent repair --phase before`
/// apply to it: [`repair_refusal`], the recommended-test edit-target
/// predicate and the held-inline-target (uncommitted production file) check.
/// A dirty tracked tree elsewhere (the before phase's premise check) is
/// state-dependent and deliberately not re-derived here; the start itself
/// still re-checks it. No parallel validator.
///
/// Where the gate cannot decide (unreadable config, an inventory that cannot
/// be built, a seam the inventory no longer names because seam ids move with
/// byte offsets, or a seam beyond the seam-limit window) it does not admit by
/// default: it returns an *unverified* refusal, so no bare start is printed
/// and the note says the seam could not be verified.
pub(crate) struct RepairStartGate {
    root: std::path::PathBuf,
    state: std::cell::OnceCell<GateState>,
    /// Unit-test builds only: seam ids admitted regardless of the inventory.
    #[cfg(test)]
    admitted: Vec<String>,
}

enum GateState {
    Loaded {
        config: Box<crate::config::RiprConfig>,
        classified: Vec<crate::analysis::ClassifiedSeam>,
        truncated: bool,
    },
    Unavailable(String),
    /// Unit-test builds only: admits every start.
    #[cfg(test)]
    Open,
}

impl RepairStartGate {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// The real gate: the inventory loads lazily on the first check.
    pub(crate) fn real(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            state: std::cell::OnceCell::new(),
            #[cfg(test)]
            admitted: Vec::new(),
        }
    }

    /// The gate every production caller uses.
    #[cfg(not(test))]
    pub(crate) fn new(root: &Path) -> Self {
        Self::real(root)
    }

    /// Unit-test builds: the lib's own unit tests exercise status with
    /// fabricated seam ids in empty directories, so `new` is an explicitly
    /// open gate there and those tests keep asserting recorded commands.
    /// Tests that need a real verdict use [`Self::real`] or
    /// [`Self::preloaded`]; the facade integration tests run the real binary.
    #[cfg(test)]
    pub(crate) fn new(root: &Path) -> Self {
        let gate = Self::real(root);
        let _ = gate.state.set(GateState::Open);
        gate
    }

    /// A gate over a fixed classified inventory, for unit tests that need a
    /// real verdict.
    #[cfg(test)]
    pub(crate) fn preloaded(root: &Path, classified: Vec<crate::analysis::ClassifiedSeam>) -> Self {
        let gate = Self::real(root);
        let _ = gate.state.set(GateState::Loaded {
            config: Box::default(),
            classified,
            truncated: false,
        });
        gate
    }

    /// Like [`Self::preloaded`], but the named seam ids are admitted, so a
    /// test can mix admitted and refused seams.
    #[cfg(test)]
    pub(crate) fn preloaded_admitting(
        root: &Path,
        classified: Vec<crate::analysis::ClassifiedSeam>,
        admitted: &[&str],
    ) -> Self {
        let mut gate = Self::preloaded(root, classified);
        gate.admitted = admitted.iter().map(|id| id.to_string()).collect();
        gate
    }

    /// `None` when the gate admits a start for `seam_id`.
    pub(crate) fn check(&self, seam_id: &str) -> Option<RepairStartRefusal> {
        // #9103 (RIPR-SPEC-0311): a gap-bound attempt's subject is a
        // canonical TypeScript gap id, not a Rust seam, so this seam-inventory
        // gate has no verdict to give. Its admission is decided where the
        // start is: the shared agent-packet validator at the gap-bound before
        // phase, which re-checks it on every restart.
        if super::gap_bound_repair::is_gap_bound_subject(seam_id) {
            return None;
        }
        #[cfg(test)]
        if self.admitted.iter().any(|id| id == seam_id) {
            return None;
        }
        let state = self.state.get_or_init(|| load_gate_state(&self.root));
        check_state(&self.root, state, seam_id)
    }

    /// Like [`Self::check`] for a recorded start that names no seam id: it
    /// cannot be verified, so it is withheld without naming a seam.
    pub(crate) fn check_optional(&self, seam_id: Option<&str>) -> Option<RepairStartRefusal> {
        match seam_id {
            Some(seam_id) => self.check(seam_id),
            None => {
                #[cfg(test)]
                if matches!(self.state.get(), Some(GateState::Open)) {
                    return None;
                }
                Some(RepairStartRefusal::unverified(
                    "the recorded start names no seam".to_string(),
                ))
            }
        }
    }
}

fn load_gate_state(root: &Path) -> GateState {
    let config = match load_for_root(root) {
        Ok(config) => config,
        Err(error) => {
            return GateState::Unavailable(format!(
                "the configuration could not be read ({error})"
            ));
        }
    };
    match inventory_for_repair(root, &config) {
        Ok((classified, limit_info)) => GateState::Loaded {
            truncated: limit_info.as_ref().is_some_and(|info| info.is_truncated()),
            config: Box::new(config),
            classified,
        },
        Err(error) => {
            GateState::Unavailable(format!("the seam inventory could not be built ({error})"))
        }
    }
}

fn check_state(root: &Path, state: &GateState, seam_id: &str) -> Option<RepairStartRefusal> {
    let (config, classified, truncated) = match state {
        GateState::Loaded {
            config,
            classified,
            truncated,
        } => (config, classified, *truncated),
        GateState::Unavailable(why) => return Some(RepairStartRefusal::unverified(why.clone())),
        #[cfg(test)]
        GateState::Open => return None,
    };
    // The explicit-subject resolver refuses every start from a truncated
    // inventory, so no seam in one can be verified as startable.
    if truncated {
        return Some(RepairStartRefusal::unverified(format!(
            "the seam inventory was truncated by the seam limit, and a start from a truncated inventory is refused; {WIDEN_SEAM_BUDGET}"
        )));
    }
    let Some(entry) = classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == seam_id)
    else {
        return Some(RepairStartRefusal::unverified(
            "the current inventory no longer names this seam (seam ids change when the code shifts)"
                .to_string(),
        ));
    };
    repair_start_refusal_for_entry(root, entry, AgentBriefPolicy::from_config(config))
}

/// Whether the shared repair-admission gate refuses a start for `seam_id`.
/// One-shot form of [`RepairStartGate`] for callers that check one seam.
pub(crate) fn repair_start_refusal(root: &Path, seam_id: &str) -> Option<RepairStartRefusal> {
    RepairStartGate::new(root).check(seam_id)
}

/// The production file whose uncommitted changes make the before phase refuse
/// `seam_id` (#9033), read from the inventory the before phase already loaded
/// (no second load) and the same held-inline-target predicate the edit cage
/// applies. Asked before any workflow artifact is written, it lets a start
/// refuse with nothing on disk. `None` when the inventory does not name the
/// seam or its target is committed.
pub(crate) fn held_inline_start_file(
    root: &Path,
    seam_id: &str,
    classified: &[crate::analysis::ClassifiedSeam],
) -> Option<String> {
    let entry = classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == seam_id)?;
    held_inline_target(root, &repair_packet_eligibility(entry).readiness)
}

/// The refusal for a start held by an uncommitted production target: states
/// that no attempt started and what to do about it (#9033).
pub(crate) fn uncommitted_start_refusal_message(root: &Path, seam_id: &str, file: &str) -> String {
    format!(
        "No repair attempt started. Commit or stash `{file}`, then run `ripr repair {} --root {}`. A repair confined to its inline `#[cfg(test)]` module starts only from the committed file",
        shell_arg(seam_id),
        shell_arg(&bound_root(&root.to_string_lossy()))
    )
}

/// The one sentence for a start held by an uncommitted test target, shared by
/// `ripr repair`, the agent card and the MCP admission so every surface names
/// the same cause (#11397).
pub(crate) fn held_inline_reason(file: &str) -> String {
    format!("the inline test target `{file}` has uncommitted changes; commit or stash them first")
}

/// What the human agent card says when the shared start gate holds a seam
/// back: the plain cause and, when the gate's remedy is a command, the one
/// next step. Derived from [`repair_start_refusal_for_entry`] only; the card
/// keeps no gate of its own (#11397).
pub(crate) struct CardStartHold {
    pub(crate) reason: String,
    pub(crate) next: Option<String>,
    /// The bash command `next` names, so a plain-text surface can print its
    /// PowerShell pair beside it.
    pub(crate) command: Option<String>,
}

pub(crate) fn card_start_hold(
    root: &Path,
    entry: &crate::analysis::ClassifiedSeam,
    policy: AgentBriefPolicy<'_>,
) -> Option<CardStartHold> {
    let refusal = repair_start_refusal_for_entry(root, entry, policy)?;
    let seam_id = entry.seam.id().as_str();
    let held = held_inline_target(root, &repair_packet_eligibility(entry).readiness).map(|file| {
        let command = format!(
            "ripr repair {} --root {}",
            shell_arg(seam_id),
            shell_arg(&bound_root(&root.to_string_lossy()))
        );
        (
            format!("commit or stash `{file}`, then run `{command}`"),
            command,
        )
    });
    let (next, command) = match held {
        Some((next, command)) => (Some(next), Some(command)),
        None => (None, None),
    };
    Some(CardStartHold {
        reason: refusal.reason,
        next,
        command,
    })
}

/// The gate applied to one classified seam: [`repair_refusal`], the
/// edit-target predicate and the held inline target, plus the related test
/// the by-hand work targets.
pub(crate) fn repair_start_refusal_for_entry(
    root: &Path,
    entry: &crate::analysis::ClassifiedSeam,
    policy: AgentBriefPolicy<'_>,
) -> Option<RepairStartRefusal> {
    let (kind, reason) = repair_refusal(entry, policy, RefusalRoot::named(root))
        .or_else(|| {
            (!crate::output::agent_seam_packets::recommended_test_is_repair_edit_target(entry))
                .then(|| {
                    (
                        RefusalKind::Other,
                        "no test file ripr may edit: the recommended test target is neither a test surface nor the seam's governed inline test module".to_string(),
                    )
                })
        })
        .or_else(|| {
            held_inline_target(root, &repair_packet_eligibility(entry).readiness).map(|file| {
                (RefusalKind::Other, held_inline_reason(&file))
            })
        })?;
    let related = entry
        .evidence
        .related_tests
        .iter()
        .find(|test| test.relation_reason == crate::domain::RelationReason::DirectOwnerCall)
        .or_else(|| entry.evidence.related_tests.first());
    let related_test = related.map(|test| {
        let file = test.file.strip_prefix(root).unwrap_or(&test.file);
        format!(
            "`{}` ({}:{})",
            test.test_name,
            file.to_string_lossy().replace('\\', "/"),
            test.line
        )
    });
    Some(RepairStartRefusal {
        reason,
        related_test,
        kind,
    })
}

/// The prose that replaces a refused repair-start command: it states the
/// refusal and that the remaining work is strengthening the named test's
/// oracle by hand (#8766; automated strengthening of existing tests is #7217).
pub(crate) fn repair_start_withheld_note(
    root_display: &str,
    seam_id: &str,
    refusal: &RepairStartRefusal,
) -> String {
    let recheck = format!("rerun `ripr check --root {}`", shell_arg(root_display));
    // A recorded start that names no seam is withheld without naming one.
    let subject = if seam_id.is_empty() {
        "the recorded start".to_string()
    } else {
        format!("seam `{seam_id}`")
    };
    match refusal.kind {
        RefusalKind::Unverified => format!(
            "no new repair attempt is offered for {subject}: could not verify that the repair gate admits it ({}); {recheck} to re-read the current seams",
            refusal.reason
        ),
        // A static reading of current evidence, not a runtime result, and
        // not a receipt: no completion is claimed for any attempt.
        RefusalKind::AlreadyGripped => format!(
            "no new repair attempt is offered for {subject}: {}. This is a static reading of the current evidence, not a runtime-verified result, and no completion is claimed for any attempt; {recheck} to re-read the seam",
            refusal.reason
        ),
        RefusalKind::RouteNotReady => {
            let work = match &refusal.related_test {
                Some(test) => {
                    format!("strengthening the oracle of the related test {test} by hand")
                }
                None => "adding or strengthening a test for that seam by hand".to_string(),
            };
            format!(
                "no new repair attempt is offered for {subject}: {} (the repair eligibility gate behind `ripr repair` and the before phase refuses it). The remaining work is {work}; then {recheck} to re-read the seam",
                refusal.reason
            )
        }
        RefusalKind::Other => format!(
            "no new repair attempt is offered for {subject}: {} (the repair eligibility gate behind `ripr repair` and the before phase refuses it); {recheck} to re-read the seam",
            refusal.reason
        ),
    }
}

/// The gap-bound start admission (#9103, RIPR-SPEC-0311; Python #10373) for
/// one finding of a check run: a TypeScript or Python finding that is not
/// static-limited, from a run that recorded its diff base, whose GapRecord the
/// shared agent-packet validator accepts. This is the single admission both
/// `ripr check` (the printed start line) and `ripr agent status` (the routed
/// warning, #10001) consult; it forks no validator and fails closed.
fn gap_bound_route(
    output: &super::CheckOutput,
    check_config: &crate::config::RiprConfig,
    finding: &crate::domain::Finding,
) -> Result<Option<crate::app::GapRepairRoute>, super::navigation::RepairRouteRefusal> {
    use super::navigation::RepairRouteRefusal::{AnalysisIncomplete, GapRouteUnavailable};
    let Some((base, record)) = gap_bound_record(output, check_config, finding) else {
        return Ok(None);
    };
    // #11136: the after phase refuses a before artifact whose analysis is
    // incomplete, so a start printed beside "analysis incomplete" cannot
    // finish. Ask the same predicate the verify/after phases apply instead of
    // forking one; fail closed when the run carries no outcome at all.
    let analysis_complete = output
        .analysis_outcome
        .as_ref()
        .map(|outcome| outcome.kind.is_complete());
    if !crate::agent::artifact::check_findings_outcome_is_complete(analysis_complete, None) {
        return Err(AnalysisIncomplete {
            limitation: incomplete_limitation_summary(output),
        });
    }
    let uncommitted_files =
        uncommitted_production_files(&output.root, &record).map_err(|_error| {
            GapRouteUnavailable {
                reason: "could not validate the prerequisites for the repair start",
            }
        })?;
    let repair_base = if uncommitted_files.is_empty() {
        base
    } else {
        if output
            .analyzed_revisions
            .as_ref()
            .is_some_and(|r| !r.working_tree)
        {
            return Err(GapRouteUnavailable {
                reason: "the committed-only check excluded dirty tracked files; commit or restore them before rerunning",
            });
        }
        if output.unanalyzed_working_tree {
            return Err(GapRouteUnavailable {
                reason: "the working tree contains source or test changes that were not analyzed",
            });
        }
        let origin = resolved_dirty_origin(&output.root, output.analyzed_revisions.as_ref())
            .map_err(|reason| GapRouteUnavailable { reason })?;
        return Ok(Some(crate::app::GapRepairRoute {
            finding_id: finding.id.clone(),
            gap_id: record.canonical_gap_id,
            base: origin,
            uncommitted_files,
        }));
    };
    Ok(Some(crate::app::GapRepairRoute {
        finding_id: finding.id.clone(),
        gap_id: record.canonical_gap_id,
        base: repair_base.to_string(),
        uncommitted_files,
    }))
}

/// The named cause of an incomplete run for the withheld repair start
/// (#11136): the first limitation the producer disclosed, in plain words with
/// its file and recovery, or the bare fact when the run carries none.
pub(crate) fn incomplete_limitation_summary(output: &super::CheckOutput) -> String {
    let Some(limitation) = output
        .analysis_outcome
        .as_ref()
        .and_then(|outcome| outcome.limitations.first())
    else {
        return "the run disclosed no analysis outcome".to_string();
    };
    let mut summary = limitation.kind.plain_label().to_string();
    if let Some(path) = &limitation.path {
        summary.push_str(&format!(" (in `{path}`)"));
    }
    summary.push_str(&format!(
        "; to proceed: {}",
        limitation.recovery.detail.trim_end_matches('.')
    ));
    summary
}

fn resolved_dirty_origin(
    root: &Path,
    revisions: Option<&crate::analysis::AnalyzedRevisions>,
) -> Result<String, &'static str> {
    let _ = root;
    revisions
        .and_then(|r| r.loaded_diff_origin.as_ref())
        .cloned()
        .ok_or("could not establish the analyzed diff origin")
}

fn gap_bound_record<'a>(
    output: &'a super::CheckOutput,
    check_config: &crate::config::RiprConfig,
    finding: &crate::domain::Finding,
) -> Option<(&'a str, crate::output::gap_decision_ledger::GapRecord)> {
    if super::gap_bound_repair::finding_is_static_limited(finding) {
        return None;
    }
    let base = output.base.as_deref()?;
    let record = match non_rust_language(&finding.probe.location.file)? {
        "TypeScript" => super::gap_bound_repair::startable_typescript_gap_record(finding)?,
        "Python" => {
            super::gap_bound_repair::startable_python_gap_record(output, check_config, finding)?
        }
        _ => return None,
    };
    Some((base, record))
}

/// Dirty paths the gap-bound start would refuse to begin from (#10698),
/// spelled for the user's working directory. The start refuses every dirty
/// tracked path outside the packet's allowed edit surface (normally only the
/// selected test file), so this asks the same packet, policy and
/// `trusted_head_surface_violations` predicate the start itself uses rather
/// than guessing from path shapes. If the packet, policy, HEAD or Git cannot answer, the route is withheld.
fn uncommitted_production_files(
    root: &Path,
    record: &crate::output::gap_decision_ledger::GapRecord,
) -> Result<Vec<String>, String> {
    let display = format!(
        "target/ripr/workflow/{}",
        super::gap_bound_repair::GAP_BOUND_BEFORE_SNAPSHOT_FILE
    );
    let packet =
        crate::output::agent_seam_packets::render_gap_bound_repair_packet(&display, record)?;
    let policy = crate::app::repair_attempt::edit_cage_policy_from_packet(
        &packet,
        &record.canonical_gap_id,
    )?;
    dirty_paths_outside_surface(root, &policy)
}

/// The dirty tracked paths the start's own premise check refuses, respelled
/// relative to `root`.
fn dirty_paths_outside_surface(
    root: &Path,
    policy: &crate::edit_cage::EditCagePolicy,
) -> Result<Vec<String>, String> {
    let head = crate::agent::artifact::current_git_head(root).map_err(|e| e.to_string())?;
    let violations =
        crate::app::repair_attempt::trusted_head_surface_violations(root, policy, &[], &head)
            .map_err(|e| e.to_string())?;
    let prefix = repo_prefix(root)?;
    Ok(violations
        .iter()
        .map(|path| relative_to_cwd_root(path, &prefix))
        .collect())
}

/// `git rev-parse --show-prefix`: the analyzed root's path inside its
/// repository (empty at the top, otherwise `dir/sub/`).
fn repo_prefix(root: &Path) -> Result<String, String> {
    crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["rev-parse", "--show-prefix"],
        std::time::Duration::from_secs(10),
        64 * 1024,
    )
    .ok()
    .filter(|output| output.status.success())
    .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    .ok_or_else(|| "could not resolve repository prefix".to_string())
}

/// A repository-root-relative path respelled relative to the analyzed root,
/// which is where the user runs the printed `git commit`.
fn relative_to_cwd_root(path: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return path.to_string();
    }
    if let Some(rest) = path.strip_prefix(prefix) {
        return rest.to_string();
    }
    let depth = prefix.split('/').filter(|part| !part.is_empty()).count();
    format!("{}{path}", "../".repeat(depth))
}

/// The gap-bound start the default human `ripr check` would print for this
/// run's selected finding, or `None` when check would print none (#10001).
pub(crate) fn selected_gap_bound_route(
    output: &super::CheckOutput,
    check_config: &crate::config::RiprConfig,
) -> Option<crate::app::GapRepairRoute> {
    let finding = crate::output::human::selected_triage_finding(output, check_config)?;
    gap_bound_route(output, check_config, finding)
        .ok()
        .flatten()
}

/// The repair route for the one finding the default human `ripr check`
/// selects (#8166 Slice C), or the named refusal when that finding may not
/// be offered a repair start (#8729). The seam↔finding binding is the
/// shared
/// [`finding_for_seam`](super::repair_card_handoff::finding_for_seam)
/// authority, and admission is the same [`repair_refusal`] gate
/// the explicit resolver applies, so the printed seam ID round-trips
/// through [`resolve_repair_subject`] to the same seam.
///
/// The classified seams are derived only from the evidence this check
/// run already computed: the selected finding's canonical gap names its
/// file, and its related tests name the files that carry their evidence.
/// Only that file's seams are classified, with test-grip evidence from
/// the finding's own related-test files
/// ([`classify_seams_in_file_with_shared_index`](crate::analysis::classify_seams_in_file_with_shared_index)).
/// A fresh workspace inventory — full or diff-scoped — is never built
/// here: on a large workspace either one builds a workspace-wide index
/// and evidence context, and the full inventory additionally classified
/// every seam in the repo, spinning human `ripr check` indefinitely
/// (#8320). Scoping to the finding's file is sound for the binding
/// because both binding branches require the seam's file to equal the
/// finding's gap file, so no seam outside that file could claim this
/// finding. Fail-closed throughout: a seam-limit-truncated candidate
/// set, zero or several binding seams, an ineligible or policy-omitted
/// seam, and any unreadable authority refuse with a named reason rather
/// than a guessed or silent route. `Silent` is deliberate only: the
/// renderer never prints a runnable line for a static-limited finding
/// (F3 — its limitation guidance on the same screen is authoritative), so
/// an eligible seam under a static-limited finding still yields no route,
/// and a run with no selected finding has nothing to name. A refusal the
/// pipeline did compute is named even for a static-limited finding (#8729):
/// "the gate refused this seam" must not read as "analysis could not
/// decide". When bounded evidence cannot decide eligibility the refusal
/// names the state; `ripr repair` itself re-checks admission against the
/// full inventory before starting any attempt.
pub(crate) fn check_repair_route(
    root: &Path,
    check_config: &crate::config::RiprConfig,
    output: &super::CheckOutput,
    seam_index: &mut Option<crate::analysis::FileSeamIndex>,
) -> crate::app::CheckRepairRoute {
    use super::navigation::{CheckRepairRoute, RepairRouteRefusal};
    let Some(finding) = crate::output::human::selected_triage_finding(output, check_config) else {
        return CheckRepairRoute::Silent;
    };
    // #8166 residual (F3): a static-limited finding never gets a runnable
    // repair line — the same static-limited definition the human triage
    // state uses (a named static limitation, or a class static analysis
    // could not establish a path for). Offering a repair start beside
    // "inspect the static limitation before treating this as repair-ready"
    // contradicts that guidance on one screen. The renderer enforces F3 on
    // the `Route` arm; here the eligibility pipeline still runs so that a
    // computed refusal is named instead of silent (#8729): a static-limited
    // finding whose seam the gate refused must not read as "analysis could
    // not decide" either.
    let static_limited = super::gap_bound_repair::finding_is_static_limited(finding);
    // #10702: an `exposed` finding means the related tests already notice the
    // change. That is `check`'s own verdict; the seam view below can only
    // disagree by reading older or wider evidence, so it must not surface a
    // "missing evidence" or "no seam" refusal beside an exposed finding.
    if finding.class == crate::domain::ExposureClass::Exposed && !static_limited {
        return CheckRepairRoute::Refused(RepairRouteRefusal::FindingAlreadyExposed);
    }
    // #9103 (RIPR-SPEC-0311): a TypeScript preview finding carries no Rust
    // canonical gap, but its GapRecord validates through the shared
    // agent-packet validator and starts a gap-bound attempt. The canonical
    // gap id and the base this run analyzed are the whole start. A finding
    // that does not validate, a static-limited one, and a run without a base
    // keep the refusals below. A Python finding takes the same route
    // (#10373); the other routed languages are unchanged.
    if !static_limited {
        match gap_bound_route(output, check_config, finding) {
            Ok(Some(route)) => return CheckRepairRoute::GapRoute(route),
            Err(refusal) => return CheckRepairRoute::Refused(refusal),
            Ok(None) => {}
        }
    }
    let Some(gap) = finding.canonical_gap.as_ref() else {
        // A TypeScript/JavaScript preview finding never has a Rust canonical
        // gap; its start is the gap-bound packet route above, so "no seam" is
        // the wrong reason. Name the packet state when a plain blocker is
        // known (unresolved expected value, moved assertion selection); other
        // causes keep the existing line until their wording is owned.
        if let Some(language) = finding.language.filter(|language| {
            matches!(
                language,
                crate::domain::LanguageId::TypeScript | crate::domain::LanguageId::JavaScript
            )
        }) && let Some(reason) =
            crate::output::typescript_packet_projection::typescript_advisory_blocker(finding)
        {
            return CheckRepairRoute::Refused(RepairRouteRefusal::PreviewPacketNotComplete {
                language: language.display_name().to_string(),
                reason,
            });
        }
        return CheckRepairRoute::Refused(RepairRouteRefusal::NoCanonicalGap);
    };
    // #9102: the seam inventory `ripr repair` binds from is built from the
    // Rust index only. A finding in another routed language therefore has no
    // seam to bind, whatever its gap, test node and verify command say; name
    // that terminal state instead of the misleading "no seam binds" refusal.
    if let Some(language) = non_rust_language(Path::new(&gap.file)) {
        return CheckRepairRoute::Refused(RepairRouteRefusal::NoSeamInventory {
            language: language.to_string(),
            file: gap.file.clone(),
        });
    }
    let Ok(config) = load_for_root(root) else {
        return CheckRepairRoute::Refused(RepairRouteRefusal::ConfigUnreadable);
    };
    let evidence_files: Vec<std::path::PathBuf> = finding
        .related_tests
        .iter()
        .filter_map(|test| {
            let relative = if test.file.is_absolute() {
                test.file.strip_prefix(root).ok()?.to_path_buf()
            } else {
                test.file.clone()
            };
            (relative.extension().is_some_and(|ext| ext == "rs") && root.join(&relative).is_file())
                .then_some(relative)
        })
        .collect();
    let Ok(classified) = crate::analysis::classify_seams_in_file_with_shared_index(
        root,
        &config,
        Path::new(&gap.file),
        &evidence_files,
        seam_index,
    ) else {
        return CheckRepairRoute::Refused(RepairRouteRefusal::ClassificationFailed);
    };
    match crate::analysis::repo_exposure_seam_limit() {
        Ok(Some((limit, _))) if classified.len() > limit => {
            return CheckRepairRoute::Refused(RepairRouteRefusal::SeamLimit);
        }
        Ok(_) => {}
        Err(_) => return CheckRepairRoute::Refused(RepairRouteRefusal::SeamLimit),
    }
    let policy = AgentBriefPolicy::from_config(&config);
    let mut bound = classified.iter().filter(|entry| {
        let canonical_gap_id = repair_packet_eligibility(entry).readiness.canonical_gap_id;
        super::repair_card_handoff::finding_for_seam(
            &output.findings,
            entry,
            canonical_gap_id.as_deref(),
            &classified,
        )
        .is_some_and(|bound_finding| bound_finding.id == finding.id)
    });
    let Some(entry) = bound.next() else {
        // #10702: a strongly gripped seam has no canonical gap id, so the
        // id-keyed join above never binds it. Name it by file, owner, kind
        // and line instead, and only when it is the one such seam: the
        // disagreement is then the whole story, not "no seam".
        let mut gripped = classified.iter().filter(|entry| {
            entry.class == crate::analysis::seams::SeamGripClass::StronglyGripped
                && super::repair_card_handoff::finding_names_seam_by_location(
                    finding,
                    entry,
                    &classified,
                )
        });
        if gripped.next().is_some() && gripped.next().is_none() {
            return CheckRepairRoute::Refused(RepairRouteRefusal::SeamGrippedFindingNot);
        }
        return CheckRepairRoute::Refused(RepairRouteRefusal::NoBindingSeam {
            seams_in_file: classified.len(),
        });
    };
    if bound.next().is_some() {
        return CheckRepairRoute::Refused(RepairRouteRefusal::SeveralBindingSeams);
    }
    if let Some((kind, reason)) = repair_refusal(entry, policy, RefusalRoot::named(root)) {
        // A bound seam carries a canonical gap id, which a strongly gripped
        // seam never does, so `AlreadyGripped` cannot reach here: that case is
        // named by the location fallback above (#10702).
        // #9244: carry the typed kind and the producer-owned missing list so
        // the check screen words a not-ready route without parsing `reason`.
        let missing = repair_packet_eligibility(entry).readiness.missing_evidence;
        return CheckRepairRoute::Refused(RepairRouteRefusal::Ineligible {
            reason,
            kind,
            missing,
            // #10622: a related test already reaching the boundary input
            // makes the weak oracle, not the input, the remaining work.
            boundary_input_observed: finding.boundary_input_observed_with_weak_oracle(),
        });
    }
    if static_limited {
        // F3: no runnable line for a static-limited finding. The refusal
        // arms above still named any concrete ineligibility; reaching here
        // means the seam was eligible, so the limitation guidance on the
        // same screen is the whole story and no advisory is added.
        return CheckRepairRoute::Silent;
    }
    // `ripr repair` confines a production-file target to its inline test
    // module and starts only from the committed file. Ask the edit cage's own
    // predicate so the check screen never offers a start the attempt refuses.
    let uncommitted_target = held_inline_target(root, &repair_packet_eligibility(entry).readiness);
    CheckRepairRoute::Route(crate::app::RepairRoute {
        finding_id: finding.id.clone(),
        seam_id: entry.seam.id().as_str().to_string(),
        uncommitted_target,
    })
}

fn ineligible_repair_item(root: &Path, item: &str, seam_id: &str, reason: &str) -> String {
    let list = format!(
        "`ripr pilot --root {}`",
        shell_arg(&bound_root(&root.to_string_lossy()))
    );
    format!(
        "repair item `{item}` names seam `{seam_id}`, which is not ready for an automatic repair: {reason}; no repair attempt was started. Run {list} to list current seam IDs"
    )
}

/// Display name of the routed non-Rust language a file belongs to, or
/// `None` for Rust and for files no adapter routes. Reads the language
/// router, the same authority the pipeline dispatches on.
fn non_rust_language(file: &Path) -> Option<&'static str> {
    match crate::analysis::route(file)? {
        crate::domain::LanguageId::Rust => None,
        language => Some(language.display_name()),
    }
}

/// A non-Rust language the changed code under the selected root touches, and
/// whether any of its changes adds or edits lines (`false`: deletion-only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChangedNonRust {
    pub(crate) language: &'static str,
    pub(crate) adds_code: bool,
}

/// Why repair could not narrow its spots to the change, when it could not
/// (#11246): no diff was readable (no base resolved), or the diff against the
/// base changed nothing. A readable diff with changes is not unscoped here.
fn unscoped_diff_note(
    diff: Option<&[crate::analysis::ChangedFile]>,
    base: Option<&str>,
) -> Option<String> {
    match (diff, base) {
        (None, Some(base)) => Some(format!(
            "ripr could not tell which lines you changed, because it could not read a diff against `{base}`, so the spots below are not narrowed to your change; check that `{base}` names a ref that exists here"
        )),
        (None, None) => Some(
            "ripr could not tell which lines you changed, because no base to compare against could be found (the same base `ripr check` uses), so the spots below are not narrowed to your change; pass `--base <ref>` to name it".to_string(),
        ),
        (Some([]), Some(base)) => Some(format!(
            "ripr could not tell which lines you changed, because nothing differs from `{base}`, so the spots below are not narrowed to your change; pass `--base <ref>` to name another base"
        )),
        (Some([]), None) => Some(
            "ripr could not tell which lines you changed, because nothing differs from the base `ripr check` compares against (for example, you are on the default branch itself), so the spots below are not narrowed to your change; pass `--base <ref>` to name another base".to_string(),
        ),
        (Some(_), _) => None,
    }
}

#[cfg(test)]
mod unscoped_note_tests {
    use super::unscoped_diff_note;

    #[test]
    fn unreadable_diff_says_no_base_could_be_found() {
        let note = unscoped_diff_note(None, None).unwrap_or_default();
        assert!(note.contains("could not tell which lines you changed"));
        assert!(note.contains("no base to compare against could be found"));
        assert!(note.contains("--base <ref>"));
    }

    #[test]
    fn an_explicit_base_is_named_when_its_diff_cannot_be_read() {
        let unreadable = unscoped_diff_note(None, Some("no-such-ref")).unwrap_or_default();
        assert!(unreadable.contains("could not read a diff against `no-such-ref`"));
        assert!(!unreadable.contains("no base to compare against could be found"));
        let empty = unscoped_diff_note(Some(&[]), Some("main")).unwrap_or_default();
        assert!(empty.contains("nothing differs from `main`"));
    }

    #[test]
    fn empty_diff_says_nothing_differs_from_the_base() {
        let note = unscoped_diff_note(Some(&[]), None).unwrap_or_default();
        assert!(note.contains("could not tell which lines you changed"));
        assert!(note.contains("nothing differs from the base"));
    }

    #[test]
    fn a_readable_nonempty_diff_adds_no_note() {
        let files = crate::analysis::parse_unified_diff(
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-a\n+b\n",
        );
        assert!(!files.is_empty(), "fixture must parse to a changed file");
        assert_eq!(unscoped_diff_note(Some(&files), None), None);
        assert_eq!(unscoped_diff_note(Some(&files), Some("main")), None);
    }
}

/// The changed files of the diff a bare `ripr check` would read under `root`
/// (or under an explicit `base`, which wins over inference): check's own
/// diff-source selection (committed history, or the working tree when it
/// holds uncommitted work), base resolution and root scoping through the
/// shared loaders. `None` when no such diff is readable.
fn live_check_diff_files(
    root: &Path,
    base: Option<&str>,
) -> Option<Vec<crate::analysis::ChangedFile>> {
    let source = super::diff_source::select_live_diff_source(
        root,
        super::diff_source::DiffSourceRequest::Default,
        None,
    );
    let text = if source.is_working_tree() {
        crate::analysis::load_worktree_diff(root, base, None)
    } else {
        crate::analysis::load_diff(root, base, None, None)
    };
    text.ok()
        .map(|text| crate::analysis::parse_unified_diff(&text))
}

/// The non-Rust languages in the diff `ripr check` would analyze under `root`
/// (#10682). Reads check's own diff-source selection (committed history, or
/// the working tree when it holds uncommitted work), base resolution and
/// root scoping through the shared loaders, and the diff parser's own path
/// decoding, so a committed-only branch, a package root and a C-quoted
/// filename agree with `check`. Best effort: an unreadable diff reads as no
/// such languages, so the caller keeps its generic message.
pub(crate) fn changed_non_rust_languages(root: &Path, base: Option<&str>) -> Vec<ChangedNonRust> {
    let Some(files) = live_check_diff_files(root, base) else {
        return Vec::new();
    };
    let mut found: Vec<ChangedNonRust> = Vec::new();
    for file in files {
        let Some(language) = non_rust_language(&file.path) else {
            continue;
        };
        let adds_code = !file.added_lines.is_empty();
        match found.iter_mut().find(|entry| entry.language == language) {
            Some(entry) => entry.adds_code |= adds_code,
            None => found.push(ChangedNonRust {
                language,
                adds_code,
            }),
        }
    }
    found.sort_by_key(|entry| entry.language);
    found
}

/// The `ripr repair` zero-seam message when the changed code is non-Rust
/// (#10682): the seam inventory behind `ripr repair` covers Rust only, so
/// "analyze the workspace" would loop through `ripr pilot` and `ripr check`.
/// Names the gap-bound start for TypeScript and Python (#10373) and the
/// manual route otherwise. A deletion-only change gets no `check` finding,
/// so it is not promised one.
pub(crate) fn non_rust_repair_guidance(root_display: &str, changed: &[ChangedNonRust]) -> String {
    let languages = changed
        .iter()
        .map(|entry| entry.language)
        .collect::<Vec<_>>()
        .join(" and ");
    let head = format!(
        "the changed code under {root_display} is {languages}, and `ripr repair` works from Rust seams only"
    );
    if !changed.iter().any(|entry| entry.adds_code) {
        return format!(
            "{head}. The change only removes code, so `ripr check --root {root_display}` names no gap for it; look at the tests of the remaining behavior by hand"
        );
    }
    let start = if changed
        .iter()
        .any(|entry| !gap_bound_start_pointer(entry.language).is_empty())
    {
        "; once it is committed and the repair packet is complete, `ripr check` also prints a `ripr agent repair --gap-id <gap> --base <base> --phase before` start"
    } else {
        ""
    };
    format!(
        "{head}. Run `ripr check --root {root_display}` for the test to add by hand and its verify command{start}"
    )
}

/// Display name of the non-Rust language a repair item names, or `None` for
/// any other item: a canonical gap id (`gap:<language>:<file>:...`) or the
/// preview-adapter finding id `ripr check` prints
/// (`probe:<file>:<language>_preview:<hash>`). Reads only the typed language
/// wire strings, never a file name.
pub(crate) fn non_rust_item_language(item: &str) -> Option<&'static str> {
    let wire = if let Some(rest) = item.strip_prefix("gap:") {
        rest.split(':').next()?
    } else {
        let parts: Vec<&str> = item.split(':').collect();
        match parts.as_slice() {
            ["probe" | "repo-probe", _file, tagged, _hash] => tagged.strip_suffix("_preview")?,
            _ => return None,
        }
    };
    match crate::domain::LanguageId::from_wire(wire)? {
        crate::domain::LanguageId::Rust => None,
        language => Some(language.display_name()),
    }
}

/// The precise terminal reason a finding or gap in a non-Rust language gets
/// no repair start (#9102). `file` is the finding's file, or empty when only
/// the gap id names the language.
pub(crate) fn no_seam_inventory_reason(language: &str, file: &str) -> String {
    render_no_seam_inventory_reason(language, file, false)
}

/// The same terminal reason in the plain words the default `ripr check`
/// screen uses (RIPR-SPEC-0122, glossary: seam to spot, discriminator to
/// missing check). Only the human check triage renders this; repair, agent
/// and JSON surfaces keep `no_seam_inventory_reason` (#11619).
pub(crate) fn no_seam_inventory_reason_plain(language: &str, file: &str) -> String {
    render_no_seam_inventory_reason(language, file, true)
}

fn render_no_seam_inventory_reason(language: &str, file: &str, plain: bool) -> String {
    let subject = if file.is_empty() {
        format!("this {language} gap")
    } else {
        format!("this {language} finding in `{file}`")
    };
    let verified_route = if language == "TypeScript" {
        // The check-findings artifact kind makes the manual TypeScript route
        // verifiable by gap id (#9103, RIPR-SPEC-0310).
        "; to verify the repair by gap id, save `ripr check --base <base> --format check-findings-json` before and after the edit, then run `ripr agent verify --before <before> --after <after> --gap-id <canonical gap printed by check> --json`, which reports the gap closed only when it reads exposed in the after snapshot"
    } else {
        ""
    };
    let (inventory, start_noun, missing) = if plain {
        ("spot list", "spot", "check")
    } else {
        ("seam inventory", "seam", "discriminator")
    };
    format!(
        "the {inventory} behind `ripr repair` and `ripr agent repair --phase before` covers Rust files only, so {subject} has no {start_noun} to start a repair from; this is a terminal state of the delegated loop, not a stale id. The gap, test node and verify command that `ripr check` prints are the manual route: add the missing {missing} to that test by hand and run the printed verify command{verified_route}"
    )
}

/// The pointer a seam-addressed refusal of a TypeScript or Python item adds
/// (#10373): such a finding can still start the delegated loop by gap, so
/// "terminal" must not read as "no repair start exists anywhere". Empty for
/// the other non-Rust languages.
pub(crate) fn gap_bound_start_pointer(language: &str) -> &'static str {
    if matches!(language, "TypeScript" | "Python") {
        ". When the finding's repair packet is complete and the check analysis is complete, `ripr check --base <base>` prints a gap-bound start (`ripr agent repair --gap-id <gap> --base <base> --phase before`) that runs the delegated loop by gap instead of by seam"
    } else {
        ""
    }
}

fn unknown_repair_item(root: &Path, item: &str) -> String {
    unknown_repair_item_reason(Some(root), item)
}

/// The refusal sentence for a repair item that names no seam. `ripr repair`
/// words it with its selected root; the MCP adapter, whose verdicts are
/// already relative to its workspace root, passes `None` and so shares the
/// cause text without an emitted root (#10718).
pub(crate) fn unknown_repair_item_reason(root: Option<&Path>, item: &str) -> String {
    if let Some(language) = non_rust_item_language(item) {
        return format!(
            "repair item `{item}` names no seam in the selected root: {}{}",
            no_seam_inventory_reason(language, ""),
            gap_bound_start_pointer(language)
        );
    }
    let list = match root {
        Some(root) => format!(
            "`ripr pilot --root {}` to list current seam IDs",
            shell_arg(&bound_root(&root.to_string_lossy()))
        ),
        None => "`ripr pilot` to list current seam IDs".to_string(),
    };
    if item.starts_with("probe:") || item.starts_with("repo-probe:") {
        return format!(
            "repair item `{item}` names no seam in the selected root; the `ripr check` finding ID is stale or maps to no current seam; run {list}"
        );
    }
    format!("repair item `{item}` names no seam in the selected root; run {list}")
}

/// One candidate in an ambiguous-item listing: the seam ID (what the retry
/// takes) followed by what distinguishes it, so several seams on one line can
/// be told apart without opening `ripr pilot` (#11466).
fn describe_ambiguous_seam(entry: &crate::analysis::ClassifiedSeam) -> String {
    const MAX_EXPRESSION: usize = 48;
    let expression = entry
        .seam
        .expression()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let expression = if expression.chars().count() > MAX_EXPRESSION {
        let cut: String = expression.chars().take(MAX_EXPRESSION).collect();
        format!("{cut}...")
    } else {
        expression
    };
    format!(
        "{} ({} `{expression}` in {})",
        entry.seam.id().as_str(),
        entry.seam.kind().as_str(),
        entry.seam.owner()
    )
}

fn ambiguous_gap_item(root: &Path, item: &str, matches: &[&str]) -> String {
    const MAX_LISTED: usize = 8;
    let mut listed: Vec<&str> = matches.iter().take(MAX_LISTED).copied().collect();
    listed.sort_unstable();
    let mut message = format!(
        "repair item `{item}` names {} seams; select one explicitly: {}",
        matches.len(),
        listed.join(", ")
    );
    if matches.len() > MAX_LISTED {
        message.push_str(&format!(" (and {} more)", matches.len() - MAX_LISTED));
    }
    message.push_str(&format!(
        ". Retry: `ripr repair <seam-id> --root {}`",
        shell_arg(&bound_root(&root.to_string_lossy()))
    ));
    message
}

/// Message when the façade's follow-on card render fails after the
/// before phase already published `attempt_id`: the start succeeded, so
/// the message names the attempt and its recovery instead of implying no
/// attempt exists. The card's own reason is preserved verbatim after the
/// start facts; a retry must reuse this attempt, not start another.
pub(crate) fn card_after_publish_message(
    attempt_id: &str,
    seam_id: &str,
    root_display: &str,
    card_error: &str,
) -> String {
    format!(
        "repair started attempt `{attempt_id}` for seam `{seam_id}`, but the handoff card is unavailable: {card_error}. The attempt awaits its edit; inspect it with `ripr status --attempt {attempt_id} --root {root_display}`, or continue after editing with `ripr continue --attempt {attempt_id} --root {root_display}`"
    )
}

/// The typed reason behind a `ripr repair` / `ripr continue` refusal (#11597).
///
/// The exit code says only "decision" (3) or "could not complete" (2); this
/// closed vocabulary says which state produced it, so a script switches on
/// the code instead of parsing prose. One owner: the façade and the shared
/// after phase both print [`RepairRefusalCode::stderr_line`], and a JSON
/// carrier added later reads the same value. Prose is unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RepairRefusalCode {
    /// `repair <item>`: no current seam is named by the item.
    ItemNamesNoSeam,
    /// `repair <item>`: the item names several seams.
    ItemNamesSeveralSeams,
    /// `repair <item>`: the one seam it names is not repair-eligible.
    SeamNotRepairEligible,
    /// Bare `repair`: no seam is repair-eligible.
    NoEligibleSeam,
    /// Bare `repair`: several eligible seams; the caller selects one.
    SeveralEligibleSeams,
    /// Bare `repair`: the selector needs an explicit item.
    RepairNeedsItem,
    /// The start was refused: the focused test file has uncommitted changes.
    StartRefusedUncommittedTarget,
    /// The start was refused: no test file or module ripr can safely edit.
    NoTestRoute,
    /// `continue`: the attempt awaits the focused test edit; retry after it.
    AwaitingEdit,
    /// `continue`: the attempt spent its after-phase retry budget.
    RetryBudgetExhausted,
    /// `continue`: no current attempt exists.
    NoCurrentAttempt,
    /// `continue`: several current attempts; select one with `--attempt`.
    AmbiguousAttempts,
    /// `continue`: the attempt ended without a receipt.
    AttemptEndedWithoutReceipt,
    /// `continue`: the attempt finished but no compliant receipt confirms it.
    AttemptCompletionUnconfirmed,
    /// `continue`: the edit cage rejected the edit; the attempt is terminal.
    EditCageViolation,
    /// `continue`: HEAD moved after the repair started; the attempt is stale.
    AttemptStale,
    /// `continue`: the cage could not compare the files; the attempt is terminal.
    AttemptIncomparable,
}

/// What the caller can do about a refusal, so retryable and terminal states
/// are not told apart by prose (#11597).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RepairRefusalDisposition {
    /// The same attempt can proceed after the stated action.
    Retryable,
    /// The attempt is over; only a fresh `ripr repair` can proceed.
    Terminal,
    /// The caller must name one of several choices.
    NeedsSelection,
    /// Nothing started; fix the stated cause and run `ripr repair` again.
    NotStarted,
}

impl RepairRefusalDisposition {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Terminal => "terminal",
            Self::NeedsSelection => "needs_selection",
            Self::NotStarted => "not_started",
        }
    }
}

impl RepairRefusalCode {
    /// Every code, for the docs/help parity test.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 17] = [
        Self::ItemNamesNoSeam,
        Self::ItemNamesSeveralSeams,
        Self::SeamNotRepairEligible,
        Self::NoEligibleSeam,
        Self::SeveralEligibleSeams,
        Self::RepairNeedsItem,
        Self::StartRefusedUncommittedTarget,
        Self::NoTestRoute,
        Self::AwaitingEdit,
        Self::RetryBudgetExhausted,
        Self::NoCurrentAttempt,
        Self::AmbiguousAttempts,
        Self::AttemptEndedWithoutReceipt,
        Self::AttemptCompletionUnconfirmed,
        Self::EditCageViolation,
        Self::AttemptStale,
        Self::AttemptIncomparable,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ItemNamesNoSeam => "item_names_no_seam",
            Self::ItemNamesSeveralSeams => "item_names_several_seams",
            Self::SeamNotRepairEligible => "seam_not_repair_eligible",
            Self::NoEligibleSeam => "no_eligible_seam",
            Self::SeveralEligibleSeams => "several_eligible_seams",
            Self::RepairNeedsItem => "repair_needs_item",
            Self::StartRefusedUncommittedTarget => "start_refused_uncommitted_target",
            Self::NoTestRoute => "no_test_route",
            Self::AwaitingEdit => "awaiting_edit",
            Self::RetryBudgetExhausted => "retry_budget_exhausted",
            Self::NoCurrentAttempt => "no_current_attempt",
            Self::AmbiguousAttempts => "ambiguous_attempts",
            Self::AttemptEndedWithoutReceipt => "attempt_ended_without_receipt",
            Self::AttemptCompletionUnconfirmed => "attempt_completion_unconfirmed",
            Self::EditCageViolation => "edit_cage_violation",
            Self::AttemptStale => "attempt_stale",
            Self::AttemptIncomparable => "attempt_incomparable",
        }
    }

    pub(crate) const fn disposition(self) -> RepairRefusalDisposition {
        match self {
            Self::AwaitingEdit => RepairRefusalDisposition::Retryable,
            Self::RetryBudgetExhausted
            | Self::AttemptEndedWithoutReceipt
            | Self::AttemptCompletionUnconfirmed
            | Self::EditCageViolation
            | Self::AttemptStale
            | Self::AttemptIncomparable => RepairRefusalDisposition::Terminal,
            Self::SeveralEligibleSeams
            | Self::ItemNamesSeveralSeams
            | Self::AmbiguousAttempts
            | Self::RepairNeedsItem => RepairRefusalDisposition::NeedsSelection,
            Self::ItemNamesNoSeam
            | Self::SeamNotRepairEligible
            | Self::NoEligibleSeam
            | Self::StartRefusedUncommittedTarget
            | Self::NoTestRoute
            | Self::NoCurrentAttempt => RepairRefusalDisposition::NotStarted,
        }
    }

    /// The code an attempt that the after phase finished as not
    /// receipt-ready carries, from its durable state.
    pub(crate) fn for_finished_state(state: &RepairAttemptState) -> Self {
        match state {
            RepairAttemptState::Stale => Self::AttemptStale,
            RepairAttemptState::Incomparable => Self::AttemptIncomparable,
            _ => Self::EditCageViolation,
        }
    }

    /// The one stderr line a script greps for, printed beside the refusal
    /// prose: `ripr: refusal code=<code> disposition=<disposition>`.
    pub(crate) fn stderr_line(self) -> String {
        format!(
            "ripr: refusal code={} disposition={}",
            self.as_str(),
            self.disposition().as_str()
        )
    }
}

/// What `ripr continue` resolved to: run the after path, report an ended
/// attempt, or ask for a current attempt explicitly.
pub(crate) enum ContinueSelection {
    /// Exactly one current attempt is eligible: run the after path for it.
    /// Awaiting attempts proceed; an explicitly named prepared attempt
    /// proceeds into the shared after selection, which names its honest
    /// restart instead of running.
    Proceed { attempt_id: RepairAttemptId },
    /// The selected attempt already ended: report it, do not run.
    AlreadyComplete {
        report: Box<AgentAttemptStatusReport>,
    },
    /// The selected attempt ended without a receipt (`stale`, `failed`,
    /// or `incomparable`): report it, but refuse with a typed outcome
    /// instead of claiming completion. Scripts must not read a failed
    /// repair as finished.
    EndedUnsuccessfully {
        report: Box<AgentAttemptStatusReport>,
    },
    /// The selected attempt's manifest says `ready_to_finish`, but the
    /// status authority's fail-closed projection cannot confirm a
    /// compliant receipt (`limited`, `corrupt_or_unavailable`, or
    /// `legacy_compatibility_only`): the after phase committed, yet no
    /// retained evidence confirms the gap closed. Report the facts and
    /// refuse with a typed outcome instead of claiming completion
    /// (#7032): the manifest state alone is not a completion claim.
    EndedUnconfirmed {
        report: Box<AgentAttemptStatusReport>,
    },
    /// No current nonterminal attempt exists under the selected root.
    NoneAvailable {
        prepared: usize,
        terminal: usize,
        trust_bound: usize,
    },
    /// Several current attempts exist: select one explicitly.
    Ambiguous { candidates: Vec<String> },
}

/// Tally of the attempts an implicit `ripr continue` sees. The newest attempt
/// is tracked so a repeated `continue` after a confirmed finish reports that
/// attempt's already-complete status instead of refusing (#11597); that route
/// is only open when the newest creation time is unique, the attempt finished,
/// and no prepared or trust-bound attempt exists anywhere in the store.
#[derive(Default)]
struct ContinueTally {
    eligible: Vec<String>,
    prepared: usize,
    terminal: usize,
    trust_bound: usize,
    latest_ms: Option<u64>,
    latest_count: usize,
    latest_finished: Option<RepairAttemptId>,
}

impl ContinueTally {
    fn add(&mut self, manifest: &RepairAttemptManifest) {
        let created = manifest.created_unix_ms;
        if self.latest_ms.is_none_or(|ms| created > ms) {
            self.latest_ms = Some(created);
            self.latest_count = 1;
            self.latest_finished = matches!(manifest.state, RepairAttemptState::ReadyToFinish)
                .then(|| manifest.repair_attempt_id.clone());
        } else if self.latest_ms == Some(created) {
            self.latest_count += 1;
            self.latest_finished = None;
        }
        match manifest.state {
            RepairAttemptState::AwaitingEdit => {
                // The façade passes no edit authorization, so a trust-bound
                // attempt can never continue on this route: the after phase
                // would refuse it. Exclude it from implicit selection and name
                // the advanced route instead. The binding is an immutable
                // manifest fact, so this filter cannot go stale between
                // selection and the after phase. An explicitly named bound
                // attempt still proceeds into the shared after selection,
                // whose refusal names the missing signals.
                if find_manifest_artifact_by_role(
                    manifest,
                    crate::app::python_repair_binding::BINDING_ARTIFACT_ROLE,
                )
                .is_some()
                {
                    self.trust_bound += 1;
                } else if crate::app::repair_attempt::after_refusal_budget_exhausted(manifest) {
                    // #8726: a spent retry budget refuses the after phase, so
                    // the attempt is ended, not a continue candidate.
                    self.terminal += 1;
                } else {
                    self.eligible
                        .push(manifest.repair_attempt_id.as_str().to_string());
                }
            }
            RepairAttemptState::Prepared => self.prepared += 1,
            RepairAttemptState::ReadyToFinish
            | RepairAttemptState::Stale
            | RepairAttemptState::Incomparable
            | RepairAttemptState::Failed => self.terminal += 1,
        }
    }

    /// The attempt an implicit already-complete report may name, if any.
    fn already_complete_candidate(&self) -> Option<&RepairAttemptId> {
        match (
            &self.latest_finished,
            self.latest_count,
            self.prepared,
            self.trust_bound,
            self.eligible.is_empty(),
        ) {
            (Some(id), 1, 0, 0, true) => Some(id),
            _ => None,
        }
    }
}

/// Resolve the attempt `ripr continue` acts on. An explicit ID loads
/// directly, like the advanced route; without one, exactly one awaiting
/// attempt must be visible — several require selection and a corrupt row
/// refuses rather than letting a partial listing misselect.
pub(crate) fn select_continue_attempt(
    root: &Path,
    attempt_id: Option<&str>,
) -> Result<ContinueSelection, String> {
    if let Some(id) = attempt_id {
        let parsed = RepairAttemptId::parse(id.to_string())
            .map_err(|error| format!("continue --attempt: {error}"))?;
        let manifest = load_repair_attempt_manifest_from(root, None, &parsed)?;
        if matches!(
            manifest.state,
            RepairAttemptState::AwaitingEdit | RepairAttemptState::Prepared
        ) {
            return Ok(ContinueSelection::Proceed { attempt_id: parsed });
        }
        let report = build_agent_attempt_status(root, root, None, &parsed)?;
        // The status authority owns the completion claim: its fail-closed
        // projection (#4798) reads `finished_current`/`finished_historical`
        // only from a compliant, bound receipt. A `ready_to_finish` manifest
        // whose receipt is unissued, unbound, unreadable, or shows the gap
        // still open (`limited`), whose retained receipt is unavailable
        // (`corrupt_or_unavailable`), or that predates attempt-local
        // terminal retention (`legacy_compatibility_only`) stays
        // unconfirmed — the façade refuses instead of overriding the
        // authoritative reading. The other terminal states ended without
        // success; the CLI reports them and refuses too.
        if matches!(manifest.state, RepairAttemptState::ReadyToFinish) {
            if matches!(
                report.attempt.status_class,
                "finished_current" | "finished_historical"
            ) {
                return Ok(ContinueSelection::AlreadyComplete {
                    report: Box::new(report),
                });
            }
            return Ok(ContinueSelection::EndedUnconfirmed {
                report: Box::new(report),
            });
        }
        return Ok(ContinueSelection::EndedUnsuccessfully {
            report: Box::new(report),
        });
    }
    let mut tally = ContinueTally::default();
    for entry in inventory_repair_attempts_from(root, None)? {
        match entry {
            RepairAttemptInventoryEntry::Valid(manifest) => tally.add(&manifest),
            RepairAttemptInventoryEntry::Invalid { directory, error } => {
                return Err(format!(
                    "continue selection refused: attempt row `{directory}` is invalid: {error}"
                ));
            }
        }
    }
    let already_complete_id = tally.already_complete_candidate().cloned();
    let ContinueTally {
        eligible,
        prepared,
        terminal,
        trust_bound,
        ..
    } = tally;
    if eligible.len() > 1 {
        return Ok(ContinueSelection::Ambiguous {
            candidates: eligible,
        });
    }
    match eligible.into_iter().next() {
        Some(id) => Ok(ContinueSelection::Proceed {
            attempt_id: RepairAttemptId::parse(id)
                .map_err(|error| format!("continue selection: {error}"))?,
        }),
        None => {
            if let Some(id) = already_complete_id {
                let report = build_agent_attempt_status(root, root, None, &id)?;
                if matches!(
                    report.attempt.status_class,
                    "finished_current" | "finished_historical"
                ) {
                    return Ok(ContinueSelection::AlreadyComplete {
                        report: Box::new(report),
                    });
                }
            }
            Ok(ContinueSelection::NoneAvailable {
                prepared,
                terminal,
                trust_bound,
            })
        }
    }
}

/// An attempt a repeated `ripr repair` resumes instead of duplicating
/// (#8742).
pub(crate) struct ReusableAttempt {
    pub(crate) attempt_id: String,
    /// The persisted after-phase command; render it with
    /// [`crate::agent::human_after_phase_follow_up`].
    pub(crate) next_command: String,
}

/// The attempt a second `ripr repair` for `seam_id` resumes: an ordinary
/// attempt (no Python trust binding) for the same seam that is still
/// awaiting its edit and was prepared at exactly the current HEAD. Its
/// before evidence is the evidence a fresh start would record, so a second
/// attempt adds nothing and would make `ripr continue` ambiguous. Any
/// other state, another seam, or a moved HEAD starts a new attempt. An
/// unreadable store or HEAD also starts a new attempt: the attempt
/// authority then makes its own decision.
pub(crate) fn reusable_awaiting_attempt(root: &Path, seam_id: &str) -> Option<ReusableAttempt> {
    let head = current_git_head(root).ok()?;
    let entries = inventory_repair_attempts_from(root, None).ok()?;
    let manifests = entries.iter().filter_map(|entry| match entry {
        RepairAttemptInventoryEntry::Valid(manifest) => Some(manifest.as_ref()),
        RepairAttemptInventoryEntry::Invalid { .. } => None,
    });
    pick_reusable_attempt(manifests, seam_id, &head)
}

/// Decision law of [`reusable_awaiting_attempt`], free of the filesystem.
/// The newest matching attempt wins when earlier duplicates already exist.
fn pick_reusable_attempt<'a>(
    manifests: impl Iterator<Item = &'a RepairAttemptManifest>,
    seam_id: &str,
    head: &str,
) -> Option<ReusableAttempt> {
    manifests
        .filter(|manifest| {
            manifest.state == RepairAttemptState::AwaitingEdit
                && manifest.seam_id == seam_id
                && manifest.repository_head == head
                // #8726: an attempt whose after-phase retry budget is spent
                // refuses its after phase, so reusing it would strand `ripr
                // repair` on a dead attempt instead of starting a fresh one.
                && !crate::app::repair_attempt::after_refusal_budget_exhausted(manifest)
                && find_manifest_artifact_by_role(
                    manifest,
                    crate::app::python_repair_binding::BINDING_ARTIFACT_ROLE,
                )
                .is_none()
        })
        .max_by_key(|manifest| manifest.created_unix_ms)
        .map(|manifest| ReusableAttempt {
            attempt_id: manifest.repair_attempt_id.as_str().to_string(),
            next_command: manifest.next_command.clone(),
        })
}

/// The production file whose uncommitted changes would make `ripr repair`
/// refuse this seam: the selected edit target (an existing test target or a
/// producer-owned proposal, the same selection the repair packet's
/// recommended test file uses), when it is a non-test-surface Rust file that
/// differs from HEAD.
fn held_inline_target(
    root: &Path,
    readiness: &crate::analysis::repair_route::RepairRouteReadiness,
) -> Option<String> {
    use crate::analysis::repair_route::RepairTargetSelection;
    let file = match &readiness.target_selection {
        RepairTargetSelection::Existing(target) => target.file(),
        RepairTargetSelection::Proposed(proposal) => proposal.file.as_path(),
        RepairTargetSelection::Missing => return None,
    };
    let relative = file.strip_prefix(root).unwrap_or(file);
    let relative = relative.to_string_lossy().replace('\\', "/");
    (!Path::new(&relative).is_absolute()
        && !crate::analysis::is_test_surface_path(&relative)
        && Path::new(&relative)
            .extension()
            .is_some_and(|ext| ext == "rs")
        && crate::edit_cage::inline_target_is_uncommitted(root, &relative))
    .then_some(relative)
}

#[cfg(test)]
pub(crate) mod tests {
    /// #11597: the refusal-code vocabulary is closed and snake_case, every
    /// code is unique, each is documented with its disposition in
    /// `docs/EXIT_CODES.md`, and the stderr line carries both.
    #[test]
    fn refusal_codes_are_unique_documented_and_render_one_line() -> Result<(), String> {
        let docs = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/EXIT_CODES.md"),
        )
        .map_err(|error| format!("read EXIT_CODES.md: {error}"))?;
        let mut seen = std::collections::BTreeSet::new();
        for code in RepairRefusalCode::ALL {
            let name = code.as_str();
            if !seen.insert(name) {
                return Err(format!("duplicate refusal code {name}"));
            }
            if !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            {
                return Err(format!("refusal code {name} is not snake_case"));
            }
            let row = format!(
                "| `{name}` | {} | `{}` |",
                if matches!(
                    code,
                    RepairRefusalCode::NoTestRoute
                        | RepairRefusalCode::EditCageViolation
                        | RepairRefusalCode::AttemptStale
                        | RepairRefusalCode::AttemptIncomparable
                ) {
                    "2"
                } else {
                    "3"
                },
                code.disposition().as_str()
            );
            if !docs.contains(&row) {
                return Err(format!("EXIT_CODES.md is missing the row `{row}`"));
            }
            let line = code.stderr_line();
            if line.contains('\n')
                || line
                    != format!(
                        "ripr: refusal code={name} disposition={}",
                        code.disposition().as_str()
                    )
            {
                return Err(format!("unexpected stderr line {line:?}"));
            }
        }
        // Retryable and terminal states are told apart by disposition alone.
        if RepairRefusalCode::AwaitingEdit.disposition() != RepairRefusalDisposition::Retryable
            || RepairRefusalCode::EditCageViolation.disposition()
                != RepairRefusalDisposition::Terminal
        {
            return Err(
                "awaiting_edit must be retryable and a cage violation terminal".to_string(),
            );
        }
        for (state, expected) in [
            (
                RepairAttemptState::Failed,
                RepairRefusalCode::EditCageViolation,
            ),
            (RepairAttemptState::Stale, RepairRefusalCode::AttemptStale),
            (
                RepairAttemptState::Incomparable,
                RepairRefusalCode::AttemptIncomparable,
            ),
        ] {
            if RepairRefusalCode::for_finished_state(&state) != expected {
                return Err(format!("finished state {state:?} must map to {expected:?}"));
            }
        }
        Ok(())
    }

    use super::*;

    /// A default-cap cut within the ceiling reruns at its exact total; an
    /// explicit limit, a repository past the ceiling and a complete inventory
    /// are left alone, so the truncation refusal still guards them.
    #[test]
    fn repair_widens_only_a_default_cut_within_the_ceiling() {
        use crate::analysis::{SeamLimitInfo, SeamLimitSource};
        let cut = |analyzed, total, source| SeamLimitInfo {
            analyzed,
            total,
            source,
        };
        let default_cap = crate::analysis::DEFAULT_REPO_EXPOSURE_SEAM_LIMIT;
        assert_eq!(
            repair_widened_cap(&cut(default_cap, 12_754, SeamLimitSource::Default)),
            Some(12_754)
        );
        assert_eq!(
            repair_widened_cap(&cut(
                default_cap,
                REPAIR_SEAM_CEILING,
                SeamLimitSource::Default
            )),
            Some(REPAIR_SEAM_CEILING)
        );
        assert_eq!(
            repair_widened_cap(&cut(
                default_cap,
                REPAIR_SEAM_CEILING + 1,
                SeamLimitSource::Default
            )),
            None,
            "past the ceiling the refusal stands"
        );
        assert_eq!(
            repair_widened_cap(&cut(default_cap, 12_754, SeamLimitSource::Configured)),
            None,
            "an explicit RIPR_REPO_EXPOSURE_SEAM_LIMIT is never overridden"
        );
        assert_eq!(
            repair_widened_cap(&cut(500, 500, SeamLimitSource::Default)),
            None,
            "a complete inventory needs no rerun"
        );
    }

    /// End to end over a real repository above the 10,000-seam default cap
    /// (#1887): the plain inventory is cut, while the repair inventory, the
    /// gate behind `ripr agent status` and the MCP completeness read all
    /// rerun at the exact total and agree the inventory is complete. The cap
    /// is restored afterwards, and a repository past the ceiling still cuts.
    #[test]
    fn repair_surfaces_agree_on_a_repository_above_the_default_seam_cap() -> Result<(), String> {
        use crate::analysis::{DEFAULT_REPO_EXPOSURE_SEAM_LIMIT, SeamLimitSource};
        if std::env::var_os("RIPR_REPO_EXPOSURE_SEAM_LIMIT").is_some() {
            return Ok(());
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-widen-{}-{stamp}", std::process::id()));
        let _guard = TempRootGuard { root: root.clone() };
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"wide\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .map_err(|error| error.to_string())?;
        let write_module = |file: usize| -> Result<(), String> {
            let mut body = String::new();
            for item in 0..100 {
                body.push_str(&format!(
                    "pub fn f{file}_{item}(x: i32) -> bool {{ x > {item} }}\n"
                ));
            }
            std::fs::write(root.join(format!("src/m{file}.rs")), body)
                .map_err(|error| error.to_string())
        };
        let write_lib = |modules: usize| -> Result<(), String> {
            let lib: String = (0..modules)
                .map(|file| format!("pub mod m{file};\n"))
                .collect();
            std::fs::write(root.join("src/lib.rs"), lib).map_err(|error| error.to_string())
        };
        // Calibrate the seams one 100-function module yields, so the fixture
        // lands between the default cap and the repair ceiling whatever the
        // extractor's per-function count is.
        write_module(0)?;
        write_lib(1)?;
        let config = load_for_root(&root)?;
        let per_module =
            crate::analysis::inventory_classified_seams_at_with_config(&root, &config)?
                .0
                .len();
        assert!(per_module > 0, "calibration module yields seams");
        let modules = (DEFAULT_REPO_EXPOSURE_SEAM_LIMIT + per_module) / per_module + 5;
        for file in 1..modules {
            write_module(file)?;
        }
        write_lib(modules)?;

        // Plain inventory: cut by the default cap.
        let (_, plain) =
            crate::analysis::inventory_classified_seams_at_with_config(&root, &config)?;
        let plain = plain.ok_or("expected a limit fact")?;
        assert!(plain.is_truncated(), "fixture must exceed the default cap");
        assert_eq!(plain.analyzed, DEFAULT_REPO_EXPOSURE_SEAM_LIMIT);
        assert_eq!(plain.source, SeamLimitSource::Default);
        assert!(plain.total <= REPAIR_SEAM_CEILING, "fixture stays in range");

        // Repair inventory: widened, complete.
        let (classified, info) = inventory_for_repair(&root, &config)?;
        assert!(info.is_none_or(|info| !info.is_truncated()));
        assert_eq!(classified.len(), plain.total);

        // Agent-status gate: same answer, not withheld as truncated.
        let gate = load_gate_state(&root);
        assert!(
            matches!(
                gate,
                GateState::Loaded {
                    truncated: false,
                    ..
                }
            ),
            "the status gate must use the widened inventory"
        );

        // The cap is restored for later plain reads.
        let (_, after) =
            crate::analysis::inventory_classified_seams_at_with_config(&root, &config)?;
        assert!(after.is_some_and(|info| info.is_truncated()));
        assert_eq!(
            crate::analysis::repo_exposure_seam_limit()?,
            Some((DEFAULT_REPO_EXPOSURE_SEAM_LIMIT, SeamLimitSource::Default))
        );
        Ok(())
    }

    fn key<'a>(
        kind: crate::analysis::seams::SeamKind,
        file: &'a str,
        line: usize,
        expression: &'a str,
    ) -> SeamKey<'a> {
        SeamKey {
            kind,
            file: Path::new(file),
            line,
            expression,
        }
    }

    /// A wrapper is matched only by the exact same-file, same-line error
    /// variant; every near miss stays an open reason.
    #[test]
    fn wrapper_match_is_exact_and_never_looser() {
        use crate::analysis::seams::SeamKind::{ErrorVariant, ReturnValue};
        let variant = key(ErrorVariant, "src/lib.rs", 9, "Err(ParseError::Empty)");
        let wrapper = key(
            ReturnValue,
            "src/lib.rs",
            9,
            "return  Err(ParseError::Empty)",
        );
        assert!(
            restates_error_variant(&wrapper, &variant),
            "whitespace-insensitive match"
        );
        let near_misses = [
            (
                "different variant",
                key(
                    ReturnValue,
                    "src/lib.rs",
                    9,
                    "return Err(ParseError::TooLong)",
                ),
            ),
            (
                "different line",
                key(
                    ReturnValue,
                    "src/lib.rs",
                    10,
                    "return Err(ParseError::Empty)",
                ),
            ),
            (
                "different file",
                key(
                    ReturnValue,
                    "src/other.rs",
                    9,
                    "return Err(ParseError::Empty)",
                ),
            ),
            (
                "not a return value",
                key(
                    ErrorVariant,
                    "src/lib.rs",
                    9,
                    "return Err(ParseError::Empty)",
                ),
            ),
            (
                "bare expression",
                key(ReturnValue, "src/lib.rs", 9, "Err(ParseError::Empty)"),
            ),
            (
                "extra tail",
                key(
                    ReturnValue,
                    "src/lib.rs",
                    9,
                    "return Err(ParseError::Empty).into()",
                ),
            ),
        ];
        for (why, near) in &near_misses {
            assert!(
                !restates_error_variant(near, &variant),
                "{why} must not match"
            );
        }
        let not_variant = key(ReturnValue, "src/lib.rs", 9, "Err(ParseError::Empty)");
        assert!(
            !restates_error_variant(&wrapper, &not_variant),
            "the gripped sibling must be an error variant"
        );
    }

    /// #10978: the candidate list is bounded by the stop's own cap, and every
    /// described candidate carries the bound root.
    #[test]
    fn decide_describes_only_the_candidates_the_stop_lists() -> Result<(), String> {
        let eligible: Vec<String> = (0..10).map(|n| format!("seam:{n}")).collect();
        let notes: Vec<RepairCandidateNote> = (0..10)
            .map(|n| note(&format!("src/lib.rs:{n}"), "x", &format!("seam:{n}")))
            .collect();
        let facts = RepairStartFacts {
            candidate_notes: &notes,
            ..facts(&eligible, 10, &[], 0, None)
        };
        match decide_repair_start(&facts)? {
            RepairStartDecision::Action(_, described) => {
                // The stop lists at most eight candidates.
                assert_eq!(described.len(), 8);
                assert_eq!(
                    described.first().map(|n| n.location.as_str()),
                    Some("src/lib.rs:0")
                );
                // The disclosure names the remainder and the route listing it.
                let text = render_repair_choice(&described, 10, ".");
                assert!(text.contains("10 different repairs"), "{text}");
                assert!(text.contains("2 more not shown"), "{text}");
                Ok(())
            }
            _ => Err("ten eligible seams must select".to_string()),
        }
    }

    fn note(location: &str, code: &str, seam: &str) -> RepairCandidateNote {
        RepairCandidateNote {
            location: location.to_string(),
            code: code.to_string(),
            test_need: crate::analysis::seams::SeamKind::ReturnValue.plain_test_need(),
            command: format!("ripr repair {seam} --root ."),
            column: None,
        }
    }

    /// #11111: two candidates on the same changed code are told apart by what
    /// a test must check, the change is named once, and no line reads the same
    /// as another; the closing line points at the command `ripr check` printed.
    #[test]
    fn same_change_candidates_are_told_apart_by_what_a_test_checks() {
        use crate::analysis::seams::SeamKind;
        let mut boundary = note("src/lib.rs:2", "age >= 18", "aaa");
        boundary.test_need = SeamKind::PredicateBoundary.plain_test_need();
        let value = note("src/lib.rs:2", "age >= 18", "bbb");
        let text = render_repair_choice(&[boundary, value], 2, ".");
        assert!(
            text.starts_with(
                "ripr can start a repair for the change at src/lib.rs:2 `age >= 18` in 2 ways"
            ),
            "{text}"
        );
        assert_eq!(text.matches("age >= 18").count(), 1, "{text}");
        assert!(
            text.contains(SeamKind::PredicateBoundary.plain_test_need()),
            "{text}"
        );
        assert!(
            text.contains(SeamKind::ReturnValue.plain_test_need()),
            "{text}"
        );
        let lines: Vec<&str> = text.lines().collect();
        let mut unique = lines.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), lines.len(), "no two lines may match:\n{text}");
        assert!(text.contains("If `ripr check` printed a `ripr repair` command"));
        for jargon in ["choose_item", "select_item", "seam", "PowerShell"] {
            assert!(!text.contains(jargon), "no `{jargon}`:\n{text}");
        }
    }

    /// Different changes keep their own location beside each choice.
    #[test]
    fn different_change_candidates_name_each_location() {
        let text = render_repair_choice(
            &[
                note("src/lib.rs:2", "a >= b", "aaa"),
                note("src/lib.rs:10", "c >= d", "bbb"),
            ],
            2,
            ".",
        );
        assert!(
            text.starts_with("ripr can start 2 different repairs"),
            "{text}"
        );
        assert!(text.contains("1. src/lib.rs:2  `a >= b`:"), "{text}");
        assert!(text.contains("2. src/lib.rs:10  `c >= d`:"), "{text}");
        assert!(!text.contains("more not shown"), "{text}");
    }

    /// A root with an apostrophe quotes differently in PowerShell, so each
    /// concrete command is paired with its PowerShell spelling; an ordinary
    /// root prints none.
    #[test]
    fn candidate_commands_carry_a_powershell_form_when_quoting_differs() {
        let root = "/home/o'neil/proj";
        // The Bash single-quote form is the stimulus on every host: Windows
        // prints a double-quoted root that PowerShell reads identically.
        let bash_root = crate::agent::loop_commands::shell_arg_for(root, false);
        let mut first = note("src/lib.rs:2", "a >= b", "aaa");
        first.command = format!("ripr repair aaa --root {bash_root}");
        let mut second = note("src/lib.rs:10", "c >= d", "bbb");
        second.command = format!("ripr repair bbb --root {bash_root}");
        let text = render_repair_choice(&[first, second], 2, root);
        for seam in ["aaa", "bbb"] {
            assert!(
                text.contains(&format!(
                    "     (PowerShell) ripr repair {seam} --root '/home/o''neil/proj'\n"
                )),
                "{text}"
            );
        }
        assert!(
            !render_repair_choice(&[note("src/lib.rs:2", "a >= b", "aaa")], 1, ".")
                .contains("PowerShell")
        );
    }

    /// Two choices with the same location, code and test need are told apart
    /// by column; distinct choices carry no column.
    #[test]
    fn identical_choices_are_told_apart_by_column() {
        let mut left = note("src/lib.rs:2", "a + a", "aaa");
        left.column = Some(5);
        let mut right = note("src/lib.rs:2", "a + a", "bbb");
        right.column = Some(11);
        let text = render_repair_choice(&[left, right], 2, ".");
        assert!(text.contains(" (column 5)\n"), "{text}");
        assert!(text.contains(" (column 11)\n"), "{text}");
        let mut other = note("src/lib.rs:9", "b", "ccc");
        other.column = Some(3);
        let plain = render_repair_choice(&[note("src/lib.rs:2", "a", "aaa"), other], 2, ".");
        assert!(!plain.contains("column"), "{plain}");
    }

    /// Descriptions follow the stop's ids and order, not the inventory's.
    #[test]
    fn notes_follow_the_stops_ids_and_order() {
        let eligible: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        let notes: Vec<RepairCandidateNote> = ["a", "b", "c"].map(|id| note(id, "x", id)).to_vec();
        let stop: Vec<String> = ["c", "a", "zzz"].map(String::from).to_vec();
        assert_eq!(
            notes_for_stop_ids(&stop, &eligible, &notes),
            vec![note("c", "x", "c"), note("a", "x", "a")]
        );
    }

    /// #10978: only a genuinely missing edit target takes the plain refusal;
    /// a malformed packet keeps its own diagnostic.
    #[test]
    fn missing_edit_target_is_told_from_other_cage_rejections() {
        let rejection = |packet: &str| {
            crate::app::repair_attempt::edit_cage_policy_from_packet(packet, "s")
                .err()
                .unwrap_or_default()
        };
        for packet in [
            r#"{"seam_id":"s","recommended_test":{"file":"not_applicable"}}"#,
            r#"{"seam_id":"s","allowed_edit_surface":[]}"#,
            r#"{"seam_id":"s","recommended_test":{}}"#,
        ] {
            let error = rejection(packet);
            assert!(is_missing_edit_target_error(&error), "{packet}: {error}");
        }
        for packet in [
            "not json",
            r#"{"seam_id":"other","allowed_edit_surface":["tests/t.rs"]}"#,
            r#"{"seam_id":"s","allowed_edit_surface":["tests/t.rs",7],"denied_edit_surface":[]}"#,
        ] {
            let error = rejection(packet);
            assert!(
                !error.is_empty() && !is_missing_edit_target_error(&error),
                "{packet}: {error}"
            );
        }
    }
    fn proposed_readiness(
        kind: crate::analysis::new_test_target::NewTestKind,
        file: &str,
    ) -> crate::analysis::repair_route::RepairRouteReadiness {
        use crate::analysis::new_test_target::{NewTestProposalProvenance, NewTestTargetProposal};
        use crate::analysis::repair_route::{
            RepairRouteReadiness, RepairRouteState, RepairTargetSelection,
        };
        RepairRouteReadiness {
            state: RepairRouteState::Ready,
            seam_id: String::new(),
            canonical_gap_id: None,
            required_evidence: Vec::new(),
            present_evidence: Vec::new(),
            missing_evidence: Vec::new(),
            target_selection: RepairTargetSelection::Proposed(NewTestTargetProposal {
                kind,
                file: std::path::PathBuf::from(file),
                owner: "price".to_string(),
                provenance: NewTestProposalProvenance::ProducerOwned,
            }),
            test_target: None,
            proposed_oracle: None,
            current_oracle: None,
            authority_boundary: "",
        }
    }

    /// A producer-proposed inline-unit target (no existing test target) names
    /// its production file as the edit target, so a dirty one is held while a
    /// clean one, an integration proposal and a missing target are not.
    #[test]
    fn held_inline_target_follows_the_selected_proposal_file() -> Result<(), String> {
        use crate::analysis::new_test_target::NewTestKind;
        use crate::analysis::repair_route::RepairTargetSelection;
        use crate::testing::fixture_git::fixture_git_ok as git_ok;
        let root = std::env::temp_dir().join(format!(
            "ripr-held-inline-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|err| err.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|err| err.to_string())?;
        std::fs::write(root.join("src/lib.rs"), "pub fn price() {}\n")
            .map_err(|err| err.to_string())?;
        git_ok(&root, &["-c", "init.templateDir=", "init", "-q"])?;
        git_ok(&root, &["config", "user.email", "ripr@example.invalid"])?;
        git_ok(&root, &["config", "user.name", "RIPR Test"])?;
        git_ok(&root, &["config", "commit.gpgSign", "false"])?;
        git_ok(&root, &["add", "."])?;
        git_ok(&root, &["commit", "-qm", "base"])?;
        let inline = proposed_readiness(NewTestKind::InlineUnit, "src/lib.rs");

        // Clean control: the committed proposed target is not held.
        assert_eq!(held_inline_target(&root, &inline), None);

        std::fs::write(root.join("src/lib.rs"), "pub fn price() { }\n")
            .map_err(|err| err.to_string())?;
        assert_eq!(
            held_inline_target(&root, &inline).as_deref(),
            Some("src/lib.rs")
        );
        // Not an inline production target: integration proposals live in tests/.
        let integration = proposed_readiness(NewTestKind::Integration, "tests/price.rs");
        assert_eq!(held_inline_target(&root, &integration), None);
        let mut missing = proposed_readiness(NewTestKind::InlineUnit, "src/lib.rs");
        missing.target_selection = RepairTargetSelection::Missing;
        assert_eq!(held_inline_target(&root, &missing), None);
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    pub(crate) fn gate_entry(
        class: crate::analysis::seams::SeamGripClass,
        related: Vec<crate::analysis::test_grip_evidence::RelatedTestGrip>,
    ) -> crate::analysis::ClassifiedSeam {
        use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
        use crate::domain::{Confidence, StageEvidence, StageState};
        let stage = || StageEvidence::new(StageState::Yes, Confidence::Medium, "test stage");
        let seam = RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            SeamKind::PredicateBoundary,
            42,
            88,
            "amount >= discount_threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= discount_threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let seam_id = seam.id().clone();
        crate::analysis::ClassifiedSeam {
            seam,
            evidence: crate::analysis::test_grip_evidence::TestGripEvidence {
                seam_id,
                related_tests: related.into_iter().map(std::sync::Arc::new).collect(),
                reach: stage(),
                activate: stage(),
                propagate: stage(),
                observe: stage(),
                discriminate: stage(),
                observed_values: Vec::new(),
                missing_discriminators: Vec::new(),
                new_test_target: None,
                statically_contradicted_related_tests: 0,
            },
            class,
        }
    }

    pub(crate) fn weak_related_test() -> crate::analysis::test_grip_evidence::RelatedTestGrip {
        crate::analysis::test_grip_evidence::RelatedTestGrip {
            test_name: "at_threshold_runs".to_string(),
            file: std::path::PathBuf::from("tests/pricing.rs"),
            line: 12,
            test_target: None,
            oracle_kind: crate::domain::OracleKind::RelationalCheck,
            oracle_strength: crate::domain::OracleStrength::Weak,
            evidence_summary: "relational assertion".to_string(),
            relation_reason: crate::domain::RelationReason::DirectOwnerCall,
            relation_confidence: crate::analysis::test_grip_evidence::RelationConfidence::High,
        }
    }

    /// #8766: a weakly gripped seam whose route is not ready is refused by
    /// the shared gate, the related test is named for the by-hand work, and
    /// the withheld note replaces a start command. The admitted-seam control
    /// (a restart still printed) lives in the facade integration test.
    #[test]
    fn route_not_ready_seam_is_refused_with_the_related_test_named() -> Result<(), String> {
        use crate::analysis::seams::SeamGripClass;
        let config = crate::config::RiprConfig::default();
        let policy = AgentBriefPolicy::from_config(&config);
        let root = Path::new("/repo");

        let not_ready = gate_entry(SeamGripClass::WeaklyGripped, vec![weak_related_test()]);
        assert!(
            !repair_packet_eligibility(&not_ready).eligible(),
            "precondition: the producer flip must refuse this seam"
        );
        let refusal = repair_start_refusal_for_entry(root, &not_ready, policy)
            .ok_or("the shared gate must refuse a route that is not ready")?;
        // #9604: the verdict now names the evidence the route still needs.
        assert!(
            refusal
                .reason
                .starts_with("repair route not ready (missing evidence: "),
            "{}",
            refusal.reason
        );
        assert_eq!(
            refusal.related_test.as_deref(),
            Some("`at_threshold_runs` (tests/pricing.rs:12)")
        );
        let note = repair_start_withheld_note("/repo", "seam-1", &refusal);
        assert!(
            note.contains("strengthening the oracle of the related test `at_threshold_runs`")
                && note.contains("by hand")
                && !note.contains("--phase before"),
            "{note}"
        );

        // An already-gripped seam is refused with the exposed wording and,
        // having no related test, points at adding one by hand.
        let gripped = gate_entry(SeamGripClass::StronglyGripped, Vec::new());
        let gripped_refusal = repair_start_refusal_for_entry(root, &gripped, policy)
            .ok_or("an already gripped seam has no gap to repair")?;
        assert!(
            gripped_refusal.reason.contains("already check it"),
            "{}",
            gripped_refusal.reason
        );
        assert_eq!(gripped_refusal.related_test, None);
        assert_eq!(gripped_refusal.kind, RefusalKind::AlreadyGripped);
        // An already-gripped seam has no gap: the note must not send the
        // reader to strengthen anything by hand, and says it is a static
        // reading rather than a runtime result.
        let no_test_note = repair_start_withheld_note("/repo", "seam-2", &gripped_refusal);
        assert!(
            no_test_note.contains("already check it")
                && no_test_note.contains("static reading")
                && no_test_note.contains("not a runtime-verified result")
                && !no_test_note.contains("by hand")
                && !no_test_note.contains("--phase before"),
            "{no_test_note}"
        );
        // A refusal that is neither already-gripped nor route-not-ready
        // (here a policy-style reason) is stated as is, with no by-hand claim.
        let other = RepairStartRefusal {
            reason: "not a headline repair target".to_string(),
            related_test: None,
            kind: RefusalKind::Other,
        };
        let other_note = repair_start_withheld_note("/repo", "seam-3", &other);
        assert!(
            other_note.contains("not a headline repair target") && !other_note.contains("by hand"),
            "{other_note}"
        );
        Ok(())
    }

    /// #11111 burn-down: an opaque seam names what ripr could not establish;
    /// a class with its own policy reason keeps the generic label. The
    /// `*_unknown` classes are headline classes and never take this branch.
    #[test]
    fn opaque_refusal_names_what_could_not_be_established() -> Result<(), String> {
        use crate::analysis::seams::SeamGripClass;
        let config = crate::config::RiprConfig::default();
        let policy = AgentBriefPolicy::from_config(&config);
        let root = Path::new("/repo");
        let opaque = repair_start_refusal_for_entry(
            root,
            &gate_entry(SeamGripClass::Opaque, Vec::new()),
            policy,
        )
        .ok_or("an opaque seam must be refused")?;
        if opaque.reason != OPAQUE_SPOT_REASON || opaque.kind != RefusalKind::Other {
            return Err(format!("opaque refusal changed: {}", opaque.reason));
        }
        let intentional = repair_start_refusal_for_entry(
            root,
            &gate_entry(SeamGripClass::Intentional, Vec::new()),
            policy,
        )
        .ok_or("an intentional seam must be refused")?;
        if intentional.reason.contains("could not establish") {
            return Err(format!(
                "only an opaque seam names what could not be established: {}",
                intentional.reason
            ));
        }
        for class in [
            SeamGripClass::ActivationUnknown,
            SeamGripClass::PropagationUnknown,
            SeamGripClass::ObservationUnknown,
            SeamGripClass::DiscriminationUnknown,
        ] {
            if !class.is_headline_eligible() {
                return Err(format!("{class:?} must stay a headline class"));
            }
        }
        Ok(())
    }

    /// #8766: where the gate cannot decide it does not admit by default: an
    /// unreadable inventory, a seam the inventory no longer names (stale seam
    /// ids) and a seam beyond a truncated inventory are all *unverified*
    /// refusals whose note prints no start command.
    #[test]
    fn undecidable_seams_are_unverified_not_admitted() -> Result<(), String> {
        use crate::analysis::seams::SeamGripClass;
        let root = Path::new("/repo");
        let unverified = |state: &GateState, seam: &str| {
            check_state(root, state, seam)
                .filter(|refusal| refusal.kind == RefusalKind::Unverified)
                .ok_or_else(|| format!("seam `{seam}` must be unverified, not admitted"))
        };

        let unavailable =
            GateState::Unavailable("the seam inventory could not be built (boom)".into());
        let refusal = unverified(&unavailable, "seam-1")?;
        let note = repair_start_withheld_note("/repo", "seam-1", &refusal);
        assert!(
            note.contains("could not verify")
                && note.contains("boom")
                && !note.contains("--phase before"),
            "{note}"
        );

        let truncated = GateState::Loaded {
            config: Box::default(),
            classified: Vec::new(),
            truncated: true,
        };
        assert!(
            unverified(&truncated, "seam-1")?
                .reason
                .contains("truncated")
        );
        // A seam the truncated inventory does name is still unverified: the
        // explicit-subject resolver refuses every start from such an inventory.
        let in_truncated = gate_entry(SeamGripClass::WeaklyGripped, vec![weak_related_test()]);
        let in_truncated_id = in_truncated.seam.id().as_str().to_string();
        let truncated_known = GateState::Loaded {
            config: Box::default(),
            classified: vec![in_truncated],
            truncated: true,
        };
        assert!(
            unverified(&truncated_known, &in_truncated_id)?
                .reason
                .contains("truncated")
        );

        let not_ready = gate_entry(SeamGripClass::WeaklyGripped, vec![weak_related_test()]);
        let known = not_ready.seam.id().as_str().to_string();
        let loaded = GateState::Loaded {
            config: Box::default(),
            classified: vec![not_ready],
            truncated: false,
        };
        assert!(
            unverified(&loaded, "stale-seam-id")?
                .reason
                .contains("no longer names this seam")
        );
        // A seam the inventory does name gets the gate's own verdict, not
        // the unverified one.
        let refused = check_state(root, &loaded, &known).ok_or("known seam must be refused")?;
        assert_eq!(refused.kind, RefusalKind::RouteNotReady);
        assert!(
            refused
                .reason
                .starts_with("repair route not ready (missing evidence: "),
            "{}",
            refused.reason
        );
        Ok(())
    }

    /// An opaque seam is still refused (class and eligibility unchanged), but
    /// the refusal names the candidate test ripr found instead of ending at
    /// "no automatic repair for this kind of finding".
    #[test]
    fn opaque_reach_refusal_names_the_candidate_test() -> Result<(), String> {
        use crate::analysis::seams::SeamGripClass;
        use crate::domain::{Confidence, StageEvidence, StageState};
        let config = crate::config::RiprConfig::default();
        let policy = AgentBriefPolicy::from_config(&config);
        let root = Path::new("/repo");

        let mut unresolved = gate_entry(SeamGripClass::Opaque, Vec::new());
        unresolved.evidence.reach = StageEvidence::new(
            StageState::Opaque,
            Confidence::Low,
            "Reach unresolved (rust_transitive_reach_unresolved): `doc - new (line 9)` (src/lib.rs:9) calls `new`, which may lead to `price` through a call path ripr does not fully trace; no gap is reported",
        );
        let refusal = repair_start_refusal_for_entry(root, &unresolved, policy)
            .ok_or("an opaque seam must still be refused")?;
        assert!(
            refusal
                .reason
                .contains("`doc - new (line 9)` (src/lib.rs:9) calls `new`")
                && refusal.reason.starts_with(
                    "ripr does not start an automatic repair here because it could not establish which test runs this code: "
                )
                && refusal.reason.contains("Check by hand")
                && !refusal.reason.contains("rust_transitive_reach_unresolved")
                && !refusal.reason.contains("no gap is reported"),
            "{}",
            refusal.reason
        );

        let mut weak = gate_entry(SeamGripClass::Opaque, vec![weak_related_test()]);
        weak.evidence.reach =
            StageEvidence::new(StageState::Weak, Confidence::Low, "nearby tests only");
        let refusal = repair_start_refusal_for_entry(root, &weak, policy)
            .ok_or("an opaque seam must still be refused")?;
        assert!(
            refusal.reason.contains("a related test is"),
            "{}",
            refusal.reason
        );

        // Macro and trait-dispatch summaries do not leak their own wording.
        let mut macro_reach = gate_entry(SeamGripClass::Opaque, vec![weak_related_test()]);
        macro_reach.evidence.reach = StageEvidence::new(
            StageState::Opaque,
            Confidence::Low,
            "Reach unresolved (rust_macro_reach_unresolved): `t` (a.rs:1) calls `r`, which reaches macro `m!` (b.rs:2) that names `price`; ripr does not expand macros, so no gap is reported",
        );
        let refusal = repair_start_refusal_for_entry(root, &macro_reach, policy)
            .ok_or("an opaque seam must still be refused")?;
        assert!(
            refusal.reason.contains("a related test is")
                && !refusal.reason.contains("no gap is reported")
                && !refusal.reason.contains("macro"),
            "{}",
            refusal.reason
        );
        assert_eq!(refusal.kind, RefusalKind::Other);

        // Negative: weak reach with no related test has nothing to name and
        // keeps the original class label.
        let mut nothing = gate_entry(SeamGripClass::Opaque, Vec::new());
        nothing.evidence.reach = StageEvidence::new(StageState::Weak, Confidence::Low, "x");
        let refusal = repair_start_refusal_for_entry(root, &nothing, policy)
            .ok_or("an opaque seam must still be refused")?;
        assert!(
            !refusal.reason.contains("Check by hand"),
            "{}",
            refusal.reason
        );
        Ok(())
    }

    /// A seam whose only target blocker is "the owner file has no test
    /// module" still names the stub route, because `agent stub` adds that
    /// module; any other target blocker keeps the bare label.
    #[test]
    fn no_test_module_blocker_still_names_the_stub_route() -> Result<(), String> {
        use crate::analysis::new_test_target::{NewTestProposalBlocker, NewTestTargetAdmission};
        use crate::analysis::seams::SeamGripClass;
        let config = crate::config::RiprConfig::default();
        let policy = AgentBriefPolicy::from_config(&config);
        let root = Path::new("/repo");
        let admission = |blocker| NewTestTargetAdmission {
            blocker: Some(blocker),
            ..NewTestTargetAdmission::default()
        };

        let mut no_module = gate_entry(SeamGripClass::Ungripped, Vec::new());
        no_module.evidence.new_test_target = Some(admission(NewTestProposalBlocker::NoTestModule));
        let refusal = repair_start_refusal_for_entry(root, &no_module, policy)
            .ok_or("a seam without a target must be refused")?;
        assert!(
            refusal
                .reason
                .contains("--at src/pricing.rs:88 --kind predicate")
                && refusal.reason.contains("the stub adds one")
                && refusal
                    .reason
                    .contains("new test proposal blocked: no governed inline cfg-test module"),
            "{}",
            refusal.reason
        );

        // Discriminating negatives: a different target blocker names no
        // route and carries no module note.
        let mut other_blocker = gate_entry(SeamGripClass::Ungripped, Vec::new());
        other_blocker.evidence.new_test_target =
            Some(admission(NewTestProposalBlocker::GeneratedOrVendor));
        let bare = repair_start_refusal_for_entry(root, &other_blocker, policy)
            .ok_or("a seam without a target must be refused")?;
        assert!(!bare.reason.contains("--kind predicate"), "{}", bare.reason);
        assert!(
            !bare.reason.contains("no test module yet"),
            "{}",
            bare.reason
        );

        Ok(())
    }

    /// #11427: an activation-unknown seam with no direct owner call names the
    /// closest related test as a by-hand check, without promising a repair.
    #[test]
    fn owner_call_absent_refusal_names_the_related_test() -> Result<(), String> {
        use crate::analysis::seams::SeamGripClass;
        use crate::domain::RelationReason;
        let config = crate::config::RiprConfig::default();
        let policy = AgentBriefPolicy::from_config(&config);
        let root = Path::new("/repo");
        let summary = "No direct owner call observed for value-insensitive seam";

        let mut affinity = weak_related_test();
        affinity.relation_reason = RelationReason::AssertionTargetAffinity;
        let mut entry = gate_entry(SeamGripClass::ActivationUnknown, vec![affinity]);
        entry.evidence.activate.summary = summary.to_string();
        let refusal = repair_start_refusal_for_entry(root, &entry, policy)
            .ok_or("an activation-unknown seam must be refused")?;
        assert!(
            refusal
                .reason
                .contains("the closest related test is `at_threshold_runs` (tests/pricing.rs:12)")
                && refusal.reason.contains("otherwise write a new test"),
            "{}",
            refusal.reason
        );

        // Negatives: no related test, or a different activate summary, keeps
        // the bare label.
        let mut none = gate_entry(SeamGripClass::ActivationUnknown, Vec::new());
        none.evidence.activate.summary = summary.to_string();
        let bare = repair_start_refusal_for_entry(root, &none, policy)
            .ok_or("an activation-unknown seam must be refused")?;
        assert!(
            !bare.reason.contains("closest related test"),
            "{}",
            bare.reason
        );
        let mut other = gate_entry(SeamGripClass::ActivationUnknown, vec![weak_related_test()]);
        other.evidence.activate.summary = "test stage".to_string();
        let bare = repair_start_refusal_for_entry(root, &other, policy)
            .ok_or("an activation-unknown seam must be refused")?;
        assert!(
            !bare.reason.contains("closest related test"),
            "{}",
            bare.reason
        );
        Ok(())
    }

    /// #11460: a boundary seam that also misses the owner-call input names the
    /// stub route when no related test calls the owner directly (the skeleton's
    /// call is that input), and keeps the bare label when one does.
    #[test]
    fn owner_call_gap_does_not_hide_the_stub_route() {
        use crate::analysis::seams::SeamGripClass;
        let boundary = "producer-owned missing discriminator fact: boundary_value".to_string();
        let activation = "incomplete evidence stage: activation".to_string();
        let missing = vec![boundary, activation];
        let root = RefusalRoot::named(Path::new("/repo"));

        let none = gate_entry(SeamGripClass::ActivationUnknown, Vec::new());
        let reason = route_not_ready_reason(&none, "repair route not ready", &missing, root);
        assert!(
            reason.contains("--at src/pricing.rs:88 --kind predicate")
                && reason.contains("a test input that reaches the changed code"),
            "{reason}"
        );

        // With the guidance trigger set too, only the stub route is printed.
        let mut both = gate_entry(SeamGripClass::ActivationUnknown, vec![weak_related_test()]);
        both.evidence.activate.summary =
            "No direct owner call observed for value-insensitive seam".to_string();
        both.evidence.related_tests = Vec::new();
        let reason = route_not_ready_reason(&both, "repair route not ready", &missing, root);
        assert!(
            reason.contains("--kind predicate") && !reason.contains("closest related test"),
            "{reason}"
        );

        // Negative: a test that already calls the owner means the gap is real.
        let direct = gate_entry(SeamGripClass::WeaklyGripped, vec![weak_related_test()]);
        let reason = route_not_ready_reason(&direct, "repair route not ready", &missing, root);
        assert!(!reason.contains("--kind predicate"), "{reason}");

        // Negative: another missing fact still keeps the bare label.
        let mut more = missing.clone();
        more.push("safe test target".to_string());
        let reason = route_not_ready_reason(&none, "repair route not ready", &more, root);
        assert!(!reason.contains("--kind predicate"), "{reason}");
    }

    /// #11466: an opaque seam with no related test still names why the path is
    /// unresolved from the reach summary (trait dispatch, macro), without
    /// leaking ripr's own "no gap is reported" tail.
    #[test]
    fn opaque_reach_without_related_tests_quotes_the_dispatch_detail() -> Result<(), String> {
        use crate::analysis::seams::SeamGripClass;
        use crate::domain::{Confidence, StageEvidence, StageState};
        let config = crate::config::RiprConfig::default();
        let policy = AgentBriefPolicy::from_config(&config);
        let root = Path::new("/repo");
        let mut entry = gate_entry(SeamGripClass::Opaque, Vec::new());
        entry.evidence.reach = StageEvidence::new(
            StageState::Opaque,
            Confidence::Low,
            "Reach unresolved (trait dispatch): `fmt` is a trait method of `Comparator`, which operators, formatting and generic calls run without naming it; test `test` (tests/test_autotrait.rs:6) uses `Comparator`. ripr does not trace trait dispatch, so no gap is reported",
        );
        let refusal = repair_start_refusal_for_entry(root, &entry, policy)
            .ok_or("an opaque seam must be refused")?;
        assert!(
            refusal
                .reason
                .contains("`fmt` is a trait method of `Comparator`")
                && refusal.reason.contains("tests/test_autotrait.rs:6")
                && !refusal.reason.contains("no gap is reported")
                && !refusal.reason.contains("does not trace trait dispatch"),
            "{}",
            refusal.reason
        );

        // Negative: a summary that is not an unresolved-reach sentence stays bare.
        let mut bare = gate_entry(SeamGripClass::Opaque, Vec::new());
        bare.evidence.reach =
            StageEvidence::new(StageState::Opaque, Confidence::Low, "something else");
        let refusal = repair_start_refusal_for_entry(root, &bare, policy)
            .ok_or("an opaque seam must be refused")?;
        assert!(
            !refusal.reason.contains("ripr found that"),
            "{}",
            refusal.reason
        );
        Ok(())
    }

    /// #11466: a predicate seam whose only blocker is the missing test module
    /// names the stub route (it adds the module); another blocker still keeps
    /// the bare label.
    #[test]
    fn module_only_blocker_names_the_predicate_stub_route() -> Result<(), String> {
        use crate::analysis::new_test_target::{NewTestProposalBlocker, NewTestTargetAdmission};
        use crate::analysis::seams::SeamGripClass;
        let root = RefusalRoot::named(Path::new("/repo"));
        let mut entry = gate_entry(SeamGripClass::Ungripped, Vec::new());
        entry.evidence.new_test_target = Some(NewTestTargetAdmission {
            blocker: Some(NewTestProposalBlocker::NoTestModule),
            ..NewTestTargetAdmission::default()
        });
        let blocked = entry
            .evidence
            .new_test_target
            .as_ref()
            .and_then(|admission| admission.missing_reason())
            .ok_or("the module blocker must name a missing reason")?;
        let only_target = vec![
            crate::analysis::repair_route::SAFE_TEST_TARGET_EVIDENCE.to_string(),
            blocked.clone(),
        ];
        let reason = route_not_ready_reason(&entry, "repair route not ready", &only_target, root);
        assert!(
            reason.contains("--at src/pricing.rs:88 --kind predicate")
                && reason.contains("no test module yet"),
            "{reason}"
        );

        // Negative: an unrelated extra fact keeps the bare label.
        let mut more = only_target.clone();
        more.push("incomplete evidence stage: propagation".to_string());
        let reason = route_not_ready_reason(&entry, "repair route not ready", &more, root);
        assert!(!reason.contains("--kind predicate"), "{reason}");

        // Negative: an empty list without the module blocker names no route.
        let plain = gate_entry(SeamGripClass::Ungripped, Vec::new());
        let reason = route_not_ready_reason(&plain, "repair route not ready", &[], root);
        assert!(!reason.contains("--kind predicate"), "{reason}");
        Ok(())
    }

    /// #11895: when the only missing fact is the test input that reaches the
    /// owner and no test calls the owner directly, the stub's own call supplies
    /// it, so the predicate stub route is named; a direct caller or a second
    /// missing fact keeps it out.
    #[test]
    fn owner_call_gap_alone_names_the_predicate_stub_route() {
        use crate::analysis::seams::SeamGripClass;
        let root = RefusalRoot::named(Path::new("/repo"));
        let activation = vec!["incomplete evidence stage: activation".to_string()];

        let none = gate_entry(SeamGripClass::ActivationUnknown, Vec::new());
        let reason = route_not_ready_reason(&none, "repair route not ready", &activation, root);
        assert!(
            reason.contains("--at src/pricing.rs:88 --kind predicate"),
            "{reason}"
        );

        // Negatives: a direct caller keeps the gap (the stub would not close
        // it), and another missing fact means the stub alone is not enough.
        let direct = gate_entry(SeamGripClass::ActivationUnknown, vec![weak_related_test()]);
        let reason = route_not_ready_reason(&direct, "repair route not ready", &activation, root);
        assert!(!reason.contains("ripr agent stub"), "{reason}");
        let mut more = activation.clone();
        more.push("incomplete evidence stage: propagation".to_string());
        let reason = route_not_ready_reason(&none, "repair route not ready", &more, root);
        assert!(!reason.contains("ripr agent stub"), "{reason}");
        let mut target = activation.clone();
        target.push(crate::analysis::repair_route::SAFE_TEST_TARGET_EVIDENCE.to_string());
        let reason = route_not_ready_reason(&none, "repair route not ready", &target, root);
        assert!(!reason.contains("ripr agent stub"), "{reason}");
    }

    /// #11895: the return-value and error-path stubs are named too when the
    /// owner-call gap is the only missing fact; a direct caller or a seam kind
    /// with no stub kind keeps them out.
    #[test]
    fn owner_call_gap_alone_names_the_return_and_error_stub_routes() {
        use crate::analysis::seams::{
            ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind,
        };
        let root = RefusalRoot::named(Path::new("/repo"));
        let activation = vec!["incomplete evidence stage: activation".to_string()];
        let build = |kind, required, related: Vec<_>| {
            let mut entry = gate_entry(SeamGripClass::ActivationUnknown, related);
            entry.seam = RepoSeam::new(
                "src/pricing.rs",
                "pricing::discounted_total",
                kind,
                42,
                88,
                "changed value",
                required,
                ExpectedSink::ReturnValue,
            );
            entry
        };
        let ret_required = || RequiredDiscriminator::ReturnValue {
            description: "changed value".to_string(),
        };

        let ret = build(SeamKind::ReturnValue, ret_required(), Vec::new());
        let reason = route_not_ready_reason(&ret, "repair route not ready", &activation, root);
        assert!(
            reason.contains("--at src/pricing.rs:88 --kind return_value"),
            "{reason}"
        );

        let err = build(
            SeamKind::ErrorVariant,
            RequiredDiscriminator::ErrorVariant {
                variant: "Bad".to_string(),
            },
            Vec::new(),
        );
        let reason = route_not_ready_reason(&err, "repair route not ready", &activation, root);
        assert!(reason.contains("--kind error_path"), "{reason}");

        // Negatives: a direct caller keeps the gap, and a second missing fact
        // keeps the stub out.
        let direct = build(
            SeamKind::ReturnValue,
            ret_required(),
            vec![weak_related_test()],
        );
        let reason = route_not_ready_reason(&direct, "repair route not ready", &activation, root);
        assert!(!reason.contains("ripr agent stub"), "{reason}");
        let mut more = activation.clone();
        more.push("incomplete evidence stage: propagation".to_string());
        let reason = route_not_ready_reason(&ret, "repair route not ready", &more, root);
        assert!(!reason.contains("ripr agent stub"), "{reason}");
    }

    /// #11466: a propagation/observation-unknown seam that a test already runs
    /// names that test; a seam no test calls directly, or a class past the
    /// unknown stages, stays bare.
    #[test]
    fn unknown_stage_refusal_names_the_test_that_runs_the_code() {
        use crate::analysis::seams::SeamGripClass;
        let root = RefusalRoot::named(Path::new("/repo"));
        let missing = vec!["incomplete evidence stage: propagation".to_string()];

        let entry = gate_entry(SeamGripClass::PropagationUnknown, vec![weak_related_test()]);
        let reason = route_not_ready_reason(&entry, "repair route not ready", &missing, root);
        assert!(
            reason.contains("`at_threshold_runs` (tests/pricing.rs:12) calls this code"),
            "{reason}"
        );

        // The refusal's own pick wins: a direct owner call listed after a
        // helper call is the one named, and other unknown stages name it too.
        let mut helper = weak_related_test();
        helper.test_name = "via_helper".to_string();
        helper.relation_reason = crate::domain::RelationReason::HelperOwnerCall;
        for class in [
            SeamGripClass::ObservationUnknown,
            SeamGripClass::DiscriminationUnknown,
        ] {
            let both = gate_entry(class, vec![helper.clone(), weak_related_test()]);
            let reason = route_not_ready_reason(&both, "repair route not ready", &missing, root);
            assert!(reason.contains("`at_threshold_runs`"), "{reason}");
            assert!(!reason.contains("`via_helper`"), "{reason}");
        }

        // Negatives: no related test, a non-reaching relation, another class.
        let none = gate_entry(SeamGripClass::PropagationUnknown, Vec::new());
        let reason = route_not_ready_reason(&none, "repair route not ready", &missing, root);
        assert!(!reason.contains("calls this code"), "{reason}");
        let mut weak = weak_related_test();
        weak.relation_reason = crate::domain::RelationReason::SameModule;
        let same_module = gate_entry(SeamGripClass::PropagationUnknown, vec![weak]);
        let reason = route_not_ready_reason(&same_module, "repair route not ready", &missing, root);
        assert!(!reason.contains("calls this code"), "{reason}");
        let weakly = gate_entry(SeamGripClass::WeaklyGripped, vec![weak_related_test()]);
        let reason = route_not_ready_reason(&weakly, "repair route not ready", &missing, root);
        assert!(!reason.contains("calls this code"), "{reason}");
    }

    /// Activation-unknown seam that a test calls directly: the refusal names
    /// that test and says its input may not reach the changed branch. A weaker
    /// relation, or a seam with no related test, names nothing.
    #[test]
    fn activation_unknown_with_a_direct_call_names_the_calling_test() {
        use crate::analysis::seams::SeamGripClass;
        let root = RefusalRoot::named(Path::new("/repo"));
        let missing = vec!["incomplete evidence stage: activation".to_string()];

        let entry = gate_entry(SeamGripClass::ActivationUnknown, vec![weak_related_test()]);
        let reason = route_not_ready_reason(&entry, "repair route not ready", &missing, root);
        assert!(
            reason.contains("`at_threshold_runs` (tests/pricing.rs:12) calls this code")
                && reason.contains("its input reaches the changed branch"),
            "{reason}"
        );

        // A weaker relation listed before the direct caller never wins.
        let mut via_helper = weak_related_test();
        via_helper.test_name = "via_helper".to_string();
        via_helper.relation_reason = crate::domain::RelationReason::HelperOwnerCall;
        let both = gate_entry(
            SeamGripClass::ActivationUnknown,
            vec![via_helper, weak_related_test()],
        );
        let reason = route_not_ready_reason(&both, "repair route not ready", &missing, root);
        assert!(reason.contains("`at_threshold_runs`"), "{reason}");
        assert!(!reason.contains("`via_helper`"), "{reason}");

        // Negatives: a weaker relation and no related test.
        let mut weak = weak_related_test();
        weak.relation_reason = crate::domain::RelationReason::HelperOwnerCall;
        let helper = gate_entry(SeamGripClass::ActivationUnknown, vec![weak]);
        let reason = route_not_ready_reason(&helper, "repair route not ready", &missing, root);
        assert!(!reason.contains("calls this code"), "{reason}");
        let none = gate_entry(SeamGripClass::ActivationUnknown, Vec::new());
        let reason = route_not_ready_reason(&none, "repair route not ready", &missing, root);
        assert!(!reason.contains("calls this code"), "{reason}");
    }

    /// #11466: a return-value or error-path seam whose only blocker is the
    /// missing test module names its stub route too (the stub adds the module);
    /// a seam kind with no stub kind stays bare.
    #[test]
    fn module_only_blocker_names_the_return_and_error_stub_routes() -> Result<(), String> {
        use crate::analysis::new_test_target::{NewTestProposalBlocker, NewTestTargetAdmission};
        use crate::analysis::seams::{
            ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind,
        };
        let root = RefusalRoot::named(Path::new("/repo"));
        let build = |kind, required| {
            let mut entry = gate_entry(SeamGripClass::Ungripped, Vec::new());
            entry.seam = RepoSeam::new(
                "src/pricing.rs",
                "pricing::discounted_total",
                kind,
                42,
                88,
                "changed value",
                required,
                ExpectedSink::ReturnValue,
            );
            entry.evidence.new_test_target = Some(NewTestTargetAdmission {
                blocker: Some(NewTestProposalBlocker::NoTestModule),
                ..NewTestTargetAdmission::default()
            });
            entry
        };
        let only_target = |entry: &crate::analysis::ClassifiedSeam| -> Result<Vec<String>, String> {
            let blocked = entry
                .evidence
                .new_test_target
                .as_ref()
                .and_then(|admission| admission.missing_reason())
                .ok_or("the module blocker must name a missing reason")?;
            Ok(vec![
                crate::analysis::repair_route::SAFE_TEST_TARGET_EVIDENCE.to_string(),
                blocked,
            ])
        };

        let ret = build(
            SeamKind::ReturnValue,
            RequiredDiscriminator::ReturnValue {
                description: "changed value".to_string(),
            },
        );
        let missing = only_target(&ret)?;
        let reason = route_not_ready_reason(&ret, "repair route not ready", &missing, root);
        assert!(
            reason.contains("--at src/pricing.rs:88 --kind return_value")
                && reason.contains("no test module yet"),
            "{reason}"
        );

        let err = build(
            SeamKind::ErrorVariant,
            RequiredDiscriminator::ErrorVariant {
                variant: "Bad".to_string(),
            },
        );
        let missing = only_target(&err)?;
        let reason = route_not_ready_reason(&err, "repair route not ready", &missing, root);
        assert!(reason.contains("--kind error_path"), "{reason}");

        // Negatives: another missing fact keeps the bare label, and a seam
        // kind without a stub kind names no route.
        let mut more = only_target(&ret)?;
        more.push("incomplete evidence stage: propagation".to_string());
        let reason = route_not_ready_reason(&ret, "repair route not ready", &more, root);
        assert!(!reason.contains("ripr agent stub"), "{reason}");
        // Negative: no module blocker and nothing missing names no route.
        let mut plain = build(
            SeamKind::ReturnValue,
            RequiredDiscriminator::ReturnValue {
                description: "changed value".to_string(),
            },
        );
        plain.evidence.new_test_target = None;
        let reason = route_not_ready_reason(&plain, "repair route not ready", &[], root);
        assert!(!reason.contains("ripr agent stub"), "{reason}");
        let field = build(
            SeamKind::SideEffect,
            RequiredDiscriminator::Effect {
                sink: "log".to_string(),
            },
        );
        let missing = only_target(&field)?;
        let reason = route_not_ready_reason(&field, "repair route not ready", &missing, root);
        assert!(!reason.contains("--kind return_value"), "{reason}");
        assert!(!reason.contains("--kind error_path"), "{reason}");
        Ok(())
    }

    /// #10622: a predicate boundary whose only missing evidence is the
    /// boundary discriminator names the concrete manual route — the
    /// predicate stub derives the boundary input from the changed
    /// comparison — instead of leaving the reader at the jargon dead end.
    /// A seam that also misses a safe test target keeps the bare label:
    /// the stub alone would not ready that route.
    #[test]
    fn boundary_only_not_ready_names_the_stub_route() -> Result<(), String> {
        use crate::analysis::seams::SeamGripClass;
        let config = crate::config::RiprConfig::default();
        let policy = AgentBriefPolicy::from_config(&config);
        let root = Path::new("/repo");

        let targeted = crate::analysis::test_grip_evidence::RelatedTestGrip {
            test_name: "it_works".to_string(),
            file: std::path::PathBuf::from("src/lib.rs"),
            line: 14,
            test_target: Some(
                crate::analysis::test_grip_evidence::TestTargetEvidence::fixture(
                    "it_works",
                    std::path::Path::new("src/lib.rs"),
                    14,
                ),
            ),
            oracle_kind: crate::domain::OracleKind::RelationalCheck,
            oracle_strength: crate::domain::OracleStrength::Weak,
            evidence_summary: "relational assertion".to_string(),
            relation_reason: crate::domain::RelationReason::DirectOwnerCall,
            relation_confidence: crate::analysis::test_grip_evidence::RelationConfidence::High,
        };
        let mut boundary_only = gate_entry(SeamGripClass::WeaklyGripped, vec![targeted.clone()]);
        // Activation not observed: the boundary input is what is missing, so
        // the stub route applies (an observed boundary is #10683, below).
        boundary_only.evidence.activate = crate::domain::StageEvidence::new(
            crate::domain::StageState::Unknown,
            crate::domain::Confidence::Low,
            "test stage",
        );
        let refusal = repair_start_refusal_for_entry(root, &boundary_only, policy)
            .ok_or("a boundary-only not-ready seam must be refused")?;
        // `/repo` gains a drive on Windows and is console-quoted when it needs
        // it, so the expected root goes through the same binding the refusal
        // prints.
        let printed_root = shell_arg(&bound_root(&root.to_string_lossy()));
        assert!(
            refusal.reason.contains(&format!(
                "ripr agent stub --root {printed_root} --at src/pricing.rs:88 --kind predicate"
            )) && refusal
                .reason
                .contains("writes a fill-in skeleton for the changed comparison")
                && refusal.reason.contains("prefilled when it derives")
                && refusal.reason.contains("then rerun `ripr repair`"),
            "{}",
            refusal.reason
        );

        // Control: a seam that also misses a safe test target names no stub
        // route, because the skeleton alone would not ready that route.
        let also_missing_target =
            gate_entry(SeamGripClass::WeaklyGripped, vec![weak_related_test()]);
        let bare = repair_start_refusal_for_entry(root, &also_missing_target, policy)
            .ok_or("a target-missing not-ready seam must be refused")?;
        assert!(!bare.reason.contains("--kind predicate"), "{}", bare.reason);

        // #10683: activation observed and only a weak oracle on an admitted
        // direct test: reuse `check`'s wording, name that test, no stub route
        // and no internal jargon. Eligibility stays fail-closed.
        let mut observed = gate_entry(SeamGripClass::WeaklyGripped, vec![targeted.clone()]);
        // The discriminate stage grades the oracle weak (finding-side parity).
        observed.evidence.discriminate = crate::domain::StageEvidence::new(
            crate::domain::StageState::Weak,
            crate::domain::Confidence::High,
            "Only relational oracle found",
        );
        assert!(!repair_packet_eligibility(&observed).eligible());
        let refusal = repair_start_refusal_for_entry(root, &observed, policy)
            .ok_or("a weak-oracle boundary seam must be refused")?;
        assert_eq!(
            refusal.reason,
            format!(
                "repair route not ready: {} Related test: src/lib.rs:14 it_works",
                crate::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP
            )
        );
        assert!(
            !refusal.reason.contains("producer-owned")
                && !refusal.reason.contains("agent stub")
                && !refusal.reason.contains("boundary test"),
            "{}",
            refusal.reason
        );

        // Discriminating negative: a credited strong oracle elsewhere is a
        // different gap, so the weak-oracle wording must not apply.
        let mut not_weak = observed.clone();
        not_weak.evidence.discriminate = crate::domain::StageEvidence::new(
            crate::domain::StageState::Unknown,
            crate::domain::Confidence::Low,
            "test stage",
        );
        assert!(super::weak_oracle_guidance(&observed).is_some());
        assert!(super::weak_oracle_guidance(&not_weak).is_none());
        // Several related tests: the boundary observer is ambiguous.
        let mut two = observed.clone();
        let mut other = targeted.clone();
        other.test_name = "other".to_string();
        two.evidence.related_tests.push(std::sync::Arc::new(other));
        assert!(super::weak_oracle_guidance(&two).is_none());

        let mut strong = targeted;
        strong.test_name = "pinned".to_string();
        strong.oracle_strength = crate::domain::OracleStrength::Strong;
        let mixed = gate_entry(
            SeamGripClass::WeaklyGripped,
            vec![weak_related_test(), strong],
        );
        assert!(super::weak_oracle_guidance(&mixed).is_none());
        Ok(())
    }

    /// A real (non-test-open) gate over a root with no usable workspace never
    /// admits a seam it cannot find.
    #[test]
    fn the_real_gate_does_not_admit_a_seam_it_cannot_find() -> Result<(), String> {
        let gate = RepairStartGate::real(Path::new("/nonexistent-ripr-gate-root"));
        let refusal = gate
            .check("seam-1")
            .ok_or("an unfindable seam must not be admitted")?;
        assert_eq!(refusal.kind, RefusalKind::Unverified);
        let unnamed = gate
            .check_optional(None)
            .ok_or("a start naming no seam must not be admitted")?;
        let note = repair_start_withheld_note("/repo", "", &unnamed);
        assert!(
            note.contains("the recorded start") && !note.contains("seam ``"),
            "{note}"
        );
        Ok(())
    }

    /// #8728 follow-up: the displayed reason list bounds at three, but the
    /// disclosed total counts distinct reasons across the whole inventory.
    /// A fourth reason repeated by later seams must not inflate the total:
    /// A, B, C, D, D has exactly one omitted distinct reason.
    #[test]
    fn no_eligible_reasons_total_dedups_repeated_omitted_reasons() {
        let (reasons, total) =
            collect_no_eligible_reasons(["a", "b", "c", "d", "d", "a", "e", "e", "e"].into_iter());
        assert_eq!(
            reasons,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(total, 5, "five distinct reasons, not one per seam");
    }

    fn awaiting(
        id_tail: char,
        seam: &str,
        head: &str,
        created: u64,
    ) -> Result<RepairAttemptManifest, String> {
        Ok(RepairAttemptManifest {
            schema_version: crate::app::repair_attempt::REPAIR_ATTEMPT_SCHEMA_VERSION.to_string(),
            kind: "repair_attempt".to_string(),
            repair_attempt_id: RepairAttemptId::parse(format!(
                "repair-attempt-{}",
                id_tail.to_string().repeat(24)
            ))?,
            state: RepairAttemptState::AwaitingEdit,
            root: ".".to_string(),
            store: None,
            repository_head: head.to_string(),
            producer_version: "test".to_string(),
            seam_id: seam.to_string(),
            gap_binding: None,
            created_unix_ms: created,
            artifacts: Vec::new(),
            next_command: format!("ripr agent repair --root . --attempt {id_tail} --phase after"),
            limitations: Vec::new(),
            non_claims: Vec::new(),
            after: None,
            last_after_refusal: None,
            after_refusal_count: 0,
            after_refusal_history: Vec::new(),
            terminal_artifacts: Vec::new(),
        })
    }

    fn tally_of(
        attempts: &[(char, RepairAttemptState, u64, bool)],
    ) -> Result<ContinueTally, String> {
        let mut tally = ContinueTally::default();
        for (tail, state, created, bound) in attempts {
            let mut manifest = awaiting(*tail, "seam1", "head1", *created)?;
            manifest.state = state.clone();
            if *bound {
                manifest
                    .artifacts
                    .push(crate::app::repair_attempt::RepairAttemptArtifact {
                        role: crate::app::python_repair_binding::BINDING_ARTIFACT_ROLE.to_string(),
                        path: "binding.json".to_string(),
                        sha256: "0".repeat(64),
                        bytes: 2,
                    });
            }
            tally.add(&manifest);
        }
        Ok(tally)
    }

    /// #11597: the implicit already-complete route is open only when the
    /// newest creation time is unique and finished, with no prepared,
    /// trust-bound or eligible attempt anywhere in the store.
    #[test]
    fn implicit_already_complete_is_fail_closed_on_every_other_attempt() -> Result<(), String> {
        use RepairAttemptState::{AwaitingEdit, Failed, Prepared, ReadyToFinish};
        let id = |tail: char| format!("repair-attempt-{}", tail.to_string().repeat(24));
        // Positive: a lone newest finished attempt, older terminal ones ignored.
        let open = tally_of(&[('a', Failed, 1, false), ('b', ReadyToFinish, 2, false)])?;
        assert_eq!(
            open.already_complete_candidate()
                .map(|id| id.as_str().to_string()),
            Some(id('b'))
        );
        // Tied newest creation times: no attempt is the newest.
        let tied = tally_of(&[
            ('a', ReadyToFinish, 5, false),
            ('b', ReadyToFinish, 5, false),
        ])?;
        assert!(tied.already_complete_candidate().is_none());
        // A newer prepared attempt is the newest and not finished.
        let newer_prepared =
            tally_of(&[('a', ReadyToFinish, 1, false), ('b', Prepared, 2, false)])?;
        assert!(newer_prepared.already_complete_candidate().is_none());
        // An older prepared attempt still blocks the report.
        let older_prepared =
            tally_of(&[('a', Prepared, 1, false), ('b', ReadyToFinish, 2, false)])?;
        assert!(older_prepared.already_complete_candidate().is_none());
        // An older trust-bound attempt blocks it too.
        let older_bound =
            tally_of(&[('a', AwaitingEdit, 1, true), ('b', ReadyToFinish, 2, false)])?;
        assert_eq!(older_bound.trust_bound, 1);
        assert!(older_bound.already_complete_candidate().is_none());
        // An older eligible awaiting attempt means continue proceeds instead.
        let older_eligible = tally_of(&[
            ('a', AwaitingEdit, 1, false),
            ('b', ReadyToFinish, 2, false),
        ])?;
        assert!(older_eligible.already_complete_candidate().is_none());
        Ok(())
    }

    #[test]
    fn reuse_picks_the_newest_awaiting_attempt_for_the_same_seam_and_head() -> Result<(), String> {
        let older = awaiting('a', "seam1", "head1", 10)?;
        let newer = awaiting('b', "seam1", "head1", 20)?;
        let picked = pick_reusable_attempt([&older, &newer].into_iter(), "seam1", "head1");
        assert_eq!(
            picked.map(|attempt| attempt.attempt_id),
            Some(newer.repair_attempt_id.as_str().to_string())
        );
        Ok(())
    }

    #[test]
    fn reuse_ignores_other_seams_other_heads_and_non_awaiting_states() -> Result<(), String> {
        let other_seam = awaiting('a', "seam2", "head1", 10)?;
        let other_head = awaiting('b', "seam1", "head0", 10)?;
        let mut finished = awaiting('c', "seam1", "head1", 10)?;
        finished.state = RepairAttemptState::ReadyToFinish;
        let mut failed = awaiting('d', "seam1", "head1", 10)?;
        failed.state = RepairAttemptState::Failed;
        assert!(
            pick_reusable_attempt(
                [&other_seam, &other_head, &finished, &failed].into_iter(),
                "seam1",
                "head1"
            )
            .is_none()
        );
        Ok(())
    }

    /// #8726: an attempt that spent its after-phase retry budget refuses its
    /// after phase, so a repeated repair must start a fresh attempt instead of
    /// resuming it. An older attempt still within budget remains reusable.
    #[test]
    fn reuse_skips_an_attempt_that_spent_its_retry_budget() -> Result<(), String> {
        let mut spent = awaiting('a', "seam1", "head1", 20)?;
        spent.after_refusal_count = crate::app::repair_attempt::REPAIR_ATTEMPT_AFTER_REFUSAL_BUDGET;
        assert!(pick_reusable_attempt([&spent].into_iter(), "seam1", "head1").is_none());
        let within = awaiting('b', "seam1", "head1", 10)?;
        let picked = pick_reusable_attempt([&spent, &within].into_iter(), "seam1", "head1");
        assert_eq!(
            picked.map(|attempt| attempt.attempt_id),
            Some(within.repair_attempt_id.as_str().to_string())
        );
        Ok(())
    }

    /// RIPR-SPEC-0310: only TypeScript has a gap-bound verify route; Python
    /// keeps the plain manual route of #9102.
    #[test]
    fn terminal_reason_names_gap_bound_verify_only_for_typescript() {
        let typescript = no_seam_inventory_reason("TypeScript", "src/pricing.ts");
        assert!(
            typescript.contains("--format check-findings-json"),
            "{typescript}"
        );
        assert!(typescript.contains("--gap-id"), "{typescript}");
        let python = no_seam_inventory_reason("Python", "pricing/core.py");
        assert!(!python.contains("check-findings-json"), "{python}");
        assert!(
            python.contains("run the printed verify command"),
            "{python}"
        );
    }

    /// #11619: the check screen's plain terminal reason keeps the same route
    /// and verify pointers as the shared reason and only swaps jargon, so the
    /// repair, agent and JSON surfaces stay on the shared wording.
    #[test]
    fn plain_terminal_reason_swaps_only_jargon() {
        for (language, file) in [
            ("TypeScript", "src/pricing.ts"),
            ("Python", "pricing/core.py"),
        ] {
            let shared = no_seam_inventory_reason(language, file);
            let plain = no_seam_inventory_reason_plain(language, file);
            assert!(shared.contains("seam inventory") && shared.contains("discriminator"));
            assert!(
                !plain.contains("seam") && !plain.contains("discriminator"),
                "{plain}"
            );
            assert_eq!(
                plain,
                shared
                    .replace("seam inventory", "spot list")
                    .replace("no seam to start", "no spot to start")
                    .replace("missing discriminator", "missing check"),
                "plain wording drifted from the shared route"
            );
        }
    }

    /// #9102: a canonical gap id or preview finding id of a routed non-Rust
    /// language is a terminal state of the delegated loop, named by the
    /// typed language wire string.
    #[test]
    fn non_rust_item_language_names_gap_and_preview_probe_ids() {
        let cases = [
            (
                "gap:python:pricing/core.py:bulk_discount_rate:predicate_boundary:predicate:units>=20",
                Some("Python"),
            ),
            (
                "probe:pricing_core.py:python_preview:6705c5d5",
                Some("Python"),
            ),
            (
                "repo-probe:src_discount.ts:typescript_preview:2396aec1",
                Some("TypeScript"),
            ),
            (
                "gap:typescript:typescript_preview:2396aec1",
                Some("TypeScript"),
            ),
            // Discriminating negatives: Rust ids, stale-looking ids, an
            // unknown language wire and malformed shapes stay un-named so
            // they keep the generic stale-id guidance.
            ("gap:rust:src/lib.rs:add:call_deletion:x", None),
            ("probe:src_lib.rs:12:predicate", None),
            ("probe:src_lib.rs:rust_preview:abcd1234", None),
            ("gap:cobol:legacy.cbl:run:x", None),
            ("probe:a_py:python_preview", None),
            ("probe:a.py:python_preview:abcd:extra", None),
            ("python_preview", None),
            ("seam-discount-boundary-equality", None),
        ];
        for (item, expected) in cases {
            assert_eq!(non_rust_item_language(item), expected, "item `{item}`");
        }
    }

    /// #9102: the unknown-item message for a non-Rust gap names the
    /// terminal reason and the manual route; a Rust-shaped stale id keeps
    /// the stale-id guidance and never claims the terminal reason.
    #[test]
    fn unknown_repair_item_names_terminal_reason_only_for_non_rust_items() {
        let root = Path::new("repo");
        let python = unknown_repair_item(root, "probe:pricing_core.py:python_preview:6705c5d5");
        assert!(
            python.contains("covers Rust files only")
                && python.contains("Python")
                && python.contains("not a stale id"),
            "{python}"
        );
        let rust = unknown_repair_item(root, "probe:src_lib.rs:12:predicate");
        assert!(
            rust.contains("stale or maps to no current seam")
                && !rust.contains("covers Rust files only"),
            "{rust}"
        );
    }

    #[test]
    fn reuse_refuses_a_python_trust_bound_attempt() -> Result<(), String> {
        let mut bound = awaiting('a', "seam1", "head1", 10)?;
        bound
            .artifacts
            .push(crate::app::repair_attempt::RepairAttemptArtifact {
                role: crate::app::python_repair_binding::BINDING_ARTIFACT_ROLE.to_string(),
                path: "binding.json".to_string(),
                sha256: "0".repeat(64),
                bytes: 1,
            });
        assert!(pick_reusable_attempt([&bound].into_iter(), "seam1", "head1").is_none());
        Ok(())
    }

    /// Owns a temp test root so a mid-test failure cleans up instead of
    /// leaking the directory; mirrors the facade `Fixture` guard.
    struct TempRootGuard {
        root: std::path::PathBuf,
    }

    impl Drop for TempRootGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn facts<'a>(
        eligible_ids: &'a [String],
        total_seams: usize,
        reasons: &'a [String],
        reasons_total: usize,
        start_spec: Option<&'a CommandSpec>,
    ) -> RepairStartFacts<'a> {
        RepairStartFacts {
            root_display: ".",
            eligible_ids,
            total_seams,
            ineligible_reasons: reasons,
            ineligible_reasons_total: reasons_total,
            head: Some("head1"),
            candidate_notes: &[],
            start_spec,
            change: NoEligibleChange::Other,
        }
    }

    #[test]
    fn decide_zero_visible_seams_yields_setup_outcome() -> Result<(), String> {
        match decide_repair_start(&facts(&[], 0, &[], 0, None))? {
            RepairStartDecision::NoEligible {
                total_seams,
                reasons,
                reasons_total,
                ..
            } => {
                if total_seams != 0 || !reasons.is_empty() || reasons_total != 0 {
                    return Err("setup outcome must carry zero seams".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(..) => {
                Err("zero visible seams must not start or select".to_string())
            }
        }
    }

    #[test]
    fn decide_zero_eligible_seams_yields_limitation_outcome() -> Result<(), String> {
        let reasons = vec!["repair route not ready".to_string()];
        match decide_repair_start(&facts(&[], 2, &reasons, 1, None))? {
            RepairStartDecision::NoEligible {
                total_seams,
                reasons,
                reasons_total,
                ..
            } => {
                if total_seams != 2
                    || reasons != ["repair route not ready".to_string()]
                    || reasons_total != 1
                {
                    return Err("limitation outcome must carry the evidence".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(..) => {
                Err("zero eligible seams must not start or select".to_string())
            }
        }
    }

    /// #8728: the bounded reason list and its total travel together, so a
    /// truncated list can disclose its remainder instead of reading as
    /// complete.
    #[test]
    fn decide_zero_eligible_carries_reason_total_for_disclosure() -> Result<(), String> {
        let reasons = vec![
            "repair route not ready".to_string(),
            "cross-language oracle visibility unresolved".to_string(),
            "repair target unresolved".to_string(),
        ];
        match decide_repair_start(&facts(&[], 5, &reasons, 5, None))? {
            RepairStartDecision::NoEligible {
                reasons,
                reasons_total,
                ..
            } => {
                if reasons.len() != 3 || reasons_total != 5 {
                    return Err(format!(
                        "bounded list must stay at three with the total beside it: {reasons:?} of {reasons_total}"
                    ));
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(..) => {
                Err("zero eligible seams must not start or select".to_string())
            }
        }
    }

    /// A single candidate without an offered command is not executable:
    /// the decision degrades to inspect, never to a start.
    #[test]
    fn decide_single_candidate_without_spec_does_not_start() -> Result<(), String> {
        let eligible = vec!["seam:demo".to_string()];
        match decide_repair_start(&facts(&eligible, 1, &[], 0, None))? {
            RepairStartDecision::Action(action, _) => {
                if action.is_executable() {
                    return Err("an unoffered candidate must not execute".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } => {
                Err("an unoffered candidate must not start".to_string())
            }
            RepairStartDecision::NoEligible { .. } => {
                Err("one eligible seam is not a zero outcome".to_string())
            }
        }
    }

    /// Several eligible seams never start: the shared selector binds a
    /// bounded choice, and the offered command is ignored for a list.
    #[test]
    fn decide_several_candidates_select_instead_of_starting() -> Result<(), String> {
        let eligible = vec!["seam:a".to_string(), "seam:b".to_string()];
        let spec = repair_start_command_spec(".", "seam:a");
        match decide_repair_start(&facts(&eligible, 2, &[], 0, Some(&spec)))? {
            RepairStartDecision::Action(action, _) => {
                if action.action_class() != NextActionClass::ChooseItem {
                    return Err("several candidates must choose, not run".to_string());
                }
                if action.is_executable() {
                    return Err("a candidate list must not be executable".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } => {
                Err("several eligible seams must not start implicitly".to_string())
            }
            RepairStartDecision::NoEligible { .. } => {
                Err("eligible seams are not a zero outcome".to_string())
            }
        }
    }

    /// One candidate with its offered command and a readable HEAD starts
    /// exactly that seam; an unreadable HEAD must not start.
    #[test]
    fn decide_single_candidate_starts_only_with_spec_and_head() -> Result<(), String> {
        let eligible = vec!["seam:demo".to_string()];
        let spec = repair_start_command_spec(".", "seam:demo");
        match decide_repair_start(&facts(&eligible, 1, &[], 0, Some(&spec)))? {
            RepairStartDecision::Start { seam_id } if seam_id == "seam:demo" => {}
            _ => return Err("one offered candidate at a fresh HEAD must start".to_string()),
        }
        let mut headless = facts(&eligible, 1, &[], 0, Some(&spec));
        headless.head = None;
        if matches!(
            decide_repair_start(&headless)?,
            RepairStartDecision::Start { .. }
        ) {
            return Err("an unreadable HEAD must not start a repair".to_string());
        }
        Ok(())
    }

    /// Every ineligibility reason keeps its own distinct, human label.
    #[test]
    fn ineligibility_labels_are_distinct_and_exact() -> Result<(), String> {
        // #9604: labels render verbatim in user-facing refusal lists, so they
        // must read as sentences, not internal-enum spellings.
        let labels = [
            (
                RepairPacketIneligibility::ClassNotHeadlineEligible,
                "ripr does not start an automatic repair for this kind of finding",
            ),
            (
                RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved,
                "ripr cannot resolve which external-language observer would notice this seam",
            ),
            (
                RepairPacketIneligibility::CrossLanguageTestTargetUnresolved,
                "ripr cannot resolve a safe external-language test target for this seam",
            ),
            (
                RepairPacketIneligibility::RouteNotReady,
                "repair route not ready",
            ),
        ];
        for (reason, expected) in labels {
            if ineligibility_label(reason) != expected {
                return Err(format!("{reason:?} must read {expected:?}"));
            }
        }
        Ok(())
    }

    /// An ambiguous item lists at most eight seams, sorted, and counts the
    /// rest; the retry names the explicit seam form, never a pick.
    #[test]
    fn ambiguous_gap_item_sorts_and_bounds_the_listing() -> Result<(), String> {
        let root = Path::new("/ripr-task-first-root");
        let few = ambiguous_gap_item(root, "gap:x", &["seam:b", "seam:a"]);
        if !few.contains("names 2 seams; select one explicitly: seam:a, seam:b.")
            || few.contains("more)")
        {
            return Err(format!(
                "two matches must list sorted without a tail:\n{few}"
            ));
        }
        let ids: Vec<String> = (0..10).map(|n| format!("seam:{n}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let many = ambiguous_gap_item(root, "gap:x", &refs);
        for expected in [
            "names 10 seams",
            "seam:0, seam:1, seam:2, seam:3, seam:4, seam:5, seam:6, seam:7 (and 2 more).",
            "ripr repair <seam-id> --root",
        ] {
            if !many.contains(expected) {
                return Err(format!("bounded listing must carry {expected:?}:\n{many}"));
            }
        }
        if many.contains("seam:8") || many.contains("seam:9") {
            return Err(format!("listing must stop at eight seams:\n{many}"));
        }
        Ok(())
    }

    /// #11466: each ambiguous candidate carries its seam ID first (what the
    /// retry takes), then kind, normalized expression and owner; a long
    /// expression is cut so one line stays readable.
    #[test]
    fn ambiguous_candidates_describe_what_tells_them_apart() {
        use crate::analysis::seams::SeamGripClass;
        let entry = gate_entry(SeamGripClass::Ungripped, Vec::new());
        let text = describe_ambiguous_seam(&entry);
        assert!(
            text.starts_with(entry.seam.id().as_str())
                && text.contains("`amount >= discount_threshold`")
                && text.contains("in pricing::discounted_total"),
            "{text}"
        );
        let mut long = gate_entry(SeamGripClass::Ungripped, Vec::new());
        long.seam = crate::analysis::seams::RepoSeam::new(
            "src/pricing.rs",
            "pricing::discounted_total",
            crate::analysis::seams::SeamKind::PredicateBoundary,
            42,
            88,
            format!("a\n   {}", "b".repeat(80)),
            crate::analysis::seams::RequiredDiscriminator::BoundaryValue {
                description: "x".to_string(),
            },
            crate::analysis::seams::ExpectedSink::ReturnValue,
        );
        let text = describe_ambiguous_seam(&long);
        assert!(text.contains("`a bbbb") && text.contains("...`"), "{text}");
        assert!(!text.contains('\n'), "{text}");
    }

    /// The bound keeps the first eight matches in the order the caller supplied
    /// (analysis order), then sorts only that window for display: it is not the
    /// eight lowest IDs overall.
    #[test]
    fn ambiguous_gap_item_bounds_before_sorting() -> Result<(), String> {
        let root = Path::new("/ripr-task-first-root");
        let ids: Vec<String> = (0..10).rev().map(|n| format!("seam:{n}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let many = ambiguous_gap_item(root, "gap:x", &refs);
        let expected =
            "seam:2, seam:3, seam:4, seam:5, seam:6, seam:7, seam:8, seam:9 (and 2 more).";
        if !many.contains(expected) {
            return Err(format!(
                "the first eight supplied matches must list sorted:\n{many}"
            ));
        }
        if many.contains("seam:0") || many.contains("seam:1,") {
            return Err(format!(
                "the two omitted matches must not be listed:\n{many}"
            ));
        }
        Ok(())
    }

    /// A stale check-printed finding ID is named as stale; any other
    /// unknown item gets the plain no-seam message. Both route to pilot.
    #[test]
    fn unknown_repair_item_distinguishes_stale_finding_ids() -> Result<(), String> {
        let root = Path::new("/ripr-task-first-root");
        for item in ["probe:abc", "repo-probe:abc"] {
            let message = unknown_repair_item(root, item);
            if !message.contains("finding ID is stale or maps to no current seam")
                || !message.contains("ripr pilot --root")
            {
                return Err(format!("{item} must be named stale:\n{message}"));
            }
        }
        let plain = unknown_repair_item(root, "seam:ghost");
        if plain.contains("stale")
            || !plain.contains("repair item `seam:ghost` names no seam in the selected root")
            || !plain.contains("ripr pilot --root")
        {
            return Err(format!("plain unknown item message drifted:\n{plain}"));
        }
        Ok(())
    }

    /// An ineligible subject names item, seam and reason, states that no
    /// attempt started, and points at the seam listing.
    #[test]
    fn ineligible_repair_item_names_reason_and_no_attempt() -> Result<(), String> {
        let message = ineligible_repair_item(
            Path::new("/ripr-task-first-root"),
            "gap:x",
            "seam:y",
            "configured off",
        );
        for expected in [
            "repair item `gap:x` names seam `seam:y`",
            "not ready for an automatic repair: configured off",
            "no repair attempt was started",
            "ripr pilot --root",
        ] {
            if !message.contains(expected) {
                return Err(format!("refusal must carry {expected:?}:\n{message}"));
            }
        }
        Ok(())
    }

    /// An explicit attempt ID that is not a valid ID refuses before any
    /// store read, naming the flag; it never falls through to selection.
    #[test]
    fn explicit_invalid_attempt_id_is_refused_by_flag() -> Result<(), String> {
        let root = Path::new("/ripr-task-first-root-missing");
        match select_continue_attempt(root, Some("../escape")) {
            Err(error) if error.starts_with("continue --attempt: ") => Ok(()),
            Err(error) => Err(format!("refusal must name the flag, got: {error}")),
            Ok(_) => Err("a path-shaped attempt ID must be refused".to_string()),
        }
    }

    /// A card failure after publication names the started attempt and its
    /// recovery: the message must never imply no attempt exists.
    #[test]
    fn card_after_publish_message_names_the_started_attempt() -> Result<(), String> {
        let message = card_after_publish_message("attempt-1", "seam:demo", "ROOT", "card refused");
        for expected in [
            "repair started attempt `attempt-1`",
            "seam `seam:demo`",
            "card refused",
            "ripr status --attempt attempt-1 --root ROOT",
            "ripr continue --attempt attempt-1 --root ROOT",
        ] {
            if !message.contains(expected) {
                return Err(format!("message must carry {expected:?}:\n{message}"));
            }
        }
        for forbidden in ["failed to start", "no attempt"] {
            if message.contains(forbidden) {
                return Err(format!(
                    "message must not imply no attempt exists:\n{message}"
                ));
            }
        }
        Ok(())
    }

    /// #10698: the files check names for the commit step are exactly the
    /// tracked production files that differ from HEAD, so the printed start
    /// is never one the before phase refuses for a dirty tree.
    #[test]
    fn dirty_paths_outside_surface_mirror_the_start_premise_check() -> Result<(), String> {
        use crate::testing::fixture_git::fixture_git_ok as run_git;

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("test clock failed: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-task-first-dirty-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("tests"))
            .and_then(|()| std::fs::create_dir_all(root.join("src")))
            .map_err(|error| format!("create {} failed: {error}", root.display()))?;
        let root = root
            .canonicalize()
            .map_err(|error| format!("canonicalize {} failed: {error}", root.display()))?;
        let _guard = TempRootGuard { root: root.clone() };
        run_git(&root, &["init"])?;
        run_git(
            &root,
            &["config", "user.email", "ripr-test@example.invalid"],
        )?;
        run_git(&root, &["config", "user.name", "RIPR Test"])?;
        for (file, text) in [
            ("pricing.py", "x = 1\n"),
            ("tests/test_pricing.py", "y = 1\n"),
            ("tests/conftest.py", "z = 1\n"),
            ("util.py", "u = 1\n"),
            ("src/[ab].py", "g = 1\n"),
        ] {
            std::fs::write(root.join(file), text)
                .map_err(|error| format!("write {file} failed: {error}"))?;
        }
        run_git(&root, &["add", "."])?;
        run_git(&root, &["commit", "--no-gpg-sign", "-m", "initial"])?;

        let packet = serde_json::json!({
            "packets": [{
                "seam_id": "gap-1",
                "allowed_edit_surface": ["tests/test_pricing.py"],
                "forbidden_files": ["pricing.py"],
                "recommended_test": {"file": "tests/test_pricing.py", "name": "tests/test_pricing.py::t"}
            }]
        });
        let policy =
            crate::app::repair_attempt::edit_cage_policy_from_packet(&packet.to_string(), "gap-1")?;

        // A missing repository is unknown, never a clean tree.
        let Err(_error) = dirty_paths_outside_surface(&root.join("missing"), &policy) else {
            return Err("missing repository was treated as clean".to_string());
        };

        // Clean tree: nothing to commit, so the start prints bare.
        assert!(dirty_paths_outside_surface(&root, &policy)?.is_empty());

        // The selected test file is the only dirty path the start allows.
        std::fs::write(root.join("tests/test_pricing.py"), "y = 2\n")
            .map_err(|error| format!("write test failed: {error}"))?;
        assert!(dirty_paths_outside_surface(&root, &policy)?.is_empty());
        // A second test-looking file is NOT allowed: the start refuses it.
        std::fs::write(root.join("tests/conftest.py"), "z = 2\n")
            .map_err(|error| format!("write conftest failed: {error}"))?;
        assert_eq!(
            dirty_paths_outside_surface(&root, &policy)?,
            ["tests/conftest.py"]
        );
        std::fs::write(root.join("pricing.py"), "x = 2\n")
            .map_err(|error| format!("write production failed: {error}"))?;
        run_git(&root, &["add", "pricing.py"])?;
        assert_eq!(
            dirty_paths_outside_surface(&root, &policy)?,
            ["pricing.py", "tests/conftest.py"]
        );
        // A deleted tracked production file also differs from HEAD, and the
        // start refuses it, so it is named too.
        std::fs::remove_file(root.join("util.py"))
            .map_err(|error| format!("delete util.py failed: {error}"))?;
        assert_eq!(
            dirty_paths_outside_surface(&root, &policy)?,
            ["pricing.py", "tests/conftest.py", "util.py"]
        );
        // A tracked file whose name looks like a glob is named literally.
        std::fs::write(root.join("src/[ab].py"), "g = 2\n")
            .map_err(|error| format!("write glob-named file failed: {error}"))?;
        assert_eq!(
            dirty_paths_outside_surface(&root, &policy)?,
            ["pricing.py", "src/[ab].py", "tests/conftest.py", "util.py"]
        );
        Ok(())
    }

    #[test]
    fn dirty_origin_requires_captured_revisions_and_resolvable_git() -> Result<(), String> {
        let Err(_error) = resolved_dirty_origin(Path::new("."), None) else {
            return Err("missing revisions were accepted".to_string());
        };
        let revisions = crate::analysis::AnalyzedRevisions {
            base_ref: "HEAD".to_string(),
            base_commit: Some("a".repeat(40)),
            merge_base_commit: None,
            loaded_diff_origin: None,
            head_commit: Some("b".repeat(40)),
            working_tree: true,
        };
        let Err(_error) = resolved_dirty_origin(Path::new("."), Some(&revisions)) else {
            return Err("missing origin was accepted".to_string());
        };
        Ok(())
    }

    /// #10698: the commit hint is spelled relative to the analyzed root, which
    /// may be a subdirectory of the repository.
    #[test]
    fn commit_hint_paths_are_relative_to_the_analyzed_root() {
        assert_eq!(relative_to_cwd_root("a.py", ""), "a.py");
        assert_eq!(relative_to_cwd_root("svc/src/a.py", "svc/"), "src/a.py");
        assert_eq!(relative_to_cwd_root("other/b.py", "svc/"), "../other/b.py");
        assert_eq!(relative_to_cwd_root("b.py", "svc/deep/"), "../../b.py");
    }

    /// Implicit continue skips trust-bound awaiting attempts: the façade
    /// passes no edit authorization, so they can never continue on this
    /// route. The attempts are minted by the attempt authority itself,
    /// so the store and its commitments are exactly what production
    /// reads; only the selector's exclusion is under test.
    #[test]
    fn implicit_continue_skips_trust_bound_attempts() -> Result<(), String> {
        use crate::app::python_repair_binding::BINDING_ARTIFACT_ROLE;
        use crate::app::repair_attempt::{
            BeforeArtifactSource, BeginRepairAttemptOptions, begin_repair_attempt_with,
        };
        use crate::testing::fixture_git::fixture_git_ok as run_git;

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("test clock failed: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-task-first-trust-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create {} failed: {error}", root.display()))?;
        let root = root
            .canonicalize()
            .map_err(|error| format!("canonicalize {} failed: {error}", root.display()))?;
        let _guard = TempRootGuard { root: root.clone() };
        run_git(&root, &["init"])?;
        run_git(
            &root,
            &["config", "user.email", "ripr-test@example.invalid"],
        )?;
        run_git(&root, &["config", "user.name", "RIPR Test"])?;
        std::fs::write(root.join("README.md"), "# test\n")
            .map_err(|error| format!("write README failed: {error}"))?;
        run_git(&root, &["add", "."])?;
        run_git(&root, &["commit", "--no-gpg-sign", "-m", "initial"])?;

        let before = root.join("before.json");
        std::fs::write(&before, "{}").map_err(|error| format!("write before failed: {error}"))?;
        let binding = root.join("binding.json");
        std::fs::write(&binding, "{}").map_err(|error| format!("write binding failed: {error}"))?;
        let begin = |seam_id: &str, bound: bool| {
            let mut sources = vec![BeforeArtifactSource {
                role: "before_snapshot",
                path: &before,
            }];
            if bound {
                sources.push(BeforeArtifactSource {
                    role: BINDING_ARTIFACT_ROLE,
                    path: &binding,
                });
            }
            begin_repair_attempt_with(BeginRepairAttemptOptions {
                root: &root,
                root_argument: &root,
                seam_id,
                sources: &sources,
                expected_repository_head: None,
                next_command_suffix: None,
                store: None,
            })
        };

        begin("seam:bound", true)?;
        match select_continue_attempt(&root, None)? {
            ContinueSelection::NoneAvailable {
                prepared,
                terminal,
                trust_bound,
            } => {
                if (prepared, terminal, trust_bound) != (0, 0, 1) {
                    return Err(format!(
                        "a bound-only store must report one trust-bound attempt, got prepared={prepared} terminal={terminal} trust_bound={trust_bound}"
                    ));
                }
            }
            _ => {
                return Err("a lone trust-bound attempt must not proceed implicitly".to_string());
            }
        }
        let unbound = begin("seam:plain", false)?;
        match select_continue_attempt(&root, None)? {
            ContinueSelection::Proceed { attempt_id }
                if attempt_id.as_str() == unbound.manifest.repair_attempt_id.as_str() => {}
            _ => {
                return Err("implicit continue must select the lone unbound attempt".to_string());
            }
        }
        Ok(())
    }
}
