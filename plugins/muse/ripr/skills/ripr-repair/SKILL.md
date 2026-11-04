---
name: ripr-repair
description: Use RIPR static exposure evidence to close one test-exposure gap — prepare a durable repair attempt, add the missing discriminating test, and verify with separately executed tests. Use when a user asks whether tests are strong enough for a change or to fix what they miss.
---

# RIPR repair

RIPR answers one question: for the behavior changed in this diff, do the
current tests appear to contain a discriminator that would notice if
that behavior were wrong? It is static evidence only. Never describe a RIPR result as `killed`, `survived`, `untested`, `proven`, or `adequate`, <!-- ripr-allow: static-language: names the prohibited terms to forbid them -->
and never promote a static class (`exposed`, `weakly_exposed`,
`reachable_unrevealed`, `no_static_path`, `infection_unknown`,
`propagation_unknown`, `static_unknown`) into a runtime outcome.

## Ownership

| Component | Owns |
| --- | --- |
| Muse | Reasoning, choosing the action, editing code, running authorized tests |
| RIPR MCP | Read-only analysis, findings, limitations, readiness, evidence |
| RIPR CLI | Durable repair-attempt preparation and static after-phase recording |
| This skill | Discovery, workflow guidance, bounded orchestration, failure handling |

The MCP server never edits, runs tests, or executes verification.
`ripr_prepare_repair` is an in-memory readiness transaction; it does not
replace the durable CLI attempt below and authorizes nothing.

Prerequisite: an MCP-capable `ripr` binary on PATH (current 0.11+;
released 0.10.0 has no `mcp` command). If no `ripr_*` tools are live,
run the `ripr-setup` workflow and stop with its outcome.

## Workflow

1. Establish scope. `ripr_workspace_status`, then `ripr_refresh`. Use
   `{"source": "worktree"}` when evaluating saved working-tree edits;
   the ordinary default analyzes the committed diff and can miss tests
   just written in the worktree. Record the snapshot id and whether
   `fresh_for_working_tree` is true. An empty scope is not an all-clear.
2. Choose one gap. Page `ripr_list_gaps` with `repair_ready: true` from
   the bounded summaries, inspect one candidate with `ripr_get_gap`,
   and record its `canonical_id`, class, file, and line. Identify the
   changed behavior, the missing discriminator, and the sink the oracle
   must observe. Reach plus a strong oracle is not `exposed` unless the
   oracle observes the changed sink; a test that only checks a
   neighboring value is the incorrect-repair control, not a repair.
3. Establish whether repair is justified. `ripr_get_top_limitation` and
   `ripr_get_repair_card`. If `repair_packet_ready` is false or a
   limitation blocks the seam, explain the exact typed stop and end. A
   wrong actionable signal is worse than a missed advisory one.
4. Prepare a durable attempt with the CLI, using the seam identity from
   the repair card's receipt binding — the seam ID it names, not an
   inferred equivalent of the canonical gap ID:

   ```text
   ripr agent repair --root . --seam-id <seam-id> --phase before --json
   ```

   Preserve the returned repair-attempt ID, expected test file, allowed
   edit scope, manifest, and exact after command (also in
   `.ripr/repair-attempts/<repair-attempt-id>.json`). Never invent an
   attempt, snapshot, or receipt ID; if preparation fails, preserve the
   state and report the exact refusal.
5. Make a focused test edit under normal authorization: assert the exact
   changed behavior at its boundary (the value, error, or ordering the
   diff alters). Do not expand into unrelated production changes, and
   do not re-bless expectations you have not independently derived from
   the contract.
6. Run the relevant tests separately (`cargo test` for Rust). Test
   execution is your action, never an MCP capability. Then run the
   strongest wrong-implementation control: the same test against the
   broken or pre-change behavior must fail. Restore correct behavior.
   A test that passes under both behaviors does not discriminate; say
   so and iterate or stop — never record it as an improvement.
7. Finish the attempt:
   `ripr agent repair --root . --attempt <repair-attempt-id> --phase after`.
   The `verify` phase is separately authorized and trust-bound; run it
   only when the user explicitly authorizes verification with an
   operator identity.
8. Refresh against the saved worktree (`ripr_refresh` with
   `{"source": "worktree"}` — the test edit is uncommitted and a
   committed refresh cannot see it), re-read the same item, and compare
   the newly observed static evidence with the original gap and the
   durable receipt.
9. Report both proof axes independently, compactly: the gap and missing
   discriminator; the action taken; execution (original-behavior and
   known-wrong-variant results); the RIPR after-phase (class movement,
   receipt/attempt identity, snapshot IDs); limitations, including that
   these observations do not establish general test-suite sufficiency.

## Token discipline

Summaries before evidence: one `ripr_list_gaps` page, one `ripr_get_gap`
at a time, quote classes and IDs rather than whole evidence documents.
Follow `continuation.next_page` instead of widening limits past the
refused bound (a larger `limit` is refused, not clamped). Refresh only
after an edit that changes the analyzed input.

## Refusals

Typed failures (`stale_snapshot`, `item_not_found`, `no_snapshot`,
`attempt_not_found`, `result_too_large`, `invalid_argument`,
`repair_not_ready`, base-trust stop reasons) are answers. Follow the
`recovery` route the failure names, or report it and stop. If a step
fails mid-attempt, preserve the attempt and explain the exact refusal;
do not invent a green completion. A better static class without a
discriminating executed test is not a completed repair.
