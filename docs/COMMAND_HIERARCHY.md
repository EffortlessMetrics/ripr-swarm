# Public command hierarchy

This is the current human-facing task map for RIPR. It keeps the public entry
points distinct while the full typed command and workflow catalog is completed
under #1613.

| User task | Primary command | Boundary |
| --- | --- | --- |
| Diagnose setup | `ripr doctor` | Checks whether the workspace can produce evidence and gives bounded recovery. It is not required before every run. |
| Inspect one change | `ripr check --base origin/main` | Ordinary first value: analyze the selected diff and name the top gap or an honest no-action/limited state. |
| Adopt RIPR in a repository | `ripr pilot --root .` | Guided repository analysis and materialization. It is broader than the ordinary one-change check. |
| Repair one named gap | `ripr agent repair --seam-id <id> --phase before`, then `--attempt <repair-attempt-id> --phase after`; for a trust-bound Python attempt, continue with `--attempt <repair-attempt-id> --phase verify` and explicit authorization | RIPR owns the evidence plumbing and bounded verification. A human or external agent owns the focused test edit; execution and static movement remain separate observations. |
| Compose PR evidence | `ripr first-pr --root . --base origin/main --head HEAD` | Composes existing artifacts into the start-here packet. It does not run analysis or repair a gap. |
| Adopt advisory CI | `ripr init --ci github` | Writes the non-blocking GitHub workflow. Blocking policy remains a later explicit repository decision. |
| Inspect advanced commands | `ripr help --all` | Complete reference for policy, reports, compatibility, and operator surfaces. |

## Repair transaction

The ordinary repair sequence is:

```bash
ripr agent repair --root . --seam-id <seam-id> --phase before
# edit one focused test outside RIPR
ripr agent repair --root . --attempt <repair-attempt-id> --phase after
```

The before phase prints the repair-attempt ID and exact continuation command.
Keep that `--attempt` command for the after phase, including across sessions.
The seam ID selects the gap; the repair-attempt ID selects its prepared
transaction.

The `--seam-id ... --phase after` form remains a compatibility route and
requires exactly one awaiting attempt for that seam. Zero or multiple matches
fail closed. See [repair attempt identity](REPAIR_ATTEMPT.md) for the current
manifest, validation, and recovery contract.

For the governed Python lane, the transaction has a third, separately authorized
phase. Bind the accepted selection on `before`, reaffirm edit authorization on
`after`, then verify the same durable attempt:

```text
ripr agent repair --root . --seam-id <seam-id> --phase before --python-repair-trust-manifest <selection.json> --python-repair-trust-attempt <selection-attempt-id> --edit-authorized --edit-authority <operator-id>
# edit one focused test outside RIPR; retain the printed repair-attempt ID
ripr agent repair --root . --attempt <repair-attempt-id> --phase after --edit-authorized --edit-authority <operator-id>
ripr agent repair --root . --attempt <repair-attempt-id> --phase verify --verify-authorized --verify-authority <operator-id>
```

`verify` accepts only `--attempt`, not the seam compatibility selector. Its
`--verify-authorized` / `--verify-authority` pair must identify the same authority
that authorized the edit. Optional `--verify-rollback` requests restoration of
the applied edit after observation; inspect the rollback disposition rather
than assuming restoration. Trust-selection flags belong only on `before`.
See [repair attempt identity](REPAIR_ATTEMPT.md#governed-python-sequence) for
prerequisites, retained evidence, fail-closed recovery, and receipt boundaries.
The verify phase does not turn execution success into static improvement or
acceptance.

The lower-level `agent start`, `brief`, `packet`, `verify`, `verify-execute`,
`receipt`, `status`, and `review-summary` commands remain available for explicit
control, compatibility, and debugging. They are not the first-hour repair path.

## Drift rule

Top-level help, exhaustive help, the root README, Quickstart, agent help, and
editor onboarding should preserve the task boundaries above. Detailed flags
belong in per-command help rather than being copied into every document.

This document is descriptive guidance, not execution authority. #1613 will
replace prose-only coordination with a typed, schema-versioned command and
workflow catalog.

## Non-claims

This hierarchy does not rename or remove commands, add automatic edits, execute
mutation testing, strengthen gate authority, or prove real-repository usability.
RIPR remains static and advisory.
