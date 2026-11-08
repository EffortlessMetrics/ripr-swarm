# RIPR-SPEC-0423: Shell Completions

Status: accepted

Owner: cli

Created: 2026-11-08

Linked issues:

- [#7438](https://github.com/EffortlessMetrics/ripr-swarm/issues/7438)

## Problem

`ripr` has many commands and several two-word commands (`cache status`,
`gate evaluate`). Typing them without tab completion is slow and error-prone.
A user needs a script they can load into their shell without `ripr` touching
their shell configuration.

## Behavior

`ripr completions <shell>` prints a completion script for one shell to stdout.

### Supported shells

Exactly `bash`, `zsh`, `fish` and `powershell`. The script is meant to be loaded
by the user, for example `source <(ripr completions bash)`.

### Completion positions

Each script completes two positions only:

1. The command name (the first word after `ripr`).
2. The word after a command, for commands that have one (for example the
   `status`, `export` and `clear` words after `cache`).

The names come from the governed command catalog: only ordinary public
canonical rows. Compatibility aliases, advanced rows and hidden rows are never
completed. The second word is offered only while no second word is typed yet;
after `ripr cache status ` the sub-words are not offered again.

### Errors and exit codes

- A valid shell name prints the script and exits `0` with empty stderr.
- `--help` or `-h` prints help and exits `0`.
- A missing shell name, an unknown shell name (with the accepted names and a
  suggestion), a second shell name, or an unknown flag prints a message to
  stderr, prints nothing to stdout, and exits `2`.

## Non-Goals

- Flags and flag values are not completed.
- Nothing is installed or written: the command only prints to stdout and does
  not read or change shell configuration files.
- No completion of paths, refs or other argument values.

## Required Evidence

- Unit tests in `crates/ripr/src/cli/commands/completions.rs` pin the exact
  script text per shell for a synthetic catalog, the exclusion of aliases and
  non-public rows, and argument checking.
- `crates/ripr/tests/completions_command.rs` runs the built binary for exit
  codes and streams, and loads the scripts in the installed shells, including
  checks that bash and fish offer the second word only in second position
  (`RIPR_REQUIRE_COMPLETION_SHELLS=1` makes a missing shell a failure).

## Acceptance Examples

1. `ripr completions bash` exits 0 and prints a script naming `check` and `completions`.
2. `ripr completions tcsh` exits 2 and lists `bash, zsh, fish, powershell`.
3. `ripr cache <TAB>` offers `clear export status`; `ripr cache status <TAB>` offers none of them.

## Test Mapping

See `.ripr/traceability.toml` behavior `RIPR-SPEC-0423`.

## Implementation Mapping

- `crates/ripr/src/cli/commands/completions.rs`

## Metrics

- `unit_test_pass_rate`
