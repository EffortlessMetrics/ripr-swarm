# Native merge-ready production-wiring candidate — cutover held

Owner: #4987 with #4986/#4836. Fresh main `2af3f38a1de75ca48d5d84555a839d50710159b6`
was merged cleanly into this lane before wiring. No equivalent subset admission
writer was found in refreshed #4987 comments/open-PR search. The newer main-health
candidate is separate. #8002 and other owner branches are untouched.

## What is now wired, and what remains historical

The `.github/workflows` files on this **candidate branch** are executable workflow
wiring, not examples: `routed-rust.yml` has explicit labeled admission, the existing
Rust/docs producers and final aggregate; `windows-subset-qualification.yml` is
the extracted callable Windows producer; `windows-advisory.yml` calls that same
producer only on schedule/manual diagnosis. Its full twice-run job is unchanged.
`pr-staleness-watchdog.yml` remains API-only and observes requested rather than
merely Ready heads, excluding unrelated metadata runs from in-flight qualification.

The two YAML files under `fixtures/convergence/windows-subset` are frozen inactive
prototype/oracle inputs. Do not deploy them: the production caller additionally
refreshes final review/mergeability/thread state; the callee has explicit
qualification/diagnostic profiles. Production payload parity tests retain all
native commands, oracles, status/log/receipt checks and cleanup after the source
identity delta. No new controller, queue, database or scheduler was added.

The first controller/custody prototype at 9b8c8eb3b was superseded by e22a2e5e7,
and its Rust admission model remains removed. Historical Ready-only/static-name
policy is migrated in the same candidate. Compatibility function names in
repo-policy predate the new event law; they now enforce labeled requests, a
request-bound protected name, metadata-isolated concurrency, and an always-running
aggregate depending on admission and selected Windows evidence. Source/docs/skills
now separate source review before admission from final REVIEW_READY after green.

## Scope and safety

Only adding `merge-ready` to an open, reviewed, non-Draft PR is a qualification
request. Opening, pushing and Ready alone do not invoke routed qualification.
Other labels emit `Merge-ready Nonrequest`, not a skipped required success.
No main-push routed full run or branch dispatch can impersonate PR qualification.
Existing unrelated feature, wheel, security, full-Windows and editor workflows
remain separately owned: this bounded candidate does not claim graph-wide zero
launches. During source staging, routed main/master pushes and manual runs retain
the existing full Rust qualification profile; PR admission and its four new leaves
are skipped, and the result still requires actual detector/router/selected Rust
success. Explicit status predicates preserve execution after skipped admission
and reject cancellation. Main/manual concurrency is separate from PR requests.
This preserves production qualification pending #4838 cutover, not a cache-warmer
suite. Retiring those legacy entrypoints requires #7827's reviewed replacement
lifecycle and matching desired/rollback generation. No warm-cache or <=20-minute
saving is assumed.

Admission is read-only native API bookkeeping, with no candidate checkout:
actor permission, current repository/target/open/Draft/label, H/B/T and parents,
mergeability, current review disposition and paginated thread state are checked.
`SOURCE_REVIEW_READY` on a current COMMENT supports a source-complete independent
review with only admitted hosted proof outstanding. GitHub identity alone does
not establish reviewer independence; the trusted requester verifies the substantive
review-of-record. Current approvals are observed without adding an approval quota.
Quota notices, skipped sensors or empty findings are not review. Product execution
retains existing trust/permissions; the source/attempt receipt separates H/B from T.

Sequential duplicate requests reconcile native runs by workflow, H/B/T title and
completed required result, without repeating producers. A prior failed result
remains failed; an unreadable or incomplete lookup refuses. Unrelated label
concurrency uses per-run metadata groups. **Native default concurrency still
replaces an existing pending request when another identical request arrives, even
with cancel-in-progress:false.** Running work is preserved, not exactly-once
submission. Read back queued/running exact-subject work before labeling/relabeling;
do not submit routine duplicates, retoggle Ready, or cancel unchanged slow work.
No optional queue setting or new controller is needed to describe this limitation.

The final result repeats live PR mergeability/readiness/reviews/thread checks,
including when reusing a prior green result. Selected evidence missing/skipped/
failed cannot green the aggregate. The final merge owner still makes the
vision/current-state decision and current REVIEW_READY judgment after green.
Separate API reads/publication/expected-head merge are non-atomic base guards;
changed integration inputs need affected-proof disposition and landed-tree readback.

