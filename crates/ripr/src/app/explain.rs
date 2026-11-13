use super::check::check_with_progress;
use super::progress::{AnalysisProgressScope, AnalysisProgressSink};
use super::selector::FindingSelection;
use super::{CheckInput, CheckOutput};
use crate::config::RiprConfig;
use crate::output;
use std::path::Path;

/// How `ripr explain` renders the one selected finding (#12104).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExplainFormat {
    /// The human explanation (default).
    #[default]
    Human,
    /// The compact `check --format agent` line style with a terminal state.
    Agent,
}

/// Output format and optional progress sink for one fresh `explain` run,
/// bundled so the entry point stays within the argument-count bound.
pub(crate) struct ExplainRun<'a> {
    pub(crate) format: ExplainFormat,
    pub(crate) sink: Option<&'a dyn AnalysisProgressSink>,
}

/// Renders the uniquely selected finding in the requested format.
fn render_selected(
    format: ExplainFormat,
    finding: &crate::domain::Finding,
    config: &RiprConfig,
    context_command: &str,
    root: &Path,
    files: Option<&[Option<std::path::PathBuf>]>,
    suppressed: bool,
) -> String {
    match format {
        ExplainFormat::Human => output::human::render_finding_with_context_command_and_files(
            finding,
            config,
            context_command,
            root,
            files,
        ),
        ExplainFormat::Agent => {
            output::human::render_finding_agent(finding, root, context_command, suppressed)
        }
    }
}

/// Computes findings and renders a single selected finding in human format.
///
/// The selector can be a finding identifier (for example
/// `probe:path_to_file.rs:42:family`), a canonical gap identity
/// (`gap:...`), or a `file:line` location. A selector naming several
/// findings refuses as ambiguous; one naming none refuses as a miss
/// (#8166 Slice B).
pub fn explain_finding(root: &Path, selector: &str) -> Result<String, String> {
    explain_finding_with_input(
        CheckInput {
            root: root.to_path_buf(),
            ..CheckInput::default()
        },
        selector,
    )
}

/// Like [`explain_finding`] but allows overriding the full check input.
pub fn explain_finding_with_input(input: CheckInput, selector: &str) -> Result<String, String> {
    explain_finding_with_config(input, selector, &RiprConfig::default())
}

pub fn explain_finding_with_config(
    input: CheckInput,
    selector: &str,
    config: &RiprConfig,
) -> Result<String, String> {
    explain_finding_with_config_and_navigation_mode(input, selector, config, false, false)
}

pub(crate) fn explain_finding_with_config_and_navigation_mode(
    input: CheckInput,
    selector: &str,
    config: &RiprConfig,
    mode_explicit: bool,
    worktree: bool,
) -> Result<String, String> {
    explain_finding_with_config_navigation_mode_and_progress(
        input,
        selector,
        config,
        mode_explicit,
        worktree,
        false,
        ExplainRun {
            format: ExplainFormat::Human,
            sink: None,
        },
    )
}

/// [`explain_finding_with_config_and_navigation_mode`] with a progress sink
/// on the fresh pipeline run (#7352): a selector-only `explain` re-runs the
/// full check pipeline, and on a cold large repository that work was
/// invisible — no stage lines or heartbeats — for the same
/// tens-of-seconds-to-minutes window `ripr check` discloses. The shared sink
/// projects the same producer-owned stage lines `check` promises; `None`
/// reproduces the legacy silent behavior for library callers. The `--from`
/// artifact path runs no pipeline and takes no sink.
pub(crate) fn explain_finding_with_config_navigation_mode_and_progress(
    input: CheckInput,
    selector: &str,
    config: &RiprConfig,
    mode_explicit: bool,
    worktree: bool,
    worktree_from_default: bool,
    run: ExplainRun<'_>,
) -> Result<String, String> {
    let ExplainRun { format, sink } = run;
    let navigation = super::finding_navigation_with_worktree(&input, None, mode_explicit, worktree);
    let committed_list_command = explain_navigation_list_command(
        &super::finding_navigation_with_worktree(&input, None, mode_explicit, false),
    );
    let root = input.root.clone();
    // The worktree retry names the same repository and base the failed run
    // analyzed; a bare `ripr explain --worktree` prefix would analyze the
    // caller's directory at its default base instead (#7328 review).
    let worktree_navigation =
        super::finding_navigation_with_worktree(&input, None, mode_explicit, true);
    let retry_command = worktree_navigation.explain_command(selector);
    let worktree_list_command = explain_navigation_list_command(&worktree_navigation);
    // A supplied scope (`--diff`, candidate tree) was the analyzed input:
    // uncommitted edits were never excluded, and a worktree retry cannot
    // replay the supplied patch, so the clause stays silent there.
    let committed_history = input.diff_file.is_none() && input.git_candidate.is_none();
    let output = if worktree {
        check_with_progress(input, config, AnalysisProgressScope::Worktree, sink)?
    } else {
        check_with_progress(input, config, AnalysisProgressScope::Diff, sink)?
    };
    match super::selector::select_finding_with_seam_reverse(
        &output.findings,
        &root,
        config,
        selector,
    ) {
        FindingSelection::Unique(finding) => Ok(render_selected(
            format,
            finding,
            config,
            &navigation.context_command(selector),
            &output.root,
            output.source_value_files(finding),
            output.suppression.as_ref().is_some_and(|outcome| {
                outcome
                    .suppressed
                    .iter()
                    .any(|entry| entry.finding_id == finding.id)
            }),
        )),
        FindingSelection::Ambiguous(candidates) => Err(refuse_selector_miss(
            sink,
            ambiguous_finding_match(
                selector,
                &candidates,
                &explain_navigation_list_command(&navigation),
            ),
        )),
        FindingSelection::Undecided(limitation) => Err(refuse_selector_miss(
            sink,
            selector_limitation_fresh_run(
                selector,
                &explain_navigation_list_command(&navigation),
                &limitation,
            ),
        )),
        FindingSelection::NoMatch => Err(refuse_selector_miss(
            sink,
            no_finding_matched_fresh_run(
                selector,
                &explain_navigation_list_command(&navigation),
                &root,
                worktree,
                committed_history,
                &retry_command,
                &worktree_list_command,
                &committed_list_command,
                worktree_from_default,
                &output,
            ),
        )),
    }
}

