# Merge flow for swarm PRs

How many agent-authored PRs reach `main` without stepping on each other. The
live queue is `/mnt/project-files/merge-queue/status.md` in the project's
shared folder; this page is the rule set behind it.

## Who merges

The thread that owns a PR merges it with a protected squash merge once:

1. required CI is green on the current head;
2. an exact-head review by a reviewer other than the author finds nothing
   blocking (correctness, compiles against current `main`, refusals say why and
   what to do next);
3. review threads are resolved, or answered with source-backed evidence;
4. `status.md` does not list an unresolved conflict or earlier PR for it.

A PR needing a settings, ruleset, release, or publication change waits for the
repository owner. Protection is never weakened to clear a merge.

## Order

`status.md` keeps a recommended order. It is advice, built from local
`git merge-tree` runs of each PR head against `main` and against each other:

- ready and conflict-free first;
- a PR that adds shared plumbing (a new `xtask` command, a changed type or
  signature) before the PRs that rely on neighbouring code;
- PRs with conflicts after the ones they conflict with, so the owner rebases
  once.

## Conflicts

- A behind-only branch needs no update. Squash merge does not require an
  up-to-date branch.
- Conflict repair belongs to the owning thread. Nobody pushes to another
  thread's branch.
- Changelog entries are separate files under `changelog.d/`, so they should not
  conflict. See `changelog.d/README.md`.
- If a rebase drops or rewrites a hunk, keep `main`'s version of any file
  another PR has already fixed; do not carry an old copy of it.

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