The parent verified required `Ripr Rust Small Result` from app15368 and an active
ruleset with **no native queue**. This candidate keeps that identity and changes
no protection/settings. Complete classic strictness remains unreadable; it is not
assumed absent and does not invent a new settings prerequisite for this repair.
Main-health (>=1 hour AND >=10 merges) is a separate proposal, not a prerequisite.

## Evidence and remaining limits

The exact integration baseline passed `cargo check --locked -p repo-policy
--all-targets`. Focused workflow policy tests, source contracts, actionlint and
mocked native Bash/expression replays bind to this candidate; terminal manifests
in `/workspace/windows-subset-proof` identify final counts/source hashes.

Independent vectors from commit 88a82f942db1bad25e4ee0e2b2f333a6d7287ac7 are imported
unchanged at `fixtures/convergence/native-late-admission`. The worker's 59 structural
assertions were not runtime proof. This executor replayed all 32 vectors: 18 actual
Bash readiness/terminal cases, 10 expression cases and four group relations whose
scheduler consequence remains documented rather than natively executed. The old
review-revocation acceptance was reproduced on e22a2e5e7; corrected final refresh
rejects it. Permanent `repo-policy --test native_admission` executes the 18 actual
Bash cases with fixed mocked read-only API replies under bounded GNU timeout.

A wider repo-policy run had 62 passes and one unavailable matching llvm-profdata
prerequisite. That unrelated diagnostic test is not reported as green; matching
LLVM tooling is not a new admission-infrastructure dependency. No product suite,
PowerShell or native Windows execution, live labeled workflow, savings or merge
readiness is claimed by the local replay. Full xtask compilation/precommit remains
unrun; focused source-test compilation is distinguished from that broad driver.

## Minimum rehearsal and review before cutover

1. Independent exact-candidate source/oracle/security review of this combined
   production wiring and its native rehearsal path, with any repairs re-reviewed.
2. At a separately admitted existing same-repo PR and reviewed H/B/T/workflow
   generation, observe unrelated-label non-required naming/metadata isolation,
   Ready alone no routed run, then **one** explicit merge-ready request. Use the
   available qualified route plus native Windows; no Contabo/fleet deployment,
   native merge queue, controller, new settings or main-health implementation is
   a prerequisite. Do not open a PR while the early-broad-CI hold remains.
3. Read actual checkout/run/attempt, complete Rust/docs and Windows evidence
   denominator, source/status/log/receipt agreement and both cleanup outcomes.
   Confirm missing/stale/final-review-revoked results block; reconcile a duplicate
   only after reading existing custody, with no extra product execution. Native
   pending replacement remains a limitation, not a promised scheduler fix.
4. Only after combined candidate + rehearsal-path review and explicit cutover
   admission, integrate through the existing protected squash process. Obtain a
   fresh post-green final decision and actual required check readback; do not arm
   merge early or change protections. If current server rejects a base condition,
   respect it and refresh affected proof rather than guessing strictness.

Source landing is held in this task: no PR, merge, live labeled request, settings
change or CI dispatch. Branch publication is permitted only after rechecking its
push filters. Rollback restores the matching caller/callee/policy/instruction
source generation and retains old attempts/receipts; no synthetic green fallback.

## Resource custody

Own branch/worktree: `codex/4987-windows-subset-admission`,
`/workspace/windows-subset-admission`. Proof/toolchain/target:
`/workspace/windows-subset-proof`. The original checkout and all other writers'
branches remain unchanged. Local target is bounded/product-independent; compact
patch/bundle/logs are retained for parent review. No broad cleanup is authorized.

## Independent source review repair

Exact candidate `789a615` review found two source defects, both repaired on the
same owner branch: embedded Windows storage tests now read the extracted callee,
and ordinary COMMENTED reviews cannot erase a native CHANGES_REQUESTED
verdict. Admission and final refresh retain the latest decisive native review
(APPROVED / CHANGES_REQUESTED / DISMISSED) separately from source attestation
comments. A later native approval or dismissal resolves that reviewer's verdict;
ordinary comments, including source-ready markers, do not. Focused regressions
exercise both rejection cases and native resolution. All 30 embedded storage
Python tests run portably; this is consumer/source proof, not Windows execution.

## Older-review and cancellation repairs after published-source review