/// #13239: a post-analysis selector refusal — a miss, an ambiguity, or an
/// undecidable location — is user-fixable usage, not an internal failure.
/// Flag the held-success sink, mirroring the `context` adapter (#9922 /
/// #9244 item 9), so the dropped CLI sink suppresses
/// `ripr progress: failed` beside the real error; the refusal message and
/// exit code are unchanged. `None` is the library-caller legacy and has no
/// sink to flag.
fn refuse_selector_miss(sink: Option<&dyn AnalysisProgressSink>, message: String) -> String {
    if let Some(sink) = sink {
        sink.user_fixable_failure();
    }
    message
}

const EXPLAIN_JSON_FORMAT_SUFFIX: &str = " --format json";

/// User-facing explain hints prefer the documented `--format json` spelling (#11570 item 4).
fn explain_list_command(list_command: &str) -> String {
    if let Some(prefix) = list_command.strip_suffix(" --json") {
        format!("{prefix}{EXPLAIN_JSON_FORMAT_SUFFIX}")
    } else {
        list_command.to_string()
    }
}

fn explain_navigation_list_command(navigation: &super::FindingNavigation) -> String {
    explain_list_command(&navigation.list_command())
}

/// The miss message names the listing command for the same scope (root,
/// base, `--worktree`, mode): a bare `ripr check --format json` would list findings
/// from the caller's directory and default base, and without `--worktree`
/// it omits the findings only the uncommitted edits produce.
pub(crate) fn no_finding_matched(
    selector: &str,
    list_command: &str,
    repair_root: Option<&Path>,
) -> String {
    // Only the validated hex id is echoed into the suggested commands, never
    // the typed selector, so a crafted `seam:` selector cannot reach a shell.
    if let Some(seam_id) = seam_id_of(selector) {
        let root_arg = repair_root
            .map(|root| {
                format!(
                    " --root {}",
                    crate::agent::loop_commands::shell_arg(
                        &crate::agent::loop_commands::bound_root(&root.display().to_string())
                    )
                )
            })
            .unwrap_or_default();
        return format!(
            "no finding matched {selector:?}; this looks like a seam id (as printed by `ripr pilot`), and no finding in the analyzed scope is bound to it; `explain`/`context` select finding ids; run `ripr repair {seam_id}{root_arg}` or `ripr agent repair --seam-id {seam_id}{root_arg} --phase before` to work on the seam"
        );
    }
    let syntax_hint = if super::selector::location_selector_needs_syntax_hint(selector) {
        " for a file location, use file:line with a nonempty path and a positive decimal line number without leading zeros (for example src/lib.rs:12);"
    } else if !selector.contains(':') {
        " give a finding id from `ripr check`, or a file location like src/lib.rs:12;"
    } else {
        ""
    };
    format!(
        "no finding matched {selector:?};{syntax_hint} run `{list_command}` to list available finding ids"
    )
}

/// The 16-lowercase-hex seam id behind a selector handed out by
/// `ripr pilot`/`ripr repair`, with or without an explicit `seam:` prefix.
/// Finding ids are `probe:`/`gap:` identities or `file:line` locations, so
/// this never captures them; anything that is not exactly an id is `None`.
fn seam_id_of(selector: &str) -> Option<&str> {
    let id = selector.strip_prefix("seam:").unwrap_or(selector);
    (id.len() == 16 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))).then_some(id)
}

