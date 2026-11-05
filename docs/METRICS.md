# Metrics

`ripr` metrics exist to track whether static findings are becoming more useful,
more truthful, and less noisy. They are not vanity counters.

Capability status is sourced from `metrics/capabilities.toml`, summarized in
[Capability matrix](CAPABILITY_MATRIX.md), and reported with
`cargo xtask metrics`. This file defines the metric categories and how to
interpret them.

## North Star

The primary product metric is:

```text
time from changed behavior to useful targeted test intent
```

A useful targeted test intent names:

- the changed behavior
- the missing or weak discriminator
- related existing tests, if any
- missing activation values, if relevant
- suggested oracle shape
- stop reasons for unknowns

## Release ladder

Question: what must be true before a release is installable, reliable, useful,
or something people would not want to be without? Each level adds a numeric bar
and the instrument that measures it. A level is met only when every row meets
its bar on the released build. A bar without a working instrument is
`not_established`, not met.

**Status of the bars.** The numbers below are working defaults, set 2026-11-04
as operating targets. The owner has not yet confirmed them as the product bar.
Change them here, in one place, when the owner rules. This ladder never
authorizes a release: scope, candidate, ship and publication stay with the
owner under [the release transaction](RELEASE_TRANSACTION.md) and
[#12830](https://github.com/EffortlessMetrics/ripr-swarm/issues/12830).

| Level | Meaning | Criterion (working default) | Instrument |
| --- | --- | --- | --- |
| R0 installable and honest | A new user installs it, gets a first result, and is not misled. | False verdict rate at most 1% (stricter than the 5% public-proof ceiling in [PUBLIC_PROOF](PUBLIC_PROOF.md)). | Verdict corpus (`cargo xtask verdict-corpus check`). |
| | | Install to first useful result at most 300 s on Linux, macOS and Windows, every shipped channel, verified against the published version. | `first_run` board; weekly published-install matrix ([#12938](https://github.com/EffortlessMetrics/ripr-swarm/pull/12938)). |
| | | No crash, hang or silent clean result on messy repositories. | Robustness corpus ([#11524](https://github.com/EffortlessMetrics/ripr-swarm/issues/11524), [#12924](https://github.com/EffortlessMetrics/ripr-swarm/issues/12924)). |
| R1 reliable | Same answer on every surface, fast and small enough to leave on. | Warm `ripr check` p50 under 1 s and p95 under 3 s up to about 1,000 Rust files. | `cargo xtask dx-scoreboard`: `speed.warm_check_p50_ms` and `speed.warm_check_p95_ms` (nearest rank over `--samples`, default 3; p95 is the maximum below 20 samples). |
| | | Peak memory under 150 MB for 100 to 800 Rust files, under 512 MB for 3,000-file Python/TypeScript repositories, hard cap 1 GiB. | `dx-scoreboard` `speed.warm_check_peak_rss_mb` (maximum over the warm samples), held per `[[size_tier]]` ceiling in `scoreboards.toml`. |
| | | Zero contradictory refusals for one item across CLI, LSP, MCP and agent output. | The manual consistency matrix, until an automated check exists ([#10712](https://github.com/EffortlessMetrics/ripr-swarm/issues/10712)). |
| | | The classification and fail-closed core is proved on every pull request (target; the lane is advisory today). | Kani lane (`cargo xtask kani`), required ([#12904](https://github.com/EffortlessMetrics/ripr-swarm/issues/12904)). |
| | | Flake rate under 1% on required lanes. | Test-analytics flake ledger. |
| R2 useful | People and agents finish the loop: gap, repair, verified. | Repair starts on at least 80% of gaps `check` reports. | Not-ready harness and the nightly outcome page ([#12903](https://github.com/EffortlessMetrics/ripr-swarm/issues/12903)). |
| | | Precision of "discriminator" claims at least 97% against real mutants, published by language and operator. | `cargo xtask mutation-spot-check`; receipt `metrics/public-proof/mutation-spot-check.json` ([#12900](https://github.com/EffortlessMetrics/ripr-swarm/issues/12900)). |
| | | Recall of runtime survivors flagged as a gap or an honest unknown at least 80%, published by language. | `cargo xtask mutation-spot-check`, same receipt (`recall_of_missed`; [#12901](https://github.com/EffortlessMetrics/ripr-swarm/issues/12901)). |
| | | Every verdict class has locked-in goldens per language. | Lock-in matrix ([#12922](https://github.com/EffortlessMetrics/ripr-swarm/issues/12922)). |
| R3 critical | Nobody wants to be without it. | Agents with ripr beat agents without it by a statistically clear margin on a public task suite. | With/without cohort ([#12902](https://github.com/EffortlessMetrics/ripr-swarm/issues/12902)). |
| | | Under a Kani or Verus harness, a changed line gets a source-bound answer that tells a strong harness from a vacuous one. | FV cohort ([#10745](https://github.com/EffortlessMetrics/ripr-swarm/issues/10745)). |
| | | Installed users act on at least 30% of findings and suppress under 20%. | None: telemetry is an owner decision and is not started. |

### Languages and support tiers

Each language is placed on the ladder separately, and a support tier states the
lowest level a language must hold. A tier is promoted only when its level is met
for that language, using that language's own numbers. Passing a fixture or
adding a roadmap entry does not promote a tier.

| Support tier | Ladder level the language must hold |
| --- | --- |
| `scaffold`, `deferred`, `blocked` | None. |
| `preview` | R0, with static limits visible in the output. |
| `usable alpha` | R0 and R1. |
| `usable` | R0, R1 and R2. |
| `stable building block` | Fixture-backed inside its documented scope; the ladder does not apply. |

The bars apply per language, not to the product average. Unmeasured is not
met.

### Position on 2026-11-04

Reported by the owning threads against main `b80ce950`; the Recall row is
re-derived against main `9b45bd6b`. Recall is reproduced by a checked-in
instrument: `cargo xtask mutation-spot-check` and its receipt
`metrics/public-proof/mutation-spot-check.json`. Repair-start and latency
figures still come from their issues
([#12903](https://github.com/EffortlessMetrics/ripr-swarm/issues/12903),
[#12947](https://github.com/EffortlessMetrics/ripr-swarm/issues/12947)) and are not yet
reproduced by a checked-in instrument. Refresh with each new baseline.

| Row | Now | Bar | Met |
| --- | --- | --- | --- |
| False verdict rate | 0 of 846 (38% abstain) | 1% | yes |
| Robustness | 75 cases, no crash, hang or silent clean | hold | yes |
| First value, all channels | Linux passes; first-run nightly red since 10-28; other OS and channels untested | 300 s everywhere | no |
| Warm check latency (`speed.warm_check_p50_ms`, `speed.warm_check_p95_ms`) | about 31 s p50 on 3,280-file ripr-swarm after [#12985](https://github.com/EffortlessMetrics/ripr-swarm/pull/12985) (was 36 to 60 s; [#12947](https://github.com/EffortlessMetrics/ripr-swarm/issues/12947)); no p95 yet, earlier figures are single samples | p50 under 1 s, p95 under 3 s up to about 1,000 Rust files | no |
| Peak memory (`speed.warm_check_peak_rss_mb`) | about 0.9 GB on ripr-swarm after [#12999](https://github.com/EffortlessMetrics/ripr-swarm/pull/12999) (was 1.3 GB); 125 to 376 MB on pilots | 150 MB for 100 to 800 Rust files, 512 MB for 3,000-file Python/TypeScript, hard cap 1 GiB | no |
| Cross-surface contradictions | Manual matrix only; [#10712](https://github.com/EffortlessMetrics/ripr-swarm/issues/10712), #10718, #10720 open | 0 | no |
| Kani on the core | 65 harnesses; classify and next_action proved; 5 refusal paths not; lane advisory | required | no |
| Flakes on required lanes | about 1.5% | under 1% | no |
| Repair start | Rust 6 of 22, Python 8, TypeScript 6; click 0 of 10; TypeScript 0 of 178 | 80% | no |
| Precision | 96.8% (61 of 63) | 97% | no |
| Recall, strict (flagged ungripped) | 10.6% (34 of 322 runtime-missed); Rust only | 80% | no |
| Recall, flagged or honest unknown (ungripped, weakly_gripped, activation_unknown, opaque) | 58.7% (189 of 322: 34 + 0 + 151 + 4); Rust only, Python and TypeScript not established | 80% | no |
| Replaces cargo-mutants (`replacement_criterion.verdict`) | not_yet: precision, flagged-or-unknown recall and scored coverage (28.3%) not met; time vs full cargo-mutants and Python/TypeScript real-mutant agreement not established; Rust only | every thresholded criterion met, none not established (placeholder bars pending the owner's call, [#12901](https://github.com/EffortlessMetrics/ripr-swarm/issues/12901)) | no |
| Agent lift, FV strength, retention | not measured | see R3 | not established |

Result: R0 is not yet met (first value across channels is unproven), and R1
and R2 are far from met. Rust is the strongest language; Python and TypeScript
fail the repair-start bar by a wide margin. The largest credibility gaps are
recall, repair follow-through outside Rust, and speed and memory on large
repositories.

## Capability Metrics

| Metric | Why it matters | Target direction |
| --- | --- | --- |
| Fixture pass rate | Guards behavior contracts. | Up |
| Golden output drift count | Shows intentional vs accidental output changes. | Down |
| Probe ownership precision | Prevents wrong related-test evidence. | Up |
| Oracle kind recognition rate | Shows how often assertions become usable facts. | Up |
| Strong-vs-weak oracle distinction rate | Tracks discriminator quality. | Up |
| Flow sink identification rate | Shows propagation evidence quality. | Up |
| Activation value extraction rate | Tracks boundary and input modeling progress. | Up |
| Unknowns with stop reasons | Keeps static limits explicit. | Up to 100% |
| Static runtime by mode | Protects editor and CI latency. | Down or bounded |
| LSP diagnostic refresh latency | Protects live feedback. | Down |

## Quality Metrics

| Metric | Why it matters | Target direction |
| --- | --- | --- |
| Findings acted on | Measures developer trust. | Up |
| Suppression rate by rule | Identifies noisy rules. | Down after config improves |
| Reopened or reverted findings | Catches misleading guidance. | Down |
| False-positive rate of top findings | Protects trust. | Down |
| Survived-mutant recall after calibration | Tests whether static gaps predict real mutation outcomes. | Up |
| Top-N precision after calibration | Keeps reports useful. | Up |
| Verdict-corpus spec-example coverage | Share of in-scope numbered spec acceptance examples (minus waived) that a runtime-labeled case cites, so each specified verdict behavior can be measured (RIPR-SPEC-0219; `cargo xtask verdict-corpus check` gates its floor). | Up to 100% |
| Verdict-corpus oracle accuracy | Share of cases labeled with a spec-adopted related-test oracle (`expected_oracle`) where ripr reports the same kind and strength; scored apart from the verdict, which cannot see an oracle error that leaves the line verdict unchanged (RIPR-SPEC-0219; `verdict-corpus check` gates the match floor). | Up to 100% |
| Verdict-corpus oracle overclaim rate | Share of those labeled cases where ripr reports a stronger oracle than the spec adopts, the reading that grants unearned credit (RIPR-SPEC-0219; `verdict-corpus check` gates the overclaim ceiling). | Down to 0 |

## Engineering Metrics

| Metric | Why it matters | Target direction |
| --- | --- | --- |
| `panic` / `unwrap` / `expect` count in production code | Enforces fallible, user-facing behavior. | Down to 0 |
| `panic` / `unwrap` / `expect` count in tests | Keeps tests explicit and maintainable. | Down to 0 |
| CI duration | Protects contribution loop. | Bounded |
| Flaky test count | Protects trust in gates. | 0 |
| Public dependency count | Keeps install and maintenance risk low. | Justified |
| Production delta size | Helps distinguish risky code churn from support evidence. | Small and scoped |
| Evidence delta completeness | Shows whether specs, tests, docs, goldens, metrics, and ADRs support behavior. | Up |

## Developer-Experience Scoreboards

`cargo xtask dx-scoreboard` measures what a developer feels when reaching for
ripr: time to the first useful result, warm check latency and memory, the size
and install cost of the generated CI workflow, false-clean and
self-contradicting output, and whether printed commands survive being pasted.
It runs against a pinned real-repository corpus, keeps targets separate from
the regression gate, and runs nightly. See
[benchmarks/dx_scoreboard/README.md](../benchmarks/dx_scoreboard/README.md)
for boards, instruments, and the ingest format, and
`metrics/dx-scoreboard/baseline.json` for the committed baseline.

## Current Baseline

Release-ladder numbers live in [Position on 2026-11-04](#position-on-2026-11-04).
Other counts are generated, not copied here:

- Repository orientation inventory: `cargo xtask current-state` writes
  `target/ripr/reports/current-state.{json,md}`; `--json` also prints the JSON.
  Its current coverage is checkout repository identity, workspace version/MSRV,
  declared development/release authorities, the command-catalog entry count, and
  four retired goal-authority markers in the current roadmap section.
  `cargo xtask current-state --check` checks those markers, support-map file
  existence, and a nonempty catalog. The broader inventory and drift coverage
  remains open under [#2330](https://github.com/EffortlessMetrics/ripr-swarm/issues/2330).
- Registered artifact and support structure: `cargo xtask repo-contract-report`
  writes `target/ripr/reports/source-of-truth-graph.{json,md}` from the artifact
  ledger and its fragments, support rows, and existing structural diagnostics.
  Use `cargo xtask check-doc-artifacts` and `cargo xtask check-support-tiers`
  for their dedicated checks. These establish registered paths, relationships,
  recorded statuses, and supported proof references within their checked scope;
  they do not decide semantic implementation completeness, earned support
  tiers, live issue acceptance, or release authorization.
- Capability status: `cargo xtask metrics` and
  [Capability matrix](CAPABILITY_MATRIX.md).
- Fixture, golden and verdict-corpus results: `cargo xtask fixtures`,
  `cargo xtask goldens check`, `cargo xtask verdict-corpus check`.
- Speed, memory and first-run: `cargo xtask dx-scoreboard` and
  `metrics/dx-scoreboard/baseline.json`.
- Panic-family inventory: `cargo xtask check-no-panic-family`.

## Anti-Metrics

These are useful context but bad primary goals:

- number of warnings
- number of generated probes
- coverage percentage
- whole-workspace graph size
- number of lines of code

The product should optimize evidence quality, not raw output volume.

Line count is especially misleading for `ripr` PRs. A large evidence-heavy PR
can reduce risk when it supports one production behavior with fixtures, goldens,
specs, docs, and metrics. A small PR can increase risk when it changes multiple
contracts without a traceability chain.
