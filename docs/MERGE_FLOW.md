# Merge flow for swarm PRs

How many agent-authored PRs reach `main` without stepping on each other. The
project's shared `/mnt/project-files/merge-queue/status.md` keeps an advisory
order and the merge log; this page is the rule set behind it.

## Who merges

The thread that owns a PR merges it with a protected squash merge once:

1. required CI is green on the current head;
2. `review-pr` on the exact published head returns `REVIEW_READY`, and a
   directed agentic review from several angles (for example correctness,
   factual accuracy against current source, tooling and policy effects,
   adversarial use) found nothing blocking. The reviewers run separately from
   the authoring pass, and a review-of-record comment on the PR names the head
   SHA, the angles, the findings, and how each was resolved. A GitHub approval
   is not required;
3. review threads are resolved, or answered with source-backed evidence;
4. the head merges cleanly into current `main`.

A PR needing a settings, ruleset, release, or publication change waits for the
repository owner. Protection is never weakened to clear a merge.

## Order

`status.md` may keep a recommended order. It is advice, not a merge gate, and
a thread does not need to consult it before merging. When kept, it comes from
local `git merge-tree` runs of open PR heads against `main`:

- ready and conflict-free first;
- a PR that adds shared plumbing (a new `xtask` command, a changed type or
  signature) before the PRs that rely on neighbouring code;
- PRs with conflicts after the ones they conflict with, so the owner rebases
  once.

## Conflicts

- A behind-only branch normally needs no update: PRs have merged while
  behind `main`. `.github/settings.yml` declares `strict: true` for the
  required check, but the live ruleset could not be read to confirm it, so if
  the server rejects a merge as out of date, update the branch and re-run.
- Conflict repair belongs to the owning thread. Nobody pushes to another
  thread's branch.
- Changelog entries are separate files under `changelog.d/`, so they should not
  conflict. See `changelog.d/README.md`.
- If a rebase drops or rewrites a hunk, keep `main`'s version of any file
  another PR has already fixed; do not carry an old copy of it.

## Required check only on the merge-ready request (#4986)

Cutover: the same required check is also earned by a merge-queue entry
(`merge_group`, see [CI](CI.md#merge-queue-request-cutover)). Once the queue is
enabled for `main`, add PRs to the queue instead of applying `merge-ready`.

Before the queue is enabled for `main` (pre-cutover flow), `Ripr Rust Small Result`
runs only when the `merge-ready` label is applied to a PR. Ready is review state and requests nothing; any other label posts only
`Merge-ready Nonrequest`.

- Open PRs as Draft and apply the shared
  [Draft to Ready handoff](PR_AUTOMATION.md#draft-to-ready-handoff) before
  requesting required qualification with `merge-ready`.
- If the current head lacks its required check after a push following the
  request, refresh affected proof and review, then remove and re-apply
  `merge-ready`. Preserve a queued or running `routed-rust.yml` run on the
  unchanged head; re-applying the label cancels it.
- Re-check auto-merge against the current exact-head prerequisites after a
  transition.
- Labels such as `full-ci` must be present when `merge-ready` is applied; add them first.
- Do not `workflow_dispatch` `routed-rust.yml` on a PR branch (#5394).

## Combined-tree check

Branch protection does not test the combined tree, so two individually green
PRs can leave `main` failing to compile (#5062 and #5109 did). After merging a
PR that changes a shared type, field, or function signature, run on the new
`main`:

```bash
cargo test -p ripr --lib --no-run
```

If it fails, say so in the thread and `status.md` before anything else merges.

## One line per merge

After each merge, append one line to the "Merged" list in `status.md`: PR
number, title, and the merge commit. Then recheck the remaining PRs against the
new `main`.

## Gaps and bot findings

A gap found while reviewing but not fixed in the PR becomes a ripr-swarm issue
(search for a duplicate first). Leftover bot nits are fixed in the PR or listed
in an issue; an automatic "addressed" label does not show a repair landed.