/// The ambiguity message for a selector naming several findings (#8166
/// Slice B): refuse with the bounded candidate set instead of silently
/// rendering the first match. Candidates are finding IDs, sorted, capped
/// like the repair resolver's bounded ambiguity list.
pub(crate) fn ambiguous_finding_match(
    selector: &str,
    candidates: &[&crate::domain::Finding],
    list_command: &str,
) -> String {
    const MAX_LISTED: usize = 8;
    let mut ids: Vec<&str> = candidates
        .iter()
        .map(|finding| finding.id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let listed: Vec<&str> = ids.iter().take(MAX_LISTED).copied().collect();
    let mut message = format!(
        "selector {selector:?} matched {} findings; select one explicitly: {}",
        candidates.len(),
        listed.join(", ")
    );
    if ids.len() > MAX_LISTED {
        message.push_str(&format!(" (and {} more)", ids.len() - MAX_LISTED));
    }
    message.push_str(&format!(
        "; run `{list_command}` to list available finding ids"
    ));
    message
}

/// The refusal for a selector the lookup could not decide (#8689): an
/// unreadable or budget-truncated seam inventory means the reverse lookup
/// checked nothing, so the message names the limitation and the recovery —
/// a wider seam budget — mirroring the repair resolver's truncation
/// wording, instead of claiming a clean miss.
pub(crate) fn selector_limitation_fresh_run(
    selector: &str,
    list_command: &str,
    limitation: &super::selector::SelectorLimitation,
) -> String {
    match limitation {
        super::selector::SelectorLimitation::InventoryUnreadable => format!(
            "could not resolve {selector:?}: the seam inventory is unreadable, so no match could be checked; run `{list_command}` to list available finding ids"
        ),
        super::selector::SelectorLimitation::InventoryTruncated { analyzed, total } => format!(
            "could not resolve {selector:?}: the seam inventory is truncated ({analyzed} of {total} seams), so no match could be checked; {budget}, or run `{list_command}` to list available finding ids",
            budget = super::task_first::WIDEN_SEAM_BUDGET
        ),
    }
}

/// The miss message for a fresh run, with the uncommitted-edits clause when
/// the selector can only have come from a `--worktree` listing (#7328): a
/// finding id copied from `ripr check --worktree` output dead-ends against
/// the committed-history re-run — and the suggested listing command analyzes
/// committed content only, so the printed advice demonstrably lists nothing.
/// The clause fires only when the run was not `--worktree`, the analyzed
/// scope was committed history (a `--diff` or candidate-tree run analyzed a
/// supplied patch, so uncommitted edits were never the input and a worktree
/// retry cannot replay it), the selector is id- or location-shaped, and the
/// selected root actually has staged or unstaged tracked changes; a clean
/// repository or a supplied scope keeps the unchanged message.
/// `retry_command` is the full working retry for this command, already bound
/// to the run's root/base and quoted (the worktree-mode navigation command),
/// not a bare prefix: a prefix dropped `--root`/`--base` and could point the
/// retry at another repository or base (#7328 review). `worktree_list_command`
/// is the worktree-mode listing from the same navigation: the clause must
/// name the listing that can actually contain the id, not a bare
/// `ripr check --worktree --json` that analyzes the caller's directory at its
/// default base.
#[allow(
    clippy::too_many_arguments,
    reason = "the miss message carries the selector, both scope listings, and the retry together; splitting the packet would misalign the recovery contract"
)]
pub(crate) fn no_finding_matched_fresh_run(
    selector: &str,
    list_command: &str,
    root: &Path,
    worktree: bool,
    committed_history: bool,
    retry_command: &str,
    worktree_list_command: &str,
    committed_list_command: &str,
    worktree_from_default: bool,
    output: &CheckOutput,
) -> String {
    // #9015: a selector copied from a committed listing carries no scope
    // pin, so a dirty-default worktree run can miss it. The recovery's
    // first listing must then replay the committed scope, not the
    // defaulted working-tree scope that just missed.
    let mut message = if worktree && worktree_from_default && committed_history {
        let mut message = no_finding_matched(selector, committed_list_command, Some(root));
        message.push_str(&format!(
            ". The working-tree scope was analyzed and also missed; list its finding ids with `{worktree_list_command}`"
        ));
        message
    } else {
        let mut message = no_finding_matched(selector, list_command, Some(root));
        if !worktree
            && committed_history
            && selector.contains(':')
            && crate::analysis::working_tree_has_tracked_changes(root)
        {
            message.push_str(&format!(
                ". Uncommitted tracked edits exist in this repository and are not analyzed without `--worktree`; if the finding came from uncommitted edits, retry `{retry_command}` (or list worktree finding ids with `{worktree_list_command}`)"
            ));
        }
        message
    };
    // #14077: one cause clause from facts the run holds. When it already
    // names the other finding lines of the same file, the #14384 hint
    // would repeat it; otherwise the hint adds where this scope's findings
    // are (and the column note), so the next selector needs no listing run.
    let cause = location_miss_cause(selector, output);
    let names_same_file_lines = cause
        .as_ref()
        .is_some_and(|cause| cause.names_same_file_lines);
    let names_incomplete = cause.as_ref().is_some_and(|cause| cause.names_incomplete);
    if let Some(LocationMissCause { text: cause, .. }) = cause {
        message.push_str(". ");
        message.push_str(&cause);
    }
    // #14406: an incomplete run never reads as an all-clear; when the cause
    // already said so, the hint keeps its wording without repeating it.
    let completeness = if names_incomplete {
        super::selector::ScopeCompleteness::IncompleteDisclosed
    } else if fresh_run_incomplete(output) {
        super::selector::ScopeCompleteness::Incomplete
    } else {
        super::selector::ScopeCompleteness::Complete
    };
    if !names_same_file_lines
        && let Some(hint) =
            super::selector::location_miss_hint(&output.findings, root, selector, completeness)
    {
        message.push('.');
        message.push_str(&hint);
    }
    append_fresh_run_language_limits(&mut message, output);
    message
}

