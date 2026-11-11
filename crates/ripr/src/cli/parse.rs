mod format;
mod mode;
mod value;

use crate::cli::command::CliCommand;
use std::io::IsTerminal;

pub(crate) use format::parse_format;
pub(crate) use mode::parse_mode;
pub(crate) use value::{expect_value, reconcile_root};

/// The parse-time conflict error for a command whose synopsis offers
/// `[--base REV|--diff PATH]` when both flags are given explicitly (#4319).
///
/// The diff loader gives `--diff` precedence and never validates `--base`
/// beside it, so both flags on one command line analyzed one input while
/// appearing to assert the other. The phrasing follows `check`'s existing
/// conflict errors (`commands/check.rs`): name the command and both flags,
/// then say to pass one. Only the fresh-run path raises this; beside
/// `--from`, both flags are assertions verified against the recording
/// (RIPR-SPEC-0140), not alternative diff sources.
pub(crate) fn base_with_diff_conflict_error(command: &str) -> String {
    format!(
        "{command} --base cannot be combined with --diff: --base and --diff are alternative diff sources; pass one"
    )
}

/// One-line stderr disclosure printed by the CLI adapters before ripr blocks
/// reading a diff from an attached terminal (`--diff -` typed at a prompt
/// rather than piped) (#4319). The documented example is `git diff
/// origin/main | ripr check --diff -`; running the right half alone used to
/// look like a silent hang.
///
/// This lives at the cli boundary on purpose: the analysis loader
/// (`analysis::diff::load`) must stay silent so a library caller of
/// `check_workspace` with `diff_file: Some("-")` never receives CLI-branded
/// text on the host's stderr. The piped or captured path must also stay
/// silent and byte-identical, hence the `IsTerminal` gate at the one
/// emission site below.
pub(crate) const ATTACHED_TERMINAL_STDIN_NOTE: &str = "ripr: reading the diff from the attached terminal; paste the diff and press Ctrl+Z then Enter on Windows, or Ctrl+D on Unix, to end input";

/// The pure disclosure decision for `--diff -` (#4319): the note is emitted
/// only when the process's stdin is an attached terminal. Keeping the
/// predicate pure lets the exact phrasing and both arms be pinned without a
/// tty; the piped arm additionally carries an end-to-end subprocess
/// guarantee in `tests/cli_smoke.rs`.
pub(crate) fn attached_terminal_stdin_note(stdin_is_terminal: bool) -> Option<&'static str> {
    stdin_is_terminal.then_some(ATTACHED_TERMINAL_STDIN_NOTE)
}

/// The one thin emission site shared by `check`, `explain`, and `context`:
/// each calls this right before dispatching a run that accepted `--diff -`.
/// Silent unless stdin is an attached terminal, so piped, redirected, and
/// captured stdin keep byte-identical output.
pub(crate) fn disclose_attached_terminal_stdin_read(diff_file: Option<&std::path::Path>) {
    emit_attached_terminal_stdin_note(diff_file, std::io::stdin().is_terminal(), |note| {
        eprintln!("{note}");
    });
}

fn emit_attached_terminal_stdin_note(
    diff_file: Option<&std::path::Path>,
    stdin_is_terminal: bool,
    mut emit: impl FnMut(&str),
) {
    if diff_file != Some(std::path::Path::new("-")) {
        return;
    }
    if let Some(note) = attached_terminal_stdin_note(stdin_is_terminal) {
        emit(note);
    }
}

