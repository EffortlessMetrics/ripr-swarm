# RIPR-SPEC-0446: Verbose Stderr Diagnostics

Status: accepted

Owner: product / cli

Created: 2026-11-15

Linked issues:

- [#14831](https://github.com/EffortlessMetrics/ripr-swarm/issues/14831) -
  verbose mode cannot show why a finding got its class or where time went,
  and harnesses cannot turn it on.
- Extends [RIPR-SPEC-0203](RIPR-SPEC-0203-global-verbose-flag-contract.md),
  whose non-goals excluded new diagnostics.

Support-tier impact:

- None. Diagnostics go to stderr only; no stdout, JSON, SARIF or exit-code
  contract changes.

Policy impact:

- None. No new dependency, process, network or file-policy surface.

## Problem

When a `ripr check` result looks wrong, the global `-v` flag printed only
languages, mode, scope and counts. It did not say why each finding landed in
its class or where the time went, and an editor server, MCP host, Git hook or
CI job that cannot edit argv had no way to turn it on.

## Behavior

1. `RIPR_VERBOSE` is the environment twin of `-v`, parsed like `RIPR_QUIET`:
   unset, empty, `0` and `false` (any case) are off; any other value is on.
2. The variable turns on the stderr diagnostics only. It never changes
   stdout, JSON or exit codes, and prints no notice of its own. The explicit
   `-v` flag keeps its existing stdout widenings (`doctor`, `agent card`,
   `agent status`, check JSON file-cache fields, the pilot timings file).
3. The variable is ignored on `ripr help --json` and on any `--version` or
   `-V` query, so their stderr stays silent.
4. With diagnostics on, every progress boundary prints
   `ripr: timing [<scope>] <stage> took <n> ms` for the stage it closes, and
   the terminal stage adds `ripr: timing [<scope>] <stage> after <n> ms total`.
   These lines close an open terminal progress row first and never panic on a
   closed stderr.
5. With diagnostics on, `ripr check` prints one
   `ripr: verdict <file>:<line> <class> [reach=.. infect=.. propagate=..
   observe=.. discriminate=..]` line per reported finding, after suppression
   and owner opt-outs, followed by
   `; first stage short of yes: <stage>: <recorded reason>` when a stage is
   neither `yes` nor `not_applicable` and has a reason, and
   `; stopped: <reasons>` when the finding carries stop reasons. Each line is
   terminal-safe and single-line, the reason is capped at 200 characters, and
   at most 50 lines print, followed by an overflow line naming how many more
   exist. A run with withheld findings names how many were withheld.
6. With diagnostics off, none of these lines is formatted or printed.

## Required Evidence

- Unit tests: verdict line wording, stop reasons, the 50-line bound and
  overflow line, hostile and long reasons; phase-timing line wording,
  terminal total and clock underflow; the `RIPR_VERBOSE` switch parse.
- Built-binary tests: `RIPR_VERBOSE=1` keeps `check --format json` stdout
  byte-identical to a plain run while stderr gains timing and verdict lines
  and no notice; `RIPR_VERBOSE=0` stays off; `help --json`, `--version` and
  `lsp --version` keep stderr empty with the variable set.
- The troubleshooting guide's "Attach a verbose log" quotes are reproduced
  against the built binary.

## Non-Goals

- A logging framework dependency or log levels.
- Changing what the `-v` flag prints on stdout.
- Per-finding traces for repo-scope or preview-only commands.

## Acceptance Examples

- `ripr -v check` on a one-line boundary change prints
  `ripr: timing [worktree] analyzing took 74 ms` and
  `ripr: verdict ./src/lib.rs:2 weakly_exposed [reach=yes infect=weak ...]; first stage short of yes: infect: ...`.
- `RIPR_VERBOSE=1 ripr check --format json > out.json` writes the same
  `out.json` as a plain run.

## Test Mapping

- `crates/ripr/src/app/verbose_trace.rs::tests`
- `crates/ripr/src/app/progress.rs::tests::phase_timing_lines_name_the_previous_stage_duration_and_the_terminal_total`
- `crates/ripr/src/cli/progress.rs::tests::verbose_env_value_enables_on_any_value_but_the_off_spellings`
- `crates/ripr/tests/cli_progress.rs` (`ripr_verbose_env_*`)
- `crates/ripr/tests/troubleshooting_guide.rs`

## Implementation Mapping

- `crates/ripr/src/lib.rs` (`set_stderr_diagnostics`, `stderr_diagnostics`)
- `crates/ripr/src/cli/mod.rs`, `crates/ripr/src/startup.rs`
- `crates/ripr/src/app/progress.rs`, `crates/ripr/src/app/verbose_trace.rs`,
  `crates/ripr/src/app/check.rs`
- `crates/ripr/src/stderr_guard.rs` (`eprintln_best_effort!`)

## Metrics

- `unit_test_pass_rate`
