use super::CheckInput;
use super::check::check_with_progress;
use super::progress::{AnalysisProgressScope, AnalysisProgressSink};
use super::selector::{FindingSelection, select_finding};
use crate::config::RiprConfig;
use crate::output;
use std::path::Path;

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
        None,
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
    sink: Option<&dyn AnalysisProgressSink>,
) -> Result<String, String> {
    let navigation = super::finding_navigation_with_worktree(&input, None, mode_explicit, worktree);
    let root = input.root.clone();
    // The worktree retry names the same repository and base the failed run
    // analyzed; a bare `ripr explain --worktree` prefix would analyze the
    // caller's directory at its default base instead (#7328 review).
    let worktree_navigation =
        super::finding_navigation_with_worktree(&input, None, mode_explicit, true);
    let retry_command = worktree_navigation.explain_command(selector);
    let worktree_list_command = worktree_navigation.list_command();
    // A supplied scope (`--diff`, candidate tree) was the analyzed input:
    // uncommitted edits were never excluded, and a worktree retry cannot
    // replay the supplied patch, so the clause stays silent there.
    let committed_history = input.diff_file.is_none() && input.git_candidate.is_none();
    let output = if worktree {
        check_with_progress(input, config, AnalysisProgressScope::Worktree, sink)?
    } else {
        check_with_progress(input, config, AnalysisProgressScope::Diff, sink)?
    };
    match select_finding(&output.findings, selector) {
        FindingSelection::Unique(finding) => {
            Ok(output::human::render_finding_with_context_command(
                finding,
                config,
                &navigation.context_command(selector),
                &output.root,
            ))
        }
        FindingSelection::Ambiguous(candidates) => Err(ambiguous_finding_match(
            selector,
            &candidates,
            &navigation.list_command(),
        )),
        FindingSelection::NoMatch => Err(no_finding_matched_fresh_run(
            selector,
            &navigation.list_command(),
            &root,
            worktree,
            committed_history,
            &retry_command,
            &worktree_list_command,
        )),
    }
}

/// The miss message names the listing command for the same scope (root,
/// base, `--worktree`, mode): a bare `ripr check --json` would list findings
/// from the caller's directory and default base, and without `--worktree`
/// it omits the findings only the uncommitted edits produce.
pub(crate) fn no_finding_matched(selector: &str, list_command: &str) -> String {
    // #8166 reopen: a 16-hex selector is the repair seam ID `ripr check`
    // prints beside the finding ID, not a finding identity. Name the
    // equivalent command that accepts it instead of dead-ending at the
    // finding listing.
    if looks_like_seam_id(selector) {
        return format!(
            "no finding matched {selector:?}; a 16-hex selector is a repair seam ID, not a check finding ID — to act on that seam run `ripr repair {selector}`; to list finding ids run `{list_command}`"
        );
    }
    let syntax_hint = if super::selector::location_selector_needs_syntax_hint(selector) {
        " for a file location, use file:line with a nonempty path and a positive decimal line number without leading zeros (for example src/lib.rs:12);"
    } else {
        ""
    };
    format!(
        "no finding matched {selector:?};{syntax_hint} run `{list_command}` to list available finding ids"
    )
}

/// A repair seam ID: exactly 16 lowercase hexadecimal characters, the
/// repository seam namespace `ripr pilot` lists. Case-restricted so an
/// uppercase spell keeps the plain miss instead of a wrong-family claim.
fn looks_like_seam_id(selector: &str) -> bool {
    selector.len() == 16
        && selector
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
pub(crate) fn no_finding_matched_fresh_run(
    selector: &str,
    list_command: &str,
    root: &Path,
    worktree: bool,
    committed_history: bool,
    retry_command: &str,
    worktree_list_command: &str,
) -> String {
    let mut message = no_finding_matched(selector, list_command);
    if worktree || !committed_history || !selector.contains(':') {
        return message;
    }
    if !crate::analysis::working_tree_has_tracked_changes(root) {
        return message;
    }
    message.push_str(&format!(
        ". Uncommitted tracked edits exist in this repository and are not analyzed without `--worktree`; if the finding came from uncommitted edits, retry `{retry_command}` (or list worktree finding ids with `{worktree_list_command}`)"
    ));
    message
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
    )
}

pub(crate) fn explain_finding_from_artifact_with_navigation_mode(
    input: CheckInput,
    selector: &str,
    config: &RiprConfig,
    artifact_path: &Path,
    asserted_base: Option<&str>,
    mode_explicit: bool,
) -> Result<String, String> {
    let navigation = super::finding_navigation(&input, Some(artifact_path), mode_explicit);
    let findings = super::check_artifact::load_findings_for_reuse(
        artifact_path,
        &input,
        config,
        asserted_base,
    )?;
    match select_finding(&findings, selector) {
        FindingSelection::Unique(finding) => {
            Ok(output::human::render_finding_with_context_command(
                finding,
                config,
                &navigation.context_command(selector),
                // The artifact was identity-verified against this root; the
                // location renders against it like every fresh route (#5996).
                &input.root,
            ))
        }
        FindingSelection::Ambiguous(candidates) => Err(ambiguous_finding_match(
            selector,
            &candidates,
            "ripr check --json",
        )),
        FindingSelection::NoMatch => Err(no_finding_matched(selector, "ripr check --json")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use std::path::PathBuf;

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
    /// selected diff. #8166 reopen: the seam-shaped miss names the
    /// equivalent repair command instead of dead-ending at the finding
    /// listing.
    #[test]
    fn explain_unmatched_and_seam_shaped_selectors_refuse_as_no_match() {
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
        let seam_message = no_finding_matched("0123456789abcdef", "ripr check --json");
        assert!(
            seam_message.contains("is a repair seam ID"),
            "the seam-shaped miss must name the ID family:\n{seam_message}"
        );
        assert!(
            seam_message.contains("ripr repair 0123456789abcdef"),
            "the seam-shaped miss must name the equivalent command:\n{seam_message}"
        );
        assert!(looks_like_seam_id("67fc764ba37d77bd"));
        assert!(!looks_like_seam_id("67FC764BA37D77BD"));
        assert!(!looks_like_seam_id("0123456789abcde"));
        assert!(
            !looks_like_seam_id("probe:src_lib.rs:predicate:4036edfa"),
            "a finding ID must keep the plain miss"
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
