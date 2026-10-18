use super::check::check_with_progress;
use super::progress::{AnalysisProgressScope, AnalysisProgressSink};
use super::selector::{FindingSelection, select_finding};
use super::{CheckInput, OutputFormat};
use crate::config::RiprConfig;
use crate::output;
use std::path::Path;

/// Produces a compact JSON context packet for one selected finding.
pub fn collect_context(
    root: &Path,
    selector: &str,
    max_related_tests: usize,
) -> Result<String, String> {
    collect_context_with_input(
        CheckInput {
            root: root.to_path_buf(),
            format: OutputFormat::Json,
            ..CheckInput::default()
        },
        selector,
        max_related_tests,
    )
}

/// Like [`collect_context`] but allows overriding the full check input.
pub fn collect_context_with_input(
    input: CheckInput,
    selector: &str,
    max_related_tests: usize,
) -> Result<String, String> {
    collect_context_with_config(input, selector, max_related_tests, &RiprConfig::default())
}

pub fn collect_context_with_config(
    input: CheckInput,
    selector: &str,
    max_related_tests: usize,
    config: &RiprConfig,
) -> Result<String, String> {
    collect_context_with_config_and_worktree(
        input,
        selector,
        max_related_tests,
        config,
        false,
        false,
    )
    // Public String compatibility wrapper: the typed core owns the
    // refusal families (#7331); Display is a pure projection, so callers
    // see the same prose either way.
    .map_err(|err| err.to_string())
}

/// [`collect_context_with_config`] over the working tree when `worktree` is
/// set (`ripr context --worktree`), so a finding `ripr check --worktree`
/// listed from uncommitted edits can be selected.
///
/// The typed core owns the refusal families (#7331): pipeline failures stay
/// untyped (the honest `analysis_failed` fallback), while the
/// selector-not-found branch is `SelectorMiss` — usage-class, so the CLI
/// keeps it prose-only instead of enveloping it.
pub(crate) fn collect_context_with_config_and_worktree(
    input: CheckInput,
    selector: &str,
    max_related_tests: usize,
    config: &RiprConfig,
    worktree: bool,
    mode_explicit: bool,
) -> Result<String, crate::core_error::CoreError> {
    collect_context_with_config_worktree_and_progress(
        input,
        selector,
        max_related_tests,
        config,
        worktree,
        mode_explicit,
        None,
    )
}

/// [`collect_context_with_config_and_worktree`] with a progress sink on the
/// fresh pipeline run (#7352): `explain` and `context` re-run the full
/// check pipeline without `--from`, and on a cold large repository that
/// work was invisible — no stage lines, heartbeats, or cost disclosure —
/// for the same tens-of-seconds-to-minutes window `ripr check` discloses.
/// The shared sink projects the same producer-owned stage lines `check`
/// promises; `None` reproduces the legacy silent behavior for library
/// callers. The `--from` artifact path runs no pipeline and takes no sink.
#[allow(
    clippy::too_many_arguments,
    reason = "the CLI progress route carries the check input, selector, render knob, config, mode flags, sink, and diff source together; splitting the packet would misalign the drill-in contract"
)]
pub(crate) fn collect_context_with_config_worktree_and_progress(
    input: CheckInput,
    selector: &str,
    max_related_tests: usize,
    config: &RiprConfig,
    worktree: bool,
    mode_explicit: bool,
    sink: Option<&dyn AnalysisProgressSink>,
) -> Result<String, crate::core_error::CoreError> {
    let input = CheckInput {
        format: OutputFormat::Json,
        ..input
    };
    let navigation = super::finding_navigation_with_worktree(&input, None, mode_explicit, worktree);
    let root = input.root.clone();
    // See the explain fresh path: the retry binds this run's root/base and
    // stays silent for supplied scopes (#7328 review).
    let worktree_navigation =
        super::finding_navigation_with_worktree(&input, None, mode_explicit, true);
    let retry_command = worktree_navigation.context_command(selector);
    let worktree_list_command = worktree_navigation.list_command();
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
        FindingSelection::Unique(finding) => {
            Ok(output::json::render_context_packet_with_explain_command(
                finding,
                max_related_tests,
                Some(navigation.explain_command(&finding.id)),
                &output.root,
            ))
        }
        FindingSelection::Ambiguous(candidates) => Err(
            crate::core_error::CoreError::selector_miss(super::explain::ambiguous_finding_match(
                selector,
                &candidates,
                &navigation.list_command(),
            )),
        ),
        FindingSelection::Undecided(limitation) => {
            Err(crate::core_error::CoreError::selector_miss(
                super::explain::selector_limitation_fresh_run(
                    selector,
                    &navigation.list_command(),
                    &limitation,
                ),
            ))
        }
        FindingSelection::NoMatch => Err(crate::core_error::CoreError::selector_miss(
            super::explain::no_finding_matched_fresh_run(
                selector,
                &navigation.list_command(),
                &root,
                worktree,
                committed_history,
                &retry_command,
                &worktree_list_command,
            ),
        )),
    }
}

