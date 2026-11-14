# RIPR-SPEC-0440: Report Output Never Replaces Its Own Input

Status: accepted

Owner: product / swarm

Created: 2026-11-11

Linked issues:

- [#14312](https://github.com/EffortlessMetrics/ripr-swarm/issues/14312) -
  `--out` may name one of the command's own inputs; the input is silently
  replaced.
- [#14352](https://github.com/EffortlessMetrics/ripr-swarm/issues/14352) -
  `--out-jsonl` overlap guards are still lexical-only (follow-up).

## Problem

Artifact report commands read JSON evidence files and write `--out` /
`--out-md` reports. An `--out` that named one of the inputs (for example
`reports gap-ledger --records a.json --out a.json`) replaced the input with
the report and exited `0`, losing evidence the user or agent wanted to rerun
from.

## Behavior

Before writing anything, an artifact report command refuses with exit `2`
when an output flag names the same file as one of its input flags:

```text
ripr: <command> <out-flag> <path> names the same file as <in-flag>; writing there would replace that input. Pass a different <out-flag> path
```

Identity is filesystem identity: both paths must exist and resolve to the
same canonical file, so an absolute spelling, a `dir/../` detour, or a
symlink to the input is refused, while a `..` after a symlinked directory
that reaches a different file is not. A missing input has nothing to replace
and is not refused here. A cargo-mutants directory input is protected through
its `outcomes.json` and `mutants.json` members. A root-relative input
(`policy suppression-health --manifest`) is compared where it is read.

Documented in-place updates stay allowed:

- `baseline update --out` naming `--baseline` (the default when `--out` is
  omitted), unless `--baseline` is also `--current`: the effective
  destination, explicit or default, is always checked against `--current`;
- `policy readiness --out` naming `--previous-readiness` (rolling trend
  update; `--out-md` naming it is still refused);
- `baseline create --dry-run`, which never writes `--out`.

Commands covered: `reports gap-ledger|ts-limitations|ts-false-actionable`,
`outcome`, `calibrate cargo-mutants`, `baseline create|diff|update`,
`zero status`, `evidence-health`, `pr-ledger record`, `pr-comments plan`,
`pr-review front-panel`, `coverage-grip frontier`, `first-action`,
`assistant-loop proof|health`, and `policy readiness|operations|history|
promote|preview-promote|waiver-aging|suppression-health`.

## Required Evidence

- Built-binary tests with exact stderr, exit `2`, empty stdout, and unchanged
  input bytes for refused cases across the covered commands.
- Discriminating negatives: distinct paths write; the in-place exemptions
  are not refused; a symlinked parent plus `..` reaching another file writes;
  a missing input is not refused.

## Non-Goals

- `gate` (inputs resolve against `--root`; its flags are owned elsewhere),
  `reports index` (directory inputs) and `review-comments`.
- `--out-jsonl` append targets (#14352).
- Hard-link detection.

## Acceptance Examples

1. `ripr reports gap-ledger --records art/in.json --out art/in.json` exits
   `2` with the message above and leaves `art/in.json` unchanged.
2. The same with `--out "$PWD/art/in.json"` or a symlink to `art/in.json` is
   refused.
3. `ripr reports gap-ledger --records art/in.json --out art/l.json --out-md
   art/l.md` writes both reports.
4. `ripr baseline update --baseline b.json --current c.json --out b.json
   --remove-resolved` is not refused by this rule.
5. `ripr policy readiness --previous-readiness p.json --out next.json
   --out-md p.json` is refused naming `--out-md` and `--previous-readiness`.
6. `ripr baseline update --baseline a.json --current a.json` (no `--out`) is
   refused naming `--baseline` and `--current`.

## Test Mapping

- `crates/ripr/tests/output_names_input_refusal.rs::output_naming_an_input_is_refused_before_writing`
  - examples 1, 5 and 6 across the covered commands.
- `crates/ripr/tests/output_names_input_refusal.rs::an_absolute_spelling_of_the_input_is_still_refused`
  and `::a_symlink_to_the_input_is_still_refused` - example 2.
- `crates/ripr/tests/output_names_input_refusal.rs::distinct_output_paths_still_write`
  - example 3.
- `crates/ripr/tests/output_names_input_refusal.rs::baseline_update_in_place_is_not_an_overwrite_refusal`
  and `::non_destructive_same_path_flows_are_not_refused` - example 4 and the
  other exemptions.
- `crates/ripr/tests/output_names_input_refusal.rs::a_symlinked_parent_followed_by_dotdot_is_not_a_false_match`
  and `::directory_members_are_inputs_and_missing_inputs_are_not` - identity
  edges.

## Implementation Mapping

- `crates/ripr/src/output/path.rs::output_replaces_input` - canonical
  identity rule.
- `crates/ripr/src/cli/commands.rs::refuse_output_naming_input` - the shared
  refusal and its wording, called from each covered command's option parser.

## Metrics

- No new metric.
