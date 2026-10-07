use crate::agent::loop_commands::shell_arg;
use crate::app::{CheckDiffProvenance, CheckOutput, FindingDrillIn};
use crate::config::RiprConfig;
use crate::domain::{
    CanonicalNextActionV1, ExposureClass, Finding, LanguageId, NextActionAlternative,
    NextActionCheckCase, NextActionCurrentness, NextActionDiffSource, NextActionInput,
    NextActionProducer, NextActionStop, ProbeFamily, StaticLimitKind, current_command_platform,
    select_canonical_next_action,
};
use crate::output::path::display_path;
use crate::output::preview_actionability::preview_actionability_for;
use crate::output::python_repair_card::python_repair_card;
use crate::output::typescript_packet_projection::typescript_gap_record_for;
use std::collections::BTreeSet;

use super::sections::{one_line, render_finding_digest_with_config};

pub(crate) struct HumanTriage<'a> {
    pub(crate) state: HumanTriageState,
    pub(crate) selected: Option<&'a Finding>,
    /// Every considered (non-suppressed) finding the default human render does
    /// not show: the lower-ranked candidates plus, when nothing was
    /// candidate-actionable, all of them. #4320 names them in `Hidden:` by
    /// `file:line (class)` so a reader can confirm coverage without a rerun;
    /// #4395(b) summarizes preview / non-Rust identity inline.
    pub(crate) omitted: Vec<&'a Finding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HumanTriageState {
    TopGap,
    NoActionableGap,
    StaticLimited,
    PreviewLimited,
    MissingScope,
}

impl HumanTriageState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TopGap => "top_gap",
            Self::NoActionableGap => "no_actionable_gap",
            Self::StaticLimited => "static_limited",
            Self::PreviewLimited => "preview_limited",
            Self::MissingScope => "missing_scope",
        }
    }

    /// The state in words; the stable id follows it in parentheses.
    fn plain_label(self) -> &'static str {
        match self {
            Self::TopGap => "a test gap to inspect or repair",
            Self::NoActionableGap => "no gap selected for repair",
            Self::StaticLimited => "limited by static analysis",
            Self::PreviewLimited => "preview language, advisory only",
            Self::MissingScope => "nothing in scope",
        }
    }
}

pub(crate) fn select_human_triage<'a>(
    output: &'a CheckOutput,
    _config: &RiprConfig,
) -> HumanTriage<'a> {
    let suppressed_ids: BTreeSet<&str> = output
        .suppression
        .iter()
        .flat_map(|outcome| {
            outcome
                .suppressed
                .iter()
                .map(|entry| entry.finding_id.as_str())
        })
        .collect();
    let mut selected = None;
    let mut considered: Vec<&'a Finding> = Vec::new();
    let mut visible_findings: usize = 0;
    for finding in &output.findings {
        if suppressed_ids.contains(finding.id.as_str()) {
            continue;
        }
        considered.push(finding);
        // Candidate-actionable eligibility (#3281): "Start here" names a
        // current candidate-side obligation. Base-side evidence and
        // unresolved subjects remain counted findings (the hidden-count
        // denominator below keeps them) but never become the top gap.
        if !finding.is_candidate_actionable() {
            continue;
        }
        visible_findings += 1;
        if selected.is_none_or(|current| triage_rank(finding) < triage_rank(current)) {
            selected = Some(finding);
        }
    }
    let state = selected.map_or_else(
        || {
            if output.no_scope_provided {
                HumanTriageState::MissingScope
            } else if !output.findings.is_empty() && visible_findings == 0 {
                HumanTriageState::NoActionableGap
            } else {
                HumanTriageState::StaticLimited
            }
        },
        |finding| {
            if is_preview_limited(finding) {
                HumanTriageState::PreviewLimited
            } else if finding.class == ExposureClass::Exposed {
                HumanTriageState::NoActionableGap
            } else if is_static_limited(finding) {
                HumanTriageState::StaticLimited
            } else {
                HumanTriageState::TopGap
            }
        },
    );
    // #4320: the hidden set is every considered finding except the one
    // selected finding (identity by finding id, not value equality).
    HumanTriage {
        state,
        selected,
        omitted: considered
            .into_iter()
            .filter(|finding| selected.is_none_or(|chosen| chosen.id != finding.id))
            .collect(),
    }
}

