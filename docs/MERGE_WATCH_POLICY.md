# Merge Freshness And Watcher Policy

This document defines how maintainers and agents should watch a PR from "checks
are running" to "safe to merge" without turning merge readiness into repeated
manual polling or unnecessary CI restarts.

The policy is advisory. It does not change branch protection, auto-merge,
merge-queue settings, required checks, workflows, comments, approvals, or branch
updates.

## Standard Packet

Start with the repo-ops packets:

```bash
cargo xtask cockpit
cargo xtask gh-pr-status --pr <number>
```

`cockpit` gives the board-level state. `gh-pr-status --pr <number>` gives the
per-PR advisory packet, including merge state, behind-main state, visible check
status, review state, Droid status, and the suggested next action. Required-check
completeness remains unknown; the packet does not read protection settings or
grant merge authority.

If GitHub CLI's GraphQL path is rate-limited or stale, use a read-only REST
fallback for inspection:

```bash
gh api repos/EffortlessMetrics/ripr-swarm/pulls/<number>
gh api repos/EffortlessMetrics/ripr-swarm/commits/<head-sha>/check-runs
```

Do not use the fallback to mutate the PR. It is only a status source.

## Watch Cadence

Avoid short-loop polling. It wastes rate limit, makes logs noisy, and rarely
changes the next safe action.

Use the shared [remote wait discipline](agent-context/remote-wait-discipline.md).
Retain the exact run/attempt and owning lead through a normal wait. An unchanged
snapshot is not completion; inspect actual failures, queue delays or missing
evidence before deciding whether a retry is warranted.

Do not poll every few seconds unless you are debugging the status tool itself.

## Safe Next Actions

Use the packet's recommendation as the default, then apply these rules:

| Action | Use when | Do not use when |
| --- | --- | --- |
| `wait` | required checks are pending and the branch is otherwise current | a required check already failed or GitHub says the branch cannot merge |
| `inspect failure` | a required check failed, was cancelled unexpectedly, or is missing | the only issue is that optional advisory checks are still running |
| `inspect integration` | a non-Draft PR is behind main after existing review/check waits | Draft, required review or running checks still have their own next transition |
| `inspect final merge gates` | the advisory snapshot has no earlier next action; inspect exact-head substantive review, checks, conversations and integration | treating a clean snapshot or unknown check completeness as permission to merge |
| `stop for judgment` | required evidence, review disposition, or candidate selection is unclear | the issue is deterministic cleanup that the repo already reports |

`inspect integration` asks whether there is an actual conflict, changed explicit
prerequisite, material combined-tree failure or applicable exact-base policy.
The `BEHIND` observation alone establishes none of these and authorizes no
branch update. Keep useful proof and review for unchanged source, inputs and
contracts; refresh affected dimensions if an integration repair changes them.

## Branch Freshness

When `main` moves while checks are running, do not restart CI reflexively. Let
active required jobs finish if their result will still be useful for repair.
Refresh only for an actual conflict, changed explicit prerequisite, material
combined-tree failure or applicable exact-base policy. A cancelled or stale
run needs its cause inspected; it does not by itself require branch movement.

Before pushing your own branch update:

1. Fetch current `origin/main`.
2. Re-read the remote PR head.
3. If someone else updated the branch, inspect and fast-forward or merge it
   locally before editing.
4. Rerun the narrow local validation that matches your update.
5. Push only your reviewed branch state.

Never force-push over external branch movement unless the maintainer explicitly
asks for that operation.

## Droid And Advisory Checks

Droid and other advisory checks are review signals unless branch protection or a
repo policy makes them required for the PR. Do not block a ready PR only because
an advisory check is still running. Do inspect advisory failures when they touch
the changed surface or identify a concrete regression risk.

If required-check completeness is unavailable, record that uncertainty. Inspect
ordinary PR-visible checks and current repository policy without reading
protection or ruleset settings. Unknown or inaccessible evidence is not passed.

## Merge Execution

The advisory packet starts the final inspection; it cannot finish it. Follow the
applicable `review-pr` and `finish-pr` procedures for current published-head
substantive review, required proof, conversation disposition and
integration judgment before a normal protected expected-head squash merge.

If a local detached or side worktree cannot merge a PR because the branch is
checked out elsewhere, merge from the primary checkout or use an explicit
GitHub-hosted merge path after re-reading the remote PR state. Do not move local
branch refs to work around that limitation.

Do not arm routine auto-merge or a merge queue. GitHub enforces the normal merge
requirements; report a concrete rejection without guessing its cause, bypassing
it, or changing settings.

## Current-state capture

`cargo xtask merge-queue capture` is the MQ0 read-only receipt for the current
merge/protection surface. It writes desired settings, live observation,
apply-route capability, and rollback identity as separate facts, then ends in
exactly one of `READY_FOR_DESIRED_STATE`, `CAPABILITY_BLOCKED`, `NOT_PROVEN`,
or `DRIFT_REPAIR_REQUIRED`.

This specialized settings-audit surface is not part of ordinary PR progression;
use live capture only within a separately authorized settings-audit task.

```bash
cargo xtask merge-queue capture --repo EffortlessMetrics/ripr-swarm --out target/ripr/reports/merge-queue
```

Ordinary tests use a sanitized `--input` snapshot and do not touch the network.
The command never applies settings, rulesets, workflow changes, or queue
dogfood. A checked-in `.github/settings.yml` file is desired-state evidence,
not live read-back.

## Cleanup

After a PR branch is merged or abandoned, clean only the worktree created for
that task. If the task used a scoped `CARGO_TARGET_DIR`, remove that exact
directory after verifying it is under the intended scratch root. If it used a
task-local default `target/`, `cargo clean` is acceptable.

PowerShell example:

```powershell
Remove-Item -LiteralPath <task-cargo-target-dir> -Recurse -Force
git worktree remove <task-worktree>
```

Do not remove unrelated worktrees, delete remote branches, or clean shared
build directories unless the maintainer explicitly asks.
