---
name: ripr-repair-workflow
description: Use RIPR static exposure evidence to add the missing discriminating test for one changed behavior, then prove it with separately executed tests.
---

# RIPR repair workflow

RIPR answers one question: for the behavior changed in this diff, do the
current tests appear to contain a discriminator that would notice if that
behavior were wrong? It is static evidence only. Never describe a RIPR
result as `killed`, `survived`, `untested`, `proven`, or `adequate`, and <!-- ripr-allow: static-language: names the prohibited terms to forbid them -->
never promote a static class (`exposed`, `weakly_exposed`,
`reachable_unrevealed`, `no_static_path`, `infection_unknown`,
`propagation_unknown`, `static_unknown`) into a runtime outcome.

## Authority boundary

The `ripr` MCP server is read-only: it never edits source, runs tests, or
executes verification. `ripr_prepare_repair` creates only an in-memory
transaction and authorizes nothing. All edits and test executions are
yours, in the open, through the normal file and shell tools.

Prerequisite: an MCP-capable `ripr` binary. Released `ripr 0.10.0` has
no `mcp` command (it exits 2, `unknown command "mcp"`), so its server
never starts and no `ripr_*` tools register. If discovery shows no
`ripr_*` tools, check `ripr --version` and report the version floor
instead of proceeding without evidence.

## Workflow

1. Orient with `ripr_workspace_status`. Record the repository root, the
   authority block (edit and execution authority must read `none` for the
   server), and any limitation. If the root is unavailable, stop and
   report the typed failure; do not guess a root.
2. Run `ripr_refresh` once. Use the committed default unless the user
   explicitly wants saved worktree edits analyzed, in which case pass
   `{"source": "worktree"}` and say so. Record the snapshot id and whether
   `fresh_for_working_tree` is true. An empty scope is not an all-clear:
   read `empty_scope_hint` when present.
3. Page `ripr_list_gaps` with `repair_ready: true` first. Work from the
   bounded summaries only; do not fetch every gap's evidence. Pick one
   item with the user (or the first ready item when the user asked for
   the top gap) and record its `canonical_id`, class, file, and line.
4. Read that one item with `ripr_get_gap`. Identify the changed behavior,
   the missing discriminator, and the sink the oracle must observe. Reach
   plus a strong oracle is not `exposed` unless the oracle observes the
   changed sink; a test that only checks a neighboring value is the
   incorrect-repair control, not a repair.
5. Check readiness before editing: `ripr_get_top_limitation` for the
   earliest limiting fact, then `ripr_get_repair_card` (or
   `ripr_prepare_repair` when a session transaction is wanted). If
   `repair_packet_ready` is false, report the typed reason and stop. A
   wrong actionable signal is worse than a missed advisory one.
6. Make one focused test edit: add or strengthen the test so it asserts
   the exact changed behavior at its boundary (the value, error, or
   ordering the diff alters). Do not edit production code to make the
   evidence look better, and do not re-bless expectations you have not
   independently derived from the contract.
7. Prove it by execution, separately from the static result. Run the
   focused test command yourself (for Rust, `cargo test` on the affected
   test). Then run the strongest available wrong-implementation control:
   temporarily break the changed behavior (or run the new test against
   the pre-change behavior) and confirm the new test fails. Restore the
   correct behavior afterward. A test that passes under both behaviors
   does not discriminate; say so and iterate or stop.
8. Re-run `ripr_refresh` and re-read the same `canonical_id` with
   `ripr_get_gap`. The step 6 test edit is normally uncommitted, so use
   `{"source": "worktree"}` here unless the edit was committed (or step
   2 already used worktree source). A committed refresh cannot see an
   uncommitted edit; its after snapshot would repeat the before class.
   Report
   two separate verdicts, never merged:
   - Static verdict: the before/after exposure class and readiness from
     RIPR for the exact snapshot ids.
   - Executed verdict: the actual test command, its exit status, and the
     wrong-implementation control result.
   A better static class without a discriminating executed test is not a
   completed repair.
9. For a durable receipt, use the CLI outside MCP:
   `ripr agent repair --root . --seam-id <seam-id> --phase before`,
   make the same focused edit, then
   `ripr agent repair --root . --attempt <repair-attempt-id> --phase after`.
   Preserve the printed attempt id across sessions. The `verify` phase is
   separately authorized and trust-bound; do not run it unless the user
   explicitly authorizes verification with an operator identity.

## Token and memory discipline

- Summaries before evidence: one `ripr_list_gaps` page, one
  `ripr_get_gap` at a time. Follow `continuation.next_page` instead of
  widening limits past the refused bound; a larger `limit` is refused,
  not clamped.
- Reuse the snapshot. Refresh only after an edit that changes the
  analyzed input; repeated reads of committed facts are identical and
  cost a new analysis each time you refresh without cause.
- Keep outputs bounded: quote the class, ids, and verdict lines, not
  whole evidence documents, in your replies.

## Refusals

Typed MCP failures (`stale_snapshot`, `item_not_found`, `no_snapshot`,
`attempt_not_found`, `result_too_large`, `invalid_argument`) are answers,
not obstacles to route around. Follow the `recovery` route the failure
names, or report it and stop. Never fabricate a snapshot, attempt, or
receipt id.
