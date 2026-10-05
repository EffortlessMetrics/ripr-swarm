# RIPR-SPEC-0151: Finding source currentness

Status: proposed

Issue: #3280 (parent #3212)

## Problem

A diff finding records a source location but not which revision owns the
actionable source. A removed-line probe carries only a projected new-side
coordinate: deleted-side evidence is presented with a coordinate that
reads as a candidate position — pointing at whatever line the projection
lands on, with no revision semantics and no way for a consumer to stop
treating it as a candidate edit target. Downstream surfaces re-derive
that judgment differently (the #3212 incident: guidance and aggregate
counts disagreed about the same deleted-side records).

## Behavior

Every finding carries a producer-owned `source_currentness` disposition:

```text
candidate_current   the source expression is present in the candidate
                     (head-side) source at the recorded location; the
                     location is a candidate edit target
base_deleted        the expression was removed on the candidate side; the
                     finding is base-side evidence, not a candidate edit
                     target
moved_or_renamed    the same expression re-appears elsewhere in the
                     candidate file, but the producer cannot establish
                     the exact candidate identity; not a candidate edit
                     target
unresolved_subject  the producing surface does not resolve currentness;
                     the explicit unknown, and the value read back from
                     artifacts written before this field existed
```

For Rust diff findings the disposition is resolved from the diff evidence
that seeded the probe: `after`-carrying probes are `candidate_current`;
removed-only probes are `base_deleted`, or `moved_or_renamed` when the same
trimmed expression text appears among the file's added lines. Repo-mode
findings are `candidate_current` by construction. Preview-language findings
are `unresolved_subject` unless their producer resolves currentness: Python and
TypeScript resolve it from the probe delta, and Perl from the rule below.

Perl fact-packet findings (#6586) resolve from what ripr observed, not from
producer claims: a finding is `candidate_current` only when its source file
is on disk under the analysis root after resolving symlinks (so ingestion
verified the packet's
digest against it) and the diff adds a line inside the packet change's
range whose text matches the verified source at that line, so a stale diff
cannot promote a finding. A source that exists but cannot be read rejects the packet at
ingestion, so it never counts as verified. A fixture-only packet, or a change
the diff does not add a line to, stays `unresolved_subject`. Candidate-current
Perl findings pass the same `is_candidate_actionable` filters as Python and
TypeScript preview findings: SARIF results, GitHub annotations,
`finding_alignment` items and the diff badge's exposure-gap count. The gap
ledger keeps Perl ineligible for gates, agent packets, PR comments and
RIPR 0/RIPR+ counts.

In this slice the disposition is informational for consumers and the
probe's recorded location coordinate is unchanged: a removed-only probe
keeps the projected new-side coordinate that the new-file index, the flow
and value classifiers, and IDE navigation already read (#1222 RANK-1).
Re-coordinating deleted-side evidence — base-side identity, consumer
projections, and edit-target suppression — is the #3212 projection
slice.

## Required Evidence

- The check JSON findings array carries `source_currentness` on every
  finding with the controlled vocabulary above (registered in
  `policy/output_contracts.txt`, `SOURCE_CURRENTNESS_VALUES`).
- `docs/OUTPUT_SCHEMA.md` documents the field, the vocabulary, and the
  deferred re-coordination of deleted-side evidence.
- Producer tests pin each disposition: deleted-tail `base_deleted`,
  moved-expression `moved_or_renamed`, added seam `candidate_current`, and
  the explicit unknown for evidence-less shapes.
- A reused line number with a different expression does not inherit base
  identity (content-addressed ids).

## Required guards

- No producer may emit a disposition it cannot prove from its own
  evidence; the unknown is explicit, never guessed.
- This slice changes no classification, stage, confidence, gate, count,
  actionability, repair-readiness, or location-coordinate outcome: the
  golden corpus diff is exactly the additive field.
- Deserializing artifacts written before this field yields
  `unresolved_subject`, not a fabricated disposition.

## Acceptance Examples

- Accept: a removed-only Rust probe whose expression has no added-line
  match yields `base_deleted` while keeping its recorded coordinate
  unchanged.
- Accept: the same removed expression re-appearing among the file's added
  lines yields `moved_or_renamed`.
- Accept: an added or replacement probe yields `candidate_current` at the
  new-side coordinate.
- Reject: guessing currentness for a preview-language finding or an
  evidence-less shape; those stay `unresolved_subject`.

## Test Mapping

`crates/ripr/src/analysis/probes/diff.rs` `source_currentness_tests` pin
the deleted-tail, moved-expression, added-seam, unresolved, and
coordinate-stability shapes plus the content-addressed-id guard.
`crates/ripr/src/analysis/language/perl/tests.rs`
`perl_finding_is_candidate_current_only_for_an_observed_changed_line` pins
the Perl rule: only an on-disk source with an added line inside the change
is `candidate_current`;
`perl_finding_through_a_symlink_out_of_the_root_stays_unresolved` pins that a
source reached through a symlink out of the root is not. The
re-blessed golden corpus (176 fixtures) carries the field on every
finding with no other behavioral delta.

## Non-Goals

This slice does not change gate, ledger, diagnostic, or repair actionability
policy; does not change any recorded location coordinate (deleted-side
re-coordination is the #3212 projection slice); does not retain rename maps
in the diff parser (pure renames stay excluded with disclosure); does not
resolve currentness for preview-language producers; and does not bump the
check schema version.

## Implementation Mapping

- `crates/ripr/src/domain/probe.rs` owns `SourceCurrentness`,
  `SOURCE_CURRENTNESS_VALUES`, the serde contract, and the
  backward-compatible `Finding` field.
- `crates/ripr/src/analysis/probes/diff.rs` resolves the disposition from
  diff evidence while retaining the projected new-side coordinate for
  removed-only probes; the disposition carries base-side semantics until
  #3281 re-coordinates consumer surfaces.
- `crates/ripr/src/analysis/language/rust/mod.rs` wires the resolution into the
  diff loop and marks repo-mode findings candidate-current.
- `crates/ripr/src/output/json/report.rs` emits the field;
  `docs/OUTPUT_SCHEMA.md` and `policy/output_contracts.txt` carry the wire
  contract.

## Metrics

The finding summary counts are unchanged; the disposition is per-finding
evidence, not a rate. This slice adds no coverage, mutation, adequacy, or
promotion metric; consumer-side metrics belong to the #3212 projection
slice.
