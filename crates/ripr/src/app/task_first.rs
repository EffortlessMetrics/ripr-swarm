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
use crate::analysis::repair_route::{RepairPacketIneligibility, repair_packet_eligibility};
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
    NextActionDiffSource, NextActionInput, NextActionProducer, current_command_platform,
    select_canonical_next_action,
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
    Action(Box<CanonicalNextActionV1>),
    NoEligible {
        total_seams: usize,
        /// Distinct ineligibility reasons rendered in the bounded list.
        reasons: Vec<String>,
        /// The total count of distinct ineligibility reasons across the
        /// visible seams (#8728). Greater than `reasons.len()` when the
        /// bounded list was truncated; the renderer discloses the
        /// remainder instead of presenting three as the whole story.
        reasons_total: usize,
    },
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
    /// The before-phase command for the single candidate, if any. Ignored
    /// unless exactly one seam is eligible.
    pub(crate) start_spec: Option<&'a CommandSpec>,
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
            .ok_or_else(|| "repair-start decision bound no subject".to_string())?;
        return Ok(RepairStartDecision::Start { seam_id });
    }
    Ok(RepairStartDecision::Action(Box::new(action)))
}

/// The one repair-admission gate, shared by the auto selector and the
/// explicit-subject resolver: a seam may start only when the producer-owned
/// packet flip is eligible and the brief policy does not omit its class.
/// Returns the refusal reason naming why the seam may not start (the
/// eligibility label, or the policy omission when the flip is eligible),
/// with the eligibility reason taking precedence over the omission.
fn repair_refusal_reason(
    entry: &crate::analysis::ClassifiedSeam,
    policy: AgentBriefPolicy<'_>,
) -> Option<String> {
    let eligibility = repair_packet_eligibility(entry);
    if !eligibility.eligible() {
        return eligibility
            .ineligibility
            .map(|reason| ineligibility_label(reason).to_string());
    }
    policy.omission_reason_for_class(entry.class)
}

