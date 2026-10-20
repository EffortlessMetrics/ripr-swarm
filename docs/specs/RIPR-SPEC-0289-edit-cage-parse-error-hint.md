# RIPR-SPEC-0289: Edit-cage parse-error refusal leads with the plain fix-the-syntax-error hint

Status: proposed

Owner: agent-loop

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8991 (edit-cage parse-error terminal refusal has no "fix the syntax
  error" hint)

Linked PRs:

- None yet

Support-tier impact:

- None. This changes only refusal narration for the agent repair loop; it
  adds no analyzer claim. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No new public command. No schema, manifest, or envelope field changes: the
  refusal narration lines already flow to stderr, the recorded
  `last_after_refusal.reason`, and the `--json` refusal/failure envelopes'
  `narration`, so every surface that renders the refusal carries the new
  first line through the one renderer.

## Problem

When an agent-repair attempt's newly written test has a parse error, the
after phase goes terminal with an `OutsideInlineTestRegion ... unparseable`
refusal. The narration named the refused path and then a long CRLF-oriented
recovery paragraph, but never said the plain thing the user needs: the test
they just wrote does not parse — fix the syntax error and start the attempt
again. The user had to infer the actual problem from the refusal vocabulary
("unparseable inline test region", line-ending recovery prose) while the
attempt was already terminal.

## Behavior

### Rule 1: the parse-error refusal leads with the plain statement

When the after-phase verdict refuses the attempt with an
`OutsideInlineTestRegion` violation whose region-observation detail reports
`unparseable` (the attempt's newly written test does not parse), the
recovery narration's first line is:

> the test written for this attempt does not parse — fix the syntax error
> and start the attempt again.

The statement names the newly written test specifically; it is not a general
syntax complaint and does not fire for any other refusal reason.

### Rule 2: the existing bounded-recovery detail stays secondary, verbatim

Below the new first line, the narration is unchanged: the refused-path
listing with the violation's `OutsideInlineTestRegion` reason (including the
`` `mod <name>`: unparseable `` detail), the terminal-attempt statement, and
the CRLF-aware undo route (`core.autocrlf` line-ending flip warning,
`git stash` aside, fresh `--phase before`, `git stash pop`, new `--attempt`
command). No recovery step, warning, or ordering below the first line
changes.

### Rule 3: one renderer owns the wording, so every surface benefits

The narration is owned by the edit-cage refusal renderer
(`repair_after_cage_recovery_lines`). The narrated lines are what stderr
prints, what the after phase records as `last_after_refusal.reason`, and
what the `--json` `repair_after_refusal` / `repair_after_failure` envelopes
serialize under `narration`, so the human surface, the recorded refusal
projected by `ripr agent status`, and the JSON envelopes all carry the new
first line without per-surface edits. The detection predicate
(`refusal_is_unparseable_new_test`) is owned beside the violation-reason
producer in `edit_cage`, which owns the region-observation label vocabulary
and the `` `mod <name>`: <label> `` detail spelling.

## Required Evidence

- An after phase whose inline-region observation reports `unparseable`
  narrates the plain first line exactly, followed by the unchanged
  refused-path listing and the CRLF-aware undo route.
- An after phase whose inline-region violation names any other reason (for
  example `production_edit`) does not carry the parse hint and keeps its
  existing narration.
- The recorded refusal (`last_after_refusal.reason`) and the `--json`
  failure envelope's `narration` carry the same first line the human
  surface prints.

## Non-Goals

- Changing when the refusal fires, what the cage refuses, or how the
  inline-region observation reaches the verdict.
- Changing the CRLF / line-ending recovery behavior, the undo route, or any
  recovery wording below the new first line.
- Altering the attempt state machine, the terminal-finish record, the
  retry budget, or the refusal recording/routing (#8766, RIPR-SPEC-0282).
- Changing the `Incomparable`-with-empty-violations narration (#7204) or
  any other refusal family.
- Changing the violation reason text held in the attempt manifest; only
  the rendered refusal narration gains the lead line.

## Acceptance Examples

1. A newly written test with a syntax error ends the attempt terminal; the
   refusal's first line reads "the test written for this attempt does not
   parse — fix the syntax error and start the attempt again.", and the
   `unparseable` region detail plus the CRLF-aware undo route still follow.
2. A production edit beside a valid new test keeps the existing narration
   with no parse hint.

## Test Mapping

- `crates/ripr/src/cli/commands/agent.rs`:
  `unparseable_new_test_refusal_leads_with_the_plain_hint`,
  `other_inline_region_refusals_do_not_lead_with_the_parse_hint`.

## Implementation Mapping

- `crates/ripr/src/cli/commands/agent.rs`:
  `repair_after_cage_recovery_lines` owns the refusal narration and the new
  lead line.
- `crates/ripr/src/edit_cage/mod.rs`:
  `refusal_is_unparseable_new_test` owns the detection predicate beside the
  `OutsideInlineTestRegion` reason producer (`evaluate_edit_cage`).
- `docs/REPAIR_ATTEMPT.md`: records the lead line in the terminal-attempt
  recovery narration contract.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