pub(crate) fn render_human_triage(
    out: &mut String,
    triage: &HumanTriage<'_>,
    output: &CheckOutput,
    config: &RiprConfig,
    drill_in: Option<&FindingDrillIn>,
    provenance: CheckDiffProvenance,
) {
    out.push_str("Start here:\n");
    out.push_str(&format!(
        "  State: {} ({})\n",
        triage.state.plain_label(),
        triage.state.as_str()
    ));
    // The canonical decision (#6304) selects the line family; the prose
    // below is unchanged, so the decision has one authority while rendered
    // bytes stay put.
    match check_case_or_fallback(triage, output, drill_in, provenance) {
        NextActionCheckCase::TopGap => out.push_str(
            "  Safe next action: inspect or repair the selected non-exposed gap; this is static advisory evidence only.\n",
        ),
        NextActionCheckCase::CandidateFilterHidAll => {
            // #4320: findings exist but the #3281 candidate filter hid
            // every one — the run is not a policy suppression. Claiming
            // suppression contradicts the suppression block (which listed
            // nothing) and misleads the reader about what happened to the
            // findings. #4320 review: the framing must also keep the
            // currentness distinction — `unresolved_subject` is the
            // explicit unknown, not base-side evidence.
            out.push_str(&no_selection_safe_action_line(&triage.omitted));
        }
        NextActionCheckCase::SuppressedByPolicy => {
            out.push_str(
                "  Safe next action: all findings are suppressed by policy; review the suppression block before treating this run as actionable.\n",
            );
        }
        NextActionCheckCase::NoDiffFinding => {
            out.push_str(
                "  Safe next action: no non-exposed diff finding was selected. This is not runtime proof, coverage adequacy, or mutation confirmation.\n",
            );
        }
        NextActionCheckCase::StaticLimited => {
            let plain_no_path = triage.selected.is_some_and(|finding| {
                finding.class == ExposureClass::NoStaticPath && finding.static_limit_kind.is_none()
            });
            out.push_str(if plain_no_path {
                "  Safe next action: review the unresolved static path and existing tests before treating this as repair-ready.\n"
            } else {
                "  Safe next action: inspect the named static limitation before treating this as repair-ready.\n"
            });
        }
        NextActionCheckCase::PreviewAdvisory => {
            // #2273: the shared repair-packet validator is the only authority
            // on packet completeness, and the line must name the real blocker:
            // a complete packet stays advisory (do not tell the operator to
            // complete fields that are already present); a blocked packet with
            // no missing fields AND a structured static-limit kind is held by
            // that named limitation, not by absent fields; anything else has
            // genuinely missing fields. Languages without a structured preview
            // packet (for example Python) fall through to the generic line.
            match triage.selected.and_then(preview_actionability_for) {
                Some(actionability) if actionability.repair_packet_ready => {
                    out.push_str(&complete_packet_preview_safe_action(
                        triage.selected,
                        &actionability.repair_route,
                    ));
                }
                Some(actionability)
                    if actionability.missing_actionability_fields.is_empty()
                        && triage
                            .selected
                            .is_some_and(|finding| finding.static_limit_kind.is_some()) =>
                {
                    out.push_str(
                        "  Safe next action: preview-language evidence is advisory; the repair packet is blocked by the named static limitation, not by missing fields; resolve the limitation and rerun preview evidence before acting.\n",
                    );
                }
                not_ready => match triage.selected {
                    Some(finding) if finding.language == Some(LanguageId::Python) => {
                        out.push_str(&python_preview_safe_action(finding));
                    }
                    Some(finding) if finding.class == ExposureClass::Exposed => {
                        out.push_str(EXPOSED_PREVIEW_SAFE_ACTION);
                    }
                    Some(finding) if let Some(actionability) = not_ready.as_ref() => {
                        out.push_str(&packet_closed_preview_safe_action(
                            finding,
                            &actionability.why_not_actionable,
                        ));
                    }
                    _ => out.push_str(
                        "  Safe next action: preview-language evidence is advisory; complete the missing repair-packet fields before acting.\n",
                    ),
                },
            }
        }
        // #4012: on an established-but-empty range the scope was provided
        // (a default base was resolved and compared) — the honest action is
        // to change something, not to provide a scope.
        NextActionCheckCase::ScopeMissing => {
            if let Some(base) = output.base.as_deref() {
                // "tracked" (#5258): `--worktree` diffs tracked edits only,
                // so the line must not promise it covers untracked files.
                out.push_str(&format!(
                    "  Safe next action: no changed files were compared against `{base}`; commit a change and re-run, or add `--worktree` to include uncommitted tracked edits.\n"
                ));
            } else {
                out.push_str(
                    "  Safe next action: provide an analysis scope; this empty output is not an all-clear.\n",
                );
            }
        }
    }
    if let Some(finding) = triage.selected {
        out.push_str(&render_finding_digest_with_config(
            finding,
            config,
            &output.root,
        ));
        if let Some(FindingDrillIn::Commands(navigation)) = drill_in {
            out.push_str("\nNext: drill into the top finding:\n");
            for command in [
                navigation.explain_command(&finding.id),
                navigation.context_command(&finding.id),
            ] {
                out.push_str(&format!("  {command}\n"));
                super::push_powershell_variant(out, "  ", &command);
            }
            // #5355: a Rust gap gets the one-step route to a runnable test.
            // A gap withheld because ripr could not read the related
            // assertions (RIPR-SPEC-0240) claims no missing test, so it gets
            // no test-writing route either.
            if finding.class != ExposureClass::Exposed
                && finding.static_limit_kind
                    != Some(StaticLimitKind::RustAssertionContextUnresolved)
                && matches!(
                    finding.probe.family,
                    ProbeFamily::Predicate
                        | ProbeFamily::ReturnValue
                        | ProbeFamily::ErrorPath
                        | ProbeFamily::MatchArm
                )
                && finding
                    .probe
                    .location
                    .file
                    .extension()
                    .and_then(|ext| ext.to_str())
                    == Some("rs")
            {
                // `--at` resolves against `--root`, so name the file
                // relative to it, not as the checkout-relative display path.
                let location = &finding.probe.location.file;
                let relative = location.strip_prefix(&output.root).unwrap_or(location);
                let file = display_path(relative);
                let command = navigation
                    .stub_command(file.trim_start_matches("./"), finding.probe.location.line);
                out.push_str("Write a test for it:\n");
                out.push_str(&format!("  {command}\n"));
                super::push_powershell_variant(out, "  ", &command);
            }
        }
    }
    // #2567: the default human render is the release-facing surface, so it must
    // not claim a hidden remainder that does not exist. `Hidden:` plus a literal
    // `0 lower-priority finding(s) omitted` reads as suppressed evidence and was
    // the dominant case in fixture output. When nothing is omitted, keep only the
    // format pointers under a `More:` heading; the count line stays for the real
    // truncation case, where it is the whole point of the section.
    //
    // #4320: when something IS hidden, the block names it — by
    // `file:line (class)`, so a reader can confirm a file they care about was
    // covered without a rerun — and distinguishes the all-base-side case, where
    // nothing was candidate-actionable and a lower-priority framing would
    // misdescribe the run.
    //
    // #5021: that currentness mix is a property of the omitted set, not of
    // whether a selection exists. When a top gap IS selected, the omitted set
    // can still hold base-side or unresolved-currentness evidence next to
    // lower-ranked candidates, so the selected-branch count line names the
    // same mix instead of labeling every omission "lower-priority".
    if triage.omitted.is_empty() {
        out.push_str("\nMore:\n");
    } else {
        out.push_str("\nHidden:\n");
        if triage.selected.is_none() {
            out.push_str(&no_selection_hidden_line(&triage.omitted));
        } else {
            out.push_str(&selected_hidden_line(&triage.omitted));
        }
        let listed = triage.omitted.len().min(HIDDEN_FINDINGS_LISTED);
        for finding in triage.omitted.iter().take(listed) {
            out.push_str(&format!(
                "    - {}:{} ({})\n",
                display_path(&finding.probe.location.file),
                finding.probe.location.line,
                finding.class.as_str()
            ));
        }
        let remaining = triage.omitted.len() - listed;
        if remaining > 0 {
            out.push_str(&format!(
                "    - … and {remaining} more omitted finding(s); every identity is in --format json.\n"
            ));
        }
    }
    out.push_str("  Full evidence: rerun with --format human-full\n");
    out.push_str("  Machine data: rerun with --format json\n\n");
}

