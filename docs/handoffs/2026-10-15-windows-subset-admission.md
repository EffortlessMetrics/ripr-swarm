# Inactive native merge-ready / Windows subset handoff

Owner: existing #4987 migration, coordinated with #4986/#4836 and their current
producer owners. Base: `9543a9bd733b0921886306abd2c2609fbe73b62e`.
Scope: a native label admission prototype and the existing Windows subset;
no #8002 specification-evidence changes, workflow activation, PR or settings.

## Governing reconciliation

The current selected minimum is **pull_request:labeled**, with `merge-ready`
as the explicit request. Draft, pushes and Ready alone are not qualification
requests. No native merge queue, new controller, custody database or admission
service is needed. This supersedes this branch's first prototype at 9b8c8eb3b,
which proposed controller-generation/atomic-custody prerequisites and a parallel
Rust decision model. That model and its module registration are removed.

Historical #4986/#4836 queue/G prose and the earlier local migration plan's
controller requirements are not the current implementation direction. Reconcile
those existing issue/instruction owners before activation; do not create rival
issues or edit their branches. The parent reports that the active ruleset has no
native queue. Complete classic protection and strictness remain unknown; this is
an activation/integration evidence gap, not a reason to add a queue.

Main-health sampling (both >=1 hour and >=10 merges) is a **separate proposal**,
not a prerequisite for this slice or justification for an extra full merge run.

## Implemented inactive source

`fixtures/convergence/windows-subset/merge-ready.yml` is the native event
prototype, outside `.github/workflows` and therefore unactivated. It retains the
existing routed Rust/docs producers and fail-closed result owner, adding the
staged Windows subset evidence child. It is not the completed cross-workflow
portfolio migration: wheel, minimal/nondefault features, security, installation,
editor and other independent producers keep their existing active behavior.

The preflight does not check out or execute candidate code. With read-only
GitHub API capability it verifies the request actor's repository write permission,
live open/non-Draft/same-repo/main-target PR state, expected H/B and the current
synthetic integration commit T, request-label presence, current independent
review and ordered B/H integration parents. Current change-requesting reviews
refuse. The existing readiness marker `REVIEW_READY` on a current independent
COMMENT review is supported; a current independent APPROVED review is also
recognized. Review adequacy remains an owner judgment: a quota notice, cancelled
provider, wrapper green or absence of findings is not a review receipt. #4985's
existing readiness consumer/instructions must reconcile this native projection
with source review during Draft and final review after qualification, without
imposing a new approval quota or accepting a naked LGTM as substantive proof.

Native workflow concurrency serializes actual requests for a PR with
`cancel-in-progress: false`. Each unrelated-label delivery has a separate
`metadata-{run_id}` group, so label noise cannot displace pending qualification.
Concurrency is **not sequential deduplication**: preflight independently reads
paginated native workflow runs and reconciles an earlier request's exact
workflow/H/B/T run title and completed required result job. Compatible success
or failure is reused without rerunning producers; unreadable, pending or missing
result evidence fails closed. A failure is not retried merely by relabeling;
explicit native attempt rerun or a repaired changed subject has a separate identity.

The result job always evaluates actual requests. Its actual expression emits
`Ripr Rust Small Result` only for the merge-ready labeled event; unrelated
labels emit `Merge-ready Nonrequest`. A failed preflight, incomplete outputs,
failed selected Rust/docs route or missing/failed Windows evidence cannot become
a skipped successful required check. Duplicate success is based on the actual
prior required result job, not the workflow wrapper's conclusion. Duplicate
failure remains failure. Final API read rechecks live H/B/T, Draft/open state and
request label before publication; the protected merge owner still performs the
final vision/current-state and review decision after green. Nothing auto-merges.

`fixtures/convergence/windows-subset/qualification.yml` is the callable product
producer staged beside the native caller. All original native subset commands,
inline oracles, status/log/receipt checks and cleanup are retained. Intentional
changes: exact integration checkout with full history/no credentials, required
H/B/T/tree/workflow-generation/run-attempt data, source precheck before product
work, separate original H/B versus executed T receipt, and request-qualified
artifact name. Caller and product permissions remain read-only. The native
caller invokes only after preflight; no privileged candidate execution is added.
Original full Windows twice-run behavior is unchanged. Advisory product FAIL
and mandatory evidence-generation FAIL remain separate; green subset wrapper
is not proof that all advisory tests passed.

## Proof

