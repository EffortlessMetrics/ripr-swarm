# Cohort measurement contract (#12902, #12912)

The question is useful test outcomes per total agent effort, not the smallest
reply or the cheapest failed attempt. This instrument reports quality and cost
separately; a lower token ratio is not an efficiency-win verdict.

## Collection and token accounting

New receipts use `receipt_schema_version: 3`. `run_one.py` consumes the terminal
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
`report_schema_version: 3`. Consumers must use the new coverage fields and null
semantics rather than treating missing values as zero. Every receipt is an
attempt, including failures. `completed_runs`, `successful_runs`,
`harness_error_runs`, `unscored_runs` and `escape_measured_runs` are separate counts.
The compatibility fields `successful_runs` and `fix_rate` retain reported
historical quality. A reported successful run requires terminal completion,
unchanged production source, a passing pristine test suite, a nonempty internally
consistent mutant result and zero recorded survivors. New verified-quality fields
add the execution and source checks in [the oracle contract](ORACLE_CONTRACT.md).
Only `verified_successful_runs` contributes to cost per verified success. A timeout with good partial tests is not completed
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

## Version 3: executed quality and whole-evaluation cost

`score` retains the evaluator-owned production/test identities, actual baseline,
each intended mutant, restored-code controls, phase outcomes and unresolved IDs.
`run_one.py` archives the exact tests transferred into grading and binds that
archive digest in `final_tests_sha256`. A later mutation of the agent checkout
cannot rewrite the archived test subject. Source/configuration is rebuilt from
the frozen task, not the agent's mutable Git HEAD. The scorer contract describes
which trusted native observations support a catch and the limits of that claim.

The report distinguishes `verified_quality_runs`, `verified_successful_runs`,
`verified_escape_rate` and `legacy_quality_runs`. New `tokens_per_verified_success`
and `cost_usd_per_verified_success` divide all measured attempt spend by verified,
completed successes; missing spend or zero verified successes produces null.
Legacy reported scores and old cost-per-reported-success fields stay readable,
but the Markdown headline uses cost per **verified** success. No historical
receipt acquires executed quality merely by being read by the new scorer.

`scoring_wall_s` measures the scoring call separately from agent execution;
`evaluation_wall_s` is agent plus scoring elapsed time, not whole-process startup,
artifact flush, cleanup or billing duration. Missing legacy timing stays unknown.
Paired comparisons report `verified_quality_pairs` and verified escape deltas;
`comparison_status` distinguishes partial pairing, missing usage, unverified
quality and a descriptive fully matched comparison. Even fully measured,
quality-verified pairs are descriptive observations, not a significance test or
an automatic token-efficiency win.

Reference-mutant qualification now consumes this same execution owner. It keeps
all attempted candidates and can exclude actual compiler-rejected mutants with
valid restoration evidence; timeout/build/process/zero-test failures cannot be
quietly removed from the selection. Incomplete qualification preserves the prior
selected mutant list and writes the unresolved qualification record for repair.

The changed instrument does not remove model-generated-oracle bias, create harder
tasks, randomize trial order, authenticate supplied receipts, record all
agent-visible tool-output bytes, or establish real-world quality improvement.
These remain separate acceptance under #12902/#12912. Before another paid cohort,
freeze comparable inputs, validate reference controls, predeclare quality/budget
criteria and preserve the planned-attempt denominator. Do not infer improvement
from output compression or a cheaper failed attempt.
