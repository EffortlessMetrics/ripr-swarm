use crate::agent::loop_commands::shell_arg;
use crate::analysis::{SourceRoleContext, classify_with, seeds_diff_probes};
use crate::app::test_stub::StubRouteDecision;
use crate::app::{CheckDiffProvenance, CheckOutput, FindingDrillIn};
use crate::config::RiprConfig;
use crate::domain::{
    CanonicalNextActionV1, ExposureClass, Finding, LanguageId, NextActionAlternative,
    NextActionCheckCase, NextActionCurrentness, NextActionDiffSource, NextActionInput,
    NextActionProducer, NextActionStop, StaticLimitKind, current_command_platform,
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
    config: &RiprConfig,
) -> HumanTriage<'a> {
    let role_context = triage_role_context(config);
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
    // #9346 (same-owner): decisive production verdicts. An unknown
    // production probe is shadowed only by a decisive verdict that covers
    // the same changed behavior, or when the unknown carries no distinct
    // changed behavior of its own (see `decisive_shadows_unknown`);
    // a lone production unknown, or one describing a distinct edit on a
    // different owner or changed line, is not.
    let decisive_findings: Vec<&Finding> = output
        .findings
        .iter()
        .filter(|finding| !suppressed_ids.contains(finding.id.as_str()))
        .filter(|finding| finding.is_candidate_actionable())
        .filter(|finding| {
            matches!(
                finding.class,
                ExposureClass::Exposed | ExposureClass::WeaklyExposed
            )
        })
        .filter(|finding| file_role_rank(finding, &output.root, &role_context) == 0)
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
        if selected.is_none_or(|current| {
            triage_rank(finding, &output.root, &role_context, &decisive_findings)
                < triage_rank(current, &output.root, &role_context, &decisive_findings)
        }) {
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
    // #8165: the default (bounded) human render speaks plain language only;
    // the `HumanTriageState` schema token stays in machine surfaces.
    out.push_str(&format!("  State: {}\n", triage.state.plain_label()));
    // The canonical decision (#6304) selects the line family; the prose
    // below is unchanged, so the decision has one authority while rendered
    // bytes stay put.
    match check_case_or_fallback(triage, output, drill_in, provenance) {
        NextActionCheckCase::TopGap => out.push_str(
            "  Safe next action: inspect or repair the selected non-exposed gap; this is static advisory evidence only\n",
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
                "  Safe next action: all findings are suppressed by policy; review the suppression block before treating this run as actionable\n",
            );
        }
        NextActionCheckCase::NoDiffFinding => {
            out.push_str(
                "  Safe next action: no non-exposed diff finding was selected. This is not runtime proof, coverage adequacy, or mutation confirmation\n",
            );
        }
        NextActionCheckCase::StaticLimited => {
            let plain_no_path = triage.selected.is_some_and(|finding| {
                finding.class == ExposureClass::NoStaticPath && finding.static_limit_kind.is_none()
            });
            out.push_str(if plain_no_path {
                "  Safe next action: review the unresolved static path and existing tests before treating this as repair-ready\n"
            } else {
                "  Safe next action: inspect the named static limitation before treating this as repair-ready\n"
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
                        "  Safe next action: preview-language evidence is advisory; the repair packet is blocked by the named static limitation, not by missing fields; resolve the limitation and rerun preview evidence before acting\n",
                    );
                }
                not_ready => match triage.selected {
                    Some(finding) if finding.language == Some(LanguageId::Python) => {
                        out.push_str(&python_preview_safe_action(finding));
                    }
                    Some(finding) if finding.class == ExposureClass::Exposed => {
                        out.push_str(EXPOSED_PREVIEW_SAFE_ACTION);
                    }
                    // #7384: TypeScript named limitations deliberately leave
                    // `static_limit_kind` unset, so the structured branch above
                    // never fires and this line fell through to generic
                    // packet-contract jargon while the real reason sat in an
                    // evidence line no human surface rendered. Name the
                    // limitation and its real producer rationale, the way
                    // `python_preview_safe_action` names a Python one.
                    Some(finding)
                        if matches!(
                            finding.language,
                            Some(LanguageId::TypeScript) | Some(LanguageId::JavaScript)
                        ) && typescript_named_limitation_why(finding).is_some() =>
                    {
                        if let Some(why) = typescript_named_limitation_why(finding) {
                            // #8630: named limitations refuse projection before
                            // packet validation. Internal missing fields are
                            // diagnostics, not inputs the operator can fill.
                            out.push_str(&format!(
                                "  Safe next action: preview-language evidence is advisory; no repair packet is emitted because of the named static limitation: {why}; `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; check by hand whether a test observes this change, then rerun `ripr check`\n"
                            ));
                        }
                    }
                    Some(finding) if let Some(actionability) = not_ready.as_ref() => {
                        out.push_str(&packet_closed_preview_safe_action(
                            finding,
                            &actionability.why_not_actionable,
                        ));
                    }
                    _ => out.push_str(
                        "  Safe next action: preview-language evidence is advisory; complete the missing repair-packet fields before acting\n",
                    ),
                },
            }
        }
        // #4012: on an established-but-empty range the scope was provided
        // (a default base was resolved and compared) — the honest action is
        // to change something, not to provide a scope.
        NextActionCheckCase::ScopeMissing => {
            // RIPR-SPEC-0116: a working-tree read already includes staged
            // and unstaged tracked edits, so `--worktree` is no remedy there;
            // only untracked files (intent-to-add) or a new edit can add scope.
            if let Some(base) = output.base.as_deref()
                && crate::output::analyzed_revisions::is_working_tree_read(output)
            {
                out.push_str(&format!(
                    "  Safe next action: the working tree has no changed tracked files against `{base}`; make a change, or `git add -N <path>` a new file, and re-run\n"
                ));
            } else if let Some(base) = output.base.as_deref()
                && head_is_on_branch(&output.root, base)
            {
                out.push_str(&format!(
                    "  Safe next action: you are on `{base}`, which is the comparison base; edit a tracked source file or switch to a feature branch\n"
                ));
            } else if let Some(base) = output.base.as_deref()
                && let Some(candidate) = empty_diff_base_candidate(output, base)
            {
                // The analyzed base resolved to HEAD itself, so committing
                // or `--worktree` against it stays empty; the one canonical
                // remediation is the probed other base, named identically
                // on stderr by the check hedge (#7644).
                out.push_str(&format!(
                    "  Safe next action: no changed files were compared against `{base}`; pass `--base {candidate}` to compare against `{candidate}` instead\n"
                ));
            } else if let Some(base) = output.base.as_deref() {
                // "tracked" (#5258): `--worktree` diffs tracked edits only,
                // so the line must not promise it covers untracked files.
                out.push_str(&format!(
                    "  Safe next action: no changed files were compared against `{base}`; commit a change and re-run, or add `--worktree` to include uncommitted tracked edits\n"
                ));
            } else {
                // No base was established: name the same canonical pair
                // every format names (domain::next_action) — provide a diff
                // scope, or explore the whole repository with pilot.
                out.push_str(&format!(
                    "  Safe next action: no analysis scope was provided; run `{}` with BASE set to an existing ref to analyze your changes, or `{}` for whole-repository exploration; this empty output is not an all-clear\n",
                    crate::domain::NO_SCOPE_BASE_COMMAND,
                    crate::domain::WHOLE_REPO_PILOT_COMMAND,
                ));
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
            let stub_eligible = finding.class != ExposureClass::Exposed
                && finding.static_limit_kind
                    != Some(StaticLimitKind::RustAssertionContextUnresolved)
                && !super::sections::untraced_reach_weak_finding(finding)
                && finding.probe.offers_stub_route()
                && finding
                    .probe
                    .location
                    .file
                    .extension()
                    .and_then(|ext| ext.to_str())
                    == Some("rs");
            // The test route is mentioned beside a held repair start only
            // when a runnable stub is actually rendered below.
            let stub_rendered = stub_eligible
                && matches!(
                    navigation.stub_route_for(&finding.id),
                    Some(StubRouteDecision::Stub { .. })
                );
            // #8166 Slice C: an eligible top finding also gets the repair
            // start, naming the seam identity the check pipeline's
            // producer-owned gate established. No route, no line: an
            // ineligible or unbound finding keeps only explain/context.
            // #8166 residual (F3): a static-limited finding never prints the
            // runnable repair line, even if a route was computed for it; the
            // limitation guidance above is its only next action.
            // #8729: a finding offered no repair start prints one advisory
            // line naming the refusal's state — a bounded route, a budget
            // truncation, an ineligible seam — so the silent gap between
            // "static limitation" and "analysis could not decide" closes.
            // The refusal advisory is not gated on the static-limited state:
            // a static-limited finding whose seam the gate concretely
            // refused must name that refusal, not merge into the limitation.
            // #9103: a TypeScript finding with a validated repair packet gets
            // a gap-bound start in place of the seam-bound line. Computed by
            // the same pipeline step; static-limited findings never get one.
            let gap_route = navigation.gap_repair_route_for(&finding.id);
            if let Some(route) = gap_route {
                if !is_static_limited(finding) {
                    let command = navigation.gap_repair_command(route);
                    out.push_str(&format!("  {command}\n"));
                    super::push_powershell_variant(out, "  ", &command);
                }
            } else {
                match navigation.repair_route_for(&finding.id) {
                    Some(route) => {
                        if !is_static_limited(finding) {
                            let command = navigation.repair_command(&route.seam_id);
                            if let Some(file) = route.uncommitted_target.as_deref() {
                                // `ripr repair` would refuse this start: its
                                // inline test module target begins from the
                                // committed file. State the requirement instead
                                // of a command that exits 2.
                                let alternative = if stub_rendered {
                                    ", or use the test route below"
                                } else {
                                    ""
                                };
                                out.push_str(&format!(
                                "  Repair start (needs a committed `{file}`; it has uncommitted changes, so commit or stash it first{alternative}): {command}\n"
                            ));
                            } else {
                                out.push_str(&format!("  {command}\n"));
                                super::push_powershell_variant(out, "  ", &command);
                            }
                        }
                    }
                    None => {
                        if let Some(refusal) = navigation.repair_route_refusal() {
                            let advisory =
                                refusal.advisory(&navigation.repair_command(&finding.id));
                            out.push_str(&format!("  {advisory}\n"));
                        }
                    }
                }
            }
            // #5355: a Rust gap gets the one-step route to a runnable test.
            // A gap withheld because ripr could not read the related
            // assertions (RIPR-SPEC-0240) claims no missing test, so it gets
            // no test-writing route either, and neither does a weak finding
            // whose reach witness ripr could not trace (#7071).
            if stub_eligible {
                // #5471: only when the check pipeline ran the stub's own
                // resolver for this finding and it produced a stub; a refusal
                // prints its reason, and no decision prints nothing.
                match navigation.stub_route_for(&finding.id) {
                    Some(StubRouteDecision::Stub { file, line, kind }) => {
                        let command = navigation.stub_command(file, *line, kind);
                        out.push_str("Write a test for it:\n");
                        out.push_str(&format!("  {command}\n"));
                        super::push_powershell_variant(out, "  ", &command);
                    }
                    Some(StubRouteDecision::Refused { reason }) => {
                        out.push_str(&format!("No test stub here: {reason}\n"));
                    }
                    None => {}
                }
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
        // #7643: when the omitted set mixes currentness categories, the
        // count line's clauses cannot be connected to individual bullets;
        // append each finding's category so the reader can tell an
        // outranked candidate from base-side evidence without a rerun.
        // Single-category sets keep the legacy bullet: the count line
        // already names the one category that applies.
        let label_currentness = {
            let mut categories = std::collections::BTreeSet::new();
            for finding in &triage.omitted {
                categories.insert(hidden_bullet_currentness(finding));
            }
            categories.len() > 1
        };
        for finding in triage.omitted.iter().take(listed) {
            let currentness = if label_currentness {
                format!(", {}", hidden_bullet_currentness(finding))
            } else {
                String::new()
            };
            out.push_str(&format!(
                "    - {}:{} ({}{})\n",
                crate::analysis::finding_location_text(&output.root, &finding.probe.location.file),
                finding.probe.location.line,
                finding.class.plain_label(),
                currentness
            ));
        }
        let remaining = triage.omitted.len() - listed;
        if remaining > 0 {
            out.push_str(&format!(
                "    - … and {remaining} more omitted finding(s); every identity is in --format json with RIPR_CHECK_FINDINGS_BYTES=0.\n"
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
    // Same slash-stable spelling as the top-level check JSON `root`.
    // Raw `Path::display()` kept argv backslashes, so
    // `document.root != canonical_next_action.subject.root` on Windows
    // (#7730). Restart/command routes keep native spelling separately.
    let root = display_path(&output.root);
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
        // No inspect target: omit a followable route rather than publishing
        // a scope descriptor such as `no_scope` as if it were a command.
        (None, _) => String::new(),
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
        // provenance of the effective analysis source (#7257), never
        // from base presence and never re-inferred here: a `--worktree`
        // run or dirty-default worktree run resolves a base yet analyzes
        // the live tree, while a supplied scope has no base yet is fixed
        // replayable content, not the live tree.
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

/// True when `HEAD` is the symbolic ref of local branch `base` (a detached
/// HEAD or any other branch is not), so the empty comparison is the base
/// compared with itself.
fn head_is_on_branch(root: &std::path::Path, base: &str) -> bool {
    crate::git::run_git_output_with_deadline(
        root,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        Some(std::time::Duration::from_secs(5)),
    )
    .ok()
    .filter(|output| output.status.success())
    .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == base)
}

/// The probed `--base` candidate for an empty result whose analyzed base
/// resolved to HEAD's own commit: the first other branch whose range
/// against HEAD is non-empty, never the current `base`. `None` when the
/// analyzed revisions do not establish base == HEAD, so the ordinary
/// commit/`--worktree` action keeps owning those states.
fn empty_diff_base_candidate(output: &CheckOutput, base: &str) -> Option<String> {
    let revisions = output.analyzed_revisions.as_ref()?;
    match (
        revisions.base_commit.as_deref(),
        revisions.head_commit.as_deref(),
    ) {
        (Some(base_commit), Some(head_commit)) if base_commit == head_commit => {}
        _ => return None,
    }
    crate::analysis::another_base_candidate(&output.root, None)
        .filter(|candidate| !candidate.is_empty() && candidate != base)
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
            Some(NextActionStop::CheckTriage { case, .. }) => Some(*case),
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

/// #7643: the per-bullet currentness label, mirroring the categories the
/// count line names: base-deleted or moved evidence is base-side, an
/// unresolved subject is not established as either category, and the
/// remainder is a lower-priority candidate.
fn hidden_bullet_currentness(finding: &Finding) -> &'static str {
    match finding.source_currentness {
        crate::domain::SourceCurrentness::BaseDeleted
        | crate::domain::SourceCurrentness::MovedOrRenamed => "base-side",
        crate::domain::SourceCurrentness::UnresolvedSubject => "unresolved currentness",
        _ => "lower-priority candidate",
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
            "  {} finding(s) omitted: {} lower-priority, {} base-side evidence (not candidate edit targets){}.\n",
            omitted.len(),
            lower_priority,
            base_side,
            suffix
        ),
        (0, unresolved, lower_priority) => format!(
            "  {} finding(s) omitted: {} lower-priority, {} unresolved currentness (not candidate edit targets){}.\n",
            omitted.len(),
            lower_priority,
            unresolved,
            suffix
        ),
        (base_side, unresolved, lower_priority) => format!(
            "  {} finding(s) omitted: {} lower-priority, {} base-side evidence and {} unresolved currentness (the last two are not candidate edit targets){}.\n",
            omitted.len(),
            lower_priority,
            base_side,
            unresolved,
            suffix
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
            "  Safe next action: all findings are base-side evidence, not candidate edit targets; rerun with --format human-full to inspect the full evidence before treating this run as actionable\n".to_string()
        }
        NoSelectionMix::AllUnresolved => {
            "  Safe next action: no finding is resolved to the candidate (subject currentness unresolved); rerun with --format human-full to inspect the full evidence before treating this run as actionable\n".to_string()
        }
        NoSelectionMix::Mixed { base_side, unresolved } => format!(
            "  Safe next action: no finding is a candidate edit target ({} base-side, {} unresolved currentness); rerun with --format human-full to inspect the full evidence before treating this run as actionable\n",
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
    const GENERIC: &str = "  Safe next action: preview-language evidence is advisory; the repair packet is complete but remains advisory, so verify independently before acting\n";
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
        "  Safe next action: preview-language evidence is advisory; the repair packet is complete: in `{target_file}`, {action}; run `{verify}`, then rerun `ripr check`\n"
    )
}

/// An exposed preview finding has nothing to repair, in any preview language.
const EXPOSED_PREVIEW_SAFE_ACTION: &str = "  Safe next action: preview-language evidence is advisory; a related test appears to observe this change, so there is no repair to make; verify independently before relying on it\n";

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
        "  Safe next action: this {language} preview finding's repair packet is not ready ({reason}); `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; {manual_step}, then rerun `ripr check`\n"
    )
}

/// #7384: TypeScript named limitations are additive evidence lines of the
/// form `typescript_limitation_why: <name> — <reason>`; they deliberately
/// leave `static_limit_kind` unset (the #1496 slice scoped them to additive
/// evidence). Read the real producer line back so the human safe-action
/// surface can name the actual blocker instead of generic packet-contract
/// jargon.
fn typescript_named_limitation_why(finding: &Finding) -> Option<String> {
    finding
        .evidence
        .iter()
        .find_map(|line| line.strip_prefix("typescript_limitation_why: "))
        .map(str::to_string)
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
        return "  Safe next action: preview-language evidence is advisory; apply the next step below to the suggested test and run its verify command before relying on it\n".to_string();
    }
    if let Some(kind) = finding.static_limit_kind.as_ref() {
        return format!(
            "  Safe next action: static limitation `{}` keeps this Python preview finding from a repair card, so `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; check by hand whether a test observes this change, then rerun `ripr check`\n",
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
        "  Safe next action: this Python preview finding has no repair card ({reason}), so `ripr pilot`, `ripr agent repair` and `ripr first-pr` will not route it; {manual_step}, then rerun `ripr check`\n"
    )
}

/// The source-role context triage can honestly build at this surface:
/// layout plus the repository's configured `[analysis]
/// production_like_targets` opt-in (#9346 review). Triage holds no Cargo
/// target metadata, so declared targets, build scripts and declared
/// crate roots stay unrecorded here; the opt-in is the one configured
/// authority the producer and this surface share, and it must win over
/// the `tests/` layout exactly as `classify_with` orders it.
fn triage_role_context(config: &RiprConfig) -> SourceRoleContext {
    let mut context = SourceRoleContext::empty();
    context.production_like_targets = config.analysis().production_like_targets().clone();
    context
}

/// #9346: the changed file's role from the producer-owned source-role
/// model (`analysis::workspace::source_role`). Three tiers, in the order
/// the role model dictates:
///
/// 0. files that seed production findings — production subjects and
///    opted-in production-like targets (`production_like_targets`);
/// 1. diff-only seeders (`seeds_diff_probes`): reviewed behavior Cargo
///    compiles when changed but repo mode keeps out of the production
///    seam inventory — repository automation under root `xtask/src/` is
///    the shape derivable from layout and the configured opt-in alone;
/// 2. evidence-role files (integration tests, benches, examples,
///    fixtures), so an unknown probe seeded by a test-file edit cannot
///    lead Start here over the production change it accompanies.
fn file_role_rank(finding: &Finding, root: &std::path::Path, context: &SourceRoleContext) -> u8 {
    let file = &finding.probe.location.file;
    let relative = file.strip_prefix(root).unwrap_or(file);
    if classify_with(relative, context).seeds_production_findings() {
        0
    } else if seeds_diff_probes(relative, context) {
        1
    } else {
        2
    }
}

/// #9346 (same-owner): the changed owner/function a probe covers. The
/// producer-owned `probe.owner` (`<file>::<owner>`) is authoritative; the
/// canonical gap's `file`/`owner` pair is the same identity in the same
/// `<file>::<owner>` shape for findings that carry only the gap.
fn triage_owner_key(finding: &Finding) -> Option<String> {
    if let Some(owner) = &finding.probe.owner {
        return Some(owner.0.clone());
    }
    finding
        .canonical_gap
        .as_ref()
        .map(|gap| format!("{}::{}", gap.file, gap.owner))
}

/// #9346 (same-behavior): the changed behavior a probe names, in the
/// identity dimensions the codebase already mints: the canonical gap's
/// subject (`file`/`owner`/`normalized_discriminator`) when the finding
/// carries a gap, otherwise the probe's own changed expression at its
/// changed line. Two probes on one owner with different subjects name
/// different edits (a signature type bound versus a predicate boundary),
/// however close they sit in the function.
fn changed_behavior_subject(finding: &Finding) -> String {
    if let Some(gap) = &finding.canonical_gap {
        return format!(
            "{}::{}::{}",
            gap.file, gap.owner, gap.normalized_discriminator
        );
    }
    format!(
        "{}:{}:{}",
        finding.probe.location.file.display(),
        finding.probe.location.line,
        finding.probe.expression.trim()
    )
}

/// Whether two probes cover the same changed behavior: same changed
/// file and line (the changed-line span the diff producer seeds from)
/// and the same behavior subject above.
fn covers_same_changed_behavior(decisive: &Finding, unknown: &Finding) -> bool {
    decisive.probe.location.file == unknown.probe.location.file
        && decisive.probe.location.line == unknown.probe.location.line
        && changed_behavior_subject(decisive) == changed_behavior_subject(unknown)
}

/// Comment-and-whitespace-insensitive text of one side of a probe delta,
/// so a comment-only re-add of a line reads as the same text it replaced.
fn behavior_neutral_text(text: &str) -> String {
    let code = text.split("//").next().unwrap_or_default();
    code.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether an unknown probe carries no distinct changed behavior of its
/// own: its before/after delta is absent (producers that record no
/// delta) or behavior-neutral (the whole-function `static_unknown` the
/// #9346 witness seeds by re-adding the signature line unchanged or
/// comment-only, while the function's real edit is the decisive line
/// below it). A substantive before/after — a changed type bound, a
/// changed predicate on another line — is a distinct edit and stays.
fn has_no_distinct_change(unknown: &Finding) -> bool {
    match (&unknown.probe.before, &unknown.probe.after) {
        (None, None) => true,
        (Some(before), Some(after)) => {
            behavior_neutral_text(before) == behavior_neutral_text(after)
        }
        _ => false,
    }
}

/// Whether a decisive production verdict shadows an unknown probe:
/// same changed owner, and either the same changed behavior or an
/// unknown with no distinct change of its own to lose. Owner identity
/// alone never shadows — two independent edits in one function keep
/// the unknown's limitation headline.
fn decisive_shadows_unknown(decisive: &Finding, unknown: &Finding) -> bool {
    let (Some(decisive_owner), Some(unknown_owner)) =
        (triage_owner_key(decisive), triage_owner_key(unknown))
    else {
        return false;
    };
    decisive_owner == unknown_owner
        && (covers_same_changed_behavior(decisive, unknown) || has_no_distinct_change(unknown))
}

fn triage_rank<'a>(
    finding: &'a Finding,
    root: &std::path::Path,
    context: &SourceRoleContext,
    decisive_findings: &[&Finding],
) -> (u8, u8, u8, u8, u8, u8, u8, i32, &'a std::path::Path, usize) {
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
    let role_rank = file_role_rank(finding, root, context);
    // #9346 (same-behavior): an unknown production probe shadowed by a
    // decisive production verdict covering the same changed behavior
    // ranks behind that verdict. Verdicts themselves are unchanged; a
    // lone production unknown, or one naming a distinct edit, keeps
    // its lead below.
    let shadowed_rank = u8::from(
        role_rank == 0
            && matches!(
                finding.class,
                ExposureClass::StaticUnknown
                    | ExposureClass::InfectionUnknown
                    | ExposureClass::PropagationUnknown
            )
            && decisive_findings
                .iter()
                .any(|decisive| decisive_shadows_unknown(decisive, finding)),
    );
    (
        // #9346: changed-file role leads the ranking. The pair (file
        // role, verdict decisiveness) defines the demotion this fixes —
        // an unknown verdict on an evidence-role file — while verdict
        // order within one role keeps its existing keys below, so a
        // production unknown still leads a production exposed finding
        // and `no_actionable_gap` keeps meaning only-exposed-selected.
        role_rank,
        shadowed_rank,
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
        ActivationEvidence, Confidence, DeltaKind, FindingCanonicalGap, NextActionClass, Probe,
        ProbeFamily, ProbeId, RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation,
        StageEvidence, StageState, Summary, SymbolId,
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
            ignored_working_tree_source_paths: Vec::new(),
            unlinked_python_tests: None,
            suppression: None,
            analysis_outcome: None,
            partial_scope: None,
            analyzed_revisions: None,
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
            Some(NextActionStop::CheckTriage { case, detail_route })
                if *case == NextActionCheckCase::TopGap =>
            {
                if !detail_route.contains("finding:top") {
                    return Err(format!(
                        "top gap must carry the selected inspect route, got {detail_route}"
                    ));
                }
            }
            other => return Err(format!("top gap stop is wrong: {other:?}")),
        }
        Ok(())
    }

    /// #7384: a TypeScript finding carrying a named limitation evidence line
    /// must name that limitation in "Safe next action" (the surfacing Python
    /// preview findings get for their static limitations), not fall through
    /// to generic packet-contract jargon.
    #[test]
    fn typescript_named_limitation_is_named_in_safe_next_action() -> Result<(), String> {
        let mut finding = test_finding("finding:ts-helper-gated");
        finding.language = Some(LanguageId::TypeScript);
        finding.class = ExposureClass::WeaklyExposed;
        finding.evidence.push(
            "typescript_limitation_why: typescript_oracle_helper_gated — the test calls              assertion helper `assertPriceBoundary(...)` around owner `computePrice`; the              adapter cannot inspect the helper body or establish its oracle semantics from              the test call site"
                .to_string(),
        );
        let output = test_output(vec![finding.clone()]);
        let triage = HumanTriage {
            state: HumanTriageState::PreviewLimited,
            selected: Some(&finding),
            omitted: Vec::new(),
        };
        let mut out = String::new();
        render_human_triage(
            &mut out,
            &triage,
            &output,
            &crate::config::RiprConfig::default(),
            None,
            CheckDiffProvenance::CommittedHistory,
        );
        if !out.contains("typescript_oracle_helper_gated") {
            return Err(format!(
                "safe next action must name the limitation, got:
{out}"
            ));
        }
        if out.contains("complete the missing repair-packet fields") {
            return Err(format!(
                "generic packet jargon must not shadow the named limitation, got:
{out}"
            ));
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
            Some(NextActionStop::CheckTriage { case, detail_route })
                if *case == NextActionCheckCase::ScopeMissing =>
            {
                if !detail_route.is_empty() {
                    return Err(format!(
                        "missing scope must not publish a scope label as an inspect route, got {detail_route}"
                    ));
                }
            }
            other => return Err(format!("scope stop is wrong: {other:?}")),
        }
        Ok(())
    }

    /// #7730: `canonical_next_action.subject.root` must use the same
    /// slash-stable renderer as the top-level JSON `root`, not raw
    /// `Path::display()`. A backslash-typed spaced argv spelling must not
    /// survive into the subject; a `.` root stays `.` so the JSON consumer
    /// that matches `subject.root == "."` still holds.
    #[test]
    fn check_adapter_subject_root_uses_display_path_not_raw_display() -> Result<(), String> {
        let triage = HumanTriage {
            state: HumanTriageState::MissingScope,
            selected: None,
            omitted: Vec::new(),
        };

        let mut backslash_root = test_output(Vec::new());
        // Join at runtime so the source does not contain a drive-letter
        // machine path (`check-local-context`), while the constructed
        // argv spelling still matches the #7730 Windows report.
        backslash_root.root = PathBuf::from(["C:", "tmp", "probe win space"].join(r"\"));
        let raw = backslash_root.root.display().to_string();
        // Independent slash-stable contract: forward-slash join of the
        // same components, not `display_path` output.
        let expected = ["C:", "tmp", "probe win space"].join("/");
        if raw == expected {
            return Err(format!(
                "fixture lost its discriminator: raw display {raw:?} already matches {expected:?}"
            ));
        }
        if !raw.contains('\\') {
            return Err(format!(
                "fixture must keep a backslash argv spelling, got {raw:?}"
            ));
        }
        let action = canonical_next_action_for_triage(
            &triage,
            &backslash_root,
            None,
            CheckDiffProvenance::CommittedHistory,
        )?;
        if action.subject().root != expected {
            return Err(format!(
                "subject.root must be {expected:?}, got {:?}",
                action.subject().root
            ));
        }
        if action.subject().root == raw {
            return Err(format!("subject.root kept the raw argv spelling {raw:?}"));
        }
        let encoded = serde_json::to_value(&action).map_err(|error| error.to_string())?;
        if encoded["subject"]["root"] != expected {
            return Err(format!(
                "serialized subject.root must be {expected:?}, got {}",
                encoded["subject"]["root"]
            ));
        }

        // Verbatim Windows prefix: separators unify to the #5996 `//?/`
        // shape instead of leaking `\\?\` into JSON identity.
        let mut verbatim = test_output(Vec::new());
        verbatim.root = PathBuf::from(["\\\\?", "C:", "tmp", "probe"].join(r"\"));
        let verbatim_raw = verbatim.root.display().to_string();
        let verbatim_expected = ["//?", "C:", "tmp", "probe"].join("/");
        if verbatim_raw == verbatim_expected {
            return Err(format!(
                "verbatim fixture lost its discriminator: raw {verbatim_raw:?} already matches {verbatim_expected:?}"
            ));
        }
        let action = canonical_next_action_for_triage(
            &triage,
            &verbatim,
            None,
            CheckDiffProvenance::CommittedHistory,
        )?;
        if action.subject().root != verbatim_expected {
            return Err(format!(
                "verbatim subject.root must be {verbatim_expected:?}, got {:?}",
                action.subject().root
            ));
        }
        if action.subject().root.contains('\\') {
            return Err(format!(
                "verbatim subject.root must not retain backslashes, got {:?}",
                action.subject().root
            ));
        }

        // A `.` root stays `.`, so consumers that treat
        // `subject.root == "."` still match after this change.
        let mut dot_root = test_output(Vec::new());
        dot_root.root = PathBuf::from(".");
        let action = canonical_next_action_for_triage(
            &triage,
            &dot_root,
            None,
            CheckDiffProvenance::CommittedHistory,
        )?;
        if action.subject().root != "." {
            return Err(format!(
                "dot root must stay '.', got {:?}",
                action.subject().root
            ));
        }
        let encoded = serde_json::to_value(&action).map_err(|error| error.to_string())?;
        if encoded["subject"]["root"] != "." {
            return Err(format!(
                "serialized dot root must stay '.', got {}",
                encoded["subject"]["root"]
            ));
        }
        Ok(())
    }

    #[test]
    fn scope_missing_names_the_probed_base_candidate_when_base_is_head() -> Result<(), String> {
        // The stdout Safe next action and the stderr hedge must name the
        // same probed `--base` candidate, not divergent remediations.
        let dir =
            std::env::temp_dir().join(format!("ripr-triage-empty-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| -> Result<(), String> {
            let output = crate::git::run_git_output_with_deadline_and_env(
                &dir,
                args,
                &[("GIT_CONFIG_NOSYSTEM", std::ffi::OsStr::new("1"))],
                None,
            )
            .map_err(|e| e.to_string())?;
            if output.status.success() {
                Ok(())
            } else {
                Err(format!("git {args:?} failed"))
            }
        };
        let setup = (|| -> Result<(), String> {
            git(&["init", "-b", "master"])?;
            git(&["config", "user.email", "ripr@example.invalid"])?;
            git(&["config", "user.name", "ripr test"])?;
            std::fs::write(dir.join("README"), "init").map_err(|e| e.to_string())?;
            git(&["add", "."])?;
            git(&["commit", "-m", "init"])?;
            git(&["branch", "base-candidate"])?;
            git(&["checkout", "-q", "--detach"])?;
            std::fs::write(dir.join("README"), "second").map_err(|e| e.to_string())?;
            git(&["add", "."])?;
            git(&["commit", "-m", "second"])?;
            Ok(())
        })();
        if let Err(error) = setup {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(error);
        }
        let mut output = test_output(Vec::new());
        output.root = dir.clone();
        output.base = Some("master".to_string());
        output.no_scope_provided = true;
        output.analyzed_revisions = Some(crate::analysis::AnalyzedRevisions {
            base_ref: "master".to_string(),
            base_commit: Some("abc".to_string()),
            merge_base_commit: None,
            head_commit: Some("abc".to_string()),
            working_tree: false,
        });
        let triage = HumanTriage {
            state: HumanTriageState::MissingScope,
            selected: None,
            omitted: Vec::new(),
        };
        let mut out = String::new();
        render_human_triage(
            &mut out,
            &triage,
            &output,
            &crate::config::RiprConfig::default(),
            None,
            CheckDiffProvenance::CommittedHistory,
        );
        let _ = std::fs::remove_dir_all(&dir);
        if !out.contains("pass `--base base-candidate`") {
            return Err(format!(
                "safe next action must name the probed candidate, got:\n{out}"
            ));
        }
        if out.contains("commit a change and re-run") {
            return Err(format!(
                "divergent commit remediation must not shadow the candidate, got:\n{out}"
            ));
        }
        Ok(())
    }

    #[test]
    fn head_is_on_branch_matches_only_the_checked_out_branch() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("ripr-triage-on-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| -> Result<(), String> {
            let output = crate::git::run_git_output_with_deadline_and_env(
                &dir,
                args,
                &[("GIT_CONFIG_NOSYSTEM", std::ffi::OsStr::new("1"))],
                None,
            )
            .map_err(|e| e.to_string())?;
            if output.status.success() {
                Ok(())
            } else {
                Err(format!("git {args:?} failed"))
            }
        };
        let result = (|| -> Result<(), String> {
            git(&["init", "-b", "master"])?;
            git(&["config", "user.email", "ripr@example.invalid"])?;
            git(&["config", "user.name", "ripr test"])?;
            std::fs::write(dir.join("README"), "init").map_err(|e| e.to_string())?;
            git(&["add", "."])?;
            git(&["commit", "-m", "init"])?;
            if !head_is_on_branch(&dir, "master") {
                return Err("HEAD on master must match base master".to_string());
            }
            if head_is_on_branch(&dir, "main") {
                return Err("HEAD on master must not match base main".to_string());
            }
            git(&["checkout", "-q", "--detach"])?;
            if head_is_on_branch(&dir, "master") {
                return Err("a detached HEAD is not on the base branch".to_string());
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
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

    /// #7257: the pre-fix producer chose provenance from
    /// `worktree_explicitly_provided`. A dirty-default run has that flag
    /// false and `worktree_run` true; the adapter must not hide that the
    /// flag-only selector yields committed history.
    #[test]
    fn worktree_flag_selector_misattributes_dirty_default_as_committed_history()
    -> Result<(), String> {
        let worktree_explicitly_provided = false;
        let worktree_run = true;
        let supplied_scope = false;
        let flag_only = if worktree_explicitly_provided {
            CheckDiffProvenance::Worktree
        } else if supplied_scope {
            CheckDiffProvenance::SuppliedScope
        } else {
            CheckDiffProvenance::CommittedHistory
        };
        if flag_only != CheckDiffProvenance::CommittedHistory {
            return Err("flag-only selector must still misattribute the dirty default".to_string());
        }
        let effective = CheckDiffProvenance::from_effective_source(worktree_run, supplied_scope);
        if effective != CheckDiffProvenance::Worktree {
            return Err("effective source must be the working tree".to_string());
        }

        let top = test_finding("finding:top");
        let triage = HumanTriage {
            state: HumanTriageState::TopGap,
            selected: Some(&top),
            omitted: Vec::new(),
        };
        let output = test_output(vec![top.clone()]);
        let misattributed = canonical_next_action_for_triage(&triage, &output, None, flag_only)?;
        match &misattributed.subject().diff_source {
            NextActionDiffSource::Committed { .. } => {}
            other => {
                return Err(format!(
                    "flag-only provenance must claim committed history: {other:?}"
                ));
            }
        }
        let bound = canonical_next_action_for_triage(&triage, &output, None, effective)?;
        match &bound.subject().diff_source {
            NextActionDiffSource::WorkingTree { .. } => {}
            other => {
                return Err(format!(
                    "effective provenance must claim the working tree: {other:?}"
                ));
            }
        }
        if bound.subject().item != misattributed.subject().item {
            return Err("item identity must not depend on provenance".to_string());
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
                Some(NextActionStop::CheckTriage { case, .. }) if *case == fallback => {}
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

    #[test]
    fn hidden_bullets_label_currentness_when_categories_mix() {
        // #7643: the count line names the mix; each bullet must connect to
        // its clause so a base-side row is not mistaken for an outranked
        // candidate.
        let top = super::super::tests::sample_finding();
        let mut candidate = super::super::tests::sample_finding();
        candidate.id = "finding:candidate".to_string();
        candidate.probe.id = ProbeId("finding:candidate".to_string());
        candidate.probe.location = SourceLocation::new("src/lib.rs", 21, 1);
        let mut base_side = super::super::tests::sample_finding();
        base_side.id = "finding:base".to_string();
        base_side.probe.id = ProbeId("finding:base".to_string());
        base_side.probe.location = SourceLocation::new("src/lib.rs", 26, 1);
        base_side.source_currentness = SourceCurrentness::BaseDeleted;
        let mut unresolved = super::super::tests::sample_finding();
        unresolved.id = "finding:unresolved".to_string();
        unresolved.probe.id = ProbeId("finding:unresolved".to_string());
        unresolved.probe.location = SourceLocation::new("src/lib.rs", 31, 1);
        unresolved.source_currentness = SourceCurrentness::UnresolvedSubject;
        let output = super::super::tests::bounded_output_with_findings(vec![
            top, candidate, base_side, unresolved,
        ]);
        let rendered = super::super::render(&output);
        assert!(
            rendered.contains("    - src/sample.rs:7 (weak, lower-priority candidate)\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("    - src/lib.rs:26 (weak, base-side)\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("    - src/lib.rs:31 (weak, unresolved currentness)\n"),
            "{rendered}"
        );
    }

    fn finding_at(id: &str, file: &str, class: ExposureClass) -> Finding {
        let mut finding = test_finding(id);
        finding.probe.location = SourceLocation::new(file, 1, 1);
        finding.class = class;
        finding
    }

    /// #9346 (same-owner): a production finding on a named owner/function,
    /// in the producer's `<file>::<owner>` identity shape.
    fn finding_for_owner(
        id: &str,
        file: &str,
        line: usize,
        class: ExposureClass,
        owner: &str,
    ) -> Finding {
        let mut finding = finding_at(id, file, class);
        finding.probe.location = SourceLocation::new(file, line, 1);
        finding.probe.owner = Some(SymbolId(format!("{file}::{owner}")));
        finding
    }

    /// #9346: a decisive production verdict must lead Start here ahead of
    /// an unknown probe seeded by a test-file edit. The unknown is listed
    /// first so insertion order cannot masquerade as ranking.
    #[test]
    fn start_here_prefers_exposed_production_over_unknown_test_file() {
        let test_unknown = finding_at(
            "finding:test-unknown",
            "tests/pricing.rs",
            ExposureClass::StaticUnknown,
        );
        let production = finding_at("finding:production", "src/lib.rs", ExposureClass::Exposed);
        let output = test_output(vec![test_unknown, production]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:production")
        );
        assert_eq!(triage.state, HumanTriageState::NoActionableGap);
        let mut out = String::new();
        render_human_triage(
            &mut out,
            &triage,
            &output,
            &RiprConfig::default(),
            None,
            CheckDiffProvenance::CommittedHistory,
        );
        assert!(!out.contains("limited by static analysis"), "{out}");
        assert!(out.contains("src/lib.rs"), "{out}");
    }

    /// #9346: the same preference covers a weak production gap.
    #[test]
    fn start_here_prefers_weak_production_over_unknown_test_file() {
        let test_unknown = finding_at(
            "finding:test-unknown",
            "tests/pricing.rs",
            ExposureClass::StaticUnknown,
        );
        let production = finding_at(
            "finding:production",
            "src/lib.rs",
            ExposureClass::WeaklyExposed,
        );
        let output = test_output(vec![test_unknown, production]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:production")
        );
        assert_eq!(triage.state, HumanTriageState::TopGap);
    }

    /// #9346: with no decisive production verdict the limitation headline
    /// stays: a production unknown is selected over a test-file unknown,
    /// and a lone test-file unknown still leads.
    #[test]
    fn start_here_keeps_static_limited_when_all_probes_are_unknown() {
        let test_unknown = finding_at(
            "finding:test-unknown",
            "tests/pricing.rs",
            ExposureClass::StaticUnknown,
        );
        let production_unknown = finding_at(
            "finding:production-unknown",
            "src/lib.rs",
            ExposureClass::StaticUnknown,
        );
        let output = test_output(vec![test_unknown, production_unknown]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:production-unknown")
        );
        assert_eq!(triage.state, HumanTriageState::StaticLimited);

        let output = test_output(vec![finding_at(
            "finding:test-unknown",
            "tests/pricing.rs",
            ExposureClass::StaticUnknown,
        )]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(triage.state, HumanTriageState::StaticLimited);
    }

    /// #9346: file role leads the ranking, so a production unknown is
    /// never demoted below a decisive test-file finding.
    #[test]
    fn start_here_does_not_demote_production_unknown_below_test_decisive() {
        let test_decisive = finding_at(
            "finding:test-decisive",
            "tests/pricing.rs",
            ExposureClass::WeaklyExposed,
        );
        let production_unknown = finding_at(
            "finding:production-unknown",
            "src/lib.rs",
            ExposureClass::StaticUnknown,
        );
        let output = test_output(vec![test_decisive, production_unknown]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:production-unknown")
        );
        assert_eq!(triage.state, HumanTriageState::StaticLimited);
    }

    /// #9346 (same-owner): the live witness shape — a whole-function
    /// `static_unknown` at the signature line and a decisive predicate
    /// probe on the next line, both for the same production function.
    /// The decisive probe leads; the unknown is listed first so insertion
    /// order cannot masquerade as ranking.
    #[test]
    fn start_here_prefers_same_owner_exposed_over_production_static_unknown() {
        let unknown = finding_for_owner(
            "finding:fn-unknown",
            "src/lib.rs",
            1,
            ExposureClass::StaticUnknown,
            "discount_cents",
        );
        let decisive = finding_for_owner(
            "finding:line-decisive",
            "src/lib.rs",
            2,
            ExposureClass::Exposed,
            "discount_cents",
        );
        let output = test_output(vec![unknown, decisive]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:line-decisive")
        );
        assert_eq!(triage.state, HumanTriageState::NoActionableGap);
        let mut out = String::new();
        render_human_triage(
            &mut out,
            &triage,
            &output,
            &RiprConfig::default(),
            None,
            CheckDiffProvenance::CommittedHistory,
        );
        assert!(!out.contains("limited by static analysis"), "{out}");
        assert!(out.contains("src/lib.rs"), "{out}");
    }

    /// #9346 (same-owner): every unknown class is shadowed by a weak
    /// decisive sibling on the same owner.
    #[test]
    fn start_here_prefers_same_owner_weak_over_production_unknowns() {
        for unknown_class in [
            ExposureClass::StaticUnknown,
            ExposureClass::InfectionUnknown,
            ExposureClass::PropagationUnknown,
        ] {
            let unknown = finding_for_owner(
                "finding:fn-unknown",
                "src/lib.rs",
                1,
                unknown_class.clone(),
                "discount_cents",
            );
            let decisive = finding_for_owner(
                "finding:line-decisive",
                "src/lib.rs",
                2,
                ExposureClass::WeaklyExposed,
                "discount_cents",
            );
            let output = test_output(vec![unknown, decisive]);
            let triage = select_human_triage(&output, &RiprConfig::default());
            assert_eq!(
                triage.selected.map(|finding| finding.id.as_str()),
                Some("finding:line-decisive"),
                "{unknown_class:?} must be shadowed by its same-owner weak sibling"
            );
            assert_eq!(triage.state, HumanTriageState::TopGap);
        }
    }

    /// #9346 (same-owner): shadowing is scoped to the owner — a decisive
    /// verdict on a different function does not demote this function's
    /// production unknown.
    #[test]
    fn start_here_keeps_production_unknown_when_decisive_is_different_owner() {
        let unknown = finding_for_owner(
            "finding:fn-unknown",
            "src/lib.rs",
            1,
            ExposureClass::StaticUnknown,
            "discount_cents",
        );
        let other = finding_for_owner(
            "finding:other-decisive",
            "src/lib.rs",
            9,
            ExposureClass::Exposed,
            "tax_cents",
        );
        let output = test_output(vec![other, unknown]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:fn-unknown")
        );
        assert_eq!(triage.state, HumanTriageState::StaticLimited);
    }

    /// #9346 (same-owner): a lone production unknown keeps leading with
    /// the limitation state.
    #[test]
    fn start_here_keeps_static_limited_for_lone_production_unknown() {
        let output = test_output(vec![finding_for_owner(
            "finding:fn-unknown",
            "src/lib.rs",
            1,
            ExposureClass::StaticUnknown,
            "discount_cents",
        )]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:fn-unknown")
        );
        assert_eq!(triage.state, HumanTriageState::StaticLimited);
    }

    /// #9346 (same-owner): an unknown that names its owner only through
    /// the canonical gap is still shadowed by a decisive sibling whose
    /// identity comes from `probe.owner`.
    #[test]
    fn start_here_shadows_unknown_named_only_by_canonical_gap() {
        let mut unknown = finding_for_owner(
            "finding:fn-unknown",
            "src/lib.rs",
            1,
            ExposureClass::StaticUnknown,
            "discount_cents",
        );
        unknown.probe.owner = None;
        unknown.canonical_gap = Some(FindingCanonicalGap {
            id: "gap:rust:src/lib.rs:discount_cents:static_unknown".to_string(),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: "discount_cents".to_string(),
            behavior_kind: "static_unknown".to_string(),
            probe_kind: "static_unknown".to_string(),
            normalized_discriminator: "pub_fn_discount_cents".to_string(),
        });
        let decisive = finding_for_owner(
            "finding:line-decisive",
            "src/lib.rs",
            2,
            ExposureClass::Exposed,
            "discount_cents",
        );
        let output = test_output(vec![unknown, decisive]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:line-decisive")
        );
        assert_eq!(triage.state, HumanTriageState::NoActionableGap);
    }

    /// #9346 review (same-behavior): a production finding whose probe
    /// carries its changed behavior (family, changed line, and the
    /// before/after delta the diff producer recorded), so an independent
    /// edit can name its own changed behavior, unlike the whole-function
    /// fallback unknown whose delta is absent or behavior-neutral.
    fn finding_with_change(
        id: &str,
        line: usize,
        class: ExposureClass,
        family: ProbeFamily,
        before: &str,
        after: &str,
        discriminator: &str,
    ) -> Finding {
        let mut finding = finding_for_owner(id, "src/lib.rs", line, class, "price");
        finding.probe.family = family;
        finding.probe.expression = after.to_string();
        finding.probe.before = Some(before.to_string());
        finding.probe.after = Some(after.to_string());
        finding.canonical_gap = Some(FindingCanonicalGap {
            id: format!("gap:rust:src/lib.rs:price:{discriminator}"),
            language: "rust".to_string(),
            file: "src/lib.rs".to_string(),
            owner: "price".to_string(),
            behavior_kind: discriminator.to_string(),
            probe_kind: finding.probe.family.as_str().to_string(),
            normalized_discriminator: discriminator.to_string(),
        });
        finding
    }

    /// #9346 review: two independent edits in one function — a changed
    /// signature type bound (its own unknown probe, on the signature
    /// line, with a substantive before/after) and an observed predicate
    /// change (decisive probe, body line, different discriminator).
    /// Owner identity alone must not shadow the bound's unknown: it
    /// still leads, with its limitation state, because the decisive
    /// verdict never examined the bound change.
    #[test]
    fn start_here_keeps_unknown_for_independent_edit_in_same_function() {
        let bound_unknown = finding_with_change(
            "finding:bound-unknown",
            1,
            ExposureClass::StaticUnknown,
            ProbeFamily::StaticUnknown,
            "pub fn price<T: Copy>(amount: i32) -> i32 {",
            "pub fn price<T: Clone>(amount: i32) -> i32 {",
            "pub_fn_price<t_clone>amount_i32",
        );
        let decisive = finding_with_change(
            "finding:predicate-decisive",
            2,
            ExposureClass::Exposed,
            ProbeFamily::Predicate,
            "amount >= threshold",
            "amount > threshold",
            "amount==threshold",
        );
        let output = test_output(vec![decisive, bound_unknown]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:bound-unknown")
        );
        assert_eq!(triage.state, HumanTriageState::StaticLimited);
        let mut out = String::new();
        render_human_triage(
            &mut out,
            &triage,
            &output,
            &RiprConfig::default(),
            None,
            CheckDiffProvenance::CommittedHistory,
        );
        assert!(out.contains("limited by static analysis"), "{out}");
    }

    /// #9346 review: the witnessed whole-function shape with an explicit
    /// recorded delta — the signature line is re-added unchanged
    /// (before == after), so the unknown names no distinct change and
    /// the decisive predicate verdict on the line below still leads.
    #[test]
    fn start_here_shadows_whole_function_unknown_with_unchanged_signature_delta() {
        let signature = "pub fn price(amount: i32) -> i32 {";
        let whole_fn_unknown = finding_with_change(
            "finding:fn-unknown",
            1,
            ExposureClass::StaticUnknown,
            ProbeFamily::StaticUnknown,
            signature,
            signature,
            "pub_fn_price_amount_i32",
        );
        let decisive = finding_with_change(
            "finding:predicate-decisive",
            2,
            ExposureClass::Exposed,
            ProbeFamily::Predicate,
            "amount >= threshold",
            "amount > threshold",
            "amount==threshold",
        );
        let output = test_output(vec![whole_fn_unknown, decisive]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:predicate-decisive")
        );
        assert_eq!(triage.state, HumanTriageState::NoActionableGap);
    }

    /// #9346 review: a decisive verdict on the very changed line and
    /// behavior subject an unknown probe names shadows that unknown,
    /// even though the unknown carries a substantive delta.
    #[test]
    fn start_here_shadows_unknown_when_decisive_covers_same_changed_line() {
        let unknown = finding_with_change(
            "finding:line-unknown",
            2,
            ExposureClass::InfectionUnknown,
            ProbeFamily::Predicate,
            "amount >= threshold",
            "amount > threshold",
            "amount==threshold",
        );
        let decisive = finding_with_change(
            "finding:line-decisive",
            2,
            ExposureClass::WeaklyExposed,
            ProbeFamily::Predicate,
            "amount >= threshold",
            "amount > threshold",
            "amount==threshold",
        );
        let output = test_output(vec![unknown, decisive]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:line-decisive")
        );
        assert_eq!(triage.state, HumanTriageState::TopGap);
    }

    /// #9346 review: `[analysis] production_like_targets` opts a file
    /// under `tests/` into production-like treatment, so its findings
    /// rank as production here exactly as the producer seeded them —
    /// a lone opted-in unknown leads, where the same file un-opted
    /// would rank as test evidence behind the production verdict.
    #[test]
    fn start_here_ranks_production_like_opted_in_test_file_as_production() {
        let opted_in_unknown = finding_at(
            "finding:opted-in-unknown",
            "tests/harness.rs",
            ExposureClass::StaticUnknown,
        );
        let production = finding_at("finding:production", "src/lib.rs", ExposureClass::Exposed);
        let output = test_output(vec![production, opted_in_unknown]);

        let mut config = RiprConfig::default();
        config
            .analysis
            .production_like_targets
            .insert(PathBuf::from("tests/harness.rs"));
        let triage = select_human_triage(&output, &config);
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:opted-in-unknown")
        );
        assert_eq!(triage.state, HumanTriageState::StaticLimited);

        // Control: without the opt-in the same pair keeps the existing
        // production-over-test-evidence order.
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:production")
        );
    }

    /// #9346 review: repository automation (`xtask/src/**`) seeds diff
    /// probes as reviewed behavior (`seeds_diff_probes`), so it ranks
    /// in its own tier — ahead of test evidence, behind ordinary
    /// production — instead of as ordinary test evidence.
    #[test]
    fn start_here_ranks_repo_automation_between_production_and_test_evidence() {
        let test_decisive = finding_at(
            "finding:test-decisive",
            "tests/pricing.rs",
            ExposureClass::WeaklyExposed,
        );
        let automation_unknown = finding_at(
            "finding:automation-unknown",
            "xtask/src/main.rs",
            ExposureClass::StaticUnknown,
        );
        let output = test_output(vec![test_decisive, automation_unknown]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:automation-unknown")
        );
        assert_eq!(triage.state, HumanTriageState::StaticLimited);

        // The tier is not ordinary production: a production verdict
        // still leads over the automation unknown.
        let production = finding_at("finding:production", "src/lib.rs", ExposureClass::Exposed);
        let automation_unknown = finding_at(
            "finding:automation-unknown",
            "xtask/src/main.rs",
            ExposureClass::StaticUnknown,
        );
        let output = test_output(vec![automation_unknown, production]);
        let triage = select_human_triage(&output, &RiprConfig::default());
        assert_eq!(
            triage.selected.map(|finding| finding.id.as_str()),
            Some("finding:production")
        );
        assert_eq!(triage.state, HumanTriageState::NoActionableGap);
    }

    #[test]
    fn hidden_bullets_stay_unlabeled_when_the_set_is_single_category() {
        // A single-category omitted set keeps the legacy bullet: the count
        // line already names the one category that applies.
        let top = super::super::tests::sample_finding();
        let mut omitted = super::super::tests::sample_finding();
        omitted.id = "finding:omitted".to_string();
        omitted.probe.id = ProbeId("finding:omitted".to_string());
        omitted.probe.location = SourceLocation::new("src/lib.rs", 21, 1);
        let output = super::super::tests::bounded_output_with_findings(vec![top, omitted]);
        let rendered = super::super::render(&output);
        // Exactly one omitted bullet, and no per-bullet currentness label:
        // the count line already named the single category.
        assert_eq!(rendered.matches("\n    - src/").count(), 1, "{rendered}");
        assert!(
            !rendered.contains("(weak, lower-priority candidate)"),
            "{rendered}"
        );
        assert!(!rendered.contains("(weak, base-side)"), "{rendered}");
        assert!(
            !rendered.contains("(weak, unresolved currentness)"),
            "{rendered}"
        );
    }
}
