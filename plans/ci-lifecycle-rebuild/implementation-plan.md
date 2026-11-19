# RIPR CI lifecycle rebuild implementation plan

Plan: RIPR-PLAN-0064
Status: proposed execution design; installed behavior is recorded separately
Owner: existing repository CI and compiler-boundary owners
Linked proposal: n/a; selected direction is retained in #4984 and #14500
Linked specs: [ProductGatePlan](../../docs/ci/PRODUCT_GATE_PLAN.md)
Linked ADRs: [ADR 0027](../../docs/adr/0027-one-user-facing-package-lockstep-support-crates.md)
Live issue / PR: [#14500](https://github.com/EffortlessMetrics/ripr-swarm/issues/14500), [#8198](https://github.com/EffortlessMetrics/ripr-swarm/issues/8198), [#11566](https://github.com/EffortlessMetrics/ripr-swarm/issues/11566)

This plan sequences delivery. [CI strategy](../../docs/CI.md#qualification-rebuild-architecture)
owns the execution architecture, [merge flow](../../docs/MERGE_FLOW.md) owns
the installed request/landing route, and native issues/PRs own current work.
No plan row appoints a worker, clears a hold, activates a workflow or changes
protection. Resume the existing candidate at its earliest missing judgment.

## Current state

The inspected source is main
`0c058d5179586b04ad1f0e874608757d7e7f0829`; the
[installed-state inventory](../../docs/ci/current-state.md) names its current
producers and receipts. Ordinary development Actions are largely retired,
but the explicit label route still runs complete PR qualification before
complete queue qualification. The primary new target is under 20 total raw
runner-minutes per complete required qualification, not merely a sub-20-minute
critical path.

The [eleven-run baseline cohort](../../docs/ci/current-state.md#baseline-cohort)
spans 83.42–109.47 raw runner-minutes,
median 98.63. [Run 44451072622/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44451072622)
cost 102.60: gates 44.95, test shards 25.22 and 29.52, other jobs 2.92.
The docs-only [run 44445213794/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44445213794)
still cost 24.37. These are observations on different subjects, not a matched
experiment or a forecast of savings.

The target requires approximately 80% less work than the current full-run
median. No additive saving is claimed from overlapping test durations.
Changes to triggers, preparation, compilation units, fixtures, execution and
retirement address distinct contributors.

## Delivery dependencies

Proceed with independent bounded corrections while preserving current
qualification. Three paths can advance without inventing competing owners:

1. Product-free policy leaves and correct whole-group selectors.
2. Compatible native preparation and test-consumer reuse.
3. Accepted compiler-unit boundaries and measured execution placement.

They join before activating the complete G portfolio. The protected finalizer
and native lifecycle path have their own proof prerequisites; their enabling
changes need not wait for the final compute target. Conversely, economical
workers do not authorize a protection cutover.

The existing staged DAG #13514 already owns ProductGatePlan's lightweight
migration and execution representation. Do not start another extraction.
The #13481 / #14493 / #14501 source-custody hold remains: recover the existing
verified coordinated bundle before changing or qualifying that stack.

## Work item: reconcile the executable plan

Status: blocked on the existing staged candidate's integration/proof, not new design authority
Live owner: #8197 / #8198; existing candidate #13514
Blocked by: current-main reconciliation and ownership of the staged source
Blocks: replacing workflow-derived obligation truth

### Goal

Have one plan describe every applicable mandatory obligation and its actual
preparation/evidence dependencies, independently of runner placement.

### Production delta

Continue #13514's pure-plan and repo-policy work. Reconcile its staged branches
with the installed route before activation: its current eight expensive
branches, private controllers and unpartitioned tests are not an accepted
economical topology. Join existing policy, corpus, capability, PR-evidence and
extension producers as well as the 13 core ProductGatePlan IDs.

### Acceptance and proof

- Compare the old and candidate plans on docs, product, optional features,
  extension, workflows, policy, mixed and unknown changes.
- Preserve command/configuration, selected cases, doctests, ignored cases,
  quarantine/retry semantics and required result meaning.
- Each mandatory obligation executes exactly once or consumes explicitly
  compatible authenticated evidence. Unknown is not an empty success.
- Seed independent failures in policy, tests, doctests, static Rust, contracts,
  extension and evidence; each must refuse the final result.
- Run the existing plan/parity tests, `cargo policy check-workflows`,
  `cargo xtask ci-policy` and selected complete hosted qualification.

### Rollback and non-goals

Keep the installed producer path until parity and economics are accepted.
Revert the projection and its call site together. No new scheduler, registry,
controller service, provider integration or prerequisite fleet.

## Work item: make product-free checks product-free in execution

Status: ready for narrow retained-obligation slices
Live owner: #14634 / #14644; #14772 and #14781 are landed interfaces
Blocked by: each check's semantic parity and real command adoption
Blocks: activating an honest lightweight docs/policy route

### Production delta

Move eligible existing semantic owners into repo-policy, retaining delegating
xtask facades. The documentation-role check is one bounded leaf; it does not
replace the whole precommit table. Verify the lightweight executable identity
before it judges inputs. Keep product-dependent checks with their producer.

The trusted-base user-text matcher has a separate existing #8198 extraction
owner. Land its lightweight interface first; switch the workflow only after
the trusted base contains it. A candidate-only matcher cannot judge itself.

### Acceptance and proof

- Positive and missing-heading/invalid-input fixtures retain verdicts, report
  material, scope and useful failures through both entrypoints.
- Reject a stale executable or implementation mismatch before evaluation.
- Show the actual Cargo graph and native command build neither ripr nor
  product-linked xtask. A feature-only dependency count is not installed-path
  evidence.
- For user text, keep the complete golden/LSP input inventory and a candidate
  that weakens its own matcher but still fails under the protected base.
- Run the affected `cargo test -p repo-policy` tests and current source/policy
  checks; qualify the real consumer before claiming runtime savings.

### Rollback and non-goals

Keep the old facade compatible. Revert the caller and owner move coherently.
No required-check removal, new advisory exemption, docs-gate activation or
wholesale transfer of the full policy table.

## Work item: reuse candidate binaries and compiled inventory

Status: active in existing Draft candidates; acceptance remains open
Live owner: #11566 / #11602 / #13705; #13906 and #13481 / #14493 / #14501
Blocked by: source-custody recovery where applicable, compatibility and complete-cost proof
Blocks: eliminating independent preparation by every consumer

### Production delta

Give each compatible Cargo build family one preparation owner. Its manifest
binds source, target, toolchain, resolved features, profile, flags, relevant
environment, Cargo-selected paths and output digests.

Guide tests borrow the exact prepared CLI rather than invoking Cargo in every
nextest process. Record reuse versus fallback. Keep intentional build/install
tests in private bounded fixtures. Preserve local fallback where useful, but
qualify a genuinely cold fallback against its deadline.

Pass the native test producer's inventory to check-file-policy. Preserve
package-only versus workspace-unified feature/host differences; matching test
names alone is insufficient. Keep check-covered-by's source scan separate.

### Acceptance and proof

- Wrong source, feature/profile, toolchain, path, digest and producer attempts
  refuse reuse; missing preparation safely rebuilds or returns its real failure.
- Trace actual Cargo launches and which consumers reached the shared path.
- Keep the exact-cover verifier and bijective case/binary identities. The
  recorded 31,435-case/411-binary run is a baseline, not a forever fixed count.
- Run the affected guide, file-policy, producer-context and shard-verifier tests.
  Demonstrate the real CLI and native inventory, not just receipt serialization.
- Charge producer, archive/cache, every shard, transfer, extraction and cleanup.
  #13906's retained 927.7-second private cold preparation versus its 600-second
  helper deadline is an unresolved acceptance gap, not a successful cold proof.

### Rollback and non-goals

Retain the current complete-build path until the replacement is accepted.
Do not reuse the rejected 7,003,890,964-byte archive representation: its retained
8m19 build/archive plus 4m23 upload preceded consumers. A small selected-binary
sample is not a qualified replacement archive.

## Work item: lower the compiler-unit floor

Status: active accepted architecture, remaining implementation not complete
Live owner: #8382 / #12726 / #14199 and ADR 0027
Blocked by: existing contract/outcome prerequisites
Blocks: economical native compilation within the runner memory envelope

### Production delta

Continue private production-plus-private-test extraction while keeping the
user-facing ripr package/library/binary and existing re-export paths.
Follow the accepted order: contract/outcome boundaries, TypeScript adapter,
Python, then LSP/MCP if still needed. Stop when measured criteria pass.

#14524 and #14418 are already merged. Retain the dependency sequence for
#14523 / #13781 / #13782. Reconcile #14519 / #14520's test-binary boundary
work against the landed #14418; it is distinct from the largest libtest unit.

### Acceptance and proof

- Attribute the largest compiler's own peak against the
  8,000,000,000-byte criterion; distinguish child-inclusive maximum RSS and
  aggregate concurrent memory.
- Measure elapsed compilation, total CPU/runner cost, swap and host headroom.
  A lower RSS with slower total compilation is a mixed result, not a speed win.
- Preserve public API paths, feature matrices, tests, package lists and install
  behavior; run the affected crate tests, public-API/workspace-shape checks and
  package/install proof appropriate to the boundary.
- Use matched fixture/configuration evidence and cold correctness. More source
  modules in the same crate do not establish a smaller compiler unit.

### Rollback and non-goals

Revert each lockstep internal boundary with its imports/re-exports and package
metadata. No new public product taxonomy, generated recombined source package,
or test-only extraction claim that it removes the whole frontend memory floor.

## Work item: reduce test and evidence preparation

Status: ready within existing fixture/evidence owners
Live owner: #11566 / #8198; retain #13973's separate Test Analytics scope
Blocked by: actual call tracing and assertion-preserving controls
Blocks: execution cost remaining after build deduplication

### Production delta

Trace the slow cases and remove repeated fixture setup, product discovery,
repository walks and unnecessarily broad imports. Keep OS/process behavior
where that is the assertion. Pure test batching is a separate measured option,
not a blanket removal of nextest isolation.

The representative PR-evidence step spent two 120-second timeouts obtaining
explicit unavailable base/head analyses. Reuse valid exact-base facts and exact-G
analysis; bound repeated attempts. Keep mandatory packet/schema checks. Any move
of currently required analysis to another posture needs an explicit decision.

### Acceptance and proof

- Keep case identity, stimulus and asserted behavior unchanged.
- Add shared-state contamination, child-cleanup and wrong-artifact controls.
- Record overlapping per-test durations separately from runner occupancy.
- Run focused affected tests and existing packet/schema/corpus contracts, then
  compare the whole actual required route.
- Preserve the already-landed combined golden/fixture evaluator (#14630).

### Rollback and non-goals

Revert the fixture/preparation change without changing required obligations.
No arbitrary timeout reduction, quarantine, feature/filter reduction or
reclassification of an assertion failure as a flake.

## Work item: select and finalize the complete native subject

Status: mixed; bounded selector corrections can land independently
Live owner: #8082 / #14500; preserve #13862 / #13863 staging ownership
Blocked by: dynamic membership, source authority and native canaries
Blocks: cheap-H / combined-G-only production cutover

### Production delta

Freeze protected B0, complete ordered PR/H membership/dependencies and G/tree
independently of the event's immediate speculative base. Keep control generation
C protected. A native upper PR's immediate base branch need not be main;
validate its authorized stack destination instead.

Correct each current selector at its existing owner. #15128 owns blobless docs
history fetching, not proof that the selected base is the protected anchor.
The extension selector's current event-base assumption is a distinct bounded
correction; it must preserve conservative selection on unavailable history.

The finalizer must distinguish successful lower-prefix settlement from
withdrawal, changed membership or recomposition. Bind exact-H checks to H;
authenticate source/publisher separately from the displayed check name.

### Acceptance and proof

- Real Git/native cases: lower extension plus upper docs, lower Rust plus upper
  docs, lower control change, non-main stack base and exact ordered membership.
- Current and revoked heads, missing or extra members, recomposition and
  successful-prefix settlement.
- Isolated protected caller omission/same-name-success spoof must not pass.
- Full actual product canary, not only the isolated small-contract rehearsal.
- Check event count and heavy execution count separately. HEADGREEN does not
  promise GitHub stops dispatching prefix events.

### Rollback and non-goals

Keep current full qualification and protection. No position-1 restriction,
fabricated prefix success, permissive skipped aggregate, direct merge fallback,
settings mutation or new App/controller prerequisite.

## Work item: own retries, failure termination and obsolete work

Status: ready for a bounded existing-controller design; native predicate pending
Live owner: #14500 / #8082 and existing shard-reuse owners
Blocked by: authoritative exact-G lifetime mapping and attempt policy
Blocks: economical retries and reclamation without cancelling valid work

### Production delta

Separate admission transaction, producer attempt and finalizer attempt.
Query native job facts before accepting earlier producer evidence. Support
finalizer-only recovery for a still-valid G without recompiling valid results,
only after the replacement attempt contract is qualified.

Before expensive phases, check the frozen G lifetime. For long work, a bounded
read-only watchdog may terminate and reap its own children on positive
retirement. Add a short protected exact-run reconciler only if needed for queued
and sibling jobs; it must never execute candidate content.

### Acceptance and proof

- Valid speculative siblings survive another run starting or main advancing.
- A lower member landing does not itself revoke a valid upper G.
- Inject a required producer failure while sibling work remains; stop and reap
  only work made unnecessary by that failure, retain the original failure and
  diagnostics, and keep the final result failed rather than treating cancellation
  as missing or reusable success.
- Positively retired G children stop and are reaped; replacement G survives.
- Unknown/ref-read failure, pagination ambiguity and transient API failure do
  not manufacture retirement or success.
- A newer failed/cancelled producer with no artifact cannot hide behind an old
  green artifact. Wrong run/configuration/generation refuses.
- Cancellation races with reruns are qualified: GitHub cancels by run ID, not
  attempt, so whole-run cancellation needs whole-lifetime retirement.

### Rollback and non-goals

Retain current conservative refusal on unsupported recovery. Disable an unsafe
retirement optimization without weakening final qualification. No per-main or
per-PR broad cancellation, nonexistent merge_group destroyed Actions trigger,
resident polling service, or retry of unchanged deterministic failures.

## Work item: activate, measure and reconcile

Status: blocked on accepted producer/trust/native prerequisites
Live owner: #4834 / #4838 / #4840 / #4841 / #4842 / #14500
Blocked by: preceding cutover-critical acceptance, separately authorized settings transaction
Blocks: declaring the full lifecycle and economics delivered

### Production delta

Use one explicit source/settings generation, with readback and rollback.
Retire the remaining label/bootstrap and routine event edges only after the
replacement is reachable and enforced. Preserve separate security, incident,
deep and release obligations. External publishers have their own perimeter:
do not replace constituent-commit secret coverage with an unqualified G-tree-only
scan that loses a secret added and removed inside the original history.

### Acceptance and proof

- Singleton and two-/three-member dynamic native stacks land only with the
  complete applicable G proof. Bad middle, stale member and late blocker refuse.
- Zero ordinary development starts across fresh and older contexts; do not
  make routine restacking the operating model.
- Full-run occupied runner-minutes below 20 for the accepted target cohort,
  plus separately reported elapsed, cold/warm, CPU, memory, disk, transfer,
  failures, retries, invalidations and cost per landed PR.
- No obligation is moved outside the measurement window to make the total pass.
- Exact landed M readback and one coherent rollback generation.

### Rollback and non-goals

Pause new admissions, reconcile in-flight Gs, restore full qualification and
verify its reachability before removing any queue-only protection.
No bypass, silent requirement weakening, release publication or new fleet spend.

## OpenClaw evidence informing these choices

These are primary October–November implementation receipts, not a claim that
OpenClaw's complete CI topology is the right cost target for RIPR.

| Change | Observed result and limit | Transfer to RIPR |
| --- | --- | --- |
| [#195712](https://github.com/openclaw/openclaw/pull/195712), narrow tooling compilation | Matching 227-case hosted row: inputs 18,898→554; outputs 24,117→368; preparation 41.769→2.514s; test step 191→166s. One observational pair; checkout also improved independently. | Narrow the actual required consumer's graph. The first allowlist matched no complete production rows, so a good local microbenchmark alone was ineffective. |
| [#183498](https://github.com/openclaw/openclaw/pull/183498), fixture/process reuse | Comparable UI API job-minutes 106.00→88.33; post-preflight completion 643→593s. One native pair, not normalized causality. | Remove repeated CLI processes, app boots and broad fixture imports while retaining behavior. |
| [#199560](https://github.com/openclaw/openclaw/pull/199560), compiler/cache lifetimes | Corrected accidental overlap of two large compiler graphs and retained input snapshots. No measured speed win claimed; failed cold/memory cache opt-in was removed. | Cache eligibility cannot multiply concurrency; retain compact metadata and join descendants before releasing resources. |
| [#196563](https://github.com/openclaw/openclaw/pull/196563), final-owner verification | Removed 170,510 redundant input/output verification entries across five borrowers; 4.750s shutdown tail is an upper bound, not measured saving. | Verify at admission and final post-join ownership boundaries rather than repeating identical scans at every wrapper. |
| [#198025](https://github.com/openclaw/openclaw/pull/198025), clean cache generation | Five paired hosted offline replays: archive 2,244,425,757→957,104,890 bytes; median extract/install 31.535→12.211s, complete inventory retained. WAN and production latency unmeasured. | Define cache contents and cold completeness; pruning that removes needed snapshots is not acceptable. |
| [#198159](https://github.com/openclaw/openclaw/pull/198159), cache-key / publisher activation | New immutable key activated the corrected publisher; production archive 57.24% smaller. Production reader adoption, PR latency and billing savings were not measured. | Track source landed → producer ran → reader adopted → work avoided → whole-route saving as separate states. |

OpenClaw also rejected a shared-build producer with a ≥722-second prerequisite
before transfer, and a three-to-two job reduction that increased cost and left
a 761-second job (#183498). Therefore compare same-runner reuse and small
compatible execution units; do not make one giant preparation job or fewer
shards an architectural rule.

## Evidence protocol and completion

Use existing native receipts before purchasing new baseline runs. For each
selected candidate, retain exact source/base/G, effective configuration,
nonempty obligation/test inventory, command counts, producer/consumer paths,
native status and resource measurements. A local model, source assertion or
serialization test does not establish installed behavior.

Use the smallest discriminating local control, an old/wrong implementation
witness, then selected hosted proof. Counterbalance matched performance
comparisons when needed and retain unfavorable attempts. Do not run broad
qualification for every candidate variant. Five paired replays can support a
narrow cache experiment; they are not a new universal five-full-CI-run gate.

For this documentation package, run:

```bash
git diff --check
cargo xtask check-doc-index
cargo xtask check-doc-artifacts
cargo xtask check-traceability
cargo xtask check-capabilities
cargo xtask check-pr
```

Missing local Rust tools remain missing native proof; publish the exact
candidate through the installed hosted route rather than claim these commands
passed. No source-only review completes the CI redesign. Completion requires
the actual protected lifecycle, conserved required portfolio and measured
economics described above.
