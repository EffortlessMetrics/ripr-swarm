# Product lifecycle and repository roles

This is an orientation map, not a new execution authority. It connects the
[charter](CHARTER.md), [tracking model](REPO_TRACKING_MODEL.md), existing
[agent procedures](../AGENTS.md), and [release transaction](RELEASE_TRANSACTION.md).
Use the current issue, source and evidence for live state. Maintainer navigation
is owned by [#2324](https://github.com/EffortlessMetrics/ripr-swarm/issues/2324);
this bounded reference is [#8187](https://github.com/EffortlessMetrics/ripr-swarm/issues/8187).

## One product, two repository roles

`ripr` helps developers and coding agents write tests that notice changed
behavior. `ripr-swarm` is its development authority. `ripr` is the canonical
source, release and distribution authority. These are roles in one product's
lifecycle, not competing products or a private-versus-public distinction.

Ordinary shared-product changes belong in swarm. Source-side release and
security work stays with its existing source owner. A development build, an
integrated source tree, a qualified artifact and an independently verified
public installation are different delivery states. Read the version-specific
installation instructions and [support tiers](status/SUPPORT_TIERS.md) before
using a development capability as a published-product promise.

```text
User need or observed friction
  -> bounded product hypothesis and behavior contract
  -> swarm implementation, focused tests, review and repair
  -> accepted ordinary change
  -> selected history-preserving source integration
  -> exact artifact qualification and authorized delivery
  -> installed-user observation
  -> evidence for the next product or workflow improvement
```

This overview does not select a release, freeze all development, or authorize
publication. The release transaction owns its actual ordering and identities.

## Four connected loops

| Loop | Question at completion | Durable home |
| --- | --- | --- |
| Product learning | Did the capability help the intended user on the observed cases? | User reports, governed evaluations, support/capability evidence and their issues |
| Change delivery | Was this bounded claim reviewed, proved, integrated and reconciled? | Issue, PR, tests, exact-source evidence and closeout |
| Release delivery | Can users obtain and use the intended qualified artifact through the named channel? | Source release transaction, artifacts, authorization and public/installed receipts |
| Operating-method improvement | Did this guidance or tool change improve actual agent behavior in the observed environment? | Existing issue/PR, instruction version, harness observation and regression example |

Finishing one loop does not finish the others. A merged leaf does not close an
umbrella's remaining acceptance. An accepted specification is not executed
proof. Repeating a worker's summary does not create an independent observation.

## Where RIPR contributes

The [static exposure model](STATIC_EXPOSURE_MODEL.md) follows:

```text
Reach -> Infect -> Propagate -> Observe -> Discriminate
```

For a `>` to `>=` change, tests below and above the threshold may miss the
new equality behavior. RIPR can point to an equality input and a relevant
assertion. The useful result is a focused testing question, not a warning
count, coverage percentage or proof of correctness.

| Observation | What it can establish | What it does not establish |
| --- | --- | --- |
| RIPR finding or before/after receipt | Static evidence, limitations and observed static movement | That a test ran, a real mutant was caught, or the change is merge-ready |
| Native focused test result | The recorded test outcome for its actual input/environment | General adequacy, other platforms or every plausible defect |
| Executed wrong-behavior control | Whether that concrete wrong behavior is rejected by the test | Correctness outside the tested counterfactual |
| Independent review | The reviewer's judgment over the inspected source and evidence | Missing execution, installed compatibility or publication authority |
| Artifact/public readback | Identity and behavior actually checked on the named delivery surface | Other channel delivery or a newer artifact's qualification |

Use the [targeted-test workflow](TARGETED_TEST_WORKFLOW.md) and
[command hierarchy](COMMAND_HIERARCHY.md) for existing commands. Follow printed
finding/attempt routes; do not invent commands or interchange probe, seam and
attempt IDs. Humans or external agents own the test edit.

A residual static warning is not a reason to weaken a good test or couple it
to implementation details merely to satisfy the analyzer. Inspect alignment
and the actual test evidence; record a precise analyzer limitation or defect
when justified. Conversely, an empty result or an `exposed` classification
cannot replace the required native proof. Measure both false confidence and
false-actionable findings.

RIPR is an instrument inside the [sibling-tool architecture](interop/sibling-tools.md).
It does not become the overall reviewer, test scheduler or merger. Existing
local reviews remain useful before Tower, CI-mode or another service is ready.
A task depends on the specific capability it consumes, not every sibling in
its parent programme.

## Reusable questions in the warm development loop

These are recurring questions, not a fixed actor roster or mandatory sequence
of new sessions. Enter the earliest missing judgment using the existing skill;
return to an affected question when evidence changes.

| Question | Existing procedure | Useful evidence or correction |
| --- | --- | --- |
| Is this the right problem, and is equivalent work already present? | `prepare-issue`, `deliver-pr` | Current consumer, issue/PR search, accepted requirement and bounded non-goals |
| What plausible wrong behavior would pass the proposed tests? | `prepare-proof` | Behavioral red, negative control, setup assertions and nonempty intended subjects; RIPR where useful |
| Does the implementation solve the intended problem? | `build-candidate` | Small coherent change, relevant native tests, failure classification and actual consumer behavior |
| Are the tests complete enough and economical? | `build-candidate`, `review-pr` | Relevant boundaries/failures, meaningful assertions, measured setup/build/wait cost; no weakening of timing contracts |
| Can the implementation be simpler or use an existing owner? | `build-candidate`, `review-pr` | Removed duplication, clear module boundary, preserved behavior and affected proof |
| Do docs, comments, configuration, compatibility and user experience agree? | `build-candidate`, `review-pr` | Real rendered/installed path where relevant, scoped documentation and explicit support limits |
| Have independent findings been resolved on the actual candidate? | `review-pr`, `finish-pr` | Readable source/proof, substantive dispositions, current review and the remaining qualification obligation |

Keep one writer per candidate. Publish useful progress and preserve evidence
without polishing every intermediate commit for ordinary squash-merged work.
A focused reviewer should receive the actual candidate and readable original
evidence before spending effort; a sender-local path alone is not a handoff.

## Evidence applicability, not green-result copying

Retain the actual command, native outcome, tested inputs, relevant environment
and original evidence location. For staged or dirty-worktree execution, capture
which bytes were tested and reconcile them to the later committed candidate.
A HEAD SHA alone cannot identify uncommitted inputs. A digest identifies bytes;
it does not independently establish their producer or successful execution.

Separate reusable dependencies/build intermediates, repository context, static
results, native test results and review judgment. A new SHA is not automatic
invalidation of all of them. A changed fixture, assertion, dependency, feature,
platform, source input or integration basis may invalidate the affected result.
A new material review finding may withdraw readiness without a code change.

Use the existing PR-native evidence/continuation path and its producer contracts.
The general warm-evidence integration is owned by
[#4985](https://github.com/EffortlessMetrics/ripr-swarm/issues/4985), not implemented
by this page. Publication to a Check is distinct from starting Actions, and a
Check summary is not the original proof. Keep unavailable, failed, cancelled,
partial and stale evidence visible. Do not create another mutable status store
or commit per-run logs to product history merely to make them discoverable.

## Late qualification: target versus active machinery

The intended SDLC flow under
[#4984](https://github.com/EffortlessMetrics/ripr-swarm/issues/4984) is:

```text
Warm draft work and repeated focused review/repair
  -> Ready for final review, not a CI launch
  -> explicit merge-ready qualification request
  -> applicable mandatory proof plus justified additional investigation
  -> separate final product/vision/current-repository judgment
  -> normal expected-head protected squash
  -> landed-state reconciliation
```

No native merge queue or automatic merge belongs to that target. One late run
is the normal path, not a ban on justified repair/rerun. Available checks,
mandatory obligations and selected additional checks are different sets.
Evidence reuse needs a reviewed applicability decision; static output cannot
silently waive a required check.

The ceiling is **CI run duration <=20 minutes**. Queue/request latency,
request-to-result latency, total local/hosted compute and cost per accepted PR
are separate measures. Count failures, retries, setup, transfer and work moved
into author sessions honestly. Reducing a configured timeout or timing only a
small subset does not establish complete qualification within that ceiling.
See [#4841](https://github.com/EffortlessMetrics/ripr-swarm/issues/4841).

This is a migration target, not an assertion that current workflows are live
in this form. Consult [CI](CI.md), [merge flow](MERGE_FLOW.md), current source
and [#1029](https://github.com/EffortlessMetrics/ripr-swarm/issues/1029) for the
actual cutover state. Do not use this overview to bypass current protections,
retire a cache writer, toggle Ready to obtain early CI, or modify another
owner's staged workflow. Source implementation, native rehearsal and authorized
activation are separate acceptance steps; a source-only repair need not wait
for unrelated deployment.

Independent main health is a different loop: the intended controller admits it
only after BOTH an hour and ten merges since the checked snapshot. No routine
full replay on every merge. A genuine failure gets priority repair and contains
affected work; demonstrably independent work can continue. Unknown impact may
require broader temporary containment. This reference does not install a
scheduler or change the health policy.

## Promotion, distribution and the return path

Ordinary swarm PRs squash-merge; history-preserving source joins and back-sync
follow the [release transaction](RELEASE_TRANSACTION.md). Do not flatten the
promoted history or select a replacement candidate from floating main. Later
swarm development is not silently added to the pinned release.

Resolve source-owned release machinery and swarm-owned product changes
semantically. A clean textual merge is not proof that the integrated commands,
metadata, workflows, schemas and artifacts work together. If the reviewed
source or artifact changes, reconsider the affected qualification.

Source release owners
[#1463](https://github.com/EffortlessMetrics/ripr/issues/1463) and
[#1606](https://github.com/EffortlessMetrics/ripr/issues/1606) retain the release
tail. Qualification concerns the exact crate, native archive, extension or
other selected package and its supported installed journey. A checkout test
cannot prove that a distribution omitted no needed file or selected the right
executable. A successful upload is not independent public verification.

Publication requires its channel-specific authority. Partial delivery remains
partial; follow the channel's recovery contract rather than overwriting an
immutable version. After public verification, reconcile release-side changes
back into swarm through the governed history-preserving return path. Read
[install channels](INSTALL_CHANNELS.md) and [versioning policy](SEMVER.md) for
installation/upgrade and compatibility boundaries, not a copied version table.

## Learn from use without inventing success

Use existing issue/PR observations and governed evaluation owners. Record the
method/skill revision, harness/model where known, actual loaded inputs,
environment, expected action, observed action, intervention and outcome. A
well-formed instruction file or graph is not evidence that a runtime loaded or
followed it. Distinguish missing information, bad tool output, poor guidance,
environment failure and judgment before deciding what to repair.

For RIPR, evaluate useful test improvements, false confidence, false-actionable
findings, route availability and effort to a useful result. Keep seeded trials,
ordinary real-repository repairs, installed-user evidence and runtime mutation
calibration in their own denominators. See the
[governed repair corpus](dogfood/RUST_REPAIR_ATTEMPTS.md); zero eligible attempts
cannot become a success rate. This reference promotes no support tier.

For the operating method, compare useful feedback time, unnecessary reruns,
repair latency, qualification cost and operator interventions. Preserve original
claims, contrary evidence and corrections; repeated summaries do not count as
corroboration. Revisit consequential uncertainty or changed inputs rather than
rechecking every historical statement on every task.

The [paired lifecycle scenarios](agent-context/product-lifecycle-scenarios.md)
exercise these reading boundaries. They are evaluation inputs and expected
outcomes, not claimed successful agent runs, new runtime gates or another
workflow engine.
