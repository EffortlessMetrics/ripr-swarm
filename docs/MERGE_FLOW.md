# Merge flow for swarm PRs

How many agent-authored PRs reach `main` without stepping on each other. The
project's shared `/mnt/project-files/merge-queue/status.md` keeps an advisory
order and the merge log; this page is the rule set behind it.

## Who merges

The thread that owns a PR adds it to the merge queue once the items below hold;
the queue runs required CI on the combined tree and performs the protected
squash merge only if that run is green:

1. local proof for the change is recorded on the PR (hosted CI runs only in the
   queue);
2. `review-pr` on the exact published head returns `REVIEW_READY`, and a
   directed agentic review from several angles (for example correctness,
   factual accuracy against current source, tooling and policy effects,
   adversarial use) found nothing blocking. The reviewers run separately from
   the authoring pass, and a review-of-record comment on the PR names the head
   SHA, the angles, the findings, and how each was resolved. A GitHub approval
   is not required;
3. review threads are resolved, or answered with source-backed evidence;
4. the head merges cleanly into current `main`.

After queueing, the owner watches the queue entry; a failed entry is repaired
and re-queued.

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

## Required check only in the merge queue (#4986)

`Ripr Rust Small Result` runs only for a merge-queue entry (`merge_group`),
against the exact combined tree that will land. Ready and labels are review
state and request nothing. There is one hosted run per landing.

- Open PRs as Draft and apply the shared
  [Draft to Ready handoff](PR_AUTOMATION.md#draft-to-ready-handoff) before
  adding the PR to the merge queue.
- A push to a queued PR dequeues it. Refresh affected proof and review, then
  queue the new head. Do not dequeue and re-queue an unchanged head with a
  running queue entry; that discards the run.
- A failing queue run drops only that entry. Fix the PR (a red result may be a
  real interaction with a sibling that landed first) and re-queue it.
- Labels such as `full-ci` must be present when the PR is queued; add them first.
- Do not `workflow_dispatch` `routed-rust.yml` on a PR branch (#5394).

## Combined-tree check

Before the merge queue, branch protection did not test the combined tree, so
two individually green PRs could leave `main` failing to compile (#5062 and
#5109 did). The queue run now qualifies the combined tree, but keep this check
for merges that bypassed it. After merging a
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
