# RIPR CI lifecycle rebuild implementation plan

Plan: RIPR-PLAN-0064
Status: proposed execution design; installed behavior is recorded separately
Owner: existing repository CI and compiler-boundary owners
Linked proposal: n/a; selected direction is retained in #4984 and #14500
Linked specs: [ProductGatePlan](../../docs/ci/PRODUCT_GATE_PLAN.md)
Technical design: [Qualification rebuild contracts](../../docs/ci/QUALIFICATION_REBUILD.md)
Linked ADRs: [ADR 0027](../../docs/adr/0027-one-user-facing-package-lockstep-support-crates.md)
Live issue / PR: [#14500](https://github.com/EffortlessMetrics/ripr-swarm/issues/14500), [#8198](https://github.com/EffortlessMetrics/ripr-swarm/issues/8198), [#11566](https://github.com/EffortlessMetrics/ripr-swarm/issues/11566)

This plan sequences delivery. The [technical reference](../../docs/ci/QUALIFICATION_REBUILD.md)
owns the proposed implementation contracts, [CI strategy](../../docs/CI.md#qualification-rebuild-architecture)
indexes the architecture, and [merge flow](../../docs/MERGE_FLOW.md) owns
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
runner-minutes per normal accepted submission/stack across all associated
merge-group attempts, runner-allocating prefixes, retries/replacements and
required preparation. Retain the separate under-20 elapsed objective; a short
critical path or favorable final G alone cannot establish the aggregate target.

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

#8082 is a reference-only split tracker under its recorded disposition, not a
branch to revive or qualify. Continue its current-main successor responsibilities
and preserve #13862 / #13863 staging custody.

### Dependency and release boundaries

| Slice | Existing candidate / claim | Required predecessor | First accepted consumer / completion evidence |
| --- | --- | --- | --- |
| Whole-group extension selection | Draft #15176 under #14500 | Existing required route | Real prefix/settlement/unknown controls, then actual selected extension result; no whole-membership claim |
| Product-free document roles | Draft #15177 under #14644 | Existing repo-policy boundary | Both entrypoints preserve reports/verdicts; actual docs-route adoption is later |
| Plan/projection reconciliation | Draft #13514 under #8197 / #8198 | Current producer/configuration inventory | Grouped producer parity and complete expected outcomes; source remains inactive until execution acceptance |
| Guide preparation | Draft #13906 | Compatible candidate CLI and real cold deadline | Required guide consumers use it; every CI miss returns through one family owner |
| Native inventory reuse | #13481 / #14493 / #14501 under #13705 | Existing owner resolves source-custody hold | Existing native validator consumes the exact compatible producer inventory |
| Compiler-unit reduction | ADR 0027; #12726 / #14199 | Existing contract/outcome boundaries | Largest compiler peak and public/package/test parity, with whole-route cost disclosed |
| Execution placement | #8198 / #8194 / #8195 | Accepted plan, compatible preparation and bounded resource behavior | Same-runner reference versus small placement comparison, complete actual-G proof and cost |
| Native subject and authority | #14500 / #4836, current-main successors | Complete membership/C binding and qualified publication mechanism | Isolated positive/spoof/stack/retry controls plus actual-product canary |
| Protected cutover | #4834 / #4838 / #4840 / #4842 | Qualified successor, full enforcement readback and separate authorization | Reachable old/new generation, actual protected M, rollback and quiet event perimeter |
| Integrated economics | #4841 / #11566 | Activated applicable lifecycle/execution path | Defined operating cohort, complete accepted-stack totals, exact-G diagnostics and explicit lifecycle cost |

These are continuation points, not new reservations or automatic assignments.
Independent enabling slices may land before the two activation prerequisites
join. The final compute target does not block a separately qualified lifecycle
correctness improvement; neither one alone closes the integrated parent.

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

Concrete reconciliation against current main is required before scheduling:

- Map dependencies/process/network to the existing ci-policy producer, and
  goldens/fixtures to check-with-fixtures; do not restore duplicate commands.
- Admit and record the installed effective hosted profile configuration,
  including CARGO_PROFILE_DEV_DEBUG=0. The staged validator currently rejects
  all CARGO_PROFILE_* overrides and misses the profile-alignment step.
- Replace the staged four-sequential-shard/first-parent assumptions with current
  two-shard observations and the complete-G subject contract.
- Separate the staged row's producer grouping from runner placement. Its
  24-row Rust denominator is not the complete docs/extension/wider portfolio.
- Qualify bootstrap under protected current source; the new controller cannot
  grant authority to its own introduction.

### Acceptance and proof

- Compare the old and candidate plans on docs, product, optional features,
  extension, workflows, policy, mixed and unknown changes.
- Preserve command/configuration, selected cases, doctests, ignored cases,
  quarantine/retry semantics and required result meaning.
- Each mandatory proposition receives one authorized discharge. A grouped
  producer may satisfy several propositions under its existing contract;
  compatible authenticated reuse is explicit. Unknown is not an empty success.
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

The product-free user-text interface and actual workflow consumer are already
installed. Modern bases use cargo policy check-user-text; older bases retain
the xtask fallback. Do not assign this extraction/adoption again. The remaining
native-stack task is independently protected C/B0 source selection: the current
workflow uses event GROUP_BASE_SHA with ancestry checks, which can still be an
unlanded lower G. Preserve old-base compatibility without letting the candidate
choose its own judge.

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
- CI rebuilds return through the one preparation owner for that family;
  consumers must not each launch Cargo on a miss. Local standalone fallback
  remains a separate qualified mode.
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
Live owner: #14500 and current-main #8082 successors; preserve #13862 / #13863 staging ownership
Blocked by: dynamic membership, source authority and native canaries
Blocks: cheap-H / combined-G-only production cutover

### Production delta

Distinguish requested selection, registered native stack and all unlanded
contributions included in G. Freeze the protected comparison anchor B0 for this
actual G, complete ordered PR/H/dependencies and G/tree independently of the
event's immediate speculative base and the earlier request-time branch tip.
Keep C independently protected. A native upper PR's immediate base branch need
not be main; validate its authorized stack destination instead.

Correct each current selector at its existing owner. #15128 owns blobless docs
history fetching, not proof that the selected base is the protected anchor.
Draft #15176 corrects the extension selector's current event-base assumption;
its native Rust/hosted qualification remains outstanding. It does not establish
complete membership, label union or trusted control source.

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
Live owner: #14500 / current-main #8082 successors and existing shard-reuse owners
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
- Aggregate raw runner-minutes below 20 per accepted submission/stack in the target cohort,
  plus separately reported elapsed, cold/warm, CPU, memory, disk, transfer,
  failures, retries, invalidations and cost per landed PR.
- Include all associated merge-group attempts, runner-allocating prefixes,
  retries/replacements and required off-run preparation. Retain exact-G totals
  as diagnostics; report stack totals and landed-PR counts without hiding an
  expensive stack in per-PR averages. H/control work and unsuccessful submissions
  also remain in explicit lifecycle totals.
- Retain the separate under-20 qualification-elapsed objective from first
  associated allocation to complete result, without resetting on retry or
  replacement. Report enqueue-to-result and enqueue-to-landing separately,
  without turning queue delay into a universal under-20 SLA. These boundaries follow the
  [#4841 clarification](https://github.com/EffortlessMetrics/ripr-swarm/issues/4841#issuecomment-6999978169).
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

### Native acceptance matrix

These are future discriminators, not claims that this documentation revision
executed them or authorizes production settings changes. Retain each actual
source, subject, native context, result and cost.

| Case | Required observation | Existing owner |
| --- | --- | --- |
| Exact singleton | Cheap product-free H admission on H, complete G proof, protected exact-M readback | #14500 / #4840 |
| Selected middle PR of a larger stack | Only the requested open downstack selection is authorized; upper members remain outside the request | #14500 |
| Dynamic two-/three-member native stack | Provider membership is read, not hardcoded; event count and heavy-execution count are separate | #14500 / #4841 |
| Lower extension/Rust/control change, upper docs | Full applicability and label union; lower control bytes never become protected C | #14500 / #8197 |
| Non-main immediate stack base | Verified dependency edges reach the authorized destination | #14500 |
| Missing/extra/reordered member or changed lower H | Refuse before expensive allocation or final success; top-H equality is insufficient | #14500 |
| Correct G membership but preceding unlanded member lacks current admission or has wrong C | Refuse; native queue membership and matching trees cannot replace authenticated exact-H admission | #14500 |
| Bad middle member | Complete G fails; prefix observation emits no canonical product success; native ejection/landing outcome retained | #4840 |
| Omitted trusted callee or forged same-name result | Provider enforcement refuses the candidate's same-App success | #4836 / #4838 |
| Missing/skipped/cancelled producer, empty or duplicate inventory | Required result refuses even if unrelated jobs are green | #8195 / #13514 |
| Grouped policy or golden/fixture producer failure | Every owned logical obligation remains non-success; no duplicate substitute command masks it | #13514 |
| Wrong build family / toolchain / target / flags | Reuse refuses; complete compatible preparation remains owned | #13705 / #13906 |
| Cold miss in several consumers | Exactly the owned family producer rebuilds; consumers do not each invoke Cargo | #13906 / #8198 |
| Old success, newer failure without an artifact | Native job history prevents fallback to old green evidence | #14500 / #10605 |
| Failed finalizer, valid unchanged producers | Qualified same-run recovery avoids compilation and discloses both producer/finalizer attempts | #14500 |
| Missing job pages, expired/corrupt artifact or changed C/plan | No final success; bounded recovery preserves the original state | #14500 |
| Uncertain publication response or delayed older finalizer | Reconcile native check identity before retry; do not overwrite a newer refusal | #14500 |
| Required producer fails while siblings run | Stop/reap only now-unnecessary owned work; retain original failure and diagnostics | #8198 / #14500 |
| Valid speculative sibling / unrelated main advance | Valid G continues; no PR- or main-wide cancellation | #14500 |
| Same-history lower prefix settles | Still-valid upper G remains live; exact settlement relation is retained | #14500 |
| Equal tree, different merge/history identity without an accepted mapping | Unknown/refused, not settled; initial SQUASH acceptance binds native per-member commits to the captured G chain and final M to complete G | #14500 |
| Withdrawn/recomposed G, including rerun race | Retired lifetime stops; its valid replacement survives | #14500 |
| Unknown/partial native reads | No success, invented retirement or duplicate queue mutation | #14500 |
| Late review blocker / withdrawal races with M | Refuse where still effective; if M wins, retain actual landing and incident outcome | #4985 / #4840 |
| Main advances again before landed readback | Locate exact M in protected history, not only current tip equality | #4840 |
| Old, reconciled and fresh PR contexts | No ordinary development starts across Actions and separately observed providers | #4842 |
| Rollback | Complete previous qualification is reachable before successor protection is removed | #4838 / #4840 |

### Delivery decision record

For each slice, keep one current native record with:

- selected claim, existing owner/candidate and exact source;
- expected producer/obligation and consumer delta;
- dependency or custody hold, plus the concrete resume condition;
- discriminating executed proof and unexecuted dimensions;
- actual allocation/cost observations, including unfavorable attempts;
- acceptance at that slice's scope and the next missing parent criterion.

This is the repository's existing issue/PR continuation record, not another
tracker. A useful local or enabling result can be accepted without pretending
it completed the whole lifecycle. Do not repeatedly run broad proof while an
unchanged known failure or a source-custody prerequisite remains unresolved.

### Focused-to-native proof sequence

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
