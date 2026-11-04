# Merge flow for swarm PRs

## Queue-only source and activation state

Steven selected native queue-time qualification under #4984 and the November 4
zero-development-Actions ruling. The `merge-queue-v1` source generation accepts
only `merge_group: checks_requested` at the ordinary protected caller. Draft,
Ready, PR updates, labels and ordinary main pushes allocate no workflow. The
label-conflict and label-staleness recovery roots are retired. The hard-cut audit
is an explicitly named release/incident dispatch.

**This is the final source candidate, not evidence of live activation.** The
preparation branches preserve the installed producer; this generation may replace
it only with the coordinated source/settings/publisher transaction below. The
previous required Actions context was `Ripr Rust Small Result`, application
15368. An external H publisher cannot satisfy an Actions-bound requirement by
sharing its name. Keep an explicit expected App binding; never set it to any App.
No unqualified direct merge is allowed between producer generations.

The reviewed Hermes profile proposes `CI Lifecycle Qualification` for both H
and G. Its numeric App identity and effective protection binding still require
admission and native readback. `.github/settings.yml` retains the legacy context
declaration as an explicit coordinated dependency: reconcile its actual consumer
before activation so it cannot reinstall the Actions-only requirement or replace
an accepted App binding with a name-only requirement.

The caller freezes actual B/H/G/tree, native run/attempt and source generation,
requires the complete G portfolio, and rechecks review, cut, eligibility and
succession. The configured canary is a singleton squash group, not an invented
PR merge or a per-PR merge-method override. It uses static owned capability for
candidate composition, main Rust and generic Linux proof; finalization reads
only the immutable trusted bundle and API state on separate control capacity.
Wheel ABI, Windows/macOS and GUI test contracts retain their qualified platforms.

Until activation is verified, retain the reviewed candidate and its evidence.
An explicitly selected bootstrap transaction belongs to the activation owner;
ordinary development must not fall back to label CI. Source, rehearsed, active
and accepted remain separate states.

## Who owns the change

Development publication, updates and review use no Actions workflow. The final
source removes the protected bootstrap event roots; installing it is conditional
on truthful H admission and qualified G execution. See
[the event inventory](CI.md#development-events-and-retired-bootstrap-route).

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

## Historical bootstrap procedure (activation owner only)

The retained predecessor requested `Ripr Rust Small Result` by applying
`merge-ready`; the final source does not. The following records the explicit
bootstrap mechanism, not an ordinary fallback. Ready is review
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
   Keep the existing producer satisfiable through the explicitly selected bootstrap transaction; do not create ordinary label runs.
4. Apply/read back queue enforcement through #4838. Demonstrate an actual native
   queue transaction before changing protected PR success to cheap admission.
5. Retire duplicate expensive label/main-push and child routes coherently. Keep
   publisher G-to-M readback without a new Actions run, plus distinct incident/deep/release obligations.
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

A main-health incident has one integration owner. Bound admission restrictions
to the actual failing integration or affected dependency; the verified repair
must be able to proceed without requiring an already-green main run. The normal
health basis is retained qualified-G evidence joined to protected M readback.
Missing readback is unknown and requires bounded reconciliation; it is neither
a healthy result nor a reason to wait for an ordinary main Actions run that this
source no longer starts. Real failing integration evidence remains refused under
its applicable guard. Record cause, scope, owner and next transition on the
existing incident.

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
