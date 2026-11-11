# Merge flow for swarm PRs

## Live route (read back 2026-11-11)

`main` requires the native merge queue. Ruleset `main` (id 18845973) has had a
`merge_queue` rule since 2026-11-10T21:32Z, with one required status check,
`Ripr Rust Small Result`. A direct merge call is refused with HTTP 405
"Changes must be made through the merge queue" (#13899). The first queue
landing was #13794 on 2026-11-11.

A PR lands in this order:

1. Draft, focused proof, substantive review and joined repairs.
2. Apply `merge-ready` to the selected, reviewed head. This earns
   `Ripr Rust Small Result` on the PR; GitHub will not queue a PR without it.
   See the [label-request procedure](#installed-label-request-procedure).
3. Inspect the finished results and current findings on the exact head, resolve
   every review thread, and record the final `REVIEW_READY` judgment.
4. Enqueue the PR: "Merge when ready" in the UI, or squash auto-merge through
   the API. On a green PR this enters the queue at once; the timeline shows
   `added_to_merge_queue`.
5. The queue runs `routed-rust.yml` on `merge_group` for the queued commit (run
   title "Merge queue <sha>", branch `gh-readonly-queue/main/pr-<n>-<sha>`) and
   squashes the PR when that Result passes.
6. Read back the landed commit and reconcile the issue.

Enqueueing authorizes the landing. It is the final judgment, not a request for
tests, so finish steps 1 to 3 first. A session whose permissions refuse the
enqueue call records the PR number and qualified head on the PR and leaves the
enqueue to the queue owner; it does not look for another merge path.

Operating facts, each observed on 2026-11-11:

- Queue settings were squash, `ALLGREEN`, up to 3 entries built and merged
  together, and a 180-minute status check timeout (raised from 60 after code
  entries were ejected at the timeout, #4838). These are live settings and can
  change without an edit here.
- A docs-only entry spent about 15 to 30 minutes in the queue and a code entry
  about 50 to 80.
- A re-run of a failed `merge_group` run cannot pass, because the Result is
  bound to the original run attempt. Enqueue the PR again instead.
- An entry that fails or times out is removed by `github-merge-queue[bot]`.
  Diagnose the run before enqueueing again.
- Disabling auto-merge did not remove an entry that was already queued.

### Withdrawing a queued PR

A material finding after enqueue means the entry must leave the queue before its
run passes, or the PR lands. A disabled auto-merge readback is not containment.
Remove the entry itself: "Remove from queue" on the PR, or convert the PR to
Draft, which GitHub drops from the queue. Then read back
`removed_from_merge_queue` on the timeline and an open, unmerged PR. If the
session cannot remove the entry or cannot confirm it, that is an
integration-control blocker: say so on the PR at once and do not report the
merge as contained. The Result's late eligibility recheck may also refuse an
entry with an unresolved thread, but it is not atomic with the landing and does
not replace removal.

Two expensive runs per code PR (the label run, then the queue run) is the
current cost, not the goal. Steven selected queue-time qualification as the only
expensive run on October 21, 2026 under
[#4984](https://github.com/EffortlessMetrics/ripr-swarm/issues/4984#issuecomment-6308919863).
That step, replacing the label run with cheap PR admission, is not installed;
its design and cutover order are in the sections below (#4838, #4840). Admission
is still per PR: the queued commit must be a single squash commit of one PR on
the group's `base_sha` (see [CI](CI.md#merge-queue-request-cutover)); batching
needs per-PR admission (#8082). A file or issue edit does not change
protection.

## Who owns the change

The PR's owning lead carries review, repair, qualification and landing through
the applicable protected route without asking for routine repo permission again.
One writer owns each candidate through its authorized issue-to-merge outcome,
with evolving directed review stages inline in the main context. Optional
in-session subagents and advisory bots (for example `ub-review`; see `CI.md`) add evidence; they are not mandatory
reviewer identities or substitutes for owner review. Preserve actual required
reviews and checks. Routine repairs, evidence comments and finish-through do
not require parent payload approval; coordinate actual overlapping writers and
consequential changes. Do not
force-push another writer's branch, start a competing repair or make a second
candidate merely to keep an agent occupied.

Before final authorization:

1. Inspect the exact published head, focused proof and the intended acceptance.
   Complete substantive review from useful different angles and record its
   source identity, findings and evidence-backed dispositions.
2. Read submitted reviews, ordinary Conversation comments and all unresolved
   inline threads, including outdated ones. Reply with the repair or refutation,
   resolve only addressed threads and read back the resolution. A quota response,
   missing review or automatic addressed label is not a clean review.
3. Reconcile product fit, scope, dependencies, current main and any material new
   finding. A clean earlier review does not suppress later valid findings.
4. Preserve actual native enforcement. Administrative permission is not permission
   to bypass a required check. Settings cutover has its own qualified apply owner.

Useful review and local evidence can be complete before future hosted proof
exists. Missing future CI must not make requesting CI circular. Conversely,
source review is not a claim that required execution already passed.

## Installed label-request procedure

`Ripr Rust Small Result` is requested by applying `merge-ready`. Ready is review
state, not the request. Other labels produce `Merge-ready Nonrequest`, not the
required result.

- Open Drafts early and use the shared
  [Draft to Ready handoff](PR_AUTOMATION.md#draft-to-ready-handoff).
- Apply lane-selecting labels such as `full-ci` before `merge-ready`.
- A push after a request needs affected review/proof refreshed. Inspect current
  request and run state before any new request, using the shared handoff. Preserve
  a current-subject in-flight run; reapplying the label cancels it.
- Do not dispatch `routed-rust.yml` on a PR branch (#5394).
- Inspect the real check results and current findings, make the final integration
  judgment, then enqueue as in the [live route](#live-route-read-back-2026-11-11).
  Do not enqueue or arm auto-merge before that judgment: the queue lands the PR
  as soon as its run passes.

The existing docs-only route requires positive surface classification and the
actual docs gate. Skipped Rust work is not described as executed Rust proof.
Main being red does not justify repeating known-failing broad work; identify the
existing incident/repair owner and preserve unrelated candidate evidence.

## Selected native-queue procedure

The queue is active, so the enqueue, G proof and squash below are live. The
PR-side half of this target (cheap admission in place of the `merge-ready` run)
is not installed yet.

```text
cheap development / Draft publication
-> focused proof + substantive review + joined repairs
-> exact-head product/scope judgment and conditional merge authorization
-> native enqueue
-> one expensive required qualification on current merge-group G
-> native protected squash if required conditions remain satisfied
-> cheap landed M reconciliation
```

Enqueue authorizes automatic landing if proof succeeds; it is not a neutral
request for tests. All expected source changes and material review dispositions
belong before it. The owner still handles ejection, new findings and final
readback. No resident polling worker is needed while native CI owns the wait.

Keep H (reviewed head), P (historical PR synthetic merge), G (native queue
composition) and M (actual landed commit/tree) distinct. A green P does not prove
compatibility with a later base. Bind G to its base, ordered constituent heads,
actual checkout/tree, selected proof, trusted producer and run/attempt. Squash
changes commit identity; reconcile the corresponding tested and landed tree.

GitHub requires required contexts before enqueue and again on G. Real cheap PR
admission may replace expensive PR qualification only after queue-only landing
is enforced and verified. Never emit unconditional/skipped required success or
allow cheap admission to authorize a direct merge. Recheck current review and
eligibility before G's final required result; unchanged H can acquire a blocker.

Start native canaries with build concurrency 1, one-entry landing, squash and
all entries required to pass. These are bootstrap settings, not a universal
throughput ceiling. Merge limits do not batch builds. Tune from measured first-G
pass rate, invalidations, queue age, capacity and cost per accepted change.
The normal <=20-minute complete-active-qualification target remains; queue delay
and failed/retried work are accounted separately, not omitted.

## Existing implementation owners and cutover order

Keep #8082's existing caller owner and the landed #8890/#8327 request work.
Do not overwrite their branches or recreate their completed slices.

| Seam | Existing owner | Queue-specific work |
|---|---|---|
| Subject and trust | #4836 / #8082 | Replace PR-only assumptions with actual G identity and verified constituent admission; preserve source/control separation |
| Review and request | #4985 / #4986 / #8157 | Exact-H admission and pre-enqueue final judgment; late finding/head-change withdrawal; no repeated expensive PR run |
| Portfolio/event perimeter | #4987 / #4841 | One applicable G transaction, conservative selectors, required child completeness; retire duplicate broad callers only after replacement proof |
| Cache and settings | #7827 / #4832 / #4834 / #4838 | Cold fixture/dependency preparation, safe cache writer, reviewed live enforcement apply/readback and rollback |
| Native acceptance | #4840 / #4842 | Actual enqueue -> G proof -> protected M; failed composition, ejection, restoration and fresh-session continuation |

Sequence the rollout:

1. Record the complete live enforcement union, protected producers, active runs,
   current source and cache authority. Do not infer them from settings YAML.
2. Stage the G producer under a non-required rehearsal identity. Admission,
   docs-only selection, `full-ci`, fork trust and PR evidence currently consume
   PR fields; an added event alone cannot adapt them. Group scope is the union
   of applicable constituent and integration risks, not one guessed PR number.
3. Prove every required G result, negative controls and frozen-source review.
   Keep full legacy PR qualification during the bounded queue bootstrap.
4. Apply/read back queue enforcement through #4838. Demonstrate an actual native
   queue transaction before changing protected PR success to cheap admission.
5. Retire duplicate expensive label/main-push and child routes coherently. Keep
   cheap main reconciliation and distinct incident/deep/release obligations.
6. Observe a real operating cohort before declaring economics or adoption accepted.

Rollback restores and verifies full PR qualification before queue-only protection
can be removed. Pause new admissions and reconcile in-flight work first. Never
leave cheap admission as the only protection for a direct landing.

The #9015/#9034 interaction is the semantic-composition falsifier. The #8861
fixture failure is also a dependency-preparation problem: a warm cache containing
an old fixture pin can hide it even on current G. Preserve the existing repair
owner and prove declared fixture preparation from a cold dependency state; do not
repin audited fixture authority merely to follow the workspace's cache contents.

## History-sync route (source and swarm merges)

This is the interim history-sync profile under the `merge-ready` label route
(#12437). Its qualification half still applies; its landing half is blocked now
that the queue is required (see the end of this section). It is not a new pull_request-triggered CI lane: it rides the existing
merge-ready-requested run and adds no workflow trigger. The selected steady
state (#4984) runs product CI only on `merge_group`; history-preserving syncs
and release transactions are a separate exceptional profile there. The
successor is a history-sync profile in the #4984 design, which must keep the
S/B/H/G identity binding, the privileged authorization and the typed
diff-scoped `not_applicable: history_sync` semantics described here.

Use it only for a history-preserving sync merge between
`EffortlessMetrics/ripr` and `EffortlessMetrics/ripr-swarm`: the source-to-swarm
sync, the swarm-to-source promotion, or a post-release back-sync. A true merge
cannot be split without losing the history it exists to keep, so it would
otherwise be refused as `qualification_scope_oversized`, and its diff is far
above the PR-evidence producer's changed-line guard.

The PR body carries the ordinary claim lines plus exactly one of each:

```text
History-sync source: EffortlessMetrics/ripr@<full 40-hex sha>
History-sync destination base: <full 40-hex sha>
```

Admission honors the route only when every condition holds; otherwise the PR
gets the ordinary route, and an oversized refusal says why the label was not
honored:

- the `history-sync` label is present and its latest label event is a labeling
  by an account whose role on this repository is `admin` or `maintain`;
- the request is a `merge-ready` pull_request event (never a merge-queue entry),
  and head and base are this repository (no fork);
- the declared source repository is the other repository of the pair, and the
  source commit `S` exists there and is on (or is) its default-branch tip;
- with `H` the exact admitted PR head and `B` the declared destination base,
  `S` and `B` are both ancestors of `H`, `B` is an ancestor of the live base
  tip, and `S` is not reachable from `B`, so the PR genuinely brings in new
  source history. The shape of `H` itself is not authority: repair commits
  after the sync merge are expected.

The rust-gates job then binds the tested commit `G` (the pull_request merge
ref) to the same identities before it records anything: `G`'s parents must be
the base tip then `H`, `B` must be in that base tip, `S` must be in `H`, and `S`
must not already be in that base tip. A moved head, a base that already carries
`S`, or any unreadable ancestry fails the job.

A validated subject is admitted as `history_sync_admitted`, past the size caps
only. Claim identity, competing candidates, the hard-cut packet, unresolved
threads, Draft state and head binding still refuse as usual. Every gate still
runs: formatting, policies, clippy, goldens, fixtures, corpora, the full tests,
doctests, the extension e2e when selected, and the late eligibility recheck,
which also requires the `history-sync` label to still be present. Only the
diff-scoped PR-evidence producers (`ripr-pr` and its check, review comments,
impacted evidence, PR summary, annotations, and the schema sweep's PR rows) are
not run, because a sync merge is not one reviewable change. The PR evidence step
records `not_applicable:history_sync:<head>` instead, and `Ripr Rust Small
Result` accepts that record only for a `history_sync_admitted` admission of the
same head. Any other run that carries it fails.

The merge queue is never relaxed: a `merge_group` run keeps the squash-only
queued-commit rule, admission does not honor `history-sync` there, and a
two-parent queued commit is refused. A history-sync PR therefore qualifies
through the pull_request-mode `merge-ready` request and lands as a true merge
commit, never a squash or rebase, bound to the qualified head `H`:
`gh pr merge <n> --merge --match-head-commit <H>`. That worked only while
the queue was not required. The queue has been required since 2026-11-10, so a
history sync now needs an explicitly authorized protection exception or a
separately decided route (#4838, #4840); neither is provided here.

## Conflicts, incidents and landing

A behind-only branch normally needs no update or unaffected proof replay. Resolve
real conflicts through the current writer with an ordinary history-preserving
main merge; do not blindly choose ours/theirs for source or generated evidence.
A future installed exact-base rule still applies until its qualified replacement.

GitHub issues, PRs, checks and queue entries carry shared state. A session-local
`/mnt/project-files/merge-queue/status.md` may be advisory but is not a required
shared coordination database or a substitute for live GitHub evidence.

A main-health incident has one integration owner. Parallel diagnosis may continue,
but ordinary queue admission pauses while the designated repair is qualified.
The health guard must admit that verified repair without granting a blanket
bypass or requiring already-green main. Record cause, current state, owner and
next transition on the existing incident.

After landing, read back M and the accepted candidate; verify the corresponding
tree and claim closure. A returned merge acknowledgment is not the readback.
An incidental post-merge compile can detect a fault but is not pre-merge
composition proof. Under #4984, the selected merge-queue-only qualification
direction supersedes the former one-hour-AND-ten-merges automatic main-health
target. Retain historical observations with their original subjects and scope;
they do not authorize routine full post-merge replay. Current installed workflow
and required-check obligations remain binding until verified cutover; this
document does not disable a workflow. Named incident, deep and release proof
retain their separate purposes and authority. Release synchronization into
`ripr`, packaging, publication and installed verification remain separate
transactions.

A small clear review fix belongs in the candidate. A genuinely out-of-scope gap
gets an existing or new evidenced issue; a valid later nit is not erased by an
earlier clean review. Close only acceptance actually delivered.

Platform contract: [Managing a merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue).