/// #4320: the `Hidden:` list names omitted findings so the reader can confirm
/// coverage without a rerun, but the default surface stays bounded: beyond
/// this window the list discloses the remainder instead of printing every
/// identity.
const HIDDEN_FINDINGS_LISTED: usize = 20;

/// The currentness mix of the omitted set. `unresolved_subject` is the
/// explicit unknown (#3281) — not base-side evidence — so the #4320
/// no-selection framing must name the actual mix instead of promoting every
/// unselected finding to a base-side claim (#4320 review).
enum NoSelectionMix {
    AllBaseSide,
    AllUnresolved,
    Mixed { base_side: usize, unresolved: usize },
}

/// #5021: the currentness counts of an omitted set, shared by the
/// no-selection and selected Hidden count lines so both surfaces name the
/// same mix. Base-side evidence (`base_deleted`, `moved_or_renamed`) is
/// never a candidate edit target; `unresolved_subject` is the explicit
/// unknown (#3281), counted separately so it is not promoted to base-side.
/// The coarse check case behind a triage outcome. Pure and infallible: the
/// canonical projection and the renderer's defensive fallback share it, so
/// the two can never disagree on the mapping.
pub(crate) fn check_case_for_triage(
    triage: &HumanTriage<'_>,
    output: &CheckOutput,
) -> NextActionCheckCase {
    match triage.state {
        HumanTriageState::TopGap => NextActionCheckCase::TopGap,
        HumanTriageState::NoActionableGap => {
            if triage.selected.is_none() && !triage.omitted.is_empty() {
                NextActionCheckCase::CandidateFilterHidAll
            } else if triage.selected.is_none() && !output.findings.is_empty() {
                NextActionCheckCase::SuppressedByPolicy
            } else {
                NextActionCheckCase::NoDiffFinding
            }
        }
        HumanTriageState::StaticLimited => NextActionCheckCase::StaticLimited,
        HumanTriageState::PreviewLimited => NextActionCheckCase::PreviewAdvisory,
        HumanTriageState::MissingScope => NextActionCheckCase::ScopeMissing,
    }
}

/// Project the check producer's canonical action from its triage outcome.
/// The triage rank already selected the top item, so the adapter binds the
/// winner (never the rank losers) and carries up to two omitted findings as
/// bounded subordinate alternatives.
pub(crate) fn canonical_next_action_for_triage(
    triage: &HumanTriage<'_>,
    output: &CheckOutput,
    drill_in: Option<&FindingDrillIn>,
    provenance: CheckDiffProvenance,
) -> Result<CanonicalNextActionV1, String> {
    let root = output.root.display().to_string();
    let scope = check_scope_descriptor(triage, output);
    let item_id = triage
        .selected
        .map(|finding| finding.id.clone())
        .unwrap_or_else(|| scope.clone());
    let detail_route = match (&triage.selected, drill_in) {
        (Some(finding), Some(FindingDrillIn::Commands(navigation))) => {
            navigation.explain_command(&finding.id)
        }
        (Some(finding), None) => finding.id.clone(),
        (None, _) => scope.clone(),
    };
    let mut alternatives: Vec<NextActionAlternative> = Vec::new();
    for omitted in triage.omitted.iter().take(2) {
        let route = match drill_in {
            Some(FindingDrillIn::Commands(navigation)) => navigation.explain_command(&omitted.id),
            None => omitted.id.clone(),
        };
        alternatives.push(NextActionAlternative {
            label: format!("considered finding {}", omitted.id),
            route,
        });
    }
    let mut limitations = Vec::new();
    if output.unanalyzed_working_tree {
        limitations.push("uncommitted working-tree edits were not analyzed".to_string());
    }
    if output.partial_scope.is_some() {
        limitations
            .push("partial diff scope: findings cover the selected partition only".to_string());
    }
    let input = NextActionInput {
        producer: NextActionProducer::CheckTopResult,
        root,
        // The diff-source mode comes from the producer's declared
        // provenance, never from base presence: a `--worktree` run
        // resolves a base yet analyzes the live tree, while a supplied
        // scope has no base yet is fixed replayable content, not the
        // live tree.
        diff_source: match provenance {
            CheckDiffProvenance::Worktree => NextActionDiffSource::WorkingTree { head: None },
            CheckDiffProvenance::SuppliedScope => NextActionDiffSource::Committed {
                base: None,
                head: None,
            },
            CheckDiffProvenance::CommittedHistory => NextActionDiffSource::Committed {
                base: output.base.clone(),
                head: None,
            },
        },
        item_id,
        check_item: triage.selected.map(|finding| finding.id.clone()),
        card_item: None,
        item_candidates: Vec::new(),
        attempts: Vec::new(),
        // Finding currentness is enforced upstream by the triage candidate
        // filter; the check producer tracks no git-head axis here.
        currentness: NextActionCurrentness {
            head_expected: None,
            head_observed: None,
            config_expected: None,
            config_observed: None,
        },
        offered_command: None,
        route_admitted: true,
        route_refusal: None,
        missing_input: None,
        platform: current_command_platform(),
        limitation: None,
        limitation_route: None,
        detail_route,
        transition_from: triage.state.as_str().to_string(),
        transition_to: None,
        restart_route: match drill_in {
            Some(FindingDrillIn::Commands(navigation)) => navigation.list_command(),
            None => format!(
                "ripr check --root {}",
                shell_arg(&output.root.display().to_string())
            ),
        },
        check_case: Some(check_case_for_triage(triage, output)),
        doctor_recovery: None,
        pilot_delegation: None,
        alternatives,
        limitations,
    };
    select_canonical_next_action(&input)
}

