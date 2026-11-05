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

1. Orient with `ripr_workspace_status` using no arguments. Record the
   returned root posture and identity, authority block (server edit and
   execution authority must read `none`), and limitations. The server
   root is fixed at startup from the Muse session's working directory;
   neither a tool argument nor a later shell `cd` retargets it. If the
   intended root is unavailable or cannot be established, stop. Start a
   fresh Muse session in the intended repository instead of guessing.
2. Choose the input scope and run `ripr_refresh` once. Pass `source`
   explicitly: `"committed"` for the committed diff, or `"worktree"` when
   the requested subject includes saved staged or unstaged edits. Carry
   the same explicit `base`, when one was selected, on every refresh;
   omit it only when deliberately using the default base. Neither option
   is sticky. Record the requested scope, returned snapshot source/base
   where disclosed, snapshot id, and `fresh_for_working_tree`. A failed
   refresh's last-known-good snapshot is not a new successful analysis.
   If the returned scope does not cover the intended edits, disclose the
   limitation and resolve it before repair. An empty scope is not an
   all-clear: read `empty_scope_hint` when present.
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
   earliest limiting fact, then `ripr_get_repair_card`, bound to the
   selected snapshot and `canonical_id`. Require the gap's
   `readiness.repair_packet_ready` and the card's `readiness.repair_ready`
   to be true; otherwise report the typed reason and stop. Preserve the
   returned seam id separately from the canonical gap id. A session-only
   `ripr_prepare_repair` may be useful, but is not a durable CLI attempt.
6. Decide whether this run needs a durable receipt **before any edit**.
   When requested, use the CLI outside MCP in the same established root:
   `ripr agent repair --root . --seam-id <returned-seam-id> --phase before --json`.
   Check that the returned packet describes the selected subject and
   allowed test-edit surface. Preserve its printed repair-attempt id,
   manifest path, and exact next command; do not substitute a session
   transaction id. Stop on refusal or `baseline_ambiguous: true` before
   editing. Rust preparation requires Git to ignore all of `target/`;
   report a missing rule for authorized resolution before preparation,
   not a configuration change during the attempt.
   Without a requested/available durable route, label the run as separate
   static and execution observations, not a durable repair receipt. Never
   reconstruct a before receipt after applying the repair.
7. Make one focused test edit within the allowed surface: add or
   strengthen the assertion for the exact changed behavior at its
   boundary (the value, error, or ordering the diff alters). Preserve
   pre-existing edits. Do not change production code to improve the
   evidence or re-bless expectations not derived from the contract.
   A new, wholly untracked test file is excluded even by worktree refresh.
   If the edit scope and Git-index change are authorized, include only
   that exact path with `git add -N -- <new-test-path>`; preserve the
   existing index and report this action. Otherwise disclose the exclusion
   and stop the claimed before/after comparison; never stage all files.
8. Execute the focused test under the user's existing execution authority.
   Record its exact selection, command, exit status, and a nonempty test
   result. For a wrong-implementation control, use an isolated disposable
   copy/worktree that reproduces the relevant current source and test
   bytes, including saved edits; never mutate the user's production files
   or index for this control. Require the exact new or strengthened test
   to pass on correct behavior and fail at the intended assertion on one
   wrong/pre-change behavior. An unrelated suite failure, compile failure,
   or zero selected tests is not the intended discriminator. Restore the
   control copy's captured correct bytes, check byte-for-byte restoration,
   and rerun the same test; confirm the control
   left the original worktree and index unchanged. A test that passes
   under both behaviors does not discriminate: iterate or report that
   result. If a safe authorized control is unavailable, record that
   missing evidence rather than inventing it or forcing an unsupported
   route. Keep control files and captured output outside the repair root.
9. If a durable before phase succeeded, use its exact printed next command
   with the same repair-attempt id to run `--phase after`. Read the actual
   receipt state and limitations; do not manufacture or edit retained
   evidence. The ordinary after phase is static receipt production, not
   test execution. Trust-bound Python edit/verification authority and HEAD
   rules remain separate: do not invent a selection manifest or operator
   identity. Run `--phase verify` only on its supported trust-bound route
   with the user's explicit verification authorization.
10. Re-run `ripr_refresh` against the repaired, correct worktree and re-read
    the same `canonical_id` with `ripr_get_gap`, bound to the new snapshot.
    Pass `{"source": "worktree"}` explicitly whenever relevant saved edits
    remain, even if an earlier refresh used it. Use explicit `"committed"`
    only when the intended input is committed, and repeat the selected
    `base` from step 2. A changed resolved base is not the same comparison.
    Check the returned source, scope, and
    `fresh_for_working_tree`; a failed refresh, excluded test, stale
    snapshot, or missing item cannot be reported as a current comparison.
    Report two separate verdicts, never merged:
    - Static verdict: before/after exposure class and readiness for the
      exact snapshot ids; durable attempt/receipt state only if produced.
    - Executed verdict: actual test selection, command, exit status, and
      wrong-implementation control result or its explicit limitation.
    A better static class alone is not a completed repair. Missing or
    non-discriminating execution evidence leaves that claim unestablished.

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
