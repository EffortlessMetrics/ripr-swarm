# RIPR-SPEC-0282: Repair attempt after-refusal history and retry budget

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

- #8726 (repair-attempt refusal history and retry budget)
- #8766 (the false-actionable recommendation class this spec keeps closed)

Linked PRs:

- #8980 (history, count, retry budget, status and facade handling)

Support-tier impact:

- None. This bounds the agent repair loop's retries; it adds no analyzer
  claim. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No new public command. The attempt manifest, `ripr agent status` output and
  `schemas/ripr/repair-attempt.schema.json` gain optional fields.

## Problem

A repair attempt recorded only its latest after-phase refusal, and each new
refusal overwrote the last. An agent that kept editing without moving the gap
looked fresh on every retry, could loop without bound, and `ripr agent status`
kept offering the refused after command as the next step.

## Behavior

### Rule 1: refusals are counted and kept, bounded

Every recorded after-phase refusal increments `after_refusal_count` (never
capped) and appends to `after_refusal_history`, which keeps the 8 most recent
entries, oldest first. Both are excluded from the before commitment and kept by
a successful finish. A manifest that predates the count seeds both from its one
`last_after_refusal` before the first append.

### Rule 2: a spent budget stops the after phase

An attempt awaiting its edit that recorded 5 refusals refuses its after phase
without running it: a typed refusal, exit 3, recovery naming a new attempt. The
stop is a state observation and is not recorded as another refusal, so it never
evicts real history.

### Rule 3: nothing offers the refused command

At a spent budget, `ripr agent status` (inventory and selected attempt, JSON and
Markdown) does not offer the after command. It recommends a new attempt only
when the shared repair-start gate admits the seam and otherwise names the
withheld start. `ripr repair` does not resume a spent attempt and prepares a
fresh one.

## Required Evidence

- Five unedited after phases record count 5 and growing history; the sixth
  refuses with exit 3 without changing the count, even after a gap-moving edit.
- Status at a spent budget reports `retry_budget_spent` and offers no command
  that names the spent attempt.
- `ripr repair` after the fifth refusal prepares a second attempt.
- A legacy manifest with only `last_after_refusal` counts one refusal.

## Non-Goals

- Changing what counts as a refusal or the refusal text bound.
- Any claim about runtime mutation outcomes.

## Acceptance Examples

1. Refusals 1 to 5 report counts 1 to 5; the 6th invocation exits 3 and leaves
   the count at 5.
2. Status after refusal 5 names `retry_budget_spent` and a fresh-start route.
3. A repeated `ripr repair` after refusal 5 starts a new attempt.

## Test Mapping

- `crates/ripr/tests/cli_smoke.rs`: the budget, history and status example.
- `crates/ripr/tests/task_first_facade.rs`: the fresh-attempt example.
- `crates/ripr/src/app/task_first.rs` and
  `crates/ripr/src/app/repair_attempt/mod.rs`: reuse exclusion and the legacy
  seed.

## Implementation Mapping

- `crates/ripr/src/app/repair_attempt/mod.rs`: count, history, budget and
  validation.
- `crates/ripr/src/cli/commands/agent.rs`: the after-phase stop.
- `crates/ripr/src/app/agent_status.rs`: disposition and next command.
- `crates/ripr/src/app/task_first.rs`: attempt reuse.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
