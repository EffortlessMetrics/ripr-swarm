# CI Current State

Snapshot: November 18, 2026 (America/New_York).
Inspected main: `0c058d5179586b04ad1f0e874608757d7e7f0829`.

This is the installed-state inventory, not a claim that the rebuild is complete.
[CI strategy](../CI.md#qualification-rebuild-architecture) owns the target
execution architecture; [merge flow](../MERGE_FLOW.md) owns the installed
request/landing procedure; the
[implementation plan](../../plans/ci-lifecycle-rebuild/implementation-plan.md)
sequences the existing owners. Recheck source and native objects when resuming.

## Installed lifecycle

- Ordinary Draft publication, Ready transitions, development updates and review
  do not request repository Actions qualification at this revision.
- Explicit `merge-ready` still requests full PR qualification. Its product
  checkout is the PR synthetic integration commit P, associated with H; this
  is not cheap product-free H admission.
- GitHub's native queue requests another complete applicable qualification on G
  through `merge_group: checks_requested`. The observed production queue remains
  SQUASH/ALLGREEN. The isolated HEADGREEN stack rehearsal is not production.
- `Ripr Rust Small Result` remains the required result. Its observed Actions
  application identity does not independently authenticate candidate workflow
  source. No successor publisher is installed by documentation.
- Ordinary main/master product replay is retired. The main-health sentinel
  observes native proof; the separate hard-cut observer still has a push root.
- Nonrequest label wrappers and the conflict-notice label subscriber remain.
  External check publishers are a separate event perimeter; quiet Actions do
  not establish zero external-provider activity.

No rules, credentials, settings or queue policy are changed by this inventory.

## Required execution

`routed-rust.yml` selects the route and joins its required results.
`rust-gates.yml` and two `rust-test-shards.yml` consumers execute the
full Rust route in parallel after admission. Both test shards independently
prepare workspace binaries; the gate also prepares native test inventory for
file-policy validation.

The installed obligations include:

| Obligation | Current owner / execution |
| --- | --- |
| Formatting and source policy | Formatting, lightweight policy preflight, then `cargo xtask ci-policy`; this is not ordinary local selected-Clippy `precommit` |
| Optional-feature static Rust | `cargo clippy -p ripr --all-targets --all-features -- -D warnings` |
| Default-feature static Rust | `cargo clippy --workspace --all-targets -- -D warnings` |
| Compiled tests | Two partitions of `cargo nextest run --workspace --profile ci`, with inventory/result exact-cover verification |
| Doctests | `cargo test --workspace --doc`, separately from nextest |
| Golden and fixture contracts | `cargo xtask goldens check-with-fixtures`; one evaluation retains both obligations |
| Verdict corpora and other product contracts | Existing required corpus, capability, promotion-honesty and evidence steps |
| PR evidence | Applicable packet/schema/evidence steps, with disclosed unavailable analysis where the current contract permits it |
| Extension proof | G-scoped selection and the existing extension result joined into the required aggregate |

The default nextest and doctest rows do not execute all optional-feature tests.
All-features Clippy does not establish all-features behavioral execution.
Test Analytics, platform advisory lanes and separately selected proof retain
their own purposes; they are not replacements for the required rows.

The separate workspace `cargo check` was removed after compilation-unit parity
was established (#10341). Policy aggregation and combined golden/fixture
evaluation have also landed. Do not reintroduce these duplicates as a new design.

## Recent measured cost

Raw runner-minutes below are the sum of non-skipped jobs'
`completed_at - started_at`. They are not CPU time, invoice totals, or the
elapsed duration of the workflow. The full-run cohort contains different G
subjects and is a baseline, not a matched before/after experiment.

| Observation | Receipt |
| --- | --- |
| Eleven successful full G runs | 83.42–109.47 raw runner-minutes; median 98.63 |
| Representative full G | [44451072622/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44451072622): 102.60 total; gates 44.95, shards 25.22 and 29.52, other jobs 2.92 |
| Exact compiled-test cover | That run reconciles 31,435 unique cases across 411 binaries; retain the verifier |
| Docs-only qualification | [44445213794/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44445213794): 24.37 total; docs gate 21.60 |
| Failed required test | [44394089976/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44394089976): 93.05 total; the gate continued 26m25s after the first failed shard |
| Obsolete alternate G | [44374235541/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44374235541): 98.55 total; product checks passed, but finalization refused after the PR landed through another G |

The obsolete-run pair does not establish when the older G became invalid or
that all its work was avoidable. The assertion failure is not classified as an
infrastructure flake.

The representative gate's file-policy step compiles `ripr` and `xtask`
targets to enumerate `covered_by` declarations. The separate source-based
`check-covered-by` check is not that native inventory. Guide tests also invoke
Cargo through a process-local `OnceLock`; nextest's process-per-test model
does not share that lock across cases. These are concrete preparation-ownership
boundaries, not permission to drop coverage or trust an ambient executable.

### Baseline cohort

These eleven successful full G runs were created November 18, 2026,
19:29:00–22:23:18 America/New_York. Every retained observation is attempt 1.
The docs-only run above is excluded. Durations are the observed latest-attempt
job spans, including setup and finalization, not just test-step timings.

| Run / attempt | G | Raw runner-minutes |
| --- | --- | ---: |
| [44451072622/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44451072622) | `08c518cfee54676d4a8f007d84b7b45490f03a2d` | 102.60 |
| [44450435258/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44450435258) | `b7d646702c92b1951f8ccb2a0fcba24825c15030` | 92.63 |
| [44448052309/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44448052309) | `d548f5601e2ca3ab53ffd41d029ed499a28425a1` | 104.63 |
| [44447180648/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44447180648) | `c11fdfdbc5f1aa55f449e1d4a67037b7659fcf1f` | 92.88 |
| [44446459957/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44446459957) | `71ea512268c8adfd77a31e76a3ca29f6864fbf1a` | 84.82 |
| [44440819916/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44440819916) | `440b4899a66d8876e88b81cfa7e3be55adb93643` | 104.80 |
| [44440465918/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44440465918) | `466ba079693b5bb3c8d7211ce2376f22effe572c` | 109.47 |
| [44440463649/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44440463649) | `a202ca26084d7591b2b521d642c88f07e23b87b2` | 83.42 |
| [44434503391/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44434503391) | `2bf87b3e6eabcf8f007c1f8c33b2bab0d536c490` | 101.85 |
| [44429498013/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44429498013) | `d810182d9d0556946e3f9b3f66457f8a6b383af4` | 91.73 |
| [44429341484/1](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/44429341484) | `3a5d7e2906a89f48a2833450e128794fa4ee13a4` | 98.63 |

## Landed interfaces versus active consumers

| Work | State at this snapshot |
| --- | --- |
| Two required nextest shards | Installed; the earlier four-shard description is historical |
| Product-policy feature split, #14781 | Landed; default automation and the normal xtask executable remain product-linked |
| Product-free `cargo policy docs-gate`, #14772 | Landed scoped interface; the required docs workflow has not switched to it |
| Markdown contract coverage, #14989 / #14992 | Landed; some Markdown changes intentionally require product-backed behavioral proof |
| Canonical `ProductGatePlan` | Observational contract on main; workflow consumers have not completed migration |
| Staged DAG, #13514 | Existing Draft, not activated or economically qualified |
| Guide binary preparation, #13906 | Existing Draft; whole-route producer/cache cost and real cold fallback remain open |
| Inventory reuse, #13481 / #14493 / #14501 | Existing Draft stack; source-custody reconciliation hold must be resolved by its existing owner |
| Compiler-unit extraction | ADR 0027 accepted; enabling boundaries have landed, larger private implementation/test extraction remains |
| Native combined-stack rehearsal | Isolated evidence; dynamic production membership, trusted finalization and full-product acceptance remain open |

## Important unresolved boundaries

The native event's immediate `merge_group.base_sha` may be a speculative lower
G, not the protected original base. Current full-membership, trusted-source and
selection assumptions must be reconciled before the one-combined-G cutover.
A lower code/extension change plus an upper docs-only change is a required
negative control. A successfully landed lower prefix is not automatically a
withdrawn or invalid upper candidate.

The terminal result currently binds admission to its run attempt. Bounded
earlier shard-artifact reuse does not authorize admission reuse or hide a newer
failed producer that uploaded no artifact.

The protected ruleset-workflow route has a reproduced provider/documentation
mismatch: a merge-group-only workflow was rejected by both the UI and supported
organization API. The [exact report](https://github.com/EffortlessMetrics/ripr-swarm/issues/14500#issuecomment-6983589778)
is distinct from RIPR's compilation and lifecycle implementation defects.
Keep current protection until an accepted native source-bound mechanism or
narrowly scoped fallback is qualified; Hermes and a new App are not presumed
dependencies.

## Measurement and authority

The newer primary target is **under 20 total raw runner-minutes for the complete
required qualification**. Historical critical-path budgets remain a separate
latency objective. Charge setup, compilation, all consumers, verification,
transfer and cleanup; also report failed/replaced work and cost per landed PR.

Cache hits are optional acceleration, not proof. No main replay or periodic
warmer is required for correctness. A cold path must recompute safely.

The historical LEM/planner/actuals documents contain proposed interfaces.
They do not establish an installed numeric planner, emitted actuals, a new
required check, or an automatic PR Plan consumer.
