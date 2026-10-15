# Inactive Windows subset admission handoff

Owner: #4987 cross-workflow migration; trusted admission remains #4986.
Candidate base: `9543a9bd733b0921886306abd2c2609fbe73b62e`.
Scope: the Windows advisory subset only, not the full Windows suite or #8002's
specification-evidence semantics. No live workflow or setting changes.

## Governing correction

The current user target is settled: **no native merge queue**. Draft, ordinary
pushes and Ready alone allocate no broad qualification in the activated target.
An explicit merge-ready request for a reviewed non-Draft H admits proof against
frozen base B and integration subject T. The final current-state/vision decision
follows green proof and precedes protected squash. Existing #4986/#4836 bodies
still describe queue/G profiles; those are historical conflicting interfaces to
reconcile in the same issues, not authority to introduce queue activation.

Current `windows-advisory.yml` still runs its subset on PR events. This candidate
does not claim zero-launch behavior is activated. Keeping that producer while
staging a tested replacement avoids silently removing mandatory native evidence.

## Source delta

`xtask/src/convergence/domain/admission/windows_subset.rs` is the existing
convergence admission seam extended with a pure decision. A trusted adapter must
authenticate the explicit request and acquire current PR, review, proof, complete
selection and Git commit observations. Caller JSON/labels/Ready are not authority.

The decision checks open/non-Draft, same trusted repository/main target, exact
PR/head/base/integration parents+tree, controller generation and current review/
proof. Missing request and unreadable custody refuse separately. An observed
absence proposes **Claim**, which is not a launch permission or required success.
An equivalent persisted request returns its actual producer run/attempt/state,
including failure, instead of launching again. Conflicting, incomplete or stale
custody refuses. A new request ID does not evade exact-subject deduplication.

`fixtures/convergence/windows-subset/qualification.yml` stages a workflow-call
producer outside `.github/workflows`. No active caller references it. Its native
commands, inline test oracles, status/log checks and cleanup are byte-preserved
against the existing subset by a Rust source-parity test. Deliberate changes:

- All six identity inputs are required; checkout is exact T with full history
  and no persisted credentials.
- A PowerShell precheck checks canonical object IDs, request run-attempt ID,
  actual workflow generation, checkout/tree and ordered B/H parents before
  toolchain/product work.
- The selector binds its producer to T. Original H/B and controller generation
  are retained separately in `windows-subset-source.json`, with product run/attempt.
- PR-event trust guards become unconditional execution only inside this admitted
  producer: the trusted caller must refuse untrusted input before invocation.
  No fork/persistent-runner elevation is implemented or authorized here.
- Artifact name includes the admitted request. Product permission remains
  contents:read; there is no required-check write or admission authority.

This staged producer preserves advisory product verdicts and mandatory evidence
generation. Its green job still does **not** claim every advisory test passed.
The selected required portfolio's finalizer must inspect native statuses and
receipt dispositions, not promote wrapper success. Full Windows's intentional
two-run population is unchanged and not replaced by this focused subset.

## Proof and limits

The bounded test entry is
`xtask/tests/windows_subset_admission_contract.rs`. It imports the actual owning
Rust source and existing convergence DTOs, without compiling the product parser
stack. Its domain cases exercise the production `decide` function; source tests
compare the entire old/staged job after the listed identity changes. A restoration
or removal must differ from the unchanged command-parity oracle.

Prepared normal package command for the implementing owner:

```text
cargo test --locked -p xtask --test windows_subset_admission_contract
```

The local proof uses pinned Rust 1.95.0 on that same entry: **15 tests pass**, zero
ignored/filtered. Standalone strict Clippy (`clippy-driver --test -D warnings`)
also compiles that entry without warnings. Three wrong implementations separately
compile successfully then exit 101 at unchanged behavioral assertions: stale base
admission, unreadable custody treated as absence, and duplicate relaunch. The
unmutated final entry passes. Rustfmt and actionlint 1.7.7 pass; actionlint's
external shellcheck/pyflakes adapters were disabled, so shell execution is not
claimed. YAML parsing confirms call-only inputs, exact checkout and read-only
authority.

This is actual domain/source-contract execution, not a whole-package Cargo check,
repository policy pass, hosted CI or native Windows qualification. Cargo
doctor/check-fast, full owning-package compilation/precommit and native PowerShell/
Windows execution are remaining integration dimensions. No empty filtered Cargo
run or fabricated Windows receipt is claimed. The package baseline was not run:
the initial host had no toolchain and the bounded proof deliberately avoids the
product-dependent xtask build. No baseline package pass is attributed to this
standalone module result.