The [follow-up review on #4987](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6182909371)
found two defects in published `851195415bd2226f6cba2569cf94a4c787645251`.
That source acceptance is superseded for these dimensions; it was not native
qualification or merge readiness.

- **Outstanding verdict survives a push.** The reducer already kept decisive
  APPROVED / CHANGES_REQUESTED / DISMISSED states separate from comments, but its
  negative predicate ignored a CHANGES_REQUESTED attached to an older commit.
  Both admission and final refresh now block any reviewer's latest decisive
  CHANGES_REQUESTED, irrespective of commit. Actual later approval or dismissal
  clears that reviewer's negative disposition. Positive approval or source-ready
  attestation still requires the current head: resolving a verdict is not fresh
  positive proof. Ordinary comments/markers cannot clear a requested change.
- **Workflow cancellation survives green producers.** The result job's
  `always()` condition intentionally survives workflow cancellation; completed
  dependency results therefore do not establish that the workflow is uncancelled.
  Its last step now uses native `if: always() && cancelled()` and exits 1 when
  selected. It runs after readiness and aggregation, including the early-success
  reuse branch, and remains reachable if earlier steps fail or are interrupted.
  A skipped cancellation step cannot erase an earlier failure. `cancelled()` is
  evaluated in the supported step-condition context, not an environment field.
  See GitHub's [cancellation reference](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-cancellation).

Two independent agents reproduced the old behavior using immutable Git source:
24 actual Bash executions at both review boundaries exposed eight wrong successes
across older-verdict/comment/marker cases. Seven cancellation cases exposed three
wrong successes: cancellation with green dependencies, after aggregation, and
prior-success reuse. Controls included native later approval/dismissal, stale
positive approval, failed and cancelled producers. These are controlled read-only
API fixtures and literal expression cases, not live API/Actions observations.

The permanent `tools/repo-policy/tests/native_admission.rs` suite now has five
subjects, retaining all prior independent drift/terminal vectors and adding both
repairs. Admission tests execute the complete actual preflight body. Each actual
run-step body has its own shell scope; failures accumulate while the terminal
check remains reachable. This avoids putting a `set -e` body on the left of an
AND-list, which would suppress Bash errexit and manufacture a false drift pass.
Cancellation tests pin the native terminal condition, execute the rejection body,
cover normal and reused green proof, and retain failed-producer refusal. Independent
expression replay evaluates the extracted condition with controlled boolean status
functions. Actionlint validates the supported condition context.

This repair reuses the existing owner branch, unchanged Windows producer and
required-check identity; no #8002, platform-lane, main-health or protection work is
included. It does not activate workflows or open a PR. Fresh main was observed at
`8b7a9ba4a7ca2f1212fc3b456d0a2bee876a5e55`; its changes since the candidate's
`16190da1b` integration basis affect product analysis and fixture-home work, not
these workflow/test owners. No restack is needed for unrelated main movement.

**Remaining native rehearsal:** exercise real API/token shapes, exact check
name/head/integration association, older outstanding review state after a push,
controlled cancellation after successful dependencies and during finalization,
sequential duplicate readback, and one actual reusable Windows integration-subject
run with PowerShell, nonempty status/log/receipt agreement and cleanup. The native
scheduler and cancellation delivery are not simulated proof. Cancellation arriving
after the last condition evaluation/publication remains a non-atomic race requiring
fresh run/check readback; this adds no atomic-publication claim. Native pending
replacement, final freshness races and the unmeasured <=20-minute target remain.

Resource admission/retention: the existing owner worktree and Git common directory
remain in `/workspace`; toolchain and mutable Cargo target remain under
`/workspace/windows-subset-proof`. This repair admits <=128 MiB additional logs,
focused test output and compact patch/bundle, with >=8 GiB reserve (29 GiB observed
free). Original 851 source and independent negative witnesses are retained in Git
and `older-review-independent` / `cancel-independent` proof directories for the
parent's review/rehearsal. No frozen executable copies, product build or unrelated
cleanup are needed; source and proof stay retained with that consumer.

## Companion repair: subset evidence integrity

The independent review of `4eebbc6e6c3c078170790d52bca5877a0bda6d54`
identified a concrete evidence gap, recorded on
[#4987](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6183220409).
This companion starts from that exact candidate on
`fix/4987-reject-empty-windows-evidence`; the original owner branch and the
separate documentation integration remain unchanged.

The four-pair guard checked only file existence. Empty status files skipped
the `status == 0` executed-test check, so four empty status/log pairs could
produce a successful reusable job. The four Bash producers also ignored a
failed status write under `set +e` before their advisory `exit 0`.

Native Windows reproduction used Git-for-Windows Bash and standalone std-only
Rust tests, with Cargo mocked rather than executed. Against the unchanged input,
six tests yielded two passes and four intended failures: all empty pairs,
one empty component among valid pairs, malformed status, and hidden status-write
failure. The positive advisory and zero-test controls passed.

The guard now requires nonempty regular status/log files and one canonical
decimal native Bash exit status in `0..255`. It validates the file before command
substitution, rejecting embedded NULs and extra blank lines too. Nonzero native
test/lint verdicts remain advisory. Successful test subsets still require an
executed-test denominator; Clippy does not acquire a test-count requirement.
Target creation and status/log capture errors fail explicitly. The selector's
PowerShell capture uses explicit `try/catch` plus terminating cmdlet errors:
the focused native control showed that adding `-ErrorAction Stop` alone did
not prevent the isolated script's final `exit 0` after a failed status write.
The selector receipt validator, native commands, source admission, cleanup,
workflow events, permissions and required-result identity remain unchanged.

Permanent tests in `xtask/tests/windows_subset_evidence.rs` extract and execute
the actual production bodies. Seven tests cover 99 controlled replays: valid
success and advisory failure, every missing/empty/nonfile component, malformed
and out-of-range statuses, zero executed tests, Bash capture failures, and native
PowerShell capture failures. The PowerShell test mocks the consumer, not its
capture cmdlets. The seven existing admission/parity tests also pass. Their
comparison recognizes only the explicit evidence-hardening delta; the historical
prototype remains frozen and every native payload/selector oracle is compared.

Focused execution uses `rustc --edition=2024 --test` on those two test files,
without compiling the product or xtask dependency graph. `actionlint` passes
with external ShellCheck/Pyflakes disabled. The production policy checker is
run separately as `cargo run --offline --locked -p repo-policy -- check-workflows`;
only that small utility is built. The PowerShell repair fits the existing inline
line budget; the policy allowance is unchanged. Logs and the original failing
test executable are retained by the desktop task for independent review.

This establishes the controlled evidence guard/capture behavior on Windows,
not a hosted producer run. Real product test/Clippy execution, selector consumer
build/replay/receipt/cleanup, API/token/check association, cancellation delivery,
sequential duplicate readback and freshness races remain unrehearsed as described
above. No workflow is activated, PR opened, protection changed or runtime/release
qualification claimed by this companion.

## Streamed review-page and timestamp-tie coverage

The admission owner continues from exact reviewed evidence companion
`6902a132eb0366a31bdd1b52a30b189847c1f82f` (includes `4eebbc6e6`). The separate
coverage attempt in [#4987 comment 6184051444](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6184051444)
failed container provisioning and made no source changes. Its proposed cases
are now executable regressions, not an inherited pass. Fresh main was read at
`d736c39d749f4fabde4e8fe11456ec340bb88e90`; intervening product/Droid-retirement
changes do not edit this caller or test owner. No unrelated restack is required.

This slice changes the existing test harness and this handoff only. Production
admission, final readiness, cancellation, H/B/T, required-check naming, source
receipt, Windows evidence and protected-merge semantics remain byte-for-byte
unchanged from 6902. Existing changelog fragments cover the production repairs;
this test-only/internal explanation has no new workflow/API/output behavior and
needs no additional fragment under CHANGELOG_POLICY.md.

The new harness emits actual newline-separated JSON page arrays to both complete
production Bash bodies. The first page contains 100 distinct reviews, as requested
by production `per_page=100`; another page follows. The mock now returns only the
first array if `--paginate` is removed. This makes lost pagination observable,
instead of always emitting the entire fixture regardless of the command.

Three additional Cargo subjects cover outstanding older-head changes requests,
ordinary and source-ready comments, genuine later approval/dismissal, fresh vs
stale positive evidence, equal timestamps with distinct IDs, and page boundaries.
The negative controls remove `--paginate`, drop the first page, or select the
first decisive review. The unchanged expected dispositions reject each wrong
behavior. Reversing tied pages changes the observed result; reversing pages with
a genuinely later timestamp does not. These are explicit assumption challenges,
not reports that GitHub returned reversed pages.

The reducer intentionally retains its current `max_by(submitted_at)` semantics:
for equal timestamps it selects the last API/input review, not the largest ID.
Descending distinct IDs in controls prevent accidentally crediting ID order.
GitHub [documents chronological review ordering](https://docs.github.com/en/rest/pulls/reviews#list-reviews-for-a-pull-request),
but does not specify an ID-based tie rule. Mocked behavior establishes the
reducer's input-order dependence; native tied-review ordering/dismissal shapes,
changes during paginated reads and actual token/API behavior still need native
rehearsal. This work does not claim that pagination is a snapshot or atomic.

Author-side focused execution uses existing bounded repo-policy Cargo ownership,
Rust 1.95.0 and jq 1.7 on Linux. The independent reader separately replayed
immutable 6902 bodies with 14 vectors at both boundaries under jq 1.6 and 1.7
(56 executions), using the same expectations for the reviewed successor.
Independent exact-head review and compact source/log hashes bind the final
published successor; no native Actions, Windows product or timing proof is implied.

### Smallest integration sequence and ownership

1. Keep this admission/evidence candidate as the shared integration basis. Obtain
   the three owners' reviewed callable leaves for `ci.yml`,
   `python-wheel-qualification.yml`, and `printed-command-paste.yml`. This task
   does not edit those files or duplicate their implementations. Reconcile their
   declared inputs/outputs and selected evidence obligations before wiring calls.
2. On this owner branch, assemble the accepted leaf commits and the shared caller
   changes as one coherent source candidate. Route admitted exact H/B/T/tree and
   workflow generation to the selected calls; preserve mandatory proof identity,
   independent review, final readiness refresh and cancellation refusal. Add the
   selected leaf results to the existing aggregate and migrate only the affected
   trigger/name/policy contracts. Do not land disconnected half-wiring.
3. Run focused combined-graph contracts and leaf-owned proof, then independent
   exact-candidate source review. Refresh changed main/owner inputs for content
   conflicts or changed prerequisites, preserving unrelated valid proof. A branch
   push/source acceptance still does not activate or qualify production.
4. Only through a separately admitted cutover/rehearsal transition, exercise the
   native event/token/check association, controlled drift/cancellation, duplicate
   readback and Windows evidence/cleanup described above. Final post-green
   vision/current-state decision and protected squash remain later transitions.
   No controller, native merge queue, settings change or main-health dependency
   is added by this sequence; <=20-minute timing remains unmeasured.

Resource custody remains the existing owner worktree, Git history and small
repo-policy target under `/workspace/windows-subset-proof`; >=8 GiB reserve and
<=128 MiB additional focused output are retained for this slice (29 GiB free
observed). Independent pagination artifacts live under `pagination-independent`;
original and mutation witnesses remain retained for the parent/rehearsal owner.


## Shared caller continuation — source integration, activation held

The successor is `codex/4987-caller-paste-integration`. Its ancestry preserves
`0cf965fcdfca893e7ce9b89767ee7f88937f1822` and reviewed printed-command leaf
`624177123a396ddb3272196ffe4d3349d1305470`. Old owner/leaf refs were checked
unchanged before shared edits. The inherited no-PR hold is superseded for this
successor by the user's explicit draft-PR assignment. Source publication does
not authorize Ready, a merge-ready label, dispatch, merge or cutover.

Combined contract:

- Only a fresh native `labeled: merge-ready` event admits work. Exact open,
  non-Draft same-repository H/base B/integration commit G with ordered parents
  `[B,H]`, tree T, trusted actor and current review/thread state remain mandatory.
- Both Windows subset and printed-command are conservatively mandatory for every
  admitted execution, including docs-only subjects. There is no new selector.
  Each local reusable call receives `proof_profile=qualification`, request ID
  `run_id-run_attempt`, exact H/B/G/T and `github.workflow_sha` as workflow
  generation. The leaf independently checks checkout/tree/parents/generation
  before product execution; this is not a second admission authority.
- The existing always-running protected result depends on both child jobs and
  accepts only native `success` for each newly executed child. Missing, skipped,
  neutral, failed, timed-out or cancelled printed results refuse, even when all
  Rust/docs/Windows results are green. The final readiness refresh and terminal
  cancellation refusal remain. Prior exact-subject required-success reuse remains
  the existing admission owner's route and does not rerun children.
- The printed leaf's legacy PR, schedule and manual routes remain byte-identical
  to the reviewed leaf. Removing the legacy PR trigger is deferred until the
  complete graph is reviewed for cutover; duplicate execution has not been
  eliminated. No deployment, release, credential, protection or settings change.
- `ci.yml` and `python-wheel-qualification.yml` remain their owners' files and
  are unchanged. Their callable revisions, selected obligations and aggregate
  wiring are outstanding. This two-child source integration is not graph-wide
  zero-early-launch acceptance and must not be activated as a completed graph.

The merge of leaf ancestry also brings already-landed main history; its conflicts
retain current Droid retirement plus the candidate's labeled admission law, and
both process-test allowances. No product changes are authored in this successor.
Rollback reverts this coherent candidate through normal repository procedures.

Observed combined-baseline `cargo check --locked -p repo-policy --all-targets`
passed on Linux x86_64 with Rust1.95.0. Initial offline resolution lacked locked
tokio-util0.7.20; normal locked resolution succeeded. New controls against
02f9a30ba failed at the intended missing-call and false-success assertions (2/2,
exit101). After wiring, native-admission10/10 and printed-leaf6/6 passed, including
real extracted Bash/jq replay and existing guard-removal controls. These are
local mocked-API/source controls, not native Actions, macOS/Windows product proof,
API/token/check association, scheduler behavior, savings or merge readiness.

Resources: own worktree `/workspace/caller-integration`, shared Git common store
`/workspace/ripr-swarm/.git`, own target/log root
`/workspace/caller-integration-proof`. Observed30GiB free and16GiB available RAM;
peak additional budget4GiB, reserve8GiB, two Cargo jobs. No original witness was
available locally; immutable source refs are preserved and prior external proof
stays attributed to its original executor. These resources are retained for
independent review and parent continuation; no unrelated resources are removed.


### CI leaf successor — three-child source graph, cutover still held

The delegated continuation now explicitly supplies reviewed CI leaf
`efaa7fa7f391adc07c3816c84053284ad7f7b922` on
`codex/4987-ci-exact-subject`. Its remote ref was checked unchanged before
integration. This supersedes only the earlier statement that CI remains
unintegrated/untouched; wheel ownership and all activation holds survive.
The exact leaf was merged at `3cbcf6e7c37a9712dcc196afedadf7f78c150cec`.
The sole additive process-allowance conflict preserves native-admission,
printed-command and CI-leaf test allowances. The leaf file itself is unchanged.

The `ci-feature` job now consumes the same seven admitted string inputs as the
other leaves and four explicit frozen booleans: `run_rust_only=true`,
`run_release_proof=true`, `run_msrv=true`, `run_vscode=true`. All rows are
conservatively mandatory for every new execution, including docs-only subjects.
No canonical applicability plan is evaluated here, no second classifier is
introduced, and no row is silently pruned based on incomplete applicability.
This deliberately over-selects proof and does not claim label-specific savings,
a portfolio deadline or final selection optimization. `full-ci`/`release-check`
requests cannot select a missing row because all retained rows are included.
The canonical plan/label-snapshot integration may narrow rows only with its own
reviewed completeness and fail-closed selected/unselected obligations.

The protected caller now depends on native `ci-feature` success as well as
Windows and printed-command success. Its own child result additionally requires
source validation, every selected native child success, every unselected child
skipped, and cancellation refusal. Candidate receipts cannot replace native
results. The caller retains fresh-event admission, exact H/B/G/T/generation,
final readiness and terminal cancellation checks plus admission-owned prior reuse.
The new CI aggregate guard runs before either docs-only or Rust route success.

Legacy CI PR/main-push/manual triggers remain exactly as supplied by the reviewed
leaf, and printed-command's legacy routes remain. Retiring legacy automatic
triggers is a separately reviewed full-graph cutover; this staged candidate must
not be activated or described as zero-early-launch acceptance. The wheel leaf is
still separately owned and unmodified. Release proof here names the retained
package/publish-dry-run/readiness checks, not publication authority.

The new pre-wiring controls run on the composed baseline3cbcf6e7c and fail at
both intended assertions: missing CI call and aggregate false-green for a failed
CI result (2 failures, native101). The repaired composed source passes the
focused Cargo suites: native admission12, printed-command6, CI leaf10 and
Rust-only routing10, with no failed/ignored subjects. The ten CI-leaf tests plus
ten routing tests are a fresh local rerun of the supplied leaf's20-test population;
its earlier check-fast/actionlint/real-Git review evidence stays attributed to
efaa7fa7 and its original executor, not the composed candidate.

Resource continuation reuses the warm exclusive target at
`/workspace/caller-integration-proof/target`;30GiB free and235MiB retained proof
were observed before the new batch. Existing4GiB additional peak/8GiB reserve and
two-job bounds remain. Logs `ci-red.log` and `ci-green.log` distinguish prewire
behavior from final source. Native Actions/expression/token/check association,
three-OS payload execution, complete wheel graph, broad local preflight and
rehearsal/economics remain unqualified. Keep PR8082 Draft; parent integrates the
remaining reviewed wheel leaf and completes full-graph review before any
separately authorized activation transition.


The refreshed PR discussion exposed a stale `docs_only_routing` test adapter.
Reproduced on the composed candidate:5pass/1fail, native101 at the aggregate-env
setup assertion. It read the newly preceding readiness-refresh step rather than
the intended result decision. The repair selects the actual named summary step,
retains the exactly-one-run-body invariant, seeds successful native admission and
three-child prerequisites, and keeps every detector/docs/Rust negative assertion.
Push/manual inputs are explicitly branch-isolation controls, not admitted events.
The repaired suite executes6/6; logs `docs-routing-before.log` and
`docs-routing-after.log` bind setup failure and restored decision coverage. This
is a test-adapter repair, not a production routing behavior change.


### Wheel successor — four-child source graph, feature-shell repair integrated

Reviewed wheel leaf `c9b70660a522cc53f722ced42c2e12dfefbe9e15` was supplied
explicitly by its owner on `codex/4987-wheel-callable-leaf`, verified at the
published ref, and merged cleanly at
`bc2304cf390ab364b9fd39a35ccacbd4be2fbe79`. Its workflow remains byte-identical
to that leaf. This supersedes the earlier wheel-integration wait. The subsequent
CI author repair described below is integrated without concurrent leaf edits.

The shared `python-wheel` job passes the same seven admitted strings as Windows,
printed and feature CI: qualification profile, native run-attempt request, H, B,
G, tree and workflow generation. There are no workflow outputs or inherited
secrets. Until canonical package applicability is evaluated completely, wheel is
conservatively selected for every new admitted execution, including docs-only.
The protected aggregate requires its native reusable-job success before either
docs or product success. Missing/skipped/neutral/failed/timed-out/cancelled wheel
results refuse. Prior admission-owned exact-subject success reuse remains intact.
The leaf's producer and both pip/uv consumers stay selected together; the original
legacy PR/manual/MCP routes are preserved. No trigger retirement or activation.

Two new pre-wiring controls fail on composed baselinebc2304cf at missing-call
and false-green wheel-result assertions (native101). The existing docs decision
fixture gains only a successful wheel prerequisite; its negative assertions and
named-step binding remain. Existing old source/test green records are historical
and do not replace the independent effective-shell repair proof below.

**Historical finding5528944270 — source repair integrated:** implicit release-step
Bash lacked pipefail while the original CI harness enabled it, allowing pkgid
failure to be masked by sed. The assigned CI author supplied reviewed successor
`327d7289d6408789f75bb3550143c93ba1f73977`, integrated at
`3247bf14963c597798639a56c05b9ba03cff206d`. Both release steps now declare Bash;
the harness derives the effective production shell. Independent real-script
controls preserve native47 with valid stdout and prevent downstream commands;
removing either shell reproduces the false green. The source finding is addressed.
Combined wheel/caller local proof and hosted native qualification remain separate;
this correction grants no activation or overall qualification.

The delegated wheel handoff requests combined check-fast/precommit because its
broad local epoch stalled on I/O. Combined executor is Linux x86_64/Bash/Rust1.95,
with an exclusive warm target at `/workspace/caller-integration-proof/target`.
Before the broader batch,30GiB free and16GiB available memory were observed;
retained focused output328MiB. The root-owned peak additional budget is expanded
to16GiB with8GiB reserve, two Cargo jobs, dev debug/incremental expansion disabled.
This replaces only the earlier4GiB generation budget. No existing resource is
removed or counted as prospective reclaimed capacity. The root retains native
session handles/logs, inspects actual terminal statuses, and keeps broad results
separate from source qualification and activation. PR8082 remains Draft.
