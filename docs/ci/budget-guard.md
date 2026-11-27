# Soft Budget Guard

The LEM budget guard is a **proposed advisory mechanism**, not an installed
qualification gate. The numeric PR planner and its rollup remain unactivated.
The existing `cargo xtask ci-budget` audit is a different, read-only report;
it does not enforce the planned guard.

The current execution targets are in [CI strategy](../CI.md#qualification-rebuild-architecture),
the [detailed qualification design](QUALIFICATION_REBUILD.md), and the
[#4841 acceptance clarification](https://github.com/EffortlessMetrics/ripr-swarm/issues/4841#issuecomment-6999978169):
under 20 total raw runner-minutes and under 20 minutes complete qualification
elapsed for a normal accepted submission or stack. The elapsed boundary is
first qualification allocation to complete result, not an assumed queue-delay
SLA. Count every associated merge-group attempt, including allocated prefix
dispatches, retries, replacements and all causally
required preparation, transport and finalization. Exact-G totals are diagnostic,
not independent acceptance. A forecast band or acknowledgement cannot remove a
required obligation or claim that either target has been measured.

## Retained ledger and cost bands

`policy/ci-budget.toml` retains `policy_state = "advisory-ledger"`,
`enforcement = "none"`, `[defaults].budget_guard = "off"`, and
`actuals_required = false`. This document changes none of those settings.

The legacy ledger's bands are:

| Band | LEM range | Legacy ledger posture | Use in planning |
| --- | ---: | --- | --- |
| `small` | 0–5 | `required` | Flag the expected order of magnitude for focused checks. |
| `medium` | 6–20 | `required` | Compare ordinary product-work forecasts. |
| `large` | 21–60 | `advisory` | Make a larger cost estimate visible for review. |
| `release` | 61+ | `on_demand_release` | Identify a release-sized estimate, not a release authorization. |

Those historical posture fields are not an implemented cost-to-applicability
rule. An expensive required test stays required. A cheap advisory result does
not become required merely by entering a smaller band. A future consumer must
reconcile these fields with the current qualification contract before using
them; no consumer may silently interpret `large` as permission to skip work.

LEM is an approximate forecast. Its inclusive medium-band endpoint establishes
neither the strict under-20 raw-runner-minute stack target nor the separate
elapsed target.

## Proposed advisory behavior

If implemented under the existing owner, the guard may compare a stated
forecast with observed cost and emit a warning. The retained design is silent
for small/medium forecasts, warns on large forecasts, and makes a release-sized
forecast explicit. An acknowledgement records acceptance of the estimated
expense; it does not authorize a merge, release or omitted check.

The guard must not fail qualification while the ledger remains unenforced.
It must not add an ordinary PR-event subscriber to obtain forecasts or warnings.
Use the retained explicit lifecycle boundary and existing report/summary
surfaces. [Current state](current-state.md) and [merge flow](../MERGE_FLOW.md),
not the legacy label notes, establish what starts work today.

## Labels and acknowledgements

The ledger records these budget intents. Actual workflow effects must be read
from the installed route; this table does not activate a trigger or override.

| Label | Retained budget intent |
| --- | --- |
| `full-ci` | A release-sized forecast for explicitly selected broader proof. |
| `release-check` | A release-sized forecast for separately authorized release-surface proof. |
| `ci-budget-ack` | Acknowledge an overrun; no change to required proof. |
| `vscode` | A larger forecast when additional editor work is explicitly selected. |
| `coverage` | A larger forecast for separately selected coverage work. |
| `clippy-future` | A medium forecast for separately selected future-Clippy evidence. |
| `ripr-waive` | Acknowledge advisory static findings, not CI cost or test failure. |

Neither `full-ci` nor `release-check` makes ordinary required workspace tests
optional when the label is absent.

## Calibration and enforcement

The earlier guard proposal called for at least 14 days of actuals before hard
enforcement. That is historical calibration guidance, not an installed timer
or a requirement to delay an otherwise accepted correctness or efficiency
change. Elapsed days alone cannot establish representative data.

Any future enforcement proposal must define the observed cohort, accounting
completeness, cold/warm composition, comparison rule, expected variance,
failure behavior and rollback. Missing telemetry cannot be converted into a
cheap estimate. Preserve unfavorable and incomplete attempts. Do not install
an automatic blocker or weaken required proof merely because a forecast band
has been exceeded.

The two measured under-20 engineering acceptance targets remain distinct from
whether an automated budget guard is enabled. Report stack totals and landed-PR
counts, not a per-PR average that conceals an expensive stack. Retain
enqueue-to-result and enqueue-to-landing separately from qualification elapsed;
they do not acquire the same threshold by implication. H admission and control
work remain separately explicit lifecycle costs.

## Accounting owner and limitations

Extend the existing `xtask/src/reports/ci_budget.rs` audit/report owner with
the attributable native job/attempt accounting described in
[CI actuals](ci-actuals.md). Its present route-duration and retry summaries are
not complete submission/stack accounting. In particular, a successful rerun is
not by itself evidence that the earlier failure was infrastructure-related.

The former `cargo xtask ci budget-guard` command spelling is a proposal, not
an available command. A new implementation needs focused fixtures for missing
jobs, duplicate records, allocated prefix dispatches, reruns, replacements,
partial API reads and changed subjects before its numbers can support
acceptance. It remains advisory unless an explicit
later policy change authorizes enforcement.

The lane whitelist is complementary scope evidence: it helps identify
workflows that need accounting. A whitelist row or PR Plan forecast cannot
prove that all allocated jobs were collected. Reconcile native executions,
including required preparation and consumers outside the main workflow.

## See also

- [CI strategy](../CI.md) and [qualification design](QUALIFICATION_REBUILD.md).
- [CI actuals](ci-actuals.md) for proposed measurement and cache-state fields.
- `policy/ci-budget.toml` for the retained advisory ledger.
- `policy/ci-lane-whitelist.toml` for workflow inventory.