/// Whether a fresh run left part of its scope unanalyzed: an incomplete
/// producer outcome (`partial_with_limitations`, `unsupported_input`,
/// `analysis_failed`, for example an oversized or malformed diff), a
/// language run that did not complete, or preview-language files no adapter
/// analyzed (#14406).
fn fresh_run_incomplete(output: &CheckOutput) -> bool {
    output
        .analysis_outcome
        .as_ref()
        .is_some_and(|outcome| !outcome.kind.is_complete())
        // A diff degraded to a partial scope (`limited_partial_scope`), the
        // same signal #14077's miss cause reads as incomplete.
        || output.partial_scope.is_some()
        || output
            .language_runs
            .iter()
            .any(|run| run.status != crate::analysis::LanguageRunStatus::Ok)
        || output
            .preview_language_advisories
            .iter()
            .any(|advisory| !advisory.analyzed(&output.language_runs))
}

/// How many same-file finding locations a location miss names.
const MAX_NEARBY_FINDING_LOCATIONS: usize = 5;

/// Why a well-formed `file:line` selector matched nothing (#14077), from
/// facts this run already holds: its finding list, its analyzed root and the
/// producer's outcome counts. One message used to answer six different
/// causes, so a typo, an unchanged line and a finding one line up were
/// indistinguishable.
///
/// The run output carries counts, not the per-file changed-line set, so this
/// never says whether the named line itself changed. Finding ids, seam ids
/// and malformed locations return `None`: they keep their own wording.
fn location_miss_cause(selector: &str, output: &CheckOutput) -> Option<LocationMissCause> {
    location_miss_cause_text(selector, output)
}

/// A [`location_miss_cause`] clause, whether it already names the same
/// file's other finding lines (so the #14384 hint must not repeat them),
/// and whether it already says the analysis was incomplete (so the hint
/// must not repeat that either, #14406).
struct LocationMissCause {
    text: String,
    names_same_file_lines: bool,
    names_incomplete: bool,
}

impl LocationMissCause {
    fn plain(text: String) -> Self {
        Self {
            text,
            names_same_file_lines: false,
            names_incomplete: false,
        }
    }
}

