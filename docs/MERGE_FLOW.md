# Merge flow for swarm PRs

## Live route and October 21 target

**Live order: label PR qualification -> final published-head review -> native
enqueue -> required queue run -> GitHub's protected squash.** The native queue is
active and landing PRs: [#14710 entered the queue on November 14, 2026 and landed
through it on November 15](https://github.com/EffortlessMetrics/ripr-swarm/pull/14710).
Direct expected-head squash is refused
([#14247](https://github.com/EffortlessMetrics/ripr-swarm/issues/14247)); the old
no-queue and conditional-activation prose below is retained only as history.

Steven selected native queue-time qualification on October 21, 2026 under
[#4984](https://github.com/EffortlessMetrics/ripr-swarm/issues/4984#issuecomment-6308919863).
That target is now live; it does not weaken review, proof, protected squash, or
separate release authority.
Since #9106, `.github/workflows/routed-rust.yml` also subscribes to
`merge_group` (`checks_requested`) so the required `Ripr Rust Small Result` can be
earned once per queue entry on the exact queued merge commit (see
[CI](CI.md#merge-queue-request-cutover)). Admission is per constituent PR in
`merge_group.base_sha..merge_group.head_sha` (#8082): every constituent commit
must form a linear single-parent squash chain, bind to its live PR head and
queue entry, pass candidate/cut/review/thread admission, and belong to the
base repository before self-hosted runners are selected. The workflow also
still requests full PR qualification on the explicit `merge-ready` label and
retains manual dispatch. Ordinary main/master product replay is retired;
[the landed observer](CI.md#main-health-sentinel) reads exact native G=M proof. The branch summary names
`Ripr Rust Small Result` (application 15368); a complete current enforcement
readback is still required before changing settings. A file or issue edit does
not install new protection.

The installed `merge-ready` label requests PR qualification; Ready alone requests
nothing, and neither one enqueues the PR. After reviewing those results and the
current findings, use the native queue procedure below. Do not treat either state
as permission to skip checks or merge directly. Current user rulings and controller
decisions supersede older no-queue target prose; source, rehearsed, active and
accepted remain separate states.

## Who owns the change

Development publication, updates and review use no Actions workflow after the
#4842 auxiliary-trigger retirement. Label cutover and hosted App activation remain
open. Main replay is retired with full H/G protection, opportunistic caches and
mandatory cold recomputation; no recurring warmer is introduced (#7827).
See [the event inventory](CI.md#development-events-and-retained-bootstrap-route).

This source stage retains the installed required H producer. While GitHub still
requires that check before enqueue, each delivery owner may request its
`merge-ready` qualification for the selected, reviewed candidate. Native queue
activation alone does not replace H qualification with cheap admission. The
final caller cutover must qualify and enforce that replacement before retiring
the label route; later publisher or control-profile prerequisites do not block
this independently landable auxiliary-event retirement.

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
  judgment, then enqueue the exact reviewed head. GitHub performs the protected
  squash after successful queue qualification. Do not arm generic auto-merge or
  use the refused direct expected-head squash route (#14247).

The existing docs-only route requires positive surface classification and the
actual docs gate. Skipped Rust work is not described as executed Rust proof.
Main being red does not justify repeating known-failing broad work; identify the
existing incident/repair owner and preserve unrelated candidate evidence.

## Native queue landing procedure

```text
cheap development / Draft publication
-> focused proof + substantive review + joined repairs
-> apply merge-ready and inspect the current PR qualification results
-> exact-head product/scope judgment and conditional merge authorization
-> native enqueue
-> one expensive required qualification on current merge-group G
-> GitHub queue-driven protected squash if required conditions remain satisfied
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
   Full legacy PR qualification was kept during the bounded queue bootstrap.
4. Apply/read back queue enforcement through #4838. Demonstrate an actual native
   queue transaction before changing protected PR success to cheap admission.
5. Retire the remaining duplicate label and child routes coherently. The bounded
   main-push retirement already keeps full H/G authority, cold recomputation and
   cheap landed observation. Distinct incident/deep/release obligations remain.
6. Observe a real operating cohort before declaring economics or adoption accepted.

Rollback restores and verifies full PR qualification before queue-only protection
can be removed. Pause new admissions and reconcile in-flight work first. Never
leave cheap admission as the only protection for a direct landing.

The #9015/#9034 interaction is the semantic-composition falsifier. The #8861
fixture failure is also a dependency-preparation problem: a warm cache containing
an old fixture pin can hide it even on current G. Preserve the existing repair
owner and prove declared fixture preparation from a cold dependency state; do not
repin audited fixture authority merely to follow the workspace's cache contents.

## Mechanical test-relocation route

Moving an inline `#[cfg(test)] mod NAME { BODY }` to its out-of-line file
(#15193) de-indents every line, so a relocation PR is all deletions plus
additions and exceeds the ordinary 160-file / 3,700-line cap without changing a
token of Rust. Admission honors a verified relocation instead (#15217):

```text
Mechanical relocation: inline-cfg-test-v1
```

With exactly that body line, `candidate_admission.py` reads every changed file
at the PR's merge base and exact head and hands them to
`.github/actions/pr-candidate-admission/mechanical_relocation.py`, read from the
trusted bundle. The verifier re-derives the relocation from the base and admits
only a head that matches it:

- each changed owner under `crates/ripr/src` or `crates/ripr-contract/src` is
  byte-identical to the base with relocated module bodies replaced by `;` (and,
  for a module whose name is neither `tests` nor `*_tests`, a
  `#[path = "<stem>/<name>_tests.rs"]` line relative to the owner's directory);
- no owner is a file that another file loads through `#[path]` or `include!`
  (listed in `policy/relocation-indirect-owners.txt`, read from the base and
  kept current by `indirect-owners --check`, which also fails on any load form
  it cannot resolve to one plain literal and on any unreviewed published-source
  `macro_rules!` that shadows a comma-neutral macro), since its children
  resolve from the loader;
- each added file is the Rust-resolved file of one relocated module and carries
  its body token for token (multi-character operators are single tokens, doc
  comments stay in place as tokens, non-ASCII identifiers are single tokens and
  other non-ASCII outside strings and comments refuses) and its comments
  unchanged, between the same identifiers and literals and before the same code
  token. rustfmt may re-wrap lines, add or drop a trailing comma before a closer (not a 1-tuple's,
  and not inside macros other than the standard comma-neutral ones), and drop
  the braces of a one-expression closure body (whose opening `|` follows `=`,
  `(`, `,`, `move` or a similar non-operand token, so never a binary `|`) or
  match-arm block that ends its closure or arm and does not start with a
  block-like expression, a `m! { .. }` call or an attribute; relative
  `include_str!`/`include_bytes!`/`include!` paths are re-based to the new
  directory;
- `policy/inline-test-modules.txt` lowers one ceiling per relocated module, and
  every other path-keyed metadata file (traceability, allowlists) equals the
  base re-keyed by the transform's own rule: references to a moved test, or to
  a function defined only in the moved body, name the test file, and an
  allowlist row moves only when its pattern left the owner;
- every listed file's blob id is the hash of the head content read, so the
  listing and the contents describe one head.

Any other file, construct, rename or unreadable fact rejects, and the PR is
judged on the ordinary route with the reason in the admission detail. Refused
constructs: `file!`, `line!`, `column!` and `stringify!` (also as raw
identifiers), `Location`, `#[track_caller]`, any `path` attribute in the body
or on the module, a `use` that imports a location, include or comma-neutral
macro name, a `macro_rules!` of a comma-neutral name, a non-test cfg, and in
the owner a location construct after the first relocated module (every later
line moves up). Residual the verifier cannot see: a macro defined in another
file (or a glob-imported dependency macro) that expands to `line!()` or shadows
a comma-neutral name, and a `#[path]` inside a doubly nested inline module of
another file (the scan resolves one inline level); the required run still
builds and runs every test. A
validated relocation that would exceed the ordinary caps trades them for the
relocation ceiling of 400 files / 40,000 changed lines, and nothing else (a
smaller one is admitted on the ordinary route): claim identity, head freshness, competing candidates and the
hard cut still apply, and the required run still builds and runs every test.
Diff-scoped PR evidence is recorded `not_applicable:mechanical_relocation:<head>`
because no behavior changed; the Result accepts that record only for a
`mechanical_relocation_admitted` head. In the merge queue a relocation must be
its group's only constituent.

To produce one, run the same transform locally and check it before pushing:

```bash
python3 .github/actions/pr-candidate-admission/mechanical_relocation.py apply <owner.rs>...
python3 .github/actions/pr-candidate-admission/mechanical_relocation.py verify --base origin/main
```

## Historical history-sync route (source and swarm merges)

This section records the former interim history-sync profile under the
`merge-ready` label route (#12437). Queue-only enforcement is now active, so the
ordinary landing procedure above does not offer this true-merge route. History-sync
PRs require an explicitly authorized protection exception or a separately decided
route; a normal direct merge is not permitted.

The former profile was not a new pull_request-triggered CI lane: it rode the
merge-ready-requested run and added no workflow trigger. Queue landing now runs
product CI on `merge_group`; the explicit `merge-ready` request and manual
dispatch remain installed until the label route retires, and history-preserving
syncs and release transactions remain a
separate exceptional profile. Any successor must keep the S/B/H/G identity
binding, privileged authorization and typed diff-scoped
`not_applicable: history_sync` semantics described here.

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
two-parent queued commit is refused. A history-sync PR formerly qualified
through the pull_request-mode `merge-ready` request and landed as a true merge
commit, never a squash or rebase, bound to the qualified head `H`:
`gh pr merge <n> --merge --match-head-commit <H>`. That command belonged to the
pre-queue-enforcement profile and is not an alternative landing route now. With
queue-only enforcement active, a history sync needs an explicitly authorized
protection exception or a separately decided route (#4838, #4840); neither is
provided here.

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
