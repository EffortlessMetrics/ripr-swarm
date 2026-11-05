# Cohort measurement contract (#12902, #12912)

The question is useful test outcomes per total agent effort, not the smallest
reply or the cheapest failed attempt. This instrument reports quality and cost
separately; a lower token ratio is not an efficiency-win verdict.

## Collection and token accounting

New receipts use `receipt_schema_version: 2`. `run_one.py` consumes the terminal
`result.usage` once, preserving input, output, cache-creation input and cache-read
input counters under `usage`. `tokens` is their sum only when all four are
nonnegative integer measurements. Explicit zero cache counters are valid; missing,
null, negative, boolean or malformed counters leave the total null. Partial
components remain visible. Assistant-message echoes are not added to cumulative
terminal usage. No token estimate is inferred from characters or bytes.

`cost_usd` is the provider SDK's estimate, labeled `cost_source: sdk_estimate`;
it is not billed cost. Token volume includes cached input, so token ratios and
dollar ratios are not interchangeable. See the provider's
[cumulative usage guidance](https://platform.claude.com/docs/en/agent-sdk/cost-tracking)
and [cache counter definitions](https://platform.claude.com/docs/en/build-with-claude/prompt-caching).

Missing terminal results and timeouts retain unknown totals, partial transcript
call counts under `observed_tool_calls` / `observed_ripr_calls`, and their actual
failure state. Error results with complete usage retain incurred cost. Scorer and
post-run metadata exceptions retain already-collected agent usage instead of
turning the trial into a free failure. Sweep exceptions retain task, config,
model, replicate, cohort and actual turn cap, plus task/protocol hashes when
available. One writer appends completed receipts; each trial has its own workspace
and control shim. Collection remains opt-in; no default CI model calls are added.

## Denominators and outcomes

The JSON report remains a list of per-config rows, now with
`report_schema_version: 2`. Consumers must use the new coverage fields and null
semantics rather than treating missing values as zero. Every receipt is an
attempt, including failures. `completed_runs`, `successful_runs`,
`harness_error_runs`, `unscored_runs` and `escape_measured_runs` are separate counts.
A successful run requires terminal completion, unchanged production source, a
passing pristine test suite, a nonempty internally consistent mutant result and
zero recorded survivors. A timeout with good partial tests is not completed
success. Empty or inconsistent oracle results are unscored, not perfect results.

`escape_rate` is the mean fraction of escaped mutants over measured runs only;
its denominator is always disclosed. Unscored failures are not assigned fictional
100% escape rates. `fix_rate` uses all attempts. Its retained Wilson interval is
descriptive and does not correct task clustering or establish an arm difference.

Each metric exposes `measured_runs`, `missing_runs` and `observed_total`. The
complete-cohort mean is null unless every attempt was measured. Missing values
never enter an average as zero. `tokens_per_success` and `cost_usd_per_success`
use **all attempt spend**, including failures, divided by successful runs; they
are null when any spend is missing or there are no successful runs. This is an
accounting ratio, not a probability-of-success or general productivity estimate.

## Paired comparisons and provenance

A sweep generates a fresh `cohort_id`. `COHORT_ID` may deliberately join files
from one frozen experiment; do not reuse it for different model snapshots,
protocols, tasks, binaries or repetitions. New receipts hash the task definition
(including the selected mutants) and common prompt, tool grants, timeout and
harness sources. These hashes bind recorded inputs, not independent attestation
of an execution or an oracle's validity.

Pairs require equal cohort, requested model, turn budget, task and replicate;
recorded task/protocol hashes and mutant denominators must also agree. Missing
hashes in legacy receipts are explicitly marked as unverified identity. Without
an explicit cohort ID, each legacy input file is its own cohort, identified by
its content hash; unrelated files cannot silently supply each other's controls.
Legacy arm names remain readable. A legacy exception without its turn cap keeps
an unknown budget rather than being assumed to have used 40 turns.

Reports list unmatched arm/control keys and subject mismatches. Observed paired
token deltas include their measured-pair count. The headline token ratio and
success-rate delta require complete pairing; the ratio additionally requires
complete usage and a nonzero control denominator. An escape-rate delta requires
complete measured quality. The token ratio is the ratio of the two paired token
sums, not an unweighted mean of per-run ratios. A lower ratio accompanied by worse
quality remains visible as a tradeoff, never an automatic win. Duplicate attempt
identities, malformed receipt identities and mixed reported RIPR versions fail
with actionable diagnostics rather than choosing or dropping observations.

## Scope of evidence

Historical receipts, tables and recorded claims are not rewritten. Legacy token
aggregates cannot recover missing cache components or prove completeness; positive
legacy totals remain descriptive measurements. New collection does not prove that
old cohorts were comparable. Model aliases are not pinned model snapshots, and a
reported RIPR version is not a verified executable digest. There is no significance
test for token savings, pooled-provider result or causal-benefit claim here.

The underlying scorer is unchanged, including its existing timeout/failure
classification. This accounting change does not validate that classifier, remove
model-generated-oracle bias, create harder tasks, randomize trial order, record
all tool-output bytes, or establish real-world quality improvements. Those remain
separate acceptance under #12902/#12912. Before spending on the next cohort, freeze
comparable inputs, choose tasks with useful headroom, and predeclare quality and
budget criteria. Do not infer improvement from output compression alone.
