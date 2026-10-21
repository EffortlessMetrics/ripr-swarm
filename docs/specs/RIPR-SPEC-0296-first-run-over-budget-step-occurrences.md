# RIPR-SPEC-0296: first-run over-budget steps count step occurrences

Status: proposed

Owner: product / xtask

Created: 2026-10-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8921 (fix(dx-scoreboard): count budget breaches once per step occurrence)
- #8722 / #8918 (residual: renderer aligned first, ingestion deferred)

Linked PRs:

- None yet

Support-tier impact:

- None. This is repository automation (`xtask dx-scoreboard`), not product
  analysis. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No threshold, schema or runner change. Baselines recorded before the
  `step-occurrences-v1` grain marker counted breached metric rows; they
  are explicitly not compared for this metric (see Rule 4). The metric definition in
  `benchmarks/dx_scoreboard/scoreboards.toml` already declares
  `unit = "steps"`; this spec aligns ingestion with that declared grain.

## Problem

`first_run.over_budget_steps` had two grains. Before this change, the
public-proof renderer (`xtask/src/public_proof.rs`, `step_over_budget`)
counted step occurrences: a step breaching both its time (`secs`) and
output (`stdout_lines`) budgets counted once. First-run row ingestion
(`first_run_rows_to_input` in `xtask/src/reports/dx_scoreboard.rs`) counted
breached metric rows, so the same step counted twice. One metric therefore
disagreed across surfaces depending on which harness produced it.

## Behavior

### Rule 1: the grain is the step occurrence

Before: breached metric rows. After: step occurrences.
`first_run.over_budget_steps` counts each step occurrence that breaches at
least one of its `secs` or `stdout_lines` budgets once. A step occurrence
breaching both budgets yields value 1 for that occurrence.

### Rule 2: occurrence identity at ingestion

Exported `first_run_row.v1` rows carry no occurrence id; they are keyed by
case, step and metric. Ingestion groups budgeted rows by step occurrence
ordinal for the case and step: exported rows are occurrence-sequential
(`secs`, `exit`, `stdout_lines` per occurrence), so the nth `secs` row
starts occurrence n and a `stdout_lines` row belongs to the latest
occurrence started for that step (its own ordinal when it precedes any
`secs` row). A missing `stdout_lines` row therefore cannot shift the next
occurrence's output onto an earlier occurrence that breached only its
time budget. Repeated occurrences with the same step name remain
separately countable.

### Rule 3: evidence lineage is retained

Both evidence entries (for example the time and stdout entries for one
occurrence) remain in the metric's evidence list, attached to the single
counted occurrence. Counting changes; evidence does not.

### Rule 4: baselines compare only at the same grain

The metric's evidence names its grain (`step-occurrences-v1:`), so the
marker survives ingest into a baseline report's sample detail.
`compare_with_baseline` does not compare `first_run.over_budget_steps`
against a baseline whose samples do not all name that grain; it reports
the metric as not compared instead. A pre-change baseline counted
breached metric rows and retains no raw rows to recount from, so
comparing across grains could hide an added over-budget step (one former
dual breach and two current single breaches both read 2). A baseline
recorded at the step-occurrence grain still compares normally.

### Rule 5: neighbouring behavior is unchanged

`workflow_lines` breaches stay on `first_run.over_budget_workflows`.
Null or missing budgets cannot breach, missing metric rows create no
phantom occurrence, and existing incomplete or out-of-order row handling
(failed steps, walk seconds, install samples) is preserved.

## Required Evidence

- A single step occurrence breaching both budgets ingests as value 1 with
  both evidence entries retained.
- Two occurrences of the same step name, each breaching both budgets,
  ingest as 2; a first occurrence breaching only stdout and a second only
  time also ingest as 2.
- A step breaching only one budget counts 1; null budgets and missing
  metric rows add no count.
- A first occurrence breaching only time with no `stdout_lines` row and
  a second breaching only output ingest as 2, not 1.
- A baseline whose samples do not name `step-occurrences-v1` is not
  compared for this metric; a baseline that names it still compares and
  regresses on a rise.
- The exported-rows ingestion agrees with the renderer grain pinned by
  `dual_metric_breach_counts_once_per_step` in
  `xtask/src/public_proof.rs`.

## Non-Goals

- Changing the renderer, the row exporter, budgets, thresholds
  or runner behavior.
- Changing `first_run.over_budget_workflows` or any other metric's grain.
- Adding an occurrence id to the `first_run_row.v1` wire shape.

## Acceptance Examples

1. One `check` step at 12 s (budget 10) and 61 stdout lines (budget 60):
   value 1, evidence lists both breaches.
2. Two `check` occurrences, each over both budgets: value 2.
3. A `doctor` step with null budgets and a `pilot` step under budget with
   no stdout row: neither adds a count.

## Test Mapping

- `xtask/src/reports/dx_scoreboard/tests.rs`:
  `first_run_rows_count_a_dual_budget_breach_once_per_step_occurrence`
  (example 1 and workflow separation),
  `first_run_rows_count_repeated_step_occurrences_separately` (example 2),
  `first_run_rows_single_budget_and_missing_rows_have_no_phantom_count`
  (example 3),
  `first_run_rows_missing_first_stdout_row_does_not_merge_occurrences`
  (missing-row occurrence identity),
  `first_run_over_budget_baseline_from_breached_row_grain_is_not_compared`
  (baseline grain).
- `xtask/src/public_proof.rs`: `dual_metric_breach_counts_once_per_step`
  pins the renderer grain the ingestion now agrees with.

## Implementation Mapping

- `xtask/src/reports/dx_scoreboard.rs`: `first_run_rows_to_input`
  groups `FIRST_RUN_BUDGETED` rows into per-occurrence slots and counts
  breached occurrences while retaining every breached row as evidence.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
