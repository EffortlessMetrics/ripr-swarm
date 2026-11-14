# Rust Evidence-Bound Repair Trust and Adoption Plan

Status: active

Parent acceptance remains open under the current issue owners below.

Owner: product-swarm  
Plan artifact: RIPR-PLAN-0062  
Linked goal: none; retired singleton goal files do not select work. Live
authority is the GitHub issues named below, related PRs/checks, and the
local worktree. `.allow/spec-system/slices/` holds PR-local scope records
consulted after GitHub selects a live issue or PR (`ImplementationSliceV1`;
no live execution state).
Current acceptance owners: #1601, #3075, #1702, #1560; CallPresence #1543.
Original linked issues: #1423, #1424, #1425, #1427, #1440, #1543, #1560
Historical starting PRs: #1489, #1487, #1483

## Outcome

Make RIPR the default evidence-to-repair protocol for Rust pull requests: one
changed behavior becomes one exact, safe, test-only repair and a current-state
before/after receipt, while unsupported cases remain explicit limitations rather
than plausible-looking recommendations. The accepted targeted-rerun contract is
now regression-protected infrastructure inside this trust and adoption goal,
not the long-range destination.

This supersedes RIPR-PLAN-0061 as the active execution sequence. Its accepted
contracts remain useful historical source, but its pending statuses and
maintainer-fixed order are not the current queue.

## Current resumption and acceptance

