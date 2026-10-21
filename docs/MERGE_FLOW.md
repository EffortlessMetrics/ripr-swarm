# Merge flow for swarm PRs

## Current route and October 21 target

Steven selected native queue-time qualification on October 21, 2026 under
[#4984](https://github.com/EffortlessMetrics/ripr-swarm/issues/4984#issuecomment-6308919863).
The target replaces the earlier no-queue mechanism; it does not weaken review,
proof, ordinary protected squash, or separate release authority.

**Target selected; live queue activation is not established by this document.**
At inspected main `9911925ad6267bb3929be323cbcfdbe6da18f613`,
`.github/workflows/routed-rust.yml` requests PR qualification on the explicit
`merge-ready` label and also runs on main/master push and manual dispatch. It
has no `merge_group` trigger. The branch summary names `Ripr Rust Small Result`
(application 15368); a complete current enforcement readback is still required
before changing settings. A file or issue edit does not install new protection.

Until the replacement is qualified and activated, use the installed label route
below. Do not treat a proposed enqueue command, Ready state, or this target as
permission to skip its checks. Current user rulings and controller decisions
supersede older no-queue target prose; source, rehearsed, active and accepted
remain separate states.

## Who owns the change

The PR's owning lead carries review, repair, qualification and landing through
the applicable protected route without asking for routine repo permission again.
One writer owns each candidate; independent readers may challenge it. Do not
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
- A push after a request needs affected review/proof refreshed and a new explicit
  request. Preserve an unchanged-head in-flight run; reapplying the label cancels it.
- Do not dispatch `routed-rust.yml` on a PR branch (#5394).
- Inspect the real check results and current findings, make the final integration
  judgment, then use an expected-head protected squash. Do not arm generic
  auto-merge as a substitute for that legacy final judgment.

The existing docs-only route requires positive surface classification and the
actual docs gate. Skipped Rust work is not described as executed Rust proof.
Main being red does not justify repeating known-failing broad work; identify the
existing incident/repair owner and preserve unrelated candidate evidence.

## Selected native-queue procedure after verified activation

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
composition proof. Do not add a routine full post-merge replay; separately
admitted main health retains its existing one-hour-AND-ten-merges cadence until
an explicit policy change. Release synchronization into `ripr`, packaging,
publication and installed verification remain separate transactions.

A small clear review fix belongs in the candidate. A genuinely out-of-scope gap
gets an existing or new evidenced issue; a valid later nit is not erased by an
earlier clean review. Close only acceptance actually delivered.

Platform contract: [Managing a merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue).
