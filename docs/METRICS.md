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

The fixture lab is present: `cargo xtask fixtures` and `cargo xtask goldens
check` exercise committed examples. The committed
[`dx-scoreboard` baseline](../metrics/dx-scoreboard/baseline.json) names its
analyzer revision, runner and population; it is a retained observation, not a
current release measurement. Its first-run board has seven `not_measured`
metrics, including time to first useful result. It cannot establish a current
first-run or installed-user outcome. Use a fresh, identity-bound receipt for
the selected candidate and its actual channels; do not treat missing data as
zero or aggregate fixture success as release qualification.

## Proposed release ladder

These are **proposed discussion criteria**, not approved release gates,
support promotions, or a current readiness assessment. Issue
[#12908](https://github.com/EffortlessMetrics/ripr-swarm/issues/12908) asks
the owner to decide the bars. Existing candidate selection, qualification,
support-tier and ship-authorization contracts remain authoritative. Evaluate
each row only on a named candidate, language, surface, corpus and channel;
preserve the denominator, failures, unknowns and receipt identity. A missing,
failed or incomplete instrument is not a pass. Higher levels include the
earlier rows; none follows automatically from a score.

| Proposed level | Numeric criterion | Instrument and boundary |
| --- | --- | --- |
| R0: installable and honest | 100% of a nonempty, predeclared channel/platform set complete their installed first-run check; 0 of the 12 missing-repository probes return a false clean result. | Candidate-bound first-run install/qualification receipts and `trust.false_clean_on_bad_input` in the [DX scoreboard](../benchmarks/dx_scoreboard/README.md). Retain the full intended matrix and any excluded or unavailable cells with reasons; #12909 owns the incomplete matrix. The nightly's published crates.io install is not by itself selected-candidate proof. The 12 probes test that narrow bad-input set, not all failure modes. |
| R1: reliable | 0 detected self-contradictions in every row of a nonempty, predeclared corpus; 0 unsafe and 0 unbound commands in the paste corpus. | `trust.self_contradictions`, `paste.unsafe_commands`, and `paste.unbound_commands` from `cargo xtask dx-scoreboard`; retain every selected row, including unavailable rows, and report partial scans as incomplete. This is a sampled guard, not a general reliability rate. |
| R2: useful | First useful result within 300 seconds for every journey in a nonempty, predeclared first-run set; top-recommendation precision at least 0.6 on a nonempty, explicitly identified mutation spot-check population. | Ingest `first_run.v1` and `ripr-mutation-spot-check-v2` into the scoreboard. The time instrument ends at the first successful check, so independent review must establish whether its output is useful; precision counts scored pilot recommendations in the sampled population, not all findings. Mutation join coverage is a separate metric. |
| R3: critical | At least 5 unique real installed CLI/packaged VS Code attempts across at least 3 repositories, including at least 1 exact attempt ending `improved` or `closed`. | The existing [Rust gap repair promotion contract](status/SUPPORT_TIERS.md#rust-gap-repair-promotion-contract), governed trust report, and installed pilot receipts. Include wrong targets, refusals, timeouts and unsuccessful attempts in the denominator; an owner decision and all other promotion conditions remain necessary. |

The numeric bars reuse existing scoreboard targets and the Rust promotion
contract. The proposed mapping below is an evaluation scope, not an assignment
of a current R-level or a change to support tier. A language without an
applicable instrument or installed receipt is **not measured** at that level;
Rust evidence cannot be borrowed for a different language.
The missing-repository and paste probes are global CLI checks; their results
alone do not establish any language's analysis or repair behavior. The current
first-run harness walks Rust crates and the mutation spot-check uses
`cargo-mutants`; other languages need their own applicable instruments.

| Language/surface | Current support boundary | Proposed evaluation mapping |
| --- | --- | --- |
| Rust exposure and repair | `usable alpha` | R0–R3 on the named Rust scope; R3 additionally requires the existing promotion decision and full governed corpus. |
| Python static facts; selected repair routing | `preview`; selected routing `usable alpha` | Language-specific first-run and mutation criteria are not measured by the current Rust instruments. Separate Python journeys and corpus are needed for R0–R2; R3 also needs a Python real-repair and installed-user protocol. |
| TypeScript and JavaScript | `preview` | Language-specific first-run and mutation criteria are not measured by the current Rust instruments. Separate journeys and corpus are needed for R0–R2; R3 also needs a language-specific real-repair and installed-user protocol. |
| Perl | `preview` routing; released producer combination not yet available | R0 and higher are not established for released Perl analysis; use the producer and receipt prerequisites in the [support map](status/SUPPORT_TIERS.md#current-support-map). |

See the [support map](status/SUPPORT_TIERS.md#current-support-map) for the
complete per-surface limits and [preview promotion criteria](policy/PREVIEW_PROMOTION_CRITERIA.md)
for the separate policy signoff. Proposed ladder thresholds do not replace
either authority.

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