/// Split every `--name=value` token into `--name` and `value` (#14292).
///
/// Each command parser matches whole tokens, so without this boundary
/// rewrite the near-universal `--format=json` spelling was refused as an
/// unknown argument on every command. The rewrite has one owner and runs
/// once on argv before global `--verbose` extraction, the JSON-refusal
/// request check and command dispatch, so every command family accepts
/// both spellings with identical meaning.
///
/// Only long flags whose name is ASCII alphanumerics and hyphens split, at
/// the first `=`; the value keeps any later `=`. An empty value
/// (`--root=`) becomes an empty value token, which the owning parser
/// refuses as it would `--root ""`. `argv[0]`, short flags, positional
/// values and every token after a literal `--` are left unchanged.
///
/// A documented switch is never split (`--quiet=false`, `--json=true`):
/// switches take no value, and splitting would turn `--quiet=false` into
/// `--quiet` plus a stray `false` that a command with a positional item
/// reads as that item, the opposite of what was typed. The intact token
/// reaches the command parser, which refuses it as an unknown argument
/// naming exactly what was typed. Whether a name is a switch comes from the
/// command help the `ripr help --json` flag inventory is built from, so the
/// rule follows the documented options rather than a second list.
#[doc(hidden)]
pub fn split_long_flag_values(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut literal = false;
    for (index, arg) in args.into_iter().enumerate() {
        if index == 0 || literal {
            out.push(arg);
            continue;
        }
        if arg == "--" {
            literal = true;
            out.push(arg);
            continue;
        }
        match long_flag_value(&arg) {
            Some((name, value)) => {
                out.push(name.to_string());
                out.push(value.to_string());
            }
            None => out.push(arg),
        }
    }
    out
}

fn long_flag_value(arg: &str) -> Option<(&str, &str)> {
    let (name, value) = arg.split_once('=')?;
    let bare = name.strip_prefix("--")?;
    let mut chars = bare.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '-');
    (first_ok && rest_ok && !is_documented_switch(name)).then_some((name, value))
}

/// Whether every command help that documents `name` documents it as a
/// switch (no value). A name no help documents, or one that takes a value
/// anywhere, is not a switch. Only consulted for `--name=value` tokens, so
/// ordinary argv never pays for the help scan.
fn is_documented_switch(name: &str) -> bool {
    let mut documented = false;
    for entry in super::command_catalog::catalog() {
        let Some(body) = super::help::help_text_for(entry.path) else {
            continue;
        };
        for flag in super::help_flags::flags_for(entry.path, body) {
            if flag.name == name {
                if flag.takes_value {
                    return false;
                }
                documented = true;
            }
        }
    }
    documented
}

/// Extract every global `-v`/`--verbose` occurrence from argv, reporting
/// whether at least one was present (#5009).
///
/// This is the single owner of the global-flag stripping rule. The CLI
/// dispatch (`cli::run`) and the binary startup route (`startup.rs`, which
/// must strip before MCP routing) both call it, so the same global flag
/// obeys one rule on every command family: all occurrences are removed in
/// one pass and a repeat is idempotent (`ripr -v -v check` behaves exactly
/// like `ripr -v check`, matching the MCP route's long-standing behavior).
///
/// Value-position boundary, disclosed on the exhaustive help reference
/// (`ripr help --all`): extraction runs before every command parser and is
/// deliberately flag-arity blind — routing the global flag through each
/// command's parser is a non-goal — so a `-v`/`--verbose` token is always
/// the global flag and can never be consumed as another flag's value.
/// `ripr check --base -v` therefore enables verbose and lets `check`
/// report its own `missing value for --base`. Inferring arity from the
/// previous token instead would break the appended-position contract for
/// boolean flags (`ripr check --quiet -v`, `ripr mcp --stdio -v`), which
/// is the common real usage; the documented limitation is the honest
/// boundary.
#[doc(hidden)]
pub fn extract_global_verbose(args: &mut Vec<String>) -> bool {
    let before = args.len();
    args.retain(|arg| arg != "--verbose" && arg != "-v");
    args.len() != before
}

/// Whether argv requests the package version before a top-level command.
///
/// Only leading flags participate. A command-local contract such as
/// `ripr lsp --version` must continue to reach that command.
#[doc(hidden)]
pub fn top_level_version_requested(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .take_while(|arg| arg.starts_with('-'))
        .any(|arg| matches!(arg.as_str(), "--version" | "-V"))
}

