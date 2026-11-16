# RIPR-SPEC-0435: Gate Check One-Command CI Gate

Status: accepted

Owner: cli

Created: 2026-11-08

Linked issues:

- [#13772](https://github.com/EffortlessMetrics/ripr-swarm/issues/13772)
- [#13627](https://github.com/EffortlessMetrics/ripr-swarm/issues/13627)

## Problem

A CI step that fails the job on new test gaps took two commands:
`ripr review-comments`, then `ripr gate evaluate` over the guidance it wrote.
Users coming from cargo-mutants expect one command whose exit status fails the
job. If the two steps are wired by hand, a failed first step can leave an older
guidance document behind, and the second step then evaluates stale evidence.

## Behavior

`ripr gate check --base REV [--head REV]` runs, in one process:

1. refuses an output path that escapes the workspace (the shared containment
   guard, plus a check that a relative guidance path's existing directory
   resolves inside the invoking directory), then removes any file at the guidance path (`--guidance-out`,
   default `target/ripr/review/comments.json`) and its `.md` companion;
2. runs the existing `review-comments` producer for `--base..--head` (head
   defaults to `HEAD`), writing that guidance path;
3. runs the existing `gate evaluate` path over that guidance with
   `--mode acknowledgeable` unless `--mode` is given, writing the usual
   `gate-decision.json` and `.md` reports.

The decision and exit come only from the shared gate evaluator and its exit
mapping: `0` for pass, advisory or acknowledged, `3` for blocked, `2` for
config_error. When review-comments fails, ripr prints the producer error to
stderr. The gate then evaluates a missing document, so the decision is
config_error (exit `2`), never pass. This holds even when review-comments
wrote a document before it failed: gate check removes that document again
before evaluating, because an errored run never feeds the gate. A guidance document that discloses an
incomplete, partial, bounded or refused producer run is a config_error through
the gate's existing checks.

All arguments are validated before analysis starts, including that the
guidance JSON, its `.md` companion and the two gate reports are four distinct
files (a guidance path ending in `.md` is refused). `--pr-guidance`,
`--gap-ledger` and other evaluate-only candidate inputs are refused, because
gate check produces its own guidance. Waiver inputs (`--label`,
`--labels-json`, `--acknowledgement-label`), `--exception-policy`,
`--acknowledgements`, `--baseline`, `--root`, `--out` and `--out-md` mean the
same as for `gate evaluate`. `--timeout-ms` goes to review-comments.

## Required Evidence

- Plan tests pin the producer argv, the gate options, the acknowledgeable
  default and the absolute guidance path when `--root` is named.
- Surface tests pin refusal of a missing or blank base, a bad mode, a bad
  `--timeout-ms`, candidate-input flags, a `.md` guidance path and guidance
  paths that share a file with a gate report, before analysis.
- A Unix test pins that a relative guidance directory linking outside the
  invoking directory is refused and the outside file is untouched.
- A fail-closed test puts a guidance document that passes on its own at the
  guidance path, runs gate check in a non-repository, and asserts exit `2`,
  a config_error report and that the stale file is gone.
- A late-failure test injects a producer that writes a passing document and
  then fails, and asserts exit `2` with both files removed; its control, the
  same producer returning success, passes.
- An end-to-end test on a git fixture asserts that a weak boundary test
  blocks (exit `3`), the waiver label and visible-only mode exit `0`, and a
  boundary-discriminating test passes.
- Command metadata binds `cmd:gate.check` to the gate exit contract.

## Known Limits

- `--labels-json` and other gate inputs resolve against `--root`, as for
  `gate evaluate`.
- Gate check removes the file at `--guidance-out` before each run. Point it
  at a path you do not need to keep.

## Non-Goals

- No second classifier and no change to `ripr check` exit codes.
- No `check --fail-on` flag.
- No claim that a blocked gap is a surviving mutant.

## Acceptance Examples

- `ripr gate check --base origin/main` on a `>` to `>=` change whose test
  never probes the boundary exits `3` with a blocked report.
- The same run with `--label ripr-waive` exits `0` (acknowledged).
- With no usable base, review-comments fails, and gate check exits `2` with
  a config_error report.

## Test Mapping

See `.ripr/traceability.d/13772-gate-check.toml`.

## Implementation Mapping

- `crates/ripr/src/cli/commands/gate.rs`
- `crates/ripr/src/cli/command_metadata.rs`
- `crates/ripr/src/cli/command_catalog.rs`
- `crates/ripr/src/cli/help/policy.rs`

## Metrics

- `unit_test_pass_rate`
