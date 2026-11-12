# RIPR-SPEC-0441: Advisory runner occupancy

Status: proposed

Owner: test-infra

Linked issues: #14525, #14500

## Problem

Legacy routed-lane durations omit coordinators and other job names. Input
snapshots and attempt ordinals also cannot establish complete attempted cost.
The advisory report needs an observed raw-work subtotal with explicit limits.

## Behavior

`cargo xtask ci-budget` retains its failure classifier and legacy lane metrics.
Its additional `runner_occupancy` object counts elapsed seconds of observed,
completed, uniquely identified jobs regardless of job name or terminal
conclusion. Skipped jobs contribute no fabricated runner work. Parallel jobs
sum occupancy; this is neither critical-path elapsed nor queue wait.

Native job IDs deduplicate repeated observations across attempt snapshots.
Conflicting identities are excluded and reported. Identical jobs repeated across
bound attempt snapshots count once, but their native attempt ownership cannot
be established from this DTO and accounting remains explicitly incomplete. Missing or empty inventories,
missing attempt/job identity, unfinished work, unsupported/malformed/reversed
timestamps, conflicting snapshots, missing earlier attempts, and checked-sum
overflow prevent complete captured-attempt accounting. Empty samples are not
complete zero-cost qualifications. Timestamps accept whole-second UTC only;
offsets and fractional timestamps remain explicitly unsupported, not truncated.

Live collection still reads only latest-attempt jobs. Separate list/job reads
are not an atomic attempt binding, so live accounting explicitly remains
incomplete. Offline embedded jobs belong to the supplied run/attempt snapshot;
multiple snapshots can retain earlier attempts. Their identities and raw job
timestamps appear in JSON. This is input evidence, not independently verified
GitHub provenance. Legacy arithmetic is unchanged: JSON basis metadata and Markdown labels identify
input-snapshot counts and sums of attempt ordinals, not distinct native runs or
observed attempts. These are not an all-attempt or deduplicated cohort report.

JSON and Markdown share the same calculation. Missing or conflicting work is
excluded from the observed subtotal, never estimated by multiplying duration
by attempt count. No average or per-PR division is produced. Portfolio
completeness remains false even when captured-attempt accounting is complete.
There is no stack-membership/cohort join, billing multiplier, all-workflow
collection, monthly-cost inference, CI speedup, or qualification dispatch.

## Required Evidence

Retain run ID, supplied attempt identity, inventory and attempt-binding flags,
and raw job IDs, status, conclusion and timestamps alongside the subtotal.
Both JSON and Markdown must expose incompleteness reasons and distinguish
captured-attempt accounting from whole-portfolio completeness.

## Non-Goals

- No all-attempt native collector or authenticated provenance for offline input.
- No inference of queue membership, critical path, billed cost or monthly cost.
- No qualification authority, workflow routing change or CI performance claim.

## Acceptance Examples

- Three distinct completed 60-second jobs, including failed and cancelled
  jobs, contribute 180 raw runner-seconds even when they ran in parallel.
- A skipped job with scheduler timestamps contributes zero jobs and seconds;
  `skipped`, `SKIPPED` and mixed ASCII case have the same meaning.
- Two identical observations of a job count once. A conflicting observation
  excludes that job and reports incomplete accounting.
- Three supplied snapshots for attempts 1, 2 and 2 retain legacy snapshot
  count 3 and ordinal sum 5, but two unique 60-second jobs contribute 120
  seconds. Rendered headings call the legacy counts snapshots, not runs.
- A latest-only attempt-2 sample cannot establish the missing first attempt.
  Malformed timestamps are excluded, not rounded or estimated.

## Test Mapping

- `xtask/src/reports/ci_budget/occupancy.rs::tests::parallel_and_failed_work_sum`
- `xtask/src/reports/ci_budget/occupancy.rs::tests::skipped_work_does_not_fabricate_occupancy`
- `xtask/src/reports/ci_budget/occupancy.rs::tests::duplicate_is_counted_once_and_conflict_excluded`
- `xtask/src/reports/ci_budget/occupancy.rs::tests::earlier_attempts_retained_not_multiplied`
- `xtask/src/reports/ci_budget/occupancy.rs::tests::timestamps_are_validated_not_truncated`
- `xtask/src/reports/ci_budget.rs::tests::multiple_attempt_snapshots_label_legacy_ordinals_and_deduped_work`
- `xtask/src/reports/ci_budget.rs::tests::occupancy_renders_nonlegacy_jobs_and_incompleteness`

## Implementation Mapping

- `xtask/src/reports/ci_budget/occupancy.rs`: pure raw occupancy calculation.
- `xtask/src/reports/ci_budget.rs`: existing collector, input DTO, aggregation,
  and JSON/Markdown output. Live collection remains latest-only and unbound.
- `.ripr/traceability.d/14525-runner-occupancy.toml`: behavior mapping.

## Metrics

- `unit_test_pass_rate` for the mapped behavioral controls.
- Observed raw runner-seconds, counted jobs, duplicate observations and named
  missing-evidence reasons are diagnostics, not speed or cost improvements.
