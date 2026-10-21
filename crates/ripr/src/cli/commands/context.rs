//! Arg-parsing and dispatch for `ripr context`.
//!
//! This is the CLI adapter layer only. Context collection semantics live in
//! `crate::app`. This module owns argv parsing, output destination selection,
//! and exit mapping for the context command family.

use crate::app::{self, CheckInput, OutputFormat};
use crate::cli::help;
use crate::cli::parse::{
    base_with_diff_conflict_error, disclose_attached_terminal_stdin_read, expect_value, parse_mode,
};
use crate::config::{CheckInputExplicit, apply_to_check_input, load_for_root};
use std::path::PathBuf;

pub(in crate::cli) fn context(args: &[String]) -> Result<(), String> {
    let mut input = CheckInput {
        format: OutputFormat::Json,
        ..CheckInput::default()
    };
    let mut explicit = CheckInputExplicit::default();
    let mut selector: Option<String> = None;
    let mut max_tests = crate::config::DEFAULT_CONTEXT_RELATED_TESTS;
    let mut explicit_max_tests = false;
    let mut quiet = false;
    // RIPR-SPEC-0140: `--from` loads a previously written check artifact
    // instead of re-running the pipeline. Scope flags passed alongside it
    // are assertions verified against the recording, not overrides.
    // `--mode` and `--no-unchanged-tests` feed the identity recomputation:
    // an artifact recorded with non-default values is only consumable when
    // the same values resolve here (flag or config).
    let mut from_artifact: Option<PathBuf> = None;
    let mut base_explicitly_provided = false;
    // `--worktree` matches `ripr check --worktree` (see `explain`).
    let mut worktree = false;
    let mut root_explicitly_provided = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                input.root = PathBuf::from(expect_value(args, i, "--root")?);
                root_explicitly_provided = true;
            }
            "--base" => {
                i += 1;
                input.base = Some(expect_value(args, i, "--base")?.to_string());
                base_explicitly_provided = true;
            }
            "--diff" => {
                i += 1;
                input.diff_file = Some(PathBuf::from(expect_value(args, i, "--diff")?));
            }
            "--from" => {
                i += 1;
                from_artifact = Some(PathBuf::from(expect_value(args, i, "--from")?));
            }
            "--mode" => {
                i += 1;
                input.mode = parse_mode(expect_value(args, i, "--mode")?)?;
                explicit.mode = true;
            }
            "--no-unchanged-tests" => {
                input.include_unchanged_tests = false;
                explicit.include_unchanged_tests = true;
            }
            "--perl-facts" => {
                i += 1;
                input.perl_facts_path = Some(PathBuf::from(expect_value(args, i, "--perl-facts")?));
            }
            "--suppression-policy" => {
                i += 1;
                input.suppression_policy = Some(PathBuf::from(expect_value(
                    args,
                    i,
                    "--suppression-policy",
                )?));
            }
            "--worktree" => worktree = true,
            "--at" => {
                i += 1;
                selector = Some(expect_value(args, i, "--at")?.to_string());
            }
            "--finding" => {
                i += 1;
                selector = Some(expect_value(args, i, "--finding")?.to_string());
            }
            "--max-related-tests" => {
                i += 1;
                // #4318 review: a cap is a render knob, and zero is valid —
                // the packet renders zero related tests, matching the config
                // surface. Only the count-style flags are positive-only.
                max_tests = crate::cli::commands_numeric::parse_non_negative_usize(
                    expect_value(args, i, "--max-related-tests")?,
                    "--max-related-tests",
                )?;
                explicit_max_tests = true;
            }
            "--json" => input.format = OutputFormat::Json,
            // #8739: same meaning as `ripr check --quiet`: stderr progress only.
            "--quiet" => quiet = true,
            "--help" | "-h" => {
                help::print_context_help();
                return Ok(());
            }
            // #8745: a non-dash token is the explain-habit paste, not an
            // unknown flag. Keep the grammar flag-only (`--at`/`--finding`);
            // name the retry instead of accepting a positional selector.
            other if !other.starts_with('-') => {
                return Err(positional_selector_hint(other));
            }
            other => return Err(crate::cli::suggest::unknown_argument("context", other)),
        }
        i += 1;
    }
    // #4319: same parse-time `--base`+`--diff` conflict as `explain`. Only
    // the fresh path conflicts: beside `--from`, both flags are assertions
    // verified against the recording (RIPR-SPEC-0140), so that verification
    // path is intentionally left alone.
    if from_artifact.is_none() && base_explicitly_provided && input.diff_file.is_some() {
        return Err(base_with_diff_conflict_error("context"));
    }
    super::worktree_scope_conflict(
        "context",
        worktree,
        input.diff_file.is_some(),
        from_artifact.is_some(),
    )?;
    if selector.is_none() && !worktree {
        return Err(super::missing_selector_error(
            "missing --at or --finding selector",
            "ripr check --json",
        ));
    }
    // #7331 review: `--from` + `--diff -` is an argv usage conflict, not an
    // artifact defect — the stdin sentinel can never verify a recorded
    // diff-file identity (RIPR-SPEC-0140) — so the CLI keeps it prose-only
    // (empty stdout, exit 2) like the gates above, before any refusal
    // envelope can reach stdout. The app layer keeps the identical check
    // for library callers.
    if from_artifact.is_some() && input.diff_file.as_deref() == Some(std::path::Path::new("-")) {
        return Err(app::check_artifact::from_stdin_sentinel_scope_conflict());
    }
    // #7331 review: implicit `--worktree` root discovery runs below argv
    // parsing and can fail on the Git probe; `context` is JSON-only, so the
    // failure writes the shared refusal envelope (with a root-unusable
    // identity) instead of leaving stdout empty.
    //
    // #8902: the implicit root is the one `ripr check` uses (this subsumes the
    // `--worktree` resolution); `--root .` opts out. A member-relative
    // `file:line` selector binds to the invocation directory like `explain`.
    // `--diff` and `--from` are read as typed from the invocation directory,
    // which is already the file the operator named.
    let prefix = match super::check::resolve_implicit_command_root(
        &mut input.root,
        root_explicitly_provided,
    ) {
        Ok(prefix) => prefix,
        Err(err) => {
            return Err(crate::cli::commands_context::refuse_json_envelope(
                crate::output::json::CONTEXT_PACKET_VERSION_CONTRACT,
                "diff",
                Some(input.mode.as_str()),
                &input.root,
                input.base.as_deref(),
                crate::core_error::CoreError::repository_root_unusable(err),
            ));
        }
    };
    let replay = from_artifact.is_some() || input.diff_file.is_some();
    // A binding failure (for example a shadowed member selector) is a
    // post-argv-parse failure like the root and config refusals, so it writes
    // the shared refusal envelope before the prose error reaches stderr.
    let selector = selector
        .map(|item| {
            super::bind_member_drill_in_selector(prefix.as_deref(), &input.root, item, replay)
        })
        .transpose()
        .map_err(|err| {
            crate::cli::commands_context::refuse_json_envelope(
                crate::output::json::CONTEXT_PACKET_VERSION_CONTRACT,
                "diff",
                Some(input.mode.as_str()),
                &input.root,
                input.base.as_deref(),
                err,
            )
        })?;
    // #7331: `context` produces JSON only, so every post-argv-parse failure
    // below writes the shared refusal envelope (keyed with the context
    // packet's own version) to stdout before the prose error reaches
    // stderr. The usage gates above (selector requirement, flag conflicts)
    // stay prose-only, exactly like `check`. The refusal context owns its
    // snapshot so `input` can move into the pipeline call afterwards.
    let parsed_context = (input.root.clone(), input.base.clone(), input.mode.as_str());
    let config = load_for_root(&input.root).map_err(|err| {
        crate::cli::commands_context::refuse_json_envelope(
            crate::output::json::CONTEXT_PACKET_VERSION_CONTRACT,
            "diff",
            Some(parsed_context.2),
            &parsed_context.0,
            parsed_context.1.as_deref(),
            crate::cli::commands_context::config_load_refusal(&parsed_context.0, err),
        )
    })?;
    apply_to_check_input(&mut input, &config, explicit);
    let worktree =
        worktree || super::dirty_tree_default_is_worktree(&input, from_artifact.is_some());
    // Capture the effective mode after config merging: a configured
    // `[analysis] mode` replaces the default, and later refusals must name
    // the mode analysis actually ran with, not the pre-merge default
    // (#7331 review).
    let envelope_context = (input.root.clone(), input.base.clone(), input.mode.as_str());
    let refuse = |error: crate::core_error::CoreError| -> String {
        // A selector miss is usage-class, not an operational refusal: the
        // CLI failure contract keeps it prose-only (empty stdout, exit 2,
        // guidance and the retry listing on stderr), exactly like
        // `explain`, so it bypasses the envelope (#7331 review).
        if error.is_selector_miss() {
            return error.to_string();
        }
        crate::cli::commands_context::refuse_json_envelope(
            crate::output::json::CONTEXT_PACKET_VERSION_CONTRACT,
            "diff",
            Some(envelope_context.2),
            &envelope_context.0,
            envelope_context.1.as_deref(),
            error,
        )
    };
    let Some(selector) = selector else {
        return Err(super::missing_selector_error(
            "missing --at or --finding selector",
            &app::finding_navigation_with_worktree(&input, None, explicit.mode, worktree)
                .list_command(),
        ));
    };
    if !explicit_max_tests {
        max_tests = config.reports().max_related_tests();
    }
    let asserted_base = if base_explicitly_provided {
        input.base.clone()
    } else {
        None
    };
    // #4319: `--diff -` reads the diff from stdin. On an attached terminal
    // that blocks until EOF with no visible sign of why, so the cli adapter
    // discloses the read before dispatching; the analysis loader itself
    // stays silent for library callers.
    // #7352: a fresh context run re-executes the full check pipeline; on a
    // cold large repository the silent window looked like a hang. The same
    // stderr-only stage lines `ripr check` projects (throttled heartbeats
    // included) disclose the work without touching machine stdout. The
    // `--from` artifact path runs no pipeline and wires no sink.
    let progress = (!quiet).then(|| {
        crate::cli::progress::CliProgressSink::for_stderr(
            std::io::IsTerminal::is_terminal(&std::io::stderr()),
            crate::cli::progress::ProgressPolicy::STANDARD,
        )
    });
    let progress_sink = progress
        .as_ref()
        .map(|sink| sink as &dyn app::AnalysisProgressSink);
    let rendered = match from_artifact.as_deref() {
        Some(artifact_path) => app::collect_context_from_artifact(
            input,
            &selector,
            max_tests,
            &config,
            artifact_path,
            asserted_base.as_deref(),
            explicit.mode,
        )
        .map_err(&refuse)?,
        None => {
            disclose_attached_terminal_stdin_read(input.diff_file.as_deref());
            app::collect_context_with_config_worktree_and_progress(
                input,
                &selector,
                max_tests,
                &config,
                worktree,
                explicit.mode,
                progress_sink,
            )
            .map_err(&refuse)?
        }
    };
    // #7352 review: commit the held `completed` terminal before the sink
    // drops, or the drop projects `ripr progress: failed` on success; the
    // artifact path wired no sink and has nothing to commit.
    if let Some(progress) = &progress {
        progress.commit_success();
    }
    println!("{rendered}");
    Ok(())
}

