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

1. Establish scope. Call `ripr_workspace_status` without arguments and
   confirm the session's startup-bound root; use `ripr-setup` if unsure.
   Pass `source` explicitly on every `ripr_refresh`: `"worktree"` for
   relevant saved tracked edits, or `"committed"` for an intentionally
   committed diff. Repeat any explicitly selected `base` on every
   refresh; neither option is sticky. Record the requested scope, the
   returned source/base where disclosed, snapshot id, and
   `fresh_for_working_tree`. Bind subsequent evidence reads to that
   snapshot. A failed refresh's last-known-good snapshot is not new
   evidence, and excluded edits are not analyzed. An empty scope is not
   an all-clear: surface `empty_scope_hint` when present.
2. Choose one gap. Page `ripr_list_gaps` with `repair_ready: true` from
   the bounded summaries, inspect one candidate with `ripr_get_gap`,
   and record its `canonical_id`, class, file, and line. Identify the
   changed behavior, the missing discriminator, and the sink the oracle
   must observe. Reach plus a strong oracle is not `exposed` unless the
   oracle observes the changed sink; a test that only checks a
   neighboring value is the incorrect-repair control, not a repair.
3. Establish whether repair is justified. `ripr_get_top_limitation` and
   `ripr_get_repair_card` must refer to the selected item and snapshot.
   Require the gap's `readiness.repair_packet_ready` and the card's
   `readiness.repair_ready` to be true. Otherwise, or when a limitation
   blocks the seam, explain the exact typed stop and end. A wrong
   actionable signal is worse than a missed advisory one.
4. Prepare a durable attempt with the CLI, using the repair card's
   `card.subject.seam_id`. Keep this seam ID distinct from the canonical
   gap ID:

   ```text
   ripr agent repair --root . --seam-id <seam-id> --phase before --json
   ```

   Run in the same established repository root and check that the
   returned packet describes the selected subject. Preserve the returned
   repair-attempt ID, expected test file, allowed edit scope, manifest
   path, and exact next command. The default manifest is
   `target/ripr/repair-attempts/<repair-attempt-id>/attempt.json`; use the
   returned location rather than guessing a path or substituting an
   in-memory MCP transaction ID. Stop before editing if preparation
   refuses or reports `baseline_ambiguous: true`. Rust preparation
   requires an effective ignore rule for all of `target/`; report a
   missing rule for authorized resolution before preparation, not a
   silent configuration change during the attempt. Never invent an
   attempt, snapshot, or receipt ID, or rewrite retained before evidence.
5. Make a focused test edit under normal authorization: assert the exact
   changed behavior at its boundary (the value, error, or ordering the
   diff alters). Do not expand into unrelated production changes, and
   do not re-bless expectations you have not independently derived from
   the contract. Stay within the returned allowed edit surface and
   preserve pre-existing changes. A wholly untracked new test file is
   excluded even from worktree analysis. If that path and its Git-index
   change are authorized, include only it with
   `git add -N -- <new-test-path>`; preserve the existing index and report
   the action. Otherwise disclose the exclusion and do not claim the
   comparison covers the new test. Never stage all files or widen the
   retained edit scope to make the attempt pass.
6. Run the relevant tests under the user's execution authority (`cargo
   test` for Rust); record the exact test selection, command, exit status,
   and a nonempty test result. Execution is never an MCP capability.
   For the wrong-implementation control, use an isolated disposable
   copy/worktree reproducing the relevant current source and test bytes,
   including saved edits. Never mutate the user's production files or
   index for this control. Require the exact new or strengthened test to
   pass on correct behavior and fail at the intended assertion on one
   wrong/pre-change behavior. An unrelated suite failure, compile
   failure, or zero selected tests is not that discriminator. Restore
   the control copy's captured correct bytes, check byte-for-byte
   restoration, and rerun the same test. Confirm the control left the
   original worktree and index unchanged. Keep control files and captured
   output outside the repair root. A test that passes under both behaviors
   does not discriminate; iterate or report that result. If a safe
   authorized control is unavailable, record the missing evidence rather
   than forcing an unsupported route or inventing a completed repair.
7. Finish using the exact next command returned by preparation, with the
   same repair-attempt ID and any returned store argument. Its ordinary
   form is `ripr agent repair --root . --attempt <repair-attempt-id> --phase after`.
   Read the actual receipt state and limitations; a successful static
   after phase is not executed-test evidence. Preserve the CLI's shared
   admission rules: ordinary focused-test descendant commits are
   supported, while trust-bound Python attempts keep their exact-HEAD
   and authority requirements. The `verify` phase remains separately
   authorized and trust-bound; run only its supported route with the
   user's explicit verification authorization and operator identity.
8. Refresh against the repaired correct input. Pass
   `{"source": "worktree"}` explicitly whenever relevant saved edits
   remain, even after an earlier worktree refresh; choose explicit
   `"committed"` only for an intentionally committed input. Repeat the
   selected `base` from step 1; a changed resolved base is not the same
   comparison. Check the returned source/scope and
   `fresh_for_working_tree`, then re-read the same `canonical_id` bound
   to the new snapshot. Failed refreshes, excluded tests, stale snapshots,
   or missing items cannot establish a current before/after comparison.
   Compare the new static evidence with the original gap and durable
   receipt, without substituting a different item.
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
