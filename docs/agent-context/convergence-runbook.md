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

Label events can reuse a merge ref built against an older base. Inspect the
actual job checkout, its parents and tree against the reviewed candidate and
required integration basis. Reconcile a material mismatch or conflict; main
moving alone does not require restacking or repeating unaffected proof. Do not
relabel a run's older checkout as execution of a newer combined tree
([LEARNINGS: combined-tree rules](../LEARNINGS.md)).

## Writer guards before every push

- Reconcile the current head, owner's disposition, active work or qualification
  and any accepted handoff. Commit author dates are object metadata, not push
  times; PR `updatedAt` also includes bot and label activity. A claim aged four
  hours prompts inspection, never automatic takeover; inspect sooner when other
  evidence warrants it. Preserve unknown custody.
- Run `git ls-remote origin refs/heads/<branch>` before every push; it must
  equal the head you recorded. A mismatch means someone moved the branch —
  fetch and reconcile, never force.
- An empty ls-remote means the branch was deleted. Treat the PR as moved, not
  as vacant: never push to a recreated branch name.

## Merge mechanics

After current-head substantive review, addressed findings, applicable proof and
all live required checks are established, use an ordinary protected squash with
the exact current PR head as the expected SHA. Confirm that the current required
`Ripr Rust Small Result` is successful for the applicable qualification and
required app, and assess all other required checks and review obligations. Read
the current requirements rather than assuming a fixed protection configuration.
A behind-only branch does not itself invalidate unaffected proof; a green
old-head check does not qualify a new head. Do not arm auto-merge or bypass a
rule. For example, after independently establishing those conditions:

```bash
gh api --method PUT "repos/EffortlessMetrics/ripr-swarm/pulls/$pr_number/merge" \
  -f merge_method=squash -f sha="$expected_head"
```

Set `expected_head` to the verified full current PR head; do not omit it or use
an unguarded fallback. The API's expected-SHA check protects the head, but the
review and check observations are not atomic with the mutation. Native
protection and final judgment remain necessary.

`BLOCKED` is a state to investigate, not proof of recompute lag or permission
to enqueue. Reconcile current review, checks and applicable rules; preserve any
actual refusal. If the head moves, refresh the review and affected proof before
another guarded operation.

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

Before pushing, relabeling or editing a PR you did not open, reconcile its
current head, owner disposition, active work and accepted handoff. Both fleets
work the same repository, and one wave raced eleven of fourteen lanes this way.
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