/// The scope identity bound when triage selected no finding: the compared
/// base, the working tree, or the explicit lack of scope.
fn check_scope_descriptor(triage: &HumanTriage<'_>, output: &CheckOutput) -> String {
    if let Some(base) = output.base.as_deref() {
        return format!("base:{base}");
    }
    if output.unanalyzed_working_tree {
        return "working_tree".to_string();
    }
    if output.no_scope_provided {
        return "no_scope".to_string();
    }
    if triage.selected.is_some() {
        return "selected".to_string();
    }
    "empty_scope".to_string()
}

/// The line-family decision for the triage renderer: the canonical
/// projection's case, falling back to the shared mapping when the projection
/// cannot be built (a producer bug, never a triage state). The fallback
/// renders the same family the projection would, so the render stays total.
fn check_case_or_fallback(
    triage: &HumanTriage<'_>,
    output: &CheckOutput,
    drill_in: Option<&FindingDrillIn>,
    provenance: CheckDiffProvenance,
) -> NextActionCheckCase {
    canonical_next_action_for_triage(triage, output, drill_in, provenance)
        .ok()
        .and_then(|action| match action.stop() {
            Some(NextActionStop::CheckTriage { case }) => Some(*case),
            _ => None,
        })
        .unwrap_or_else(|| check_case_for_triage(triage, output))
}

fn omitted_currentness_counts(omitted: &[&Finding]) -> (usize, usize) {
    let base_side = omitted
        .iter()
        .filter(|finding| {
            matches!(
                finding.source_currentness,
                crate::domain::SourceCurrentness::BaseDeleted
                    | crate::domain::SourceCurrentness::MovedOrRenamed
            )
        })
        .count();
    let unresolved = omitted
        .iter()
        .filter(|finding| {
            finding.source_currentness == crate::domain::SourceCurrentness::UnresolvedSubject
        })
        .count();
    (base_side, unresolved)
}

fn no_selection_mix(omitted: &[&Finding]) -> NoSelectionMix {
    let (base_side, unresolved) = omitted_currentness_counts(omitted);
    match (base_side, unresolved) {
        (0, 0) => NoSelectionMix::AllBaseSide,
        (_, 0) => NoSelectionMix::AllBaseSide,
        (0, _) => NoSelectionMix::AllUnresolved,
        (b, u) => NoSelectionMix::Mixed {
            base_side: b,
            unresolved: u,
        },
    }
}

/// #5021: the `Hidden:` count line for a run where a top gap was selected.
/// `triage.omitted` is every considered finding except the selected one, so
/// it can hold base-side or unresolved-currentness evidence next to
/// lower-ranked candidates; a bare "N lower-priority finding(s) omitted"
/// would misdescribe that evidence the same way the #4320 no-selection path
/// already refuses to. Pure lower-priority omitted sets keep the legacy
/// single clause unchanged.
fn selected_hidden_line(omitted: &[&Finding]) -> String {
    let suffix = omitted_identity_suffix(omitted);
    let (base_side, unresolved) = omitted_currentness_counts(omitted);
    let lower_priority = omitted.len() - (base_side + unresolved);
    match (base_side, unresolved, lower_priority) {
        (0, 0, _) => format!(
            "  {} lower-priority finding(s) omitted from default human output{}.\n",
            omitted.len(),
            suffix
        ),
        (_, 0, 0) => format!(
            "  All {} omitted finding(s) are base-side evidence, not candidate edit targets{}.\n",
            omitted.len(),
            suffix
        ),
        (0, _, 0) => format!(
            "  All {} omitted finding(s) have unresolved subject currentness — not established base-side or candidate edit targets{}.\n",
            omitted.len(),
            suffix
        ),
        (_, _, 0) => format!(
            "  All {} omitted finding(s) are not candidate edit targets ({} base-side, {} unresolved currentness){}.\n",
            omitted.len(),
            base_side,
            unresolved,
            suffix
        ),
        (base_side, 0, lower_priority) => format!(
            "  {} lower-priority finding(s) omitted; {} base-side evidence, not candidate edit targets{}.\n",
            lower_priority, base_side, suffix
        ),
        (0, unresolved, lower_priority) => format!(
            "  {} lower-priority finding(s) omitted; {} unresolved currentness, not candidate edit targets{}.\n",
            lower_priority, unresolved, suffix
        ),
        (base_side, unresolved, lower_priority) => format!(
            "  {} lower-priority finding(s) omitted; {} base-side evidence and {} unresolved currentness, not candidate edit targets{}.\n",
            lower_priority, base_side, unresolved, suffix
        ),
    }
}