fn ineligibility_label(reason: RepairPacketIneligibility) -> &'static str {
    match reason {
        RepairPacketIneligibility::ClassNotHeadlineEligible => "not a headline repair target",
        RepairPacketIneligibility::CrossLanguageOracleVisibilityUnresolved => {
            "cross-language oracle visibility unresolved"
        }
        RepairPacketIneligibility::CrossLanguageTestTargetUnresolved => {
            "cross-language test target unresolved"
        }
        RepairPacketIneligibility::RouteNotReady => "repair route not ready",
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

/// Gather the live authorities and decide one repair start. A truncated seam
/// inventory fails closed: selecting from a partial list could hide
/// candidates and silently pick first.
pub(crate) fn resolve_repair_start(root: &Path) -> Result<RepairStartDecision, String> {
    let config = load_for_root(root)?;
    let (classified, limit_info) =
        crate::analysis::inventory_classified_seams_at_with_config(root, &config)?;
    if let Some(info) = &limit_info
        && info.analyzed < info.total
    {
        return Err(format!(
            "repair start refused: the seam inventory is truncated ({} of {} seams); rerun with a wider seam budget",
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
    let mut eligible_ids = Vec::new();
    let mut refusal_reasons: Vec<String> = Vec::new();
    for entry in &classified {
        match repair_refusal_reason(entry, policy) {
            None => eligible_ids.push(entry.seam.id().as_str().to_string()),
            // Name the shared gate's own reason among the no-eligible
            // reasons so the limitation is diagnosable.
            Some(reason) => refusal_reasons.push(reason),
        }
    }
    let (reasons, reasons_total) =
        collect_no_eligible_reasons(refusal_reasons.iter().map(String::as_str));
    let bound = bound_root(&root.to_string_lossy());
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
        start_spec: spec.as_ref(),
    })
}

/// What an explicit `ripr repair` item resolved to: the seam to start,
/// or the deliberate message naming why no seam was selected.
pub(crate) enum RepairSubject {
    Seam(String),
    Decision(String),
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
    /// Exactly one seam, refused by the shared repair gate.
    Ineligible { seam_id: String, reason: String },
    /// Several seams name the item; the bounded seam IDs to choose from.
    Ambiguous(Vec<String>),
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
        RepairSubjectOutcome::Ineligible { seam_id, reason } => {
            RepairSubject::Decision(ineligible_repair_item(root, item, &seam_id, &reason))
        }
        RepairSubjectOutcome::Ambiguous(seam_ids) => {
            let seam_refs: Vec<&str> = seam_ids.iter().map(String::as_str).collect();
            RepairSubject::Decision(ambiguous_gap_item(root, item, &seam_refs))
        }
        RepairSubjectOutcome::Unknown => RepairSubject::Decision(unknown_repair_item(root, item)),
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
/// gate as the auto selector ([`repair_refusal_reason`]); zero mappings are
/// `Unknown` (stale), several are `Ambiguous`, and neither starts an attempt.
pub(crate) fn resolve_repair_subject_outcome(
    root: &Path,
    item: &str,
) -> Result<RepairSubjectOutcome, String> {
    let config = load_for_root(root)?;
    let (classified, limit_info) =
        crate::analysis::inventory_classified_seams_at_with_config(root, &config)?;
    if let Some(info) = &limit_info
        && info.analyzed < info.total
    {
        return Err(format!(
            "repair item refused: the seam inventory is truncated ({} of {} seams); rerun with a wider seam budget",
            info.analyzed, info.total
        ));
    }
    let policy = AgentBriefPolicy::from_config(&config);
    if let Some(entry) = classified
        .iter()
        .find(|entry| entry.seam.id().as_str() == item)
    {
        return Ok(match repair_refusal_reason(entry, policy) {
            None => RepairSubjectOutcome::Seam(item.to_string()),
            Some(reason) => RepairSubjectOutcome::Ineligible {
                seam_id: item.to_string(),
                reason,
            },
        });
    }
    let matches: Vec<&crate::analysis::ClassifiedSeam> = classified
        .iter()
        .filter(|entry| {
            canonical_gap_identity(entry).is_some_and(|identity| identity.id == item)
                || producer_gap_id_names_seam(item, entry)
                || probe_id_names_seam(item, entry, &classified)
                || super::selector::selector_matches_file_line(
                    crate::analysis::stable_path_text(entry.seam.file()).as_ref(),
                    entry.seam.display_line(),
                    item,
                )
        })
        .collect();
    match matches.len() {
        1 => {
            let seam_id = matches[0].seam.id().as_str().to_string();
            Ok(match repair_refusal_reason(matches[0], policy) {
                None => RepairSubjectOutcome::Seam(seam_id.clone()),
                Some(reason) => RepairSubjectOutcome::Ineligible { seam_id, reason },
            })
        }
        0 => Ok(RepairSubjectOutcome::Unknown),
        _ => Ok(RepairSubjectOutcome::Ambiguous(
            matches
                .iter()
                .map(|entry| entry.seam.id().as_str().to_string())
                .collect(),
        )),
    }
}

/// The repair route for the one finding the default human `ripr check`
/// selects (#8166 Slice C), or the named refusal when that finding may not
/// be offered a repair start (#8729). The seam↔finding binding is the
/// shared
/// [`finding_for_seam`](super::repair_card_handoff::finding_for_seam)
/// authority, and admission is the same [`repair_refusal_reason`] gate
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
    let static_limited = finding.static_limit_kind.is_some()
        || matches!(
            finding.class,
            crate::domain::ExposureClass::NoStaticPath
                | crate::domain::ExposureClass::InfectionUnknown
                | crate::domain::ExposureClass::PropagationUnknown
                | crate::domain::ExposureClass::StaticUnknown
        );
    let Some(gap) = finding.canonical_gap.as_ref() else {
        return CheckRepairRoute::Refused(RepairRouteRefusal::NoCanonicalGap);
    };
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
        return CheckRepairRoute::Refused(RepairRouteRefusal::NoBindingSeam);
    };
    if bound.next().is_some() {
        return CheckRepairRoute::Refused(RepairRouteRefusal::SeveralBindingSeams);
    }
    if let Some(reason) = repair_refusal_reason(entry, policy) {
        return CheckRepairRoute::Refused(RepairRouteRefusal::Ineligible { reason });
    }
    if static_limited {
        // F3: no runnable line for a static-limited finding. The refusal
        // arms above still named any concrete ineligibility; reaching here
        // means the seam was eligible, so the limitation guidance on the
        // same screen is the whole story and no advisory is added.
        return CheckRepairRoute::Silent;
    }
    CheckRepairRoute::Route(crate::app::RepairRoute {
        finding_id: finding.id.clone(),
        seam_id: entry.seam.id().as_str().to_string(),
    })
}

fn ineligible_repair_item(root: &Path, item: &str, seam_id: &str, reason: &str) -> String {
    let list = format!(
        "`ripr pilot --root {}`",
        shell_arg(&bound_root(&root.to_string_lossy()))
    );
    format!(
        "repair item `{item}` names seam `{seam_id}`, which is not repair-eligible: {reason}; no repair attempt was started. Run {list} to list current seam IDs"
    )
}

fn unknown_repair_item(root: &Path, item: &str) -> String {
    let list = format!(
        "`ripr pilot --root {}` to list current seam IDs",
        shell_arg(&bound_root(&root.to_string_lossy()))
    );
    if item.starts_with("probe:") || item.starts_with("repo-probe:") {
        return format!(
            "repair item `{item}` names no seam in the selected root; the `ripr check` finding ID is stale or maps to no current seam; run {list}"
        );
    }
    format!("repair item `{item}` names no seam in the selected root; run {list}")
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
    let mut eligible = Vec::new();
    let mut prepared = 0usize;
    let mut terminal = 0usize;
    let mut trust_bound = 0usize;
    for entry in inventory_repair_attempts_from(root, None)? {
        match entry {
            RepairAttemptInventoryEntry::Valid(manifest) => match manifest.state {
                RepairAttemptState::AwaitingEdit => {
                    // The façade passes no edit authorization, so a
                    // trust-bound attempt can never continue on this
                    // route: the after phase would refuse it. Exclude it
                    // from implicit selection and name the advanced
                    // route instead. The binding is an immutable
                    // manifest fact, so this filter cannot go stale
                    // between selection and the after phase. An
                    // explicitly named bound attempt still proceeds
                    // into the shared after selection, whose refusal
                    // names the missing signals.
                    if find_manifest_artifact_by_role(
                        &manifest,
                        crate::app::python_repair_binding::BINDING_ARTIFACT_ROLE,
                    )
                    .is_some()
                    {
                        trust_bound += 1;
                    } else {
                        eligible.push(manifest.repair_attempt_id.as_str().to_string());
                    }
                }
                RepairAttemptState::Prepared => prepared += 1,
                RepairAttemptState::ReadyToFinish
                | RepairAttemptState::Stale
                | RepairAttemptState::Incomparable
                | RepairAttemptState::Failed => terminal += 1,
            },
            RepairAttemptInventoryEntry::Invalid { directory, error } => {
                return Err(format!(
                    "continue selection refused: attempt row `{directory}` is invalid: {error}"
                ));
            }
        }
    }
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
        None => Ok(ContinueSelection::NoneAvailable {
            prepared,
            terminal,
            trust_bound,
        }),
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

#[cfg(test)]
mod tests {
    use super::*;
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
            created_unix_ms: created,
            artifacts: Vec::new(),
            next_command: format!("ripr agent repair --root . --attempt {id_tail} --phase after"),
            limitations: Vec::new(),
            non_claims: Vec::new(),
            after: None,
            last_after_refusal: None,
            terminal_artifacts: Vec::new(),
        })
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
            start_spec,
        }
    }

    #[test]
    fn decide_zero_visible_seams_yields_setup_outcome() -> Result<(), String> {
        match decide_repair_start(&facts(&[], 0, &[], 0, None))? {
            RepairStartDecision::NoEligible {
                total_seams,
                reasons,
                reasons_total,
            } => {
                if total_seams != 0 || !reasons.is_empty() || reasons_total != 0 {
                    return Err("setup outcome must carry zero seams".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(_) => {
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
            } => {
                if total_seams != 2
                    || reasons != ["repair route not ready".to_string()]
                    || reasons_total != 1
                {
                    return Err("limitation outcome must carry the evidence".to_string());
                }
                Ok(())
            }
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(_) => {
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
            RepairStartDecision::Start { .. } | RepairStartDecision::Action(_) => {
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
            RepairStartDecision::Action(action) => {
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
