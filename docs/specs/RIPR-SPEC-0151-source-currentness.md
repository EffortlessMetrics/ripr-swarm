# RIPR-SPEC-0151: Finding source currentness

Status: proposed

Issue: #3280 (parent #3212)

## Problem

A diff finding records a source location but not which revision owns the
actionable source. A removed-line probe currently records a projected
new-side coordinate, so a base expression deleted from a 13-line candidate
file can be presented at line 29 — an impossible candidate edit target.
Downstream consumers cannot tell candidate-current obligations from
deleted-side evidence, and every surface re-derives that judgment
differently.

## Behavior

Every finding carries a producer-owned `source_currentness` disposition:

```text
candidate_current   the source expression is present in the candidate
                     (head-side) source at the recorded location; the
                     location is a candidate edit target
base_deleted        the expression was removed on the candidate side; the
                     finding is base-side evidence carrying the base-side
                     coordinate, not a candidate edit target
moved_or_renamed    the same expression re-appears elsewhere in the
                     candidate file, but the producer cannot prove the
                     exact candidate identity; not a candidate edit target
unresolved_subject  the producing surface does not resolve currentness;
                     the explicit unknown, and the value read back from
                     artifacts written before this field existed
```

For Rust diff findings the disposition is resolved from the diff evidence
that seeded the probe: `after`-carrying probes are `candidate_current`;
removed-only probes are `base_deleted`, or `moved_or_renamed` when the same
trimmed expression text appears among the file's added lines. Repo-mode
findings are `candidate_current` by construction. Preview-language findings
are `unresolved_subject` until their producers resolve currentness.

A removed-only Rust probe records the base-side line number as its location
coordinate. Finding identity does not change: probe ids are content
addressed over path, family, owner, and normalized expression, excluding
line numbers.

In this slice the disposition is informational for consumers. Gate,
actionability, and repair policy consume it in the #3212 projection slice.

## Required Evidence

- The check JSON findings array carries `source_currentness` on every
  finding with the controlled vocabulary above (registered in
  `policy/output_contracts.txt`, `SOURCE_CURRENTNESS_VALUES`).
- `docs/OUTPUT_SCHEMA.md` documents the field, the vocabulary, and the
  base-side coordinate rule for `base_deleted`.
- Producer tests pin each disposition: deleted-tail `base_deleted`,
  moved-expression `moved_or_renamed`, added seam `candidate_current`, and
  the explicit unknown for evidence-less shapes.
- A reused line number with a different expression does not inherit base
  identity (content-addressed ids).

## Required guards

- No producer may emit a disposition it cannot prove from its own
  evidence; the unknown is explicit, never guessed.
- `base_deleted` and `moved_or_renamed` findings carry no candidate edit
  target: their location coordinate is base-side.
- Deserializing artifacts written before this field yields
  `unresolved_subject`, not a fabricated disposition.
- This slice changes no gate, count, actionability, or repair-readiness
  outcome.

## Acceptance Examples

- Accept: a removed-only Rust probe whose expression has no added-line
  match yields `base_deleted` with the base-side line coordinate.
- Accept: the same removed expression re-appearing among the file's added
  lines yields `moved_or_renamed`.
- Accept: an added or replacement probe yields `candidate_current` at the
  new-side coordinate.
- Reject: guessing currentness for a preview-language finding or an
  evidence-less shape; those stay `unresolved_subject`.

## Test Mapping

`crates/ripr/src/analysis/probes/diff.rs` `source_currentness_tests` pin
the deleted-tail, moved-expression, added-seam, and unresolved shapes plus
the base-side coordinate and content-addressed-id guard. The re-blessed
golden corpus (176 fixtures) carries the field on every finding;
`multi_hunk_removed_line_wrong_target` additionally pins the base-side
coordinate on a real removed-line diff.

## Non-Goals

This slice does not change gate, ledger, diagnostic, or repair actionability
policy; does not retain rename maps in the diff parser (pure renames stay
excluded with disclosure); does not resolve currentness for preview-language
producers; and does not bump the check schema version. Those land in the
#3212 projection slice and later producer work.

## Implementation Mapping

- `crates/ripr/src/domain/probe.rs` owns `SourceCurrentness`,
  `SOURCE_CURRENTNESS_VALUES`, the serde contract, and the
  backward-compatible `Finding` field.
- `crates/ripr/src/analysis/probes/diff.rs` resolves the disposition from
  diff evidence and records the base-side coordinate for removed-only
  probes.
- `crates/ripr/src/analysis/language/rust.rs` wires the resolution into the
  diff loop and marks repo-mode findings candidate-current.
- `crates/ripr/src/output/json/report.rs` emits the field;
  `docs/OUTPUT_SCHEMA.md` and `policy/output_contracts.txt` carry the wire
  contract.

## Metrics

The finding summary counts are unchanged; the disposition is per-finding
evidence, not a rate. This slice adds no coverage, mutation, adequacy, or
promotion metric; consumer-side metrics belong to the #3212 projection
slice.