/// The `Hidden:` count line for a run where nothing was candidate-actionable.
fn no_selection_hidden_line(omitted: &[&Finding]) -> String {
    let suffix = omitted_identity_suffix(omitted);
    match no_selection_mix(omitted) {
        NoSelectionMix::AllBaseSide => format!(
            "  All {} finding(s) are base-side evidence, not candidate edit targets — rerun with --format human-full for the full evidence{}.\n",
            omitted.len(),
            suffix
        ),
        NoSelectionMix::AllUnresolved => format!(
            "  All {} finding(s) have unresolved subject currentness — not established base-side or candidate edit targets; rerun with --format human-full for the full evidence{}.\n",
            omitted.len(),
            suffix
        ),
        NoSelectionMix::Mixed {
            base_side,
            unresolved,
        } => format!(
            "  None of the {} finding(s) is a candidate edit target ({} base-side, {} unresolved currentness) — rerun with --format human-full for the full evidence{}.\n",
            omitted.len(),
            base_side,
            unresolved,
            suffix
        ),
    }
}

/// The safe-next-action line for a run where nothing was candidate-actionable.
fn no_selection_safe_action_line(omitted: &[&Finding]) -> String {
    match no_selection_mix(omitted) {
        NoSelectionMix::AllBaseSide => {
            "  Safe next action: all findings are base-side evidence, not candidate edit targets; rerun with --format human-full to inspect the full evidence before treating this run as actionable.\n".to_string()
        }
        NoSelectionMix::AllUnresolved => {
            "  Safe next action: no finding is resolved to the candidate (subject currentness unresolved); rerun with --format human-full to inspect the full evidence before treating this run as actionable.\n".to_string()
        }
        NoSelectionMix::Mixed { base_side, unresolved } => format!(
            "  Safe next action: no finding is a candidate edit target ({} base-side, {} unresolved currentness); rerun with --format human-full to inspect the full evidence before treating this run as actionable.\n",
            base_side, unresolved
        ),
    }
}

/// #4395(b): the Hidden count line names omitted preview / non-Rust identity
/// from fields already on those findings. This is not the #2615 availability
/// projection. Rust-only remainder stays the count line alone.
fn omitted_identity_suffix(omitted: &[&Finding]) -> String {
    match omitted_language_identity(omitted) {
        Some(identity) => format!(" ({identity})"),
        None => String::new(),
    }
}

fn omitted_language_identity(omitted: &[&Finding]) -> Option<String> {
    if !omitted.iter().any(|finding| {
        is_preview_limited(finding) || finding.language.is_some_and(|id| id != LanguageId::Rust)
    }) {
        return None;
    }

    let mut parts = Vec::new();
    for language in LanguageId::ALL {
        let count = omitted
            .iter()
            .filter(|finding| finding.language == Some(language))
            .count();
        if count == 0 {
            continue;
        }
        let preview = omitted
            .iter()
            .any(|finding| finding.language == Some(language) && is_preview_limited(finding));
        if preview {
            parts.push(format!("{} preview: {count}", language.display_name()));
        } else {
            parts.push(format!("{}: {count}", language.display_name()));
        }
    }
    let unlabeled_preview = omitted
        .iter()
        .filter(|finding| finding.language.is_none() && is_preview_limited(finding))
        .count();
    if unlabeled_preview > 0 {
        parts.push(format!("preview-language: {unlabeled_preview}"));
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(", "))
}

/// A complete (validator-approved) preview packet: name the packet's own
/// repair action, test file, and verify command so the operator can act on
/// it, rather than a bare "verify independently" with no route. Falls back to
/// the generic advisory line when the projected record lacks a test file or
/// verify command (for example a synthetic Perl packet). Readiness is only
/// read here, never decided.
fn complete_packet_preview_safe_action(finding: Option<&Finding>, repair_route: &str) -> String {
    const GENERIC: &str = "  Safe next action: preview-language evidence is advisory; the repair packet is complete but remains advisory, so verify independently before acting.\n";
    let Some(record) = finding.and_then(typescript_gap_record_for) else {
        return GENERIC.to_string();
    };
    let target_file = record
        .repair_route
        .as_ref()
        .and_then(|route| route.target_file.as_deref())
        .filter(|file| !file.trim().is_empty());
    let verify = record
        .verification_commands
        .first()
        .map(String::as_str)
        .filter(|command| !command.trim().is_empty());
    let (Some(target_file), Some(verify)) = (target_file, verify) else {
        return GENERIC.to_string();
    };
    // The action is the packet's own instruction and carries the concrete
    // assertion shape, so it is collapsed to one line but not truncated.
    let action = repair_route
        .replacen(" in the related test", "", 1)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "  Safe next action: preview-language evidence is advisory; the repair packet is complete: in `{target_file}`, {action}; run `{verify}`, then rerun `ripr check`.\n"
    )
}

/// An exposed preview finding has nothing to repair, in any preview language.
const EXPOSED_PREVIEW_SAFE_ACTION: &str = "  Safe next action: preview-language evidence is advisory; a related test appears to observe this change, so there is no repair to make; verify independently before relying on it.\n";