Changelog: `changelog.d/4987-stage-windows-subset-admission.md`, canonical
Internal / CI marker, bullet and trailing (#4987). No spec lifecycle, traceability,
support promotion, production analyzer, permissions or required identity change.

## Exact rollout prerequisites

1. #4987/#4986 re-read current ownership and reconcile the historical queue prose
   to the settled no-queue H/B/T/M contract. This slice neither replaces their
   controller nor claims its deployment. Current all-state finding searches and
   #4987's latest comments found no competing subset admission migration; preserve
   #7929 runner, #7915/#7920 review and #8002 spec owners.
2. The existing trusted default-branch controller authenticates actor/request
   origin and source generation. Fetch actual PR open/draft/head/base, independent
   review/findings, applicable proof and complete applicability inputs. Validate
   commit objects through the existing Git observation port. The pure normalized
   inputs alone cannot establish these external facts.
3. Implement controller-native **atomic** custody claim/readback for the exact
   repository/PR/target/H/B/T/producer+plan generation/profile. `Decision::Claim`
   needs compare-and-set under serialized custody; two simultaneous absent reads
   must produce one persisted winner. Persist producer run/attempt and reconcile
   lost responses. Existing `Reuse` returns unchanged running or failed work;
   retries require separate explicit disposition, never a new request-ID loophole.
4. Authenticate the call edge and bind generation to the reviewed selector,
   producer and policy blobs. The six workflow inputs are data, not an admission
   capability. Invoke only after claim succeeds; keep controller privilege/cache
   separate from candidate execution. Re-read/revoke on H/base/findings/Draft
   movement; stale completion cannot publish current protected success.
5. Run the standard focused Cargo/workflow/policy checks and a separately admitted
   native Windows rehearsal on exact T. Preserve the four focused native commands,
   nonempty test counts, selector controls, captured status/log/receipt agreement,
   source receipt and producer/staging cleanup. Test wrong SHA/tree/parents before
   product allocation. This Linux source test does not execute the PowerShell
precheck or prove filesystem/process behavior on Windows.
6. Wire the existing selected-portfolio finalizer to authenticated native artifact
   import and current run/attempt. Missing artifacts, advisory FAIL, malformed or
   partial evidence, wrong subject/generation, cancellation and stale attempt
   cannot satisfy mandatory selected proof. Keep `Ripr Rust Small Result` exact
   on H only after completed compatible integration proof; cheap admission cannot
   green that context on a no-queue branch.
7. #4832/#4834 verify actual live protection/app/base semantics and coordinated
   merge-writer custody. Expected-head merge alone is not expected-base CAS.
   Base movement requires affected integration disposition, not old T proof.
   Preserve current protections and obtain authorized settings observation; do
   not weaken strict checks to meet the one-run target.
8. Only after separate authorized coherent activation, promote the fixture to a
   registered reusable workflow, replace the existing subset caller in the same
   generation, and remove its ordinary PR-event allocation. Keep daily/manual
   diagnostic purpose and full-suite contract explicit. Run event-level natural
   canaries for Draft/Ready/push/labels, explicit request, missing/stale/duplicate
   custody and final proof. No default skipped-workflow forest as the end state.
9. #7827 qualifies cache continuity before any full main-push producer retirement;
   #4841 measures normal <=20-minute qualification without reducing proof. Main
   health is controller-selected only after both >=1 hour and >=10 accepted
   merges. This subset slice does not implement health or launch per-merge replay.

Rollback before activation removes these staged additions and the one module
declaration; active workflow bytes are unchanged. After activation, #4838 restores
the complete matching caller/producer/controller/result/settings generation,
retaining original attempts and receipts. No source-only emergency success path.

## Resource custody

Lane-owned worktree `/workspace/windows-subset-admission`, branch
`codex/4987-windows-subset-admission`, shared Git common directory owned by the
original repository, and `/workspace/windows-subset-proof` are retained for parent
review. The latter holds the workspace-local pinned toolchain, bounded test
executables/logs and wrong-implementation witnesses. No Cargo target was built.
Storage admission was 2 GiB additional with an 8 GiB reserve against 30 GiB free;
no unrelated state is eligible for deletion. The original checkout and earlier
`plans/late-ci-migration` deliverables remain untouched.
