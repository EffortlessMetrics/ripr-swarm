# PR convergence runbook

The mechanics of carrying one open PR through qualification and merge in this
repository: what fires the required check, what the label event actually tests,
which guards run before every push, and how to converge, reconcile and hand
over. Every rule here was learned by paying for its absence; the incident
records live in [LEARNINGS](../LEARNINGS.md) and the label-request sequence in
[PR automation](../PR_AUTOMATION.md#draft-to-ready-handoff). For local proof
and shell discipline see [validation](validation.md).

## Qualification: what fires the required check

Branch protection requires exactly one context: `Ripr Rust Small Result`.

That context is earned only by the `merge-ready` label event or by a merge-queue
entry (`routed-rust.yml:4-20`). `ready_for_review` requests nothing, and any
other label event still runs the workflow but names its result job
`Merge-ready Nonrequest` — a skipped required job reports success, so the
required context must be earned by a merge-ready run or stay absent
(`routed-rust.yml:472`). A branch or tag `workflow_dispatch` is diagnostic-only:
the result job fails itself on purpose
(`routed-rust.yml:621-624`), so it can never produce a passing required check
on a PR head.

The aggregate result job re-binds live eligibility to the admission recorded at
the start of the run. Admission writes the request as
`GITHUB_RUN_ID-GITHUB_RUN_ATTEMPT` (`routed-rust.yml:294`), and the recheck
requires that exact string to equal the live run and attempt
(`routed-rust.yml:486`, `routed-rust.yml:499`). An in-place rerun of failed
jobs runs under a new attempt while the succeeded admission step keeps its
original output, so a rerun can structurally never pass. To converge a stale or
failed qualification, remove and re-apply `merge-ready` for a fresh run on the
exact head; do not re-apply the label on an unchanged head while a required run
is queued or running, because it cancels that run.

## Merge refs: what the label event actually tests

A `CONFLICTING` PR has no `refs/pull/<N>/merge`. A `merge-ready` label applied
to a conflicting PR produces no usable qualification subject — the run dies
without posting a fresh required check. Resolve conflicts first, then re-fire
the label.

Label events can also reuse a stale merge ref built against an older base:
GitHub does not recompute the ref on a draft-to-ready route or a re-route.
Pushing to the branch (a synchronize event) forces a fresh ref, so after main
moves under a PR, merge main into the branch and push instead of toggling the
label alone. Distrust a re-routed gate until a fetched
`refs/pull/<N>/merge` shows the expected base as an ancestor
([LEARNINGS: combined-tree rules](../LEARNINGS.md)).

## Writer guards before every push

- Judge another writer's activity from the commits API, never from `updatedAt`
  on the PR: bot sweeps and label churn pollute the timestamp.
- Treat four hours as the activity window for convergence ownership.
- Run `git ls-remote origin refs/heads/<branch>` before every push; it must
  equal the head you recorded. A mismatch means someone moved the branch —
  fetch and reconcile, never force.
- An empty ls-remote means the branch was deleted. Treat the PR as moved, not
  as vacant: never push to a recreated branch name.

## Merge mechanics

Merge with the normal protected squash (`gh pr merge --squash`). The protection
here is one required check with `strict: false`, so a behind-only branch stays
valid and an old-head green check does not expire.

A `BLOCKED` merge state while the required check is green and threads are
resolved is usually GitHub's recompute lag, not a policy wall. The documented
route is `gh pr merge --squash --auto --delete-branch`, which queues the merge
until the state settles ([LEARNINGS: BLOCKED is recompute lag](../LEARNINGS.md)).
When the CLI still refuses, the REST endpoint is the sanctioned fallback:

```bash
gh api --method PUT repos/EffortlessMetrics/ripr-swarm/pulls/<N>/merge \
  -f merge_method=squash
```

After any `gh` error, verify the actual PR and merge state through the API
before retrying: a non-zero merge exit is ambiguous, and a merge that actually
landed makes a blind retry a second-merge error
([LEARNINGS: merge-then-cleanup](../LEARNINGS.md)).

## Reconciliation traps

GitHub's close-keyword matcher ignores negation. The claim-boundary sentence in
PR #11084 — "It does not … or close #4519." — auto-closed issue #4519 when the
PR merged; the fix was to reopen the issue and append to the owned comment.
Never write a close keyword in a PR body or comment unless you intend the
close, including inside sentences that say the opposite.

## Ownership before working a PR

Before pushing, relabeling or editing a PR you did not open, read its commits
API for another convergence actor inside the activity window. Both fleets work
the same repository, and one wave raced eleven of fourteen lanes this way.
When a race is confirmed, yield: preserve proven work under new branch refs,
leave a handback comment that names the head, evidence and resume condition,
and move to another claim. Deleting or overwriting a raced candidate is not a
repair.

## Platform discipline

Never bless goldens on Windows: separator-sensitive hashes make
Windows-blessed goldens fail Linux CI ([LEARNINGS: separator
normalization](../LEARNINGS.md)), and the Windows advisory lane deliberately
carries no golden gates ([CI strategy](../CI.md)). Use WSL Ubuntu with the
pinned toolchain as the local Linux proof route when Linux-shaped evidence is
needed before push; hosted qualification remains the arbiter. WSL is not
equivalent evidence for a native Windows claim either — each platform proves
its own branch ([validation](validation.md)).