/// #4216 (TS/JS arm of row 1): the shared validator behind
/// `preview_actionability_for` kept this preview finding's repair packet
/// closed, so `check` emits no repair packet, the gap ledger carries no repair
/// route (#4224), and no ripr command routes it. "Complete the missing
/// repair-packet fields" asked for something the operator cannot supply. The
/// line is terminal instead: it quotes `preview_actionability_for`'s
/// `why_not_actionable` (in most closed-packet cases the validator never
/// ran) and names the manual step. Readiness is only read here, never
/// decided.
fn packet_closed_preview_safe_action(finding: &Finding, why_not_actionable: &str) -> String {
    let language = finding
        .language
        .map_or("preview-language", LanguageId::display_name);
    let manual_step = match finding.class {
        ExposureClass::NoStaticPath => "add a test that calls it by hand",
        // An unknown class is a visibility limit (for example the Bun bridge's
        // cross-language gap), not a known weak test: ask for the same check
        // the Python static-limit line names, not a test edit.
        ExposureClass::StaticUnknown
        | ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown => "check by hand whether a test observes this change",
        _ => "add or strengthen a test by hand",
    };
    // Only the quoted reason is bounded; the routing and manual-step parts
    // stay whole.
    // The authority's reason opens with a generic preview preamble and ends
    // with `validator: <specific cause>`; under the line budget the specific
    // cause is the part the user can act on, so show it when present.
    // The cause itself may still open with the fixed eligibility phrase;
    // drop it so the remedy ("derive an input ...") fits the budget.
    let specific = why_not_actionable
        .split_once("validator: ")
        .map_or(why_not_actionable, |(_, cause)| cause);
    let specific = specific
        .strip_prefix("is not agent-packet eligible: ")
        .unwrap_or(specific);
    let reason = one_line(specific);
    format!(
        "  Safe next action: this {language} preview finding's repair packet is not ready ({reason}); `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; {manual_step}, then rerun `ripr check`.\n"
    )
}

/// #4216 row 1: Python has no structured preview packet, so the generic
/// "complete the missing repair-packet fields" line told the operator to do
/// something they cannot, and pilot, first-pr and status each routed back to
/// another command. The Python repair card (`python_repair_card`) is the one
/// authority on whether a Python finding carries a repair route: with a card,
/// its suggested test and verify command are the route; without one, no ripr
/// command will route the finding, so the line says why and names the only
/// step left, a manual test or check. An exposed finding has nothing to repair.
fn python_preview_safe_action(finding: &Finding) -> String {
    if finding.class == ExposureClass::Exposed {
        return EXPOSED_PREVIEW_SAFE_ACTION.to_string();
    }
    if python_repair_card(finding).is_some() {
        return "  Safe next action: preview-language evidence is advisory; apply the next step below to the suggested test and run its verify command before relying on it.\n".to_string();
    }
    if let Some(kind) = finding.static_limit_kind.as_ref() {
        return format!(
            "  Safe next action: static limitation `{}` keeps this Python preview finding from a repair card, so `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; check by hand whether a test observes this change, then rerun `ripr check`.\n",
            kind.as_str()
        );
    }
    let (reason, manual_step) = if finding.class == ExposureClass::NoStaticPath {
        (
            "no Python test reaches this code",
            "add a test that calls it by hand",
        )
    } else if finding.activation.missing_discriminators.is_empty() {
        (
            "static evidence names no concrete missing discriminator",
            "add or strengthen a test by hand",
        )
    } else {
        (
            "its test placement or related-test evidence is incomplete",
            "add or strengthen a test by hand",
        )
    };
    format!(
        "  Safe next action: this Python preview finding has no repair card ({reason}), so `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; {manual_step}, then rerun `ripr check`.\n"
    )
}

fn triage_rank(finding: &Finding) -> (u8, u8, u8, u8, u8, i32, &std::path::Path, usize) {
    let class_rank = match finding.class {
        ExposureClass::ReachableUnrevealed => 2,
        ExposureClass::WeaklyExposed => 3,
        ExposureClass::NoStaticPath => 4,
        ExposureClass::InfectionUnknown
        | ExposureClass::PropagationUnknown
        | ExposureClass::StaticUnknown => 5,
        ExposureClass::Exposed => 9,
    };
    let preview_rank = u8::from(is_preview_limited(finding));
    let repair_rank = if finding.class != ExposureClass::Exposed && has_ranked_repair_route(finding)
    {
        0
    } else {
        class_rank
    };
    (
        preview_rank,
        repair_rank,
        u8::from(finding.canonical_gap.is_none()),
        u8::from(finding.oracle_related_tests().next().is_none()),
        u8::from(finding.missing.is_empty()),
        -(finding.confidence * 100.0) as i32,
        finding.probe.location.file.as_path(),
        finding.probe.location.line,
    )
}

/// A stable finding ranks by its repair route; a preview finding only by the
/// repair authority for its language (#4216 rc rehearsal): a Python finding
/// with a repair card outranks one without, so Start here does not pick a
/// card-less finding that no ripr command routes over one pilot and first-pr
/// do route. Other preview languages keep their class rank. Classification is
/// unchanged; only the selection order moves.
fn has_ranked_repair_route(finding: &Finding) -> bool {
    if !is_preview_limited(finding) {
        return has_repair_route(finding);
    }
    finding.language == Some(LanguageId::Python) && python_repair_card(finding).is_some()
}

fn has_repair_route(finding: &Finding) -> bool {
    finding.recommended_next_step.is_some()
        || finding
            .evidence
            .iter()
            .any(|line| line.starts_with("suggested_verify_command: "))
}

fn is_static_limited(finding: &Finding) -> bool {
    // The producer's named limitation remains authoritative even when its
    // conservative class is reachable_unrevealed or weakly_exposed.
    finding.static_limit_kind.is_some()
        || matches!(
            finding.class,
            ExposureClass::NoStaticPath
                | ExposureClass::InfectionUnknown
                | ExposureClass::PropagationUnknown
                | ExposureClass::StaticUnknown
        )
}