fn positional_selector_hint(arg: &str) -> String {
    format!(
        "unknown context argument {arg:?}: `ripr context` takes the finding selector via `--at` (or `--finding`); try `ripr context --at {arg}`. Run `ripr context --help`."
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::args;
    use super::*;

    #[test]
    fn context_rejects_invalid_max_related_tests() {
        let result = context(&args(&[
            "--at",
            "probe:file.rs:1:predicate",
            "--max-related-tests",
            "many",
        ]));
        assert!(
            matches!(result, Err(message) if message.starts_with("--max-related-tests requires a non-negative integer"))
        );
    }

    /// #4318 review: zero is a valid explicit per-invocation render cap (the
    /// packet renders zero related tests), so it must pass parsing and fail
    /// later — here on the missing root — never with the numeric-shape
    /// message. The config key is the opposite contract: a zero
    /// `reports.max_related_tests` now fails closed at config parse (#7353).
    #[test]
    fn context_accepts_zero_max_related_tests_at_parse_time() {
        let result = context(&args(&[
            "--root",
            "missing-ripr-root-for-context",
            "--at",
            "probe:file.rs:1:predicate",
            "--max-related-tests",
            "0",
        ]));
        assert!(
            matches!(
                &result,
                Err(message) if !message.contains("requires a non-negative integer")
            ),
            "zero is a valid cap and must not fail numeric parsing: {result:?}"
        );
        assert!(
            matches!(
                &result,
                Err(message) if message.contains("missing-ripr-root-for-context")
            ),
            "the accepted zero cap must reach the root-dependent failure: {result:?}"
        );
    }

    #[test]
    fn context_requires_selector() {
        assert_eq!(
            context(&args(&[])),
            Err("missing --at or --finding selector; pass a finding id (e.g. `probe:src_lib.rs:error_path:abc123`) or `file:line`. Run `ripr check --json` to list finding ids".to_string())
        );
    }

    #[test]
    fn context_rejects_unknown_argument() {
        assert_eq!(
            context(&args(&["--unknown", "value"])),
            Err("unknown context argument \"--unknown\". Run `ripr context --help`.".to_string())
        );
    }

    /// Exercised through the parser, not `unknown_argument` directly: wiring
    /// the help lookup is worthless if the parser never consults it, and this
    /// arm previously returned a bare `format!` that bypassed the suggestion
    /// helper entirely.
    #[test]
    fn context_suggests_the_nearest_flag_for_a_typo() {
        assert_eq!(
            context(&args(&["--fromm", "artifact.json"])),
            Err(
                "unknown context argument \"--fromm\". Did you mean `--from`? \
                 Run `ripr context --help`."
                    .to_string()
            )
        );
    }

    /// #8745: a pasted finding id is the explain-habit follow-up, not an
    /// unknown flag. `closest_flag` returns nothing for non-dash args, so the
    /// generic unknown-argument arm never names `--at`. Grammar stays
    /// flag-only; this arm only names the retry.
    #[test]
    fn context_hints_at_for_a_pasted_finding_id() {
        assert_eq!(
            context(&args(&["probe:src_lib.rs:return_value:abc123"])),
            Err(
                "unknown context argument \"probe:src_lib.rs:return_value:abc123\": \
                 `ripr context` takes the finding selector via `--at` (or `--finding`); \
                 try `ripr context --at probe:src_lib.rs:return_value:abc123`. \
                 Run `ripr context --help`."
                    .to_string()
            )
        );
    }

    #[test]
    fn context_hints_at_for_a_file_line_positional() {
        assert_eq!(
            context(&args(&["src_lib.rs:12"])),
            Err("unknown context argument \"src_lib.rs:12\": \
                 `ripr context` takes the finding selector via `--at` (or `--finding`); \
                 try `ripr context --at src_lib.rs:12`. \
                 Run `ripr context --help`."
                .to_string())
        );
    }

    /// `--at <id>` remains the accepted grammar. The positional hint must not
    /// fire when the selector is already behind the flag.
    #[test]
    fn context_still_parses_at_selector() {
        let result = context(&args(&[
            "--root",
            "missing-ripr-root-for-context",
            "--at",
            "probe:src_lib.rs:return_value:abc123",
        ]));
        assert!(
            matches!(
                &result,
                Err(message)
                    if !message.contains("takes the finding selector via")
                        && message.contains("missing-ripr-root-for-context")
            ),
            "`--at` must still parse the selector and reach the root-dependent failure: {result:?}"
        );
        let finding = context(&args(&[
            "--root",
            "missing-ripr-root-for-context",
            "--finding",
            "probe:src_lib.rs:return_value:abc123",
        ]));
        assert!(
            matches!(
                &finding,
                Err(message)
                    if !message.contains("takes the finding selector via")
                        && message.contains("missing-ripr-root-for-context")
            ),
            "`--finding` must still parse the selector: {finding:?}"
        );
    }

    #[test]
    fn context_requires_values_for_value_flags() {
        assert_eq!(
            context(&args(&["--at"])),
            Err("missing value for --at".to_string())
        );
        assert_eq!(
            context(&args(&["--finding"])),
            Err("missing value for --finding".to_string())
        );
        assert_eq!(
            context(&args(&["--root"])),
            Err("missing value for --root".to_string())
        );
        assert_eq!(
            context(&args(&["--perl-facts"])),
            Err("missing value for --perl-facts".to_string())
        );
        assert_eq!(
            context(&args(&["--suppression-policy"])),
            Err("missing value for --suppression-policy".to_string())
        );
    }

    /// #4319: same parse-time `--base`+`--diff` conflict as `explain`. The
    /// loader gives `--diff` precedence and never validates `--base` beside
    /// it, so both flags on one command line silently analyzed the diff while
    /// appearing to assert the base. The conflict fails before any pipeline
    /// run and before the selector requirement. Message pinned verbatim.
    #[test]
    fn context_rejects_base_and_diff_together_at_parse_time() {
        let expected = Err(
            "context --base cannot be combined with --diff: --base and --diff are alternative diff sources; pass one"
                .to_string(),
        );
        assert_eq!(
            context(&args(&[
                "--diff",
                "sample.diff",
                "--base",
                "refs/heads/nope",
                "--at",
                "probe:src_lib.rs:error_path:abcd",
            ])),
            expected
        );
        assert_eq!(
            context(&args(&[
                "--base",
                "refs/heads/nope",
                "--diff",
                "sample.diff"
            ])),
            expected,
            "the conflict must not depend on flag order or selector presence"
        );
    }

    /// `--from` scope flags are assertions verified against the recording
    /// (RIPR-SPEC-0140, `app/check_artifact.rs::verify_scope_assertions`),
    /// not alternative diff sources, so the fresh-run conflict must not fire
    /// on the reuse path. The parse proceeds past the gate and fails later,
    /// on the missing artifact — never with the conflict message.
    #[test]
    fn context_keeps_base_and_diff_as_from_artifact_assertions() {
        let result = context(&args(&[
            "--from",
            "does-not-exist.json",
            "--diff",
            "sample.diff",
            "--base",
            "refs/heads/nope",
            "--at",
            "probe:src_lib.rs:error_path:abcd",
        ]));
        assert!(
            !matches!(&result, Err(message) if message.contains("cannot be combined with --diff")),
            "`--from` + `--base` + `--diff` is the reuse-verification path, not a diff-source conflict: {result:?}"
        );
    }

    /// #7331 review: `--from` + `--diff -` is an argv usage conflict, not an
    /// artifact defect, so the CLI gate must fire before any refusal
    /// envelope — and before the artifact is even read — with the exact
    /// prose the smoke contract pins (empty stdout, exit 2, `pass --diff
    /// <path>` guidance on stderr).
    #[test]
    fn context_rejects_from_with_stdin_diff_as_usage_conflict() {
        let expected = Err(
            "--from cannot be combined with --diff -: stdin cannot verify a recorded \
             diff-file identity; save stdin to a named diff file and pass --diff <path> \
             when creating and reusing the artifact"
                .to_string(),
        );
        assert_eq!(
            context(&args(&[
                "--from",
                "missing-artifact.json",
                "--diff",
                "-",
                "--at",
                "probe:src_lib.rs:error_path:abcd",
            ])),
            expected
        );
    }
}
