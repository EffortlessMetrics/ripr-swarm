# RIPR-SPEC-0428: Check missed-txt One-Line Gap List

Status: accepted

Owner: output

Created: 2026-11-08

Linked issues:

- [#13626](https://github.com/EffortlessMetrics/ripr-swarm/issues/13626)
- [#13612](https://github.com/EffortlessMetrics/ripr-swarm/issues/13612)

## Problem

Scripts and agents that grep or diff a flat list of gaps (the habit
`mutants.out/missed.txt` builds) have to parse check JSON today. A flat list
is only safe if an empty list can never be mistaken for a clean,
fully analyzed run.

## Behavior

`ripr check --format missed-txt` prints, on stdout:

- one line per listed gap, `path:line:col: <class> <probe family>: <expression> -> <missing discriminator>`;
- a finding is listed exactly when the `github` format would annotate it
  (not suppressed by `--suppression-policy`, candidate-actionable, severity
  not `off` for its file) and its class is not `exposed`. No other classifier
  decides membership;
- lines sort by path, line, column, class, family, expression and
  discriminator, independent of finding order;
- the discriminator is the first missing discriminator value, else the first
  `missing` entry, else `-`;
- whitespace inside a field collapses to one space and control and bidi
  characters are escaped, so each record is one line;
- leading `# ripr: ...` comment lines disclose every state the `github`
  format treats as not clean, from the same shared helpers: an incomplete
  analysis outcome with each limitation and recovery, limitations on a
  complete outcome, unanalyzed, untracked or gitignored working-tree sources,
  no analysis scope, unanalyzed preview-language files, and findings omitted
  as suppressed or not current;
- a refused run prints one `# ripr run refused: ...` line and keeps its exit;
- zero bytes are printed only for a fully analyzed, scoped, limitation-free
  run that omits no finding and lists no gap.

Static vocabulary only: the output names exposure classes, never runtime
mutation outcomes.

## Required Evidence

Unit tests over constructed check outputs pin exact lines, ordering
independence, each not-clean disclosure, the omitted-finding count, the
severity filter, refusal rendering and the absence of runtime-outcome words.
A dispatch test pins that `--suppression-policy` is accepted for the format.

## Known Limits

- Paths and expressions are not otherwise escaped; readers treat a line that
  starts with `# ` as a comment.
- Probe columns are reported as the probe location provides them.

## Non-Goals

- No exit-code change: `check` stays advisory (docs/EXIT_CODES.md).
- No claim that a listed line is a surviving mutant.

## Acceptance Examples

- A one-line `>` to `>=` change with a test that never probes the boundary
  lists `src/lib.rs:1:1: weakly_exposed predicate: n >= 10 -> n == 10`.
- A run with no analysis scope prints a `# ripr: no analysis scope: ...`
  line and no gap line.

## Test Mapping

See `.ripr/traceability.d/13626-missed-txt-format.toml`.

## Implementation Mapping

- `crates/ripr/src/output/missed_txt.rs`
- `crates/ripr/src/output/github.rs` (shared not-clean disclosures)
- `crates/ripr/src/output/format.rs`

## Metrics

- `unit_test_pass_rate`