/// Like [`collect_context_with_config`] but loads the finding set from a
/// previously written check artifact (`--from`, RIPR-SPEC-0140) instead of
/// re-running the pipeline. The artifact identity gate is fail-closed; on a
/// verified hit, selection and rendering are identical to the fresh path.
/// `max_related_tests` is a render-time knob: it is not part of the artifact
/// identity and is honored fresh, including beyond the `check --json`
/// render cap, because the artifact stores the uncapped related-tests list.
///
/// The typed error carries the refusal family (`check_artifact_unusable`,
/// #7331) so the CLI refusal envelope names it; the selector-not-found
/// branch is `SelectorMiss` — usage-class, so the CLI keeps it prose-only
/// instead of enveloping it (no artifact defect is claimed).
pub(crate) fn collect_context_from_artifact(
    input: CheckInput,
    selector: &str,
    max_related_tests: usize,
    config: &RiprConfig,
    artifact_path: &Path,
    asserted_base: Option<&str>,
    mode_explicit: bool,
) -> Result<String, crate::core_error::CoreError> {
    let findings = super::check_artifact::load_findings_for_reuse_core(
        artifact_path,
        &input,
        config,
        asserted_base,
    )?;
    // As in `explain --from`, the navigation names the artifact: replaying
    // it keeps the verified identity without re-running the pipeline.
    // An explicit Draft must survive replay over a non-Draft repo default,
    // just as it does on the fresh context and artifact explain routes.
    let navigation = super::finding_navigation(&input, Some(artifact_path), mode_explicit);
    match select_finding(&findings, selector) {
        FindingSelection::Unique(finding) => {
            Ok(output::json::render_context_packet_with_explain_command(
                finding,
                max_related_tests,
                Some(navigation.explain_command(&finding.id)),
                // The artifact records the analyzed root; the packet location
                // renders against it like every fresh route (#5996).
                input.root.as_path(),
            ))
        }
        FindingSelection::Ambiguous(candidates) => {
            Err(crate::core_error::CoreError::selector_miss(
                super::explain::ambiguous_finding_match(selector, &candidates, "ripr check --json"),
            ))
        }
        // `select_finding` never inventories seams, so it cannot produce a
        // limitation; the arm keeps the match exhaustive and renders the
        // honest state if a future producer starts carrying one.
        FindingSelection::Undecided(limitation) => {
            Err(crate::core_error::CoreError::selector_miss(
                super::explain::selector_limitation_fresh_run(
                    selector,
                    "ripr check --json",
                    &limitation,
                ),
            ))
        }
        FindingSelection::NoMatch => Err(crate::core_error::CoreError::selector_miss(
            super::explain::no_finding_matched(selector, "ripr check --json"),
        )),
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

    #[test]
    fn collect_context_with_input_renders_selected_finding_packet() -> Result<(), String> {
        let rendered = collect_context_with_input(
            sample_diff_input(),
            "probe:crates_ripr_examples_sample_src_lib.rs:error_path:a776c683",
            2,
        )?;

        assert!(rendered.contains("\"tool\": \"ripr\""));
        assert!(rendered.contains("\"family\": \"error_path\""));
        assert!(rendered.contains("\"missing_discriminators\""));
        assert!(rendered.contains("InvoiceError::InvalidCurrency"));
        Ok(())
    }

    /// #3952: the witness's explain command replays the input identity that
    /// produced the finding. Without it, `ripr explain` re-analyzes the
    /// default branch instead of the `--diff` (or `--base`) the user chose.
    #[test]
    fn context_explain_command_carries_the_input_diff() -> Result<(), String> {
        let input = sample_diff_input();
        let diff = input
            .diff_file
            .as_ref()
            .map(|path| {
                // The drill-in binds a relative --diff to the producing
                // directory (#3948), rendered with the shared root rule.
                crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root(
                    &path.display().to_string(),
                ))
            })
            .ok_or("sample input must carry a diff file")?;
        let selector = "probe:crates_ripr_examples_sample_src_lib.rs:error_path:a776c683";
        let rendered = collect_context_with_input(input, selector, 2)?;
        let packet: serde_json::Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse packet: {err}"))?;
        let command = packet
            .pointer("/witness/explain_command")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("fixture must render a witness: {rendered}"))?;
        if !command.starts_with("ripr explain --root ")
            || !command.contains(&format!("--diff {diff}"))
            || !command.ends_with(selector)
        {
            return Err(format!("explain command must replay --diff: {command}"));
        }
        Ok(())
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

    fn context_fixture(name: &str, selector: &str) -> Result<String, String> {
        let mut input = fixture_diff_input(name);
        let config = crate::config::load_for_root(&input.root)?;
        crate::config::apply_to_check_input(
            &mut input,
            &config,
            crate::config::CheckInputExplicit::default(),
        );
        collect_context_with_config(input, selector, 2, &config)
    }

    /// #8166 Slice B: the shared-location selector that `explain` refuses
    /// must refuse identically here, as the usage-class selector miss
    /// (prose-only, exit 2 at the CLI), naming both candidates.
    #[test]
    fn context_shared_location_refuses_as_ambiguous_naming_candidates() {
        let result = context_fixture("collection_argument_plain", "src/lib.rs:3");
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

    /// #8166 Slice B: ambiguity stays in the selector-miss family so the
    /// CLI keeps it prose-only instead of writing a refusal envelope.
    #[test]
    fn context_ambiguous_selector_is_typed_as_selector_miss() -> Result<(), String> {
        let mut input = fixture_diff_input("collection_argument_plain");
        let config = crate::config::load_for_root(&input.root)?;
        crate::config::apply_to_check_input(
            &mut input,
            &config,
            crate::config::CheckInputExplicit::default(),
        );
        let error = collect_context_with_config_and_worktree(
            input,
            "src/lib.rs:3",
            2,
            &config,
            false,
            false,
        )
        .err()
        .ok_or_else(|| "ambiguous location must refuse, not render".to_string())?;
        if error.is_selector_miss() {
            Ok(())
        } else {
            Err(format!(
                "ambiguous selector must stay usage-class selector-miss: {error}"
            ))
        }
    }

    /// #8166 Slice B: a unique canonical gap identity resolves exactly the
    /// finding that carries it.
    #[test]
    fn context_gap_identity_resolves_its_unique_finding() -> Result<(), String> {
        let rendered = context_fixture(
            "collection_argument_plain",
            "gap:rust:src/lib.rs:add:call_deletion:call_deletion:v.push_x*1",
        )?;
        let packet: serde_json::Value =
            serde_json::from_str(&rendered).map_err(|err| format!("parse packet: {err}"))?;
        if packet["probe"]["id"].as_str() == Some("probe:src_lib.rs:call_deletion:1f64eda5") {
            Ok(())
        } else {
            Err(format!("gap selector resolved another finding: {rendered}"))
        }
    }

    /// #8166 Slice B: zero matches still refuse with the no-match message,
    /// including a 16-hex seam-shaped selector.
    #[test]
    fn context_unmatched_and_seam_shaped_selectors_refuse_as_no_match() {
        for selector in ["probe:not-a-real-id", "0123456789abcdef"] {
            let result = context_fixture("collection_argument_plain", selector);
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
    fn collect_context_public_wrapper_reports_invalid_root() {
        let result = collect_context(
            Path::new("missing-ripr-root-for-context"),
            "probe:missing",
            1,
        );

        // #3952: a missing root must fail as a missing root, not as an
        // unresolvable default base.
        assert!(
            result
                .err()
                .is_some_and(|err| err.contains("does not exist or is not a directory"))
        );
    }
}