fn is_preview_limited(finding: &Finding) -> bool {
    finding
        .language_status
        .is_some_and(|status| status.as_str() == "preview")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, NextActionClass, Probe, ProbeId, RevealEvidence,
        RiprEvidence, SourceCurrentness, SourceLocation, StageEvidence, StageState, Summary,
    };
    use std::path::PathBuf;

    fn test_finding(id: &str) -> Finding {
        Finding {
            id: id.to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId(id.to_string()),
                location: SourceLocation::new("src/lib.rs", 1, 1),
                owner: None,
                family: ProbeFamily::StaticUnknown,
                delta: DeltaKind::Unknown,
                before: None,
                after: None,
                expression: "unknown syntax".to_string(),
                expected_sinks: vec![],
                required_oracles: vec![],
            },
            class: ExposureClass::StaticUnknown,
            ripr: RiprEvidence {
                reach: StageEvidence::new(StageState::Unknown, Confidence::Low, "reach"),
                infect: StageEvidence::new(StageState::Unknown, Confidence::Low, "infect"),
                propagate: StageEvidence::new(StageState::Unknown, Confidence::Low, "propagate"),
                reveal: RevealEvidence {
                    observe: StageEvidence::new(StageState::Unknown, Confidence::Low, "observe"),
                    discriminate: StageEvidence::new(
                        StageState::Unknown,
                        Confidence::Low,
                        "discriminate",
                    ),
                },
            },
            confidence: 0.2,
            evidence: vec![],
            missing: vec![],
            flow_sinks: vec![],
            activation: ActivationEvidence::default(),
            stop_reasons: vec![],
            related_tests_matched_total: None,
            related_tests: vec![],
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

    fn test_output(findings: Vec<Finding>) -> CheckOutput {
        CheckOutput {
            harness_projections: Vec::new(),
            schema_version: "0.1".to_string(),
            tool: "ripr".to_string(),
            mode: Mode::Draft,
            root: PathBuf::from("repo"),
            base: Some("HEAD~1".to_string()),
            summary: Summary::default(),
            findings,
            preview_language_advisories: Vec::new(),
            language_runs: Vec::new(),
            no_scope_provided: false,
            unanalyzed_working_tree: false,
            untracked_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            analysis_outcome: None,
            partial_scope: None,
        }
    }

    #[test]
    fn check_cases_mirror_the_triage_states() {
        let top = test_finding("finding:top");
        let omitted = test_finding("finding:omitted");
        let output = test_output(vec![top.clone(), omitted.clone()]);
        let rows = vec![
            (
                HumanTriage {
                    state: HumanTriageState::TopGap,
                    selected: Some(&top),
                    omitted: vec![&omitted],
                },
                NextActionCheckCase::TopGap,
            ),
            (
                HumanTriage {
                    state: HumanTriageState::NoActionableGap,
                    selected: None,
                    omitted: vec![&omitted],
                },
                NextActionCheckCase::CandidateFilterHidAll,
            ),
            (
                HumanTriage {
                    state: HumanTriageState::NoActionableGap,
                    selected: None,
                    omitted: Vec::new(),
                },
                NextActionCheckCase::SuppressedByPolicy,
            ),
            (
                HumanTriage {
                    state: HumanTriageState::NoActionableGap,
                    selected: Some(&top),
                    omitted: Vec::new(),
                },
                NextActionCheckCase::NoDiffFinding,
            ),
            (
                HumanTriage {
                    state: HumanTriageState::StaticLimited,
                    selected: Some(&top),
                    omitted: Vec::new(),
                },
                NextActionCheckCase::StaticLimited,
            ),
            (
                HumanTriage {
                    state: HumanTriageState::PreviewLimited,
                    selected: Some(&top),
                    omitted: Vec::new(),
                },
                NextActionCheckCase::PreviewAdvisory,
            ),
            (
                HumanTriage {
                    state: HumanTriageState::MissingScope,
                    selected: None,
                    omitted: Vec::new(),
                },
                NextActionCheckCase::ScopeMissing,
            ),
        ];
        for (triage, case) in &rows {
            assert_eq!(check_case_for_triage(triage, &output), *case);
        }
        // Empty findings with no selection is the no-diff case, not a
        // suppression claim.
        let empty = test_output(Vec::new());
        let triage = HumanTriage {
            state: HumanTriageState::NoActionableGap,
            selected: None,
            omitted: Vec::new(),
        };
        assert_eq!(
            check_case_for_triage(&triage, &empty),
            NextActionCheckCase::NoDiffFinding
        );
    }

    #[test]
    fn check_adapter_binds_the_ranked_winner_with_bounded_alternatives() -> Result<(), String> {
        let top = test_finding("finding:top");
        let second = test_finding("finding:second");
        let third = test_finding("finding:third");
        let fourth = test_finding("finding:fourth");
        let output = test_output(vec![top.clone()]);
        let triage = HumanTriage {
            state: HumanTriageState::TopGap,
            selected: Some(&top),
            omitted: vec![&second, &third, &fourth],
        };
        let navigation = crate::app::FindingNavigation::legacy();
        let drill_in = FindingDrillIn::Commands(navigation);
        let action = canonical_next_action_for_triage(
            &triage,
            &output,
            Some(&drill_in),
            CheckDiffProvenance::CommittedHistory,
        )?;
        if action.action_class() != NextActionClass::InspectDetails {
            return Err("top gap must inspect its details".to_string());
        }
        if action.subject().item.as_deref() != Some("finding:top") {
            return Err("adapter bound the wrong finding".to_string());
        }
        // Rank losers are bounded subordinate alternatives, never silent.
        if action.alternatives().len() != 2 {
            return Err(format!(
                "expected two bounded alternatives, got {}",
                action.alternatives().len()
            ));
        }
        if !action.alternatives()[0].route.contains("finding:second") {
            return Err("omitted alternative lost its identity".to_string());
        }
        match action.stop() {
            Some(NextActionStop::CheckTriage { case }) if *case == NextActionCheckCase::TopGap => {}
            other => return Err(format!("top gap stop is wrong: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn check_adapter_names_scope_without_navigation() -> Result<(), String> {
        let output = test_output(Vec::new());
        let triage = HumanTriage {
            state: HumanTriageState::MissingScope,
            selected: None,
            omitted: Vec::new(),
        };
        let action = canonical_next_action_for_triage(
            &triage,
            &output,
            None,
            CheckDiffProvenance::CommittedHistory,
        )?;
        if action.action_class() != NextActionClass::SatisfyPrerequisite {
            return Err("missing scope must satisfy its prerequisite".to_string());
        }
        if action.subject().item.as_deref() != Some("base:HEAD~1") {
            return Err(format!(
                "scope descriptor is wrong: {:?}",
                action.subject().item
            ));
        }
        // No navigation: the restart route replays the bound root.
        match action.stop() {
            Some(NextActionStop::CheckTriage { case })
                if *case == NextActionCheckCase::ScopeMissing => {}
            other => return Err(format!("scope stop is wrong: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn check_adapter_binds_limitations_and_scope_modes() -> Result<(), String> {
        let mut output = test_output(Vec::new());
        output.base = None;
        output.unanalyzed_working_tree = true;
        output.no_scope_provided = true;
        let triage = HumanTriage {
            state: HumanTriageState::MissingScope,
            selected: None,
            omitted: Vec::new(),
        };
        let action = canonical_next_action_for_triage(
            &triage,
            &output,
            None,
            CheckDiffProvenance::Worktree,
        )?;
        if action.subject().item.as_deref() != Some("working_tree") {
            return Err("working-tree scope mislabeled".to_string());
        }
        if action.limitations().is_empty() {
            return Err("working-tree edits limitation lost".to_string());
        }
        match &action.subject().diff_source {
            NextActionDiffSource::WorkingTree { .. } => {}
            other => return Err(format!("diff mode flipped: {other:?}")),
        }

        let mut scoped = test_output(Vec::new());
        scoped.base = None;
        scoped.no_scope_provided = true;
        let action = canonical_next_action_for_triage(
            &triage,
            &scoped,
            None,
            CheckDiffProvenance::CommittedHistory,
        )?;
        if action.subject().item.as_deref() != Some("no_scope") {
            return Err("missing scope mislabeled".to_string());
        }
        Ok(())
    }

    #[test]
    fn check_adapter_binds_diff_source_from_provenance_not_base() -> Result<(), String> {
        // Base presence identifies neither mode: a `--worktree` run
        // resolves a base yet analyzes the live tree, while a supplied
        // scope has no base yet is fixed replayable content.
        let top = test_finding("finding:top");
        let triage = HumanTriage {
            state: HumanTriageState::TopGap,
            selected: Some(&top),
            omitted: Vec::new(),
        };
        // Worktree with a resolved base stays the live tree.
        let output = test_output(vec![top.clone()]);
        let action = canonical_next_action_for_triage(
            &triage,
            &output,
            None,
            CheckDiffProvenance::Worktree,
        )?;
        match &action.subject().diff_source {
            NextActionDiffSource::WorkingTree { .. } => {}
            other => return Err(format!("worktree run mislabeled: {other:?}")),
        }
        // A supplied scope without a base is fixed content, not the tree.
        let mut supplied = test_output(vec![top.clone()]);
        supplied.base = None;
        let action = canonical_next_action_for_triage(
            &triage,
            &supplied,
            None,
            CheckDiffProvenance::SuppliedScope,
        )?;
        match &action.subject().diff_source {
            NextActionDiffSource::Committed {
                base: None,
                head: None,
            } => {}
            other => return Err(format!("supplied scope mislabeled: {other:?}")),
        }
        // Committed history carries the compared base.
        let action = canonical_next_action_for_triage(
            &triage,
            &output,
            None,
            CheckDiffProvenance::CommittedHistory,
        )?;
        match &action.subject().diff_source {
            NextActionDiffSource::Committed {
                base: Some(base),
                head: None,
            } if base == "HEAD~1" => {}
            other => return Err(format!("committed run mislabeled: {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn check_adapter_never_fails_a_triage_state() -> Result<(), String> {
        // The renderer's defensive fallback exists only for producer bugs:
        // every triage state projects, so the canonical path decides.
        let top = test_finding("finding:top");
        let output = test_output(vec![top.clone()]);
        for state in [
            HumanTriageState::TopGap,
            HumanTriageState::NoActionableGap,
            HumanTriageState::StaticLimited,
            HumanTriageState::PreviewLimited,
            HumanTriageState::MissingScope,
        ] {
            let triage = HumanTriage {
                state,
                selected: Some(&top),
                omitted: Vec::new(),
            };
            let action = canonical_next_action_for_triage(
                &triage,
                &output,
                None,
                CheckDiffProvenance::CommittedHistory,
            )
            .map_err(|error| format!("{} must project: {error}", state.as_str()))?;
            let fallback = check_case_for_triage(&triage, &output);
            match action.stop() {
                Some(NextActionStop::CheckTriage { case }) if *case == fallback => {}
                other => {
                    return Err(format!(
                        "{} projection disagrees with its case: {other:?}",
                        state.as_str()
                    ));
                }
            }
        }
        Ok(())
    }
}