The [product roadmap](../docs/ROADMAP.md) and
[current delivery priorities](../docs/IMPLEMENTATION_PLAN.md#current-delivery-priorities)
supply direction; current GitHub issues, PRs, source and retained execution
evidence select the next claim. The July sequence and receipts below preserve
the original implementation history. They do not make completed infrastructure
work pending again, establish today's performance or select a release blocker.

The existing [#1560](https://github.com/EffortlessMetrics/ripr-swarm/issues/1560)
reconciliation owns the current repair-evidence path:

| Step and owner | Entry and useful result | Acceptance and discriminating boundary |
| --- | --- | --- |
| Reconcile one real route — [#1601](https://github.com/EffortlessMetrics/ripr-swarm/issues/1601) | Revisit the selected [#1580](https://github.com/EffortlessMetrics/ripr-swarm/issues/1580) opportunity only with its applicable producer prerequisite present. Record a complete current route, a precise refusal, or an explicit unobserved boundary. | Bind repository, changed behavior, source identity, canonical gap/seam, safe test target, missing discriminator and literal commands to the same subject. Missing producer facts, stale source, wrong targets and unresolved propagation remain limitations; they do not become an admitted repair. |
| Execute one governed attempt — [#3075](https://github.com/EffortlessMetrics/ripr-swarm/issues/3075) | Consume an admitted route under the existing repository authorization and test-only edit boundary. Retain the actual before/edit/native-verification/after/terminal chain. | Retain the test-only diff, independently justified discriminator, selected native command, exit and executed-test count, current static movement, receipt identities and interventions. Positive verification requires at least one executed test. A green zero-test run, unsafe edit, wrong subject or stale after-state cannot count as successful verification or repair. |
| Qualify usability — [#1702](https://github.com/EffortlessMetrics/ripr-swarm/issues/1702) | Exercise its existing five-attempt CLI/VS Code pilot once eligible journeys exist. | Preserve channel, installed artifact, literal public actions, assistance, stops and terminal outcomes. One attempt or one CLI projection does not close the pilot or qualify an editor client. |
| Expand governed trust evidence — [#1560](https://github.com/EffortlessMetrics/ripr-swarm/issues/1560) | Add eligible unique opportunities and attempts to the existing corpus; retain the twenty-attempt, three-authorized-repository acceptance. | Apply the existing schema, denominator rules and support review. Preserve failures, limitations, exclusions and repeat observations. Report route yield, route correctness, transaction usability and repair movement separately; zero eligible attempts cannot establish a success percentage. |

The route precedes its governed attempt. The pilot and broader corpus have
distinct acceptance; the table is not a new global serial queue. Corpus
collection remains independent of CallPresence closure as described below.
[#1543](https://github.com/EffortlessMetrics/ripr-swarm/issues/1543) retains
the CallPresence proof or durable limitation disposition; that disposition
cannot supply missing corpus attempts. Personal repeated-use acceptance remains
with [#1693](https://github.com/EffortlessMetrics/ripr-swarm/issues/1693);
installed journey acceptance remains with
[#4508](https://github.com/EffortlessMetrics/ripr-swarm/issues/4508).
Neither a pilot nor this broader corpus becomes a release gate through this plan.

Reuse the delivered report and denominator machinery. #3074 is a completed
historical observation, and #3076's denominator reporting is delivered; neither
is an outstanding implementation prerequisite. The governing input remains
`metrics/rust-repair-trust/corpus.json`, with counting and exclusion rules in
[Rust repair attempts](../docs/dogfood/RUST_REPAIR_ATTEMPTS.md).
`cargo xtask rust-repair-trust-report` validates and summarizes that retained
input; it does not execute a real repair.

For the next bounded slice, publish the exact unmet predicate, existing owner,
source/artifact identity, positive and negative evidence, and next transition
on its controlling issue or PR. If a producer, target or execution result is
missing, retain the blocked or limited outcome with its owner and resume
condition. Continue independent eligible work. A blocked child is unfinished
parent acceptance, even when its infrastructure or sibling work has landed.

## Spec-system authority

The following records the structural-tool adoption context. Dialect issues
and old installed-version observations do not select current repair work;
read their current owners before relying on a blocker or command.

Cargo-allow's opt-in `spec-system` profile is the authoritative structural
validator for governed artifact paths, kinds, lifecycle states, and graph links.
RIPR xtask remains the repo-facing proof executor and may invoke cargo-allow,
but must not independently reimplement the same graph rules. Every campaign
control-plane PR records cargo-allow doctor, audit, and worklist outputs.

The profile is advisory while its findings are made low-noise. The former
`.ripr/goals/active.toml` execution manifest was deleted in #1701 and is
not a live selector; GitHub issues named in this plan carry current
execution state. `.allow/spec-system/slices/` remains PR-local scope after
that selection, not a second scheduler. cargo-allow
issues [#2260](https://github.com/EffortlessMetrics/cargo-allow/issues/2260)
and [#2196](https://github.com/EffortlessMetrics/cargo-allow/issues/2196)
remain the dialect blockers for spec-system validation (the prior #2119
blocker was closed as not planned on 2026-08-25 when the explicit
dialect/source-adapter architecture superseded it), not a
reason to revive the deleted file. The installed cargo-allow 0.1.10
requires `--config .allow/profiles/spec-system.toml` for this owned profile;
cargo-allow issue #2117 tracks the owned-versus-legacy default-path friction.

## Reconciled sequence

Historical July implementation sequence follows. Item IDs and dependencies
are retained for the original contracts; use the current resumption section
above for remaining acceptance and consult live owners before implementation.

| Order | Work item | Dependency | Evidence |
| ---: | --- | --- | --- |
| 0A | `control-plane/cargo-allow-spec-system-adoption` | — | cargo-allow doctor, audit, and worklist artifacts |
| 0B | `control-plane/rust-one-shot-goal` | 0A | goals/doc/plan checks and structural indexing |
| 0C | `control-plane/cargo-allow-active-goal-dialect` | 0A | blocked on cargo-allow [#2260](https://github.com/EffortlessMetrics/cargo-allow/issues/2260) / [#2196](https://github.com/EffortlessMetrics/cargo-allow/issues/2196) or separately approved migration |
| 1A | `output/bounded-start-here` | 0 | human/human-full fixtures and output contracts |
| 1B | `docs/first-screen-agent-loop` | 1A | README/doc checks |
| 2A | `review/card-oracle-projection` | 0 | review-card schema and traceability checks |
| 2B | `review/canonical-working-set-id` | 2A | canonical identity output contracts |
| 3A | `gate/exact-repair-route` | 1A, 2B | failure-time packet fixture |
| 3B | `gate/concrete-targeted-mutation` | 3A | candidate/limitation matrix |
| 4A | `analysis/field-constant-observation` | 2B | positive and adversarial corpus |
| 4B | `analysis/constructor-field-observation` | 2B | same-crate ambiguity corpus |
| 5 | `perf/targeted-rerun` | 4A, 4B | complete infrastructure: SPEC-0123, parity, invalidation, graph provenance, and benchmark receipt |
| 6A | `analysis/call-presence-gate-producer` | 3B | authorized real/current-repository caller-to-effect-sink receipt or named limitation |
| 6B | `dogfood/rust-route-quality-corpus` | 3B, 5 | authorized three-repository corpus, exclusion-backed pilot audit, six-attempt pilot, then 20 receipt-backed attempts |
| 6C | `dogfood/route-quality-closeout` | 6A, 6B | support review and campaign closeout after corpus completion and CallPresence disposition |

Work items 1A and 2A may use isolated worktrees. Items 4A and 4B may be
parallel only after their file/contract overlap is checked. Dependencies are
landing dependencies, not approval gates.

## Trust and adoption phase (2026-07-12)

The remaining campaign asks whether developers can trust the shipped route on
real Rust work. Every actionable result must carry producer-owned
`canonical_gap_id`, `seam_id`, `file:line`, `gap_state`, changed behavior,
missing discriminator, related test or production caller, focused test intent,
verify command, targeted-rerun command, receipt command, inspection command,
and authority boundary. If any fact is unavailable, the result is a named
limitation with a concrete investigation route; no renderer or path heuristic
may manufacture it.

The CallPresence packet is complete only when an authorized real or
current-repository receipt proves `production caller -> exact call seam ->
observable call/effect sink -> matching observing test -> complete route`.
Dynamic or unresolved receivers, method-name strings, ambiguous aliases or
owners, helper-only reachability, unrelated assertions, and opaque
macro-generated calls remain limitations unless producer evidence becomes
unambiguous.
The current blocked packet is recorded in
`docs/handoffs/2026-07-12-call-presence-evidence-packet.md`; its synthetic
positive analyzer tests and stale bounded repository scan are explicitly
excluded from promotion.

The dogfood packet requires at least 20 real repair attempts across at least 3
authorized Rust repositories. Each attempt records its repository and revision,
canonical gap and seam, before receipt, test-only repair intent and changed
files, verification command/result, after receipt, movement outcome, and
limitations. Only `closed`, `improved`, `unchanged`, `regressed`, and `limited`
are counted; synthetic fixture rows are not real-repository evidence. Real
pilot observations that cannot become counted attempts are retained in the
corpus `exclusions` array with exact SHA, source reference, evidence artifact,
command, closed exclusion reason, and claim boundary; exclusions never enter
attempt or improvement denominators.

Corpus collection is independent of CallPresence closure. The corpus packet
may proceed once repository authorization is recorded and must measure
CallPresence limitations when they occur; final route-quality closeout waits
for both the corpus threshold and a CallPresence proof or durable limitation
disposition.

The governed input is `metrics/rust-repair-trust/corpus.json`. Run
`cargo xtask rust-repair-trust-report` to write the JSON and Markdown scorecard
under `target/ripr/reports/`. Missing authorization, malformed rows, or
under-threshold denominators remain `limited`; the report must not reinterpret
the legacy `fixtures/real-repair-attempts/corpus.json` as Rust adoption
evidence.

Targeted rerun remains governed by the accepted SPEC-0123 contract: exact
selector identity, explicit before state, cache/invalidation disclosure,
selector-scoped parity, input fingerprints, graph provenance, and closed
movement vocabulary. The registered benchmark remains a floor (warm p50 <= 30
seconds, at least 5x faster than registered cold full, matched selected-scope
parity); faster without parity is limited.

Certification is current-head evidence. Receipts name the exact head SHA, later
mutation invalidates prior certification, and a reviewer who changes the branch
acts as a fixer rather than an independent reviewer for that pass. Cargo-allow
remains advisory structural authority, RIPR xtask remains proof executor, and
retired singleton goal files do not select work. Live execution state is the
GitHub issues named in this plan, related PRs/checks, and the local
worktree. `.allow/spec-system/slices/` is PR-local scope, not live selection.

## Contract

The default human surface has one `Start here:` state, at most one selected
item, an omitted count, and direct paths to exhaustive and JSON evidence.
Limited results never look clean. Actionable Rust packets use domain-supplied
canonical identity and complete repair, verify, receipt, and evidence fields;
partial packets fail closed. Gate output names the seam, missing discriminator,
test intent, verification, receipt, and explain route. Targeted mutation emits
a bounded candidate and command or a named limitation. Value-flow additions are
conservative and fixture-first. Targeted reruns disclose cache reuse and
invalidation, never silently serving stale evidence.

## Targeted rerun benchmark slice (2026-07-11)

The benchmark receipt command and controlled fixture are now available through
`cargo xtask targeted-rerun-benchmark`. The historical receipt on
`5aecff41` uses five samples and records matched parity, warm targeted p50 of
228 ms, cold-full p50 of 1512 ms, and a 6.6316x cold-full to warm-targeted p50
speedup. It also exercises an explicit file-fact cache reset and records
`recomputed_file_facts`.

This closed the benchmark-receipt slice on that recorded source. PR #1532
additionally ships explicit `path::test_node` selection for changed-test reruns.
The first targeted-rerun delivery was complete on that recorded source. PR #1550
completed the graph-provenance follow-up from issue #1548: receipts now
attribute local package/member and feature graph availability, explicitly name
unavailable external dependency metadata, and fail parity closed when required
local graph provenance is missing or differs. The implementation reads local
Cargo manifests directly and does not use network or ambient Cargo metadata.

## Targeted rerun invalidation and parity slices (2026-07-11)

PR #1534 merged file-content invalidation disclosure. When a prior same-path
file-fact envelope exists under a different content key, the rerun receipt now
reports `file_content_changed` and names the affected path; cold misses and
unsupported input families remain explicitly named.

PR #1535 merged targeted/full parity evidence expansion. `--check-parity` now
compares producer-owned related-test and missing-discriminator evidence in
addition to canonical identity, class, file, and owner, and emits per-seam
`parity.mismatches[]` fields before failing closed. At that point the
targeted-rerun item remained open for broader manifest/package/config
invalidation attribution, expanded parity over those inputs, and final
campaign closeout; subsequent workspace-input and parity slices are recorded
below.

PR #1537 merged owned workspace-input fingerprint disclosure. Aggregate cache
identity now includes recursively discovered workspace manifests, lockfile, and
toolchain selector (with `RIPR_CFG_FEATURES` included when supplied). Targeted
receipts carry the fingerprint, and an explicit before receipt names changed
components as `input_changed:<field>` with
`invalidation_status: "workspace_input_changed"`.

PR #1539 merged full-pipeline parity over the owned workspace-input fingerprint.
When targeted and full inventory inputs differ, parity fails closed with
`full_pipeline_parity_input_mismatch` and names the changed input components.
PR #1540 merged explicit selector-ledger content fingerprinting: `--gap`
receipts hash the supplied ledger bytes, while `--changed-test` reports that
field as `not_applicable`; a changed ledger is disclosed as
`input_changed:selector_ledger_hash`. The historical benchmark revalidation
is recorded in #1547. The graph-provenance follow-up then merged as #1550 and
completed the targeted-rerun work item; only the explicitly blocked CallPresence
producer evidence and receipt-backed dogfood closeout remain.

## Gate route dogfood status (2026-07-10)

This is the July delivery/blocker snapshot. Its completed infrastructure and
dated observations remain evidence for those subjects; current route and
corpus resumption is governed by the section above and the linked issues.

The structured route and shared human/generated-CI projection are shipped. A
CLI-backed calibrated blocking receipt now pins the full route directly in
`gate-decision.{json,md}` and rejects artifact-download or PR-guidance lookup as
the repair step.

The structural #1440 gate-route delivery is complete: policy-eligible
decisions now carry a self-contained repair route and producer-owned inspection
command without artifact archaeology. The remaining real CallPresence
acceptance is tracked separately in issue #1543. Current CallPresence probes
exercised through `ripr review-comments` can remain `static_limitation` when
propagation to a side-effect/call-effect sink is unknown; they therefore do not
carry a policy-eligible receipt. Keep those cases fail-closed rather than
synthesizing a blocking call-observation receipt. Resolving that producer
limitation belongs in the `analysis/call-presence-gate-producer` slice, not a
gate renderer.

The campaign blockers recorded in that July snapshot were explicit: #1543
needed an authorized real/current-repository CallPresence receipt before any policy-eligible route
can be claimed; the cargo-allow active-goal dialect remains blocked on
cargo-allow [#2260](https://github.com/EffortlessMetrics/cargo-allow/issues/2260)
and [#2196](https://github.com/EffortlessMetrics/cargo-allow/issues/2196);
and the final dogfood item lacks the required receipt-backed attempts across at
least three authorized Rust repositories. Synthetic fixture rows do not satisfy
that corpus requirement.

## Targeted mutation route delivery (2026-07-11)

PR #1545 merged the concrete #1425 route. Diff-scoped PR evidence now emits a
`targeted_mutation_route` with a bounded producer-owned predicate/operator
candidate and `cargo mutants --file ...` command when the probe has an
unambiguous source file, line, and expression. Unsupported, ambiguous, or
missing producer facts remain `static_limitation` with a named reason. The
route is preserved through `impacted-evidence`; mutation execution and runtime
claims remain out of scope. Future work may broaden safe candidate families
only behind producer-owned facts and checked fixtures.

## Boundaries and closeout

No preview promotion, automatic test generation, default mutation execution,
default CI hardening, release/signing/publishing/credential work, workspace
restructure, or unsupported static-evidence claims. New dependencies and public
CLI/schema choices need separately accepted scope. Each bounded slice yields a
reviewed implementation/evidence result or an explicit blocked/limited result
with its owner and resume condition. The plan's
required parent acceptance remains open while any required predicate is unmet.
Close the parent only when those predicates have accepted evidence, or the
existing governing authority explicitly accepts their supersession or scope
disposition. A blocked item, merged infrastructure or completed sibling is not
parent completion. Keep current-subject proof and applicable protected-check
evidence bound to the claimed source, with receipts, benchmark limits and
support review in agreement.