fn location_miss_cause_text(selector: &str, output: &CheckOutput) -> Option<LocationMissCause> {
    if selector.starts_with("probe:")
        || selector.starts_with("repo-probe:")
        || selector.starts_with("gap:")
        || seam_id_of(selector).is_some()
        || super::selector::location_selector_needs_syntax_hint(selector)
    {
        return None;
    }
    let (selector_file, selector_line) = selector.rsplit_once(':')?;
    let wanted_line = selector_line.parse::<usize>().ok()?;
    // A `file:line:column` selector would read its line as part of the path
    // and be told the path is misspelled; the #14384 hint names the column
    // form instead.
    if selector_file
        .rsplit_once(':')
        .is_some_and(|(_, line)| !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let wanted_file = super::selector::normalize_locator_path(selector_file);
    if wanted_file.is_empty() {
        return None;
    }

    // Same file, another line: the user is one retry away from the finding.
    let mut nearby: Vec<(usize, String)> = output
        .findings
        .iter()
        .filter_map(|finding| {
            let stored = crate::analysis::stable_path_text(&finding.probe.location.file);
            super::selector::locator_paths_match(stored.as_ref(), selector_file).then(|| {
                (
                    finding.probe.location.line,
                    // Shown root-relative, as every rendered location is: a
                    // fresh run stores absolute (on Windows, verbatim) paths.
                    super::selector::normalize_locator_path(
                        &crate::analysis::finding_location_text(
                            &output.root,
                            &finding.probe.location.file,
                        ),
                    ),
                )
            })
        })
        .collect();
    nearby.sort();
    nearby.dedup();
    if !nearby.is_empty() {
        let total = nearby.len();
        nearby.sort_by_key(|(line, file)| (line.abs_diff(wanted_line), *line, file.clone()));
        nearby.truncate(MAX_NEARBY_FINDING_LOCATIONS);
        nearby.sort();
        let listed = nearby
            .iter()
            .map(|(line, file)| format!("{file}:{line}"))
            .collect::<Vec<_>>()
            .join(", ");
        let scope = if total > nearby.len() {
            format!("the {} nearest of {total} are", nearby.len())
        } else if total == 1 {
            "it is at".to_string()
        } else {
            "they are at".to_string()
        };
        let noun = if total == 1 {
            "a finding at another line"
        } else {
            "findings at other lines"
        };
        return Some(LocationMissCause {
            text: format!(
                "No finding is at line {wanted_line}, but this run has {noun} of that file: {scope} {listed}. Retry with one of those locations"
            ),
            names_same_file_lines: true,
            names_incomplete: false,
        });
    }

    // Only a plain relative path is probed on disk, so a selector can never
    // be used to ask whether something outside the analyzed root exists.
    // Selection also accepts a path suffix (`lib.rs` for `src/lib.rs`), which
    // only finding files can resolve; with none matching, the miss says the
    // text is not a path from the root rather than that no such file exists.
    let relative = Path::new(&wanted_file);
    let confined = relative
        .components()
        .all(|component| matches!(component, std::path::Component::Normal(_)));
    if !confined {
        return None;
    }
    let root = output.root.display();
    if !output.root.join(relative).is_file() {
        return Some(LocationMissCause::plain(format!(
            "{wanted_file:?} is not the path of a file from the analyzed root `{root}`, and no finding is in a file whose path ends with it; give the file's path from that root, check the spelling, or pass `--root`"
        )));
    }
    let mut cause = format!("{wanted_file:?} exists, but this run has no finding anywhere in it");
    // Only a complete run supports "nothing there to report": an incomplete
    // or partitioned run may never have analyzed the file, and a run with no
    // producer outcome establishes neither.
    let Some(outcome) = &output.analysis_outcome else {
        return Some(LocationMissCause::plain(cause));
    };
    if !outcome.kind.is_complete() || output.partial_scope.is_some() {
        cause.push_str(
            ". The analysis was incomplete, so that file may not have been analyzed; `ripr check` for the same scope lists what was left out",
        );
        return Some(LocationMissCause {
            text: cause,
            names_same_file_lines: false,
            names_incomplete: true,
        });
    }
    let counts = &outcome.counts;
    cause.push_str(&format!(
        ". ripr reports a finding only on a changed line whose behavior it checks, so an unchanged line, a comment or a line of test code has none. This run read {} changed file(s) and {} changed line(s), {} of them candidate line(s) ripr considers",
        counts.changed_file_count, counts.changed_line_count, counts.candidate_line_count
    ));
    Some(LocationMissCause::plain(cause))
}

/// Preserve the check producer's scope limitations on a fresh lookup miss.
/// Sample paths are capped, and an opaque id need not name a language, so
/// these describe this run rather than attribute the selector's miss to an
/// adapter. Artifact lookups have no complete advisory carrier and never
/// call this projection (#10720).
fn append_fresh_run_language_limits(message: &mut String, output: &CheckOutput) {
    let mut limits = Vec::new();
    for advisory in &output.preview_language_advisories {
        if advisory.analyzed(&output.language_runs) {
            continue;
        }
        let (adapter, files) = advisory.prose_names();
        let noun = if advisory.file_count == 1 {
            "file"
        } else {
            "files"
        };
        let mut limit = format!(
            "{adapter}: {} {files} {noun}; {}",
            advisory.file_count,
            advisory.unaudited_why(&output.language_runs)
        );
        if let Some(reason) = advisory
            .non_success_run(&output.language_runs)
            .and_then(|run| run.reason.as_deref())
        {
            limit.push_str(&format!(". Reason: {reason}"));
        }
        limits.push(limit);
    }
    // A language run can carry a failure without a preview advisory (for
    // example a partial Rust run). Retain its native reason as well, but
    // avoid repeating runs already projected with their advisory above.
    for run in &output.language_runs {
        if run.status == crate::analysis::LanguageRunStatus::Ok
            || output
                .preview_language_advisories
                .iter()
                .any(|advisory| advisory.language == run.language)
        {
            continue;
        }
        let language = crate::domain::LanguageId::display_name_for_wire(&run.language)
            .unwrap_or(&run.language);
        let mut limit = format!(
            "{language}: analysis did not complete successfully ({})",
            run.status.as_str()
        );
        if let Some(reason) = &run.reason {
            limit.push_str(&format!(". Reason: {reason}"));
        }
        limits.push(limit);
    }
    if !limits.is_empty() {
        message.push_str("\nAnalysis limitations for this run:\n  ");
        message.push_str(&limits.join("\n  "));
    }
}

/// Like [`explain_finding_with_config`] but loads the finding set from a
/// previously written check artifact (`--from`, RIPR-SPEC-0140) instead of
/// re-running the pipeline. The artifact identity gate is fail-closed; on a
/// verified hit, selection and rendering are identical to the fresh path, so
/// the rendered output is byte-identical to a recomputed run given the same
/// render options. `asserted_base` is an explicitly passed `--base`, verified
/// against the recording rather than used as an override.
#[cfg(test)]
pub(crate) fn explain_finding_from_artifact(
    input: CheckInput,
    selector: &str,
    config: &RiprConfig,
    artifact_path: &Path,
    asserted_base: Option<&str>,
) -> Result<String, String> {
    explain_finding_from_artifact_with_navigation_mode(
        input,
        selector,
        config,
        artifact_path,
        asserted_base,
        false,
        ExplainFormat::Human,
    )
}

pub(crate) fn explain_finding_from_artifact_with_navigation_mode(
    input: CheckInput,
    selector: &str,
    config: &RiprConfig,
    artifact_path: &Path,
    asserted_base: Option<&str>,
    mode_explicit: bool,
    format: ExplainFormat,
) -> Result<String, String> {
    let navigation = super::finding_navigation(&input, Some(artifact_path), mode_explicit);
    let findings = super::check_artifact::load_findings_for_reuse(
        artifact_path,
        &input,
        config,
        asserted_base,
    )?;
    // The artifact path reuses the same seam-side reverse lookups as the
    // fresh path: seam IDs and hash-form canonical gap IDs bind through the
    // current root's classified seams, so one ID spells the same finding on
    // every explain surface (#8688).
    match super::selector::select_finding_with_seam_reverse(
        &findings,
        &input.root,
        config,
        selector,
    ) {
        FindingSelection::Unique(finding) => Ok(render_selected(
            format,
            finding,
            config,
            &navigation.context_command(selector),
            // The artifact was identity-verified against this root; the
            // location renders against it like every fresh route (#5996).
            &input.root,
            None,
            // `--from` replay applies no suppression policy; the CLI refuses
            // the agent format when one is supplied (fail closed).
            false,
        )),
        FindingSelection::Ambiguous(candidates) => Err(ambiguous_finding_match(
            selector,
            &candidates,
            &explain_navigation_list_command(&navigation),
        )),
        // `select_finding` never inventories seams, so it cannot produce a
        // limitation; the arm keeps the match exhaustive and renders the
        // honest state if a future producer starts carrying one.
        FindingSelection::Undecided(limitation) => Err(selector_limitation_fresh_run(
            selector,
            &explain_navigation_list_command(&navigation),
            &limitation,
        )),
        FindingSelection::NoMatch => {
            let mut message = no_finding_matched(
                selector,
                &explain_navigation_list_command(&navigation),
                None,
            );
            if let Some(hint) = super::selector::location_miss_hint(
                &findings,
                &input.root,
                selector,
                // #14440: the artifact records findings, not whether
                // its run was complete.
                super::selector::ScopeCompleteness::Unrecorded,
            ) {
                message.push('.');
                message.push_str(&hint);
            }
            Err(message)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use std::path::PathBuf;

    const SAMPLE_LIST_COMMAND: &str = "ripr check --format json";

    fn sample_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/sample")
    }

    fn sample_diff_input() -> CheckInput {
        let root = sample_root();
        CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("example.diff")),
            mode: Mode::Draft,
            ..CheckInput::default()
        }
    }

    fn fixture_diff_input(name: &str) -> CheckInput {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures")
            .join(name);
        CheckInput {
            root: fixture.join("input"),
            diff_file: Some(fixture.join("diff.patch")),
            mode: Mode::Draft,
            ..CheckInput::default()
        }
    }

    fn require_contains(rendered: &str, needle: &str) -> Result<(), String> {
        if rendered.contains(needle) {
            Ok(())
        } else {
            Err(format!(
                "expected rendered explain output to contain {needle:?}"
            ))
        }
    }

    fn require_not_contains(rendered: &str, needle: &str) -> Result<(), String> {
        if rendered.contains(needle) {
            Err(format!(
                "expected rendered explain output to omit {needle:?}"
            ))
        } else {
            Ok(())
        }
    }

    fn explain_fixture(name: &str, selector: &str) -> Result<String, String> {
        let mut input = fixture_diff_input(name);
        let config = crate::config::load_for_root(&input.root)?;
        crate::config::apply_to_check_input(
            &mut input,
            &config,
            crate::config::CheckInputExplicit::default(),
        );
        explain_finding_with_config(input, selector, &config)
    }

    #[test]
    fn explain_finding_with_input_renders_selected_finding() -> Result<(), String> {
        let rendered = explain_finding_with_input(
            sample_diff_input(),
            "probe:crates_ripr_examples_sample_src_lib.rs:error_path:a776c683",
        )?;

        assert!(rendered.contains("Static exposure"));
        assert!(rendered.contains("no_static_path"));
        assert!(rendered.contains("InvoiceError::InvalidCurrency"));
        // #7937: the expectation goes through the same rendering owners the
        // drill-in builder uses (`bound_root` for the analyzed root,
        // `bound_root_path` + `root_path_display` for the diff file, then
        // `shell_arg`). Re-deriving the spelling from `Path::display`
        // asserted the renderer's POSIX form against the renderer's own
        // Windows form: a native-separator display spelling versus the
        // stable-separator bound spelling, so the pin failed only where it
        // was already honest.
        let root = crate::agent::loop_commands::shell_arg(
            &crate::agent::loop_commands::bound_root(&sample_root().display().to_string()),
        );
        let diff = crate::agent::loop_commands::shell_arg(
            &crate::agent::loop_commands::root_path_display(
                &crate::agent::loop_commands::bound_root_path(&sample_root().join("example.diff")),
            ),
        );
        assert!(rendered.contains(
            &format!(
                "Next: ripr context --root {root} --diff {diff} --at probe:crates_ripr_examples_sample_src_lib.rs:error_path:a776c683"
            )
        ));
        Ok(())
    }

    fn outcome_of_kind(
        kind: crate::analysis_outcome::AnalysisOutcomeKind,
        limitations: Vec<crate::analysis_outcome::AnalysisLimitation>,
    ) -> Result<crate::analysis_outcome::AnalysisOutcome, String> {
        crate::analysis_outcome::AnalysisOutcome::new(
            kind,
            crate::analysis_outcome::AnalysisIdentity::default(),
            crate::analysis_outcome::AnalysisOutcomeCounts::default(),
            limitations,
        )
    }

    #[test]
    fn fresh_run_incomplete_reads_an_incomplete_producer_outcome() -> Result<(), String> {
        use crate::analysis_outcome::{
            AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcomeKind, AnalysisRecovery,
            AnalysisRecoveryKind, AnalysisStage,
        };
        // #14406: an oversized or malformed diff leaves no language-run or
        // preview record, only the producer outcome; the location-miss hint
        // must still read that run as incomplete.
        let mut output = crate::app::check_workspace(sample_diff_input())?;
        output.language_runs.clear();
        output.preview_language_advisories.clear();
        // The sample has no Cargo.toml, so its own outcome is partial (the
        // path-dependency graph is unavailable) and already reads incomplete.
        assert!(fresh_run_incomplete(&output));
        let Some(outcome) = output.analysis_outcome.as_mut() else {
            return Err("the sample run carries a producer outcome".to_string());
        };
        outcome.kind = AnalysisOutcomeKind::CompleteWithFindings;
        assert!(!fresh_run_incomplete(&output));
        output.analysis_outcome = None;
        assert!(!fresh_run_incomplete(&output));
        output.analysis_outcome = Some(outcome_of_kind(
            AnalysisOutcomeKind::PartialWithLimitations,
            vec![AnalysisLimitation::new(
                AnalysisLimitationKind::DiffScopeOversized,
                AnalysisStage::DiffLoad,
                AnalysisRecovery::new(AnalysisRecoveryKind::NarrowDiff, "Narrow the diff.")?,
            )],
        )?);
        assert!(fresh_run_incomplete(&output));
        Ok(())
    }

    #[test]
    #[cfg(feature = "lang-typescript")]
    fn explain_finding_renders_typescript_actionable_packet_field_note() -> Result<(), String> {
        let rendered = explain_fixture(
            "ts_repair_packet_complete",
            "probe:src_discount.ts:typescript_preview:2396aec1",
        )?;

        require_contains(&rendered, "TypeScript repair packet (advisory)")?;
        require_contains(&rendered, "repair packet ready: true")?;
        require_contains(
            &rendered,
            "canonical gap: gap:typescript:typescript_preview:2396aec1",
        )?;
        require_contains(&rendered, "edit surface: tests/discount.test.ts")?;
        require_contains(
            &rendered,
            "verify: npx --no-install jest tests/discount.test.ts",
        )?;
        require_contains(
            &rendered,
            "receipt: ripr receipt write --gap gap:typescript:typescript_preview:2396aec1 ",
        )?;
        require_not_contains(&rendered, "ripr outcome")?;
        require_contains(&rendered, "authority: preview_advisory_only")?;
        require_not_contains(&rendered, "status: not actionable")?;
        Ok(())
    }

    #[test]
    #[cfg(feature = "lang-typescript")]
    fn explain_finding_renders_typescript_blocked_packet_limitation() -> Result<(), String> {
        let rendered = explain_fixture(
            "ts_static_limit",
            "probe:src_cache.ts:typescript_preview:bac50022",
        )?;

        require_contains(&rendered, "TypeScript repair packet (advisory)")?;
        require_contains(&rendered, "repair packet ready: false")?;
        require_contains(&rendered, "status: not actionable")?;
        require_contains(
            &rendered,
            "limitation: static limit `dynamic_dispatch` prevents bounded TypeScript repair guidance",
        )?;
        require_contains(
            &rendered,
            "next capability needed: resolve the named static limit and re-run TypeScript preview evidence extraction",
        )?;
        require_not_contains(
            &rendered,
            "canonical gap: gap:typescript:typescript_preview:bac50022",
        )?;
        Ok(())
    }

    /// #8166 Slice B: `collection_argument_plain` carries two findings at
    /// `src/lib.rs:3` (`call_deletion` and `side_effect`). A location
    /// selector naming that line must refuse as ambiguous with the bounded
    /// candidate set, never silently render the first match.
    #[test]
    fn explain_shared_location_refuses_as_ambiguous_naming_candidates() {
        let result = explain_fixture("collection_argument_plain", "src/lib.rs:3");
        let message = result
            .err()
            .unwrap_or_else(|| "ambiguous location must refuse, not render".to_string());
        assert!(
            message.contains("matched 2 findings"),
            "ambiguous refusal must name the match count: {message}"
        );
        assert!(
            message.contains("probe:src_lib.rs:call_deletion:1f64eda5"),
            "ambiguous refusal must name the first candidate: {message}"
        );
        assert!(
            message.contains("probe:src_lib.rs:side_effect:3ceb2918"),
            "ambiguous refusal must name the second candidate: {message}"
        );
    }

    /// #8166 Slice B: a unique canonical gap identity resolves exactly the
    /// finding that carries it.
    #[test]
    fn explain_gap_identity_resolves_its_unique_finding() -> Result<(), String> {
        let rendered = explain_fixture(
            "collection_argument_plain",
            "gap:rust:src/lib.rs:add:call_deletion:call_deletion:v.push_x*1",
        )?;
        require_contains(&rendered, "probe:src_lib.rs:call_deletion:1f64eda5")?;
        require_not_contains(&rendered, "probe:src_lib.rs:side_effect:3ceb2918")
    }

    /// #8166 Slice B: zero matches still refuse with the no-match message,
    /// including a 16-hex seam-shaped selector that names no finding in the
    /// selected diff.
    #[test]
    fn crafted_seam_prefixed_selector_never_reaches_a_suggested_command() {
        for selector in ["seam:$(touch pwned)", "seam:0123456789abcdeg", "seam:`id`"] {
            let message = no_finding_matched(selector, SAMPLE_LIST_COMMAND, None);
            assert!(!message.contains("ripr repair"), "{message}");
            assert!(
                message.contains("to list available finding ids"),
                "{message}"
            );
        }
        let message = no_finding_matched("seam:0123456789abcdef", SAMPLE_LIST_COMMAND, None);
        assert!(
            message.contains("`ripr repair 0123456789abcdef`"),
            "{message}"
        );
    }

    /// #9244 item 10: a bare word with no colon names neither a finding id
    /// nor a location, so the miss says what a selector looks like. Ids and
    /// well-formed `file:line` selectors keep their existing wording.
    #[test]
    fn bare_word_selector_miss_names_the_accepted_selector_forms() {
        let message = no_finding_matched("nocolon", SAMPLE_LIST_COMMAND, None);
        assert!(
            message.contains(
                "give a finding id from `ripr check`, or a file location like src/lib.rs:12"
            ),
            "{message}"
        );
        assert!(message.contains("src/lib.rs:12"), "{message}");
        assert!(
            message.contains("to list available finding ids"),
            "{message}"
        );
        for selector in ["probe:not-a-real-id", "src/lib.rs:12"] {
            let message = no_finding_matched(selector, SAMPLE_LIST_COMMAND, None);
            assert!(
                !message.contains("give a finding id from"),
                "{selector}: {message}"
            );
        }
    }

    #[test]
    fn explain_unmatched_and_seam_shaped_selectors_refuse_as_no_match() {
        for selector in ["probe:not-a-real-id", "0123456789abcdef"] {
            let result = explain_fixture("collection_argument_plain", selector);
            let message = result.err().unwrap_or_default();
            let seam_shaped = selector == "0123456789abcdef";
            assert_eq!(
                message.contains("this looks like a seam id"),
                seam_shaped,
                "seam-id hint only for seam-shaped selectors: {message}"
            );
            assert_eq!(
                message.contains("to list available finding ids"),
                !seam_shaped,
                "finding-id miss keeps the check hint, seam id drops it: {message}"
            );
            if !seam_shaped {
                assert!(
                    message.contains("--format json"),
                    "finding-id miss must suggest documented json format flag: {message}"
                );
                assert!(
                    !message.contains(" --json`"),
                    "explain must not suggest bare --json alias in list hint: {message}"
                );
            }
            if seam_shaped {
                assert!(
                    message.contains("ripr repair 0123456789abcdef"),
                    "{message}"
                );
                assert!(
                    message.contains("ripr agent repair --seam-id 0123456789abcdef")
                        && message.contains("--phase before"),
                    "{message}"
                );
                assert!(message.contains("--root "), "{message}");
            }
        }
        for selector in ["probe:not-a-real-id", "0123456789abcdef"] {
            let result = explain_fixture("collection_argument_plain", selector);
            let message = result
                .err()
                .unwrap_or_else(|| format!("unmatched selector {selector} must refuse"));
            assert!(
                message.contains(&format!("no finding matched {selector:?}")),
                "zero-match refusal must keep the no-match message for {selector}: {message}"
            );
        }
    }

    #[test]
    fn explain_list_command_rewrites_json_alias_suffix() {
        assert_eq!(
            explain_list_command("ripr check --root ./x --json"),
            "ripr check --root ./x --format json"
        );
        assert_eq!(
            explain_list_command("ripr check --format json"),
            "ripr check --format json"
        );
    }

    #[test]
    fn explain_finding_public_wrapper_reports_invalid_root() {
        let result = explain_finding(Path::new("missing-ripr-root-for-explain"), "probe:missing");

        // #3952: a missing root must fail as a missing root, not as an
        // unresolvable default base.
        assert!(
            result
                .err()
                .is_some_and(|err| err.contains("does not exist or is not a directory"))
        );
    }
}