pub(super) fn parse_args(args: Vec<String>) -> Result<CliCommand, String> {
    // Version is a process-level identity query. Resolve it before dispatch so
    // an output-looking flag (or a help-looking flag) cannot turn a version
    // request into analysis/help output. The CLI dispatch handles `--verbose`
    // separately; keeping this precedence in the parser makes the same rule
    // hold for library callers and installed-binary invocations.
    if top_level_version_requested(&args) {
        return Ok(CliCommand::Version);
    }

    let command = args.get(1).map(|s| s.as_str());
    let command_args = args.get(2..).map_or_else(Vec::new, <[String]>::to_vec);
    CliCommand::from_parts(command, command_args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Mode, OutputFormat};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn split_long_flag_values_rewrites_only_long_flag_assignments() {
        assert_eq!(
            split_long_flag_values(args(&[
                "ripr",
                "check",
                "--format=json",
                "--base=origin/main",
                "--path=src/a=b.rs",
                "--root=",
                "--quiet",
                "-v",
                "positional=1",
                "-x=1",
                "--=x",
                "--bad name=x",
                "--quiet=false",
                "--json=yes",
                "--base=a=b",
                "--root=no",
            ])),
            args(&[
                "ripr",
                "check",
                "--format",
                "json",
                "--base",
                "origin/main",
                "--path",
                "src/a=b.rs",
                "--root",
                "",
                "--quiet",
                "-v",
                "positional=1",
                "-x=1",
                "--=x",
                "--bad name=x",
                "--quiet=false",
                "--json=yes",
                "--base",
                "a=b",
                "--root",
                "no",
            ])
        );
    }

    #[test]
    fn split_long_flag_values_leaves_argv0_and_tokens_after_double_dash() {
        assert_eq!(
            split_long_flag_values(args(&[
                "--root=.",
                "check",
                "--base=main",
                "--",
                "--format=json",
                "--quiet",
            ])),
            args(&[
                "--root=.",
                "check",
                "--base",
                "main",
                "--",
                "--format=json",
                "--quiet",
            ])
        );
    }

    #[test]
    fn split_long_flag_values_feeds_the_same_command_as_the_spaced_spelling() {
        assert_eq!(
            parse_args(split_long_flag_values(args(&[
                "ripr",
                "check",
                "--format=json",
                "--base=main"
            ]))),
            Ok(CliCommand::Check(args(&[
                "--format", "json", "--base", "main"
            ])))
        );
    }

    #[test]
    fn parse_args_returns_top_level_command_shape() {
        assert_eq!(parse_args(args(&["ripr"])), Ok(CliCommand::Help));
        assert_eq!(
            parse_args(args(&["ripr", "--version"])),
            Ok(CliCommand::Version)
        );
        assert_eq!(
            parse_args(args(&["ripr", "check", "--format", "json"])),
            Ok(CliCommand::Check(args(&["--format", "json"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "pilot", "--max-seams", "3"])),
            Ok(CliCommand::Pilot(args(&["--max-seams", "3"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "outcome", "--format", "json"])),
            Ok(CliCommand::Outcome(args(&["--format", "json"])))
        );
        assert_eq!(
            parse_args(args(&[
                "ripr",
                "rerun",
                "--changed-test",
                "tests/pricing.rs"
            ])),
            Ok(CliCommand::Rerun(args(&[
                "--changed-test",
                "tests/pricing.rs"
            ])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "review-comments", "--base", "main"])),
            Ok(CliCommand::ReviewComments(args(&["--base", "main"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "gate", "evaluate"])),
            Ok(CliCommand::Gate(args(&["evaluate"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "pr-ledger", "record"])),
            Ok(CliCommand::PrLedger(args(&["record"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "pr-comments", "plan"])),
            Ok(CliCommand::PrComments(args(&["plan"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "coverage-grip", "frontier"])),
            Ok(CliCommand::CoverageGrip(args(&["frontier"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "assistant-loop", "proof"])),
            Ok(CliCommand::AssistantLoop(args(&["proof"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "first-action", "--root", "."])),
            Ok(CliCommand::FirstAction(args(&["--root", "."])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "reports", "index"])),
            Ok(CliCommand::Reports(args(&["index"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "calibrate", "cargo-mutants"])),
            Ok(CliCommand::Calibrate(args(&["cargo-mutants"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "agent", "brief", "--json"])),
            Ok(CliCommand::Agent(args(&["brief", "--json"])))
        );
        assert_eq!(
            parse_args(args(&["ripr", "swarm", "queue", "--language", "python"])),
            Ok(CliCommand::Swarm(args(&["queue", "--language", "python"])))
        );
        assert_eq!(
            parse_args(args(&[
                "ripr",
                "swarm",
                "ingest",
                "--result",
                "agent-result.json"
            ])),
            Ok(CliCommand::Swarm(args(&[
                "ingest",
                "--result",
                "agent-result.json"
            ])))
        );
    }

    #[test]
    fn top_level_version_predicate_stops_at_command_boundary() {
        for argv in [
            args(&["ripr", "--version"]),
            args(&["ripr", "--help", "--version"]),
            args(&["ripr", "-v", "--version"]),
            args(&["ripr", "--version", "-v"]),
        ] {
            assert!(
                top_level_version_requested(&argv),
                "leading version flags should be top-level: {argv:?}"
            );
        }
        for argv in [
            args(&["ripr", "check", "--version"]),
            args(&["ripr", "lsp", "--version"]),
        ] {
            assert!(
                !top_level_version_requested(&argv),
                "command-local version flags must not be intercepted: {argv:?}"
            );
        }
    }

    #[test]
    fn version_precedes_help_and_output_looking_flags() {
        for argv in [
            args(&["ripr", "--version", "--json"]),
            args(&["ripr", "--json", "--version"]),
            args(&["ripr", "--version", "--help"]),
            args(&["ripr", "--verbose", "--version"]),
        ] {
            assert_eq!(
                parse_args(argv),
                Ok(CliCommand::Version),
                "version must be resolved before other top-level-looking flags"
            );
        }

        assert_eq!(
            parse_args(args(&["ripr", "lsp", "--version"])),
            Ok(CliCommand::Lsp(args(&["--version"]))),
            "command-local LSP version remains distinct from top-level version"
        );
    }

    #[test]
    fn extract_global_verbose_removes_every_occurrence_in_any_position() {
        let mut argv = args(&["ripr", "-v", "check", "--quiet", "--verbose", "-v"]);
        assert!(extract_global_verbose(&mut argv));
        assert_eq!(argv, args(&["ripr", "check", "--quiet"]));
    }

    #[test]
    fn extract_global_verbose_is_idempotent_for_a_repeated_flag() {
        // #5009: the chosen rule is "all occurrences removed", so a repeat
        // must reduce to the same argv as a single flag on every command
        // family, the MCP route included.
        let mut single = args(&["ripr", "-v", "check"]);
        let mut repeated = args(&["ripr", "-v", "-v", "check"]);
        assert!(extract_global_verbose(&mut single));
        assert!(extract_global_verbose(&mut repeated));
        assert_eq!(single, repeated);
    }

    #[test]
    fn extract_global_verbose_keeps_lookalike_and_plain_tokens() {
        let mut argv = args(&["ripr", "check", "--verbose-extra", "-vv", "--base", "main"]);
        assert!(!extract_global_verbose(&mut argv));
        assert_eq!(
            argv,
            args(&["ripr", "check", "--verbose-extra", "-vv", "--base", "main"])
        );
    }

    #[test]
    fn parse_args_preserves_unknown_command_error() {
        assert_eq!(
            parse_args(args(&["ripr", "unknown"])),
            Err("unknown command \"unknown\". Run `ripr --help`.".to_string())
        );
    }

    struct ModeScenario {
        given_mode: &'static str,
        then_result: Result<Mode, String>,
    }

    #[test]
    fn given_mode_strings_when_parse_mode_then_returns_expected_result() {
        let scenarios = [
            ModeScenario {
                given_mode: "instant",
                then_result: Ok(Mode::Instant),
            },
            ModeScenario {
                given_mode: "draft",
                then_result: Ok(Mode::Draft),
            },
            ModeScenario {
                given_mode: "fast",
                then_result: Ok(Mode::Fast),
            },
            ModeScenario {
                given_mode: "deep",
                then_result: Ok(Mode::Deep),
            },
            ModeScenario {
                given_mode: "ready",
                then_result: Ok(Mode::Ready),
            },
            ModeScenario {
                given_mode: "slow",
                then_result: Err(
                    "unknown mode \"slow\". Accepted: instant, draft, fast, deep, ready."
                        .to_string(),
                ),
            },
            ModeScenario {
                given_mode: "redy",
                then_result: Err(
                    "unknown mode \"redy\". Did you mean `ready`? Accepted: instant, draft, fast, deep, ready."
                        .to_string(),
                ),
            },
            ModeScenario {
                given_mode: "fats",
                then_result: Err(
                    "unknown mode \"fats\". Did you mean `fast`? Accepted: instant, draft, fast, deep, ready."
                        .to_string(),
                ),
            },
        ];

        for scenario in scenarios {
            let actual = parse_mode(scenario.given_mode);
            assert_eq!(
                actual, scenario.then_result,
                "mode scenario failed for given={:?}",
                scenario.given_mode
            );
        }
    }

    #[test]
    fn given_format_strings_when_parse_format_then_returns_expected_result() {
        assert_eq!(parse_format("human"), Ok(OutputFormat::Human));
        assert_eq!(parse_format("text"), Ok(OutputFormat::Human));
        assert_eq!(parse_format("json"), Ok(OutputFormat::Json));
        assert_eq!(
            parse_format("agent-seam-packets-json"),
            Ok(OutputFormat::AgentSeamPacketsJson)
        );
        assert_eq!(
            parse_format("repo-exposure-summary-json"),
            Ok(OutputFormat::RepoExposureSummaryJson)
        );
        assert_eq!(
            parse_format("xml"),
            Err(
                "unknown format \"xml\". Common analysis formats: human, human-full, json, github, sarif. Run `ripr check --help --all` for every accepted format."
                    .to_string()
            )
        );
        // The accepted list comes with a near-miss suggestion (#4318): a
        // dropped letter resolves to the format the user meant.
        assert_eq!(
            parse_format("jsn"),
            Err(
                "unknown format \"jsn\". Did you mean `json`? Run `ripr check --help --all` for every accepted format."
                    .to_string()
            )
        );
    }

    #[test]
    fn given_args_and_index_when_expect_value_then_returns_value_or_missing_error() {
        let values = args(&["--diff", "sample.diff"]);

        let when_value_is_present = expect_value(&values, 1, "--diff");
        assert_eq!(when_value_is_present, Ok("sample.diff"));

        let when_value_is_missing = expect_value(&values, 2, "--diff");
        assert_eq!(
            when_value_is_missing,
            Err("missing value for --diff; pass a value right after it".to_string())
        );
    }

    /// #4319: the conflict error names the command, both flags, and the
    /// pass-one repair, matching `check`'s conflict style. Pinned verbatim
    /// for both consumers so a wording change is a visible contract change.
    #[test]
    fn base_and_diff_conflict_error_names_the_command_and_both_flags() {
        assert_eq!(
            base_with_diff_conflict_error("explain"),
            "explain --base cannot be combined with --diff: --base and --diff are alternative diff sources; pass one"
        );
        assert_eq!(
            base_with_diff_conflict_error("context"),
            "context --base cannot be combined with --diff: --base and --diff are alternative diff sources; pass one"
        );
    }

    /// #4319: the note is a constant behind a pure predicate at the cli
    /// boundary, so the exact phrasing is pinned without a tty. The `false`
    /// arm is the piped or captured-stdin path, which must stay silent; its
    /// end-to-end guarantee lives in the cli_smoke subprocess test. The
    /// shared emitter's positive callback is tested below with an injected
    /// terminal state; actual terminal detection and stderr remain a runtime
    /// terminal spot-check.
    #[test]
    fn attached_terminal_stdin_note_fires_only_for_a_terminal() {
        // Pinned as a literal, not via the const, so a wording change is a
        // visible contract change.
        assert_eq!(
            attached_terminal_stdin_note(true),
            Some(
                "ripr: reading the diff from the attached terminal; paste the diff and press Ctrl+Z then Enter on Windows, or Ctrl+D on Unix, to end input"
            )
        );
        assert_eq!(attached_terminal_stdin_note(false), None);
    }

    #[test]
    fn terminal_stdin_disclosure_emits_once_only_for_a_terminal_diff_source() {
        let mut emitted = Vec::new();
        for (path, terminal) in [(None, true), (Some("file.diff"), true), (Some("-"), false)] {
            emit_attached_terminal_stdin_note(path.map(std::path::Path::new), terminal, |note| {
                emitted.push(note.to_owned());
            });
        }
        assert!(
            emitted.is_empty(),
            "non-terminal or file sources emitted {emitted:?}"
        );
        emit_attached_terminal_stdin_note(Some(std::path::Path::new("-")), true, |note| {
            emitted.push(note.to_owned());
        });
        assert_eq!(
            emitted,
            [
                "ripr: reading the diff from the attached terminal; paste the diff and press Ctrl+Z then Enter on Windows, or Ctrl+D on Unix, to end input"
            ]
        );
    }
}