Seven standalone Rust source-contract tests pass, zero ignored/filtered, with
Rust 1.95.0 and strict standalone Clippy. They compare the entire old/staged
subset after the intentional identity deltas and pin the actual event/name/
concurrency/finalizer declarations; static-required-name and subset-removal
mutations are rejected. Rustfmt passes.

The actual retained Bash preflight and aggregate bodies were executed in 22
bounded mocked-GitHub replays: current request, unrelated/missing label,
stale head/base, Draft/closed/withdrawn request, missing review, wrong parents,
sequential duplicate success/failure, missing prior result and unreadable run
lookup, selected/admission failure and nonrequest result. No live API mutation
or product execution occurs in this replay. Two initial harness setup errors
(unset summary variables and overwritten label environment) were corrected;
only the terminal complete replay is passing evidence.

Five cases evaluate the actual YAML result-name, concurrency-group and run-title
expressions, including case-insensitive merge-ready and unrelated labels. The
replay implements only the used GitHub expression subset (logical operators,
case-insensitive equality and format), not a general Actions scheduler.
Three wrong workflow implementations are rejected by those unchanged replay
expectations: a static required result name, shared unrelated-label concurrency,
and removal of sequential deduplication. The unmodified source passes afterward.
Actionlint 1.7.7 passes with both local staged callers/callees materialized in a
private lint fixture and the retained existing rust-gates workflow; external
shellcheck/pyflakes adapters are disabled. Native Windows/PowerShell, hosted
workflow behavior, standard Cargo/precommit and real API permission/readback
remain unestablished. No <=20-minute or global-zero-launch claim is made.

Prepared repository test command:

```text
cargo test --locked -p xtask --test windows_subset_admission_contract
```

The native replay driver, exact responses/logs, expression results, compiled
witnesses and final patch/bundle are retained in `/workspace/windows-subset-proof`.
Changelog: `changelog.d/4987-stage-windows-subset-admission.md`, canonical
Internal / CI marker and trailing (#4987).

## Exact rollout prerequisites

1. Refresh #4987/#4986 ownership and reconcile the native label request,
   source-review/readiness projection and final post-green decision in existing
   instructions. Preserve #7929 runner and #7915/#7920 review owners and #8002.
2. Complete current native readiness/findings interpretation and prove pagination,
   missing permissions and review-state changes against actual repository records.
   API failure is refusal, never empty review or absence of protection.
3. Promote caller/callee together as a reviewed source generation, with tests
   checking actual expressions. Verify native run-title/job identity and
   sequential deduplication; an unrelated label must not create the required
   name or share request concurrency. Do not substitute a draft job guard.
4. Run focused standard workflow/policy checks and separately admitted native
   Windows rehearsal on exact T. Verify source/attempt receipt, nonempty test
   counts, native status/log/receipt agreement and both cleanup outcomes.
5. Wire each applicable existing producer and selected denominator to the
   existing finalizer before claiming graph-wide migration. Preserve wheel,
   feature/runtime, security, package/install/editor and PR-evidence obligations;
   no copied selector or extra default full suite. The staged aggregate here
   covers the existing routed Rust/docs plus Windows evidence boundary only.
6. Observe actual required-check/app mapping and strict-base behavior. There is
   no native queue. Separate head/base reads and an expected-head squash merge
   are not atomic expected-base CAS. Refresh affected integration proof when B
   materially changes; retain unaffected review/local evidence. Recheck final
   readiness/current state after green and read back landed tree M against T.
7. With separately authorized coherent activation, change the native caller and
   existing subset event allocation together. Keep independent daily/manual
   Windows purpose explicit. Observe natural Draft/push/Ready/ordinary labels
   versus explicit request, refusal, duplicate and protected squash behavior.
   No settings relaxation or protection bypass is part of this implementation.

Rollback before activation removes the staged fixture/test/docs additions;
active workflow bytes are unchanged. Later activation needs matching source and
result-generation rollback through the existing rollout owner. Main-health and
whole-program economics do not block this source slice.

## Resource custody

Isolated branch `codex/4987-windows-subset-admission` and worktree
`/workspace/windows-subset-admission` remain owned by this lane. The original
checkout and prior plan files are untouched. `/workspace/windows-subset-proof`
retains the pinned toolchain, compact source/test/replay evidence and witnesses.
No Cargo target or CI run was allocated; the 2 GiB additional-storage budget and
8 GiB reserve remain intact. A transient executor disconnect was checked by a
simple repository read on the same environment; execution resumed without
executor substitution or broader access.
