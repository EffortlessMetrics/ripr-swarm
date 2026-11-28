# Frozen experiments for RIPR hill climbing

The experiment layer answers: **on the same planned work, what changed in
verified outcomes and total effort?** It extends the [cohort core](CORE_CONTRACT.md).
It does not replace its scorer, the collection/reference work in #13071, the
per-task descriptive economics in #13517, or the analyzer's verdict corpus.

## The improvement loop

1. Choose the behavior or cost to improve. Keep false verdicts, abstention,
   executed quality, time and cost as separate objectives. Write the candidate's
   acceptance before inspecting its outcome.
2. Qualify the instrument with the existing native selftest. For analyzer changes,
   retain fresh `cargo xtask verdict-corpus check-all` evidence on the actual
   candidate. The DX trust board compares committed verdict records on unchanged
   common cases; it is not fresh analyzer execution. Use the existing
   [DX scoreboards](../../dx_scoreboard/README.md) for correctness and performance
   gates, including their case-population and same-runner rules.
3. Freeze a design before collection. Include ordinary tasks, no-op and negative
   controls, and independently selected challenging tasks. Reserve different task
   families for development and holdout. Pilot difficulty on development data.
4. Use the frozen schedule to collect all planned attempts with the existing
   collector. Keep failed attempts and their observed spend. Never replace a
   failed trial with a successful retry at the same ID; retries need a new,
   explicitly accounted experiment/protocol. Do not launch paid runs to debug
   deterministic instrument tests.
5. Compare development results first, including the predefined strata and named
   task quality regressions. Improve the candidate, freeze a new experiment, and
   repeat. Do not optimize against the reserved holdout after seeing its results.
6. Assess the selected candidate on the reserved partition and fresh correctness
   evidence. Review quality regressions before cost reductions. A favorable mean
   cannot erase a routine-task regression or incomplete experiment.

Neither an exit-zero report nor a bootstrap interval is an automatic promotion
decision. The tool always records `promotion_decision: not_made`. A release or
efficacy claim also needs the relevant native integration and fresh experiment
evidence; instrument tests alone do not supply it.

## Commands

From the repository root, using Python 3.11+:

```sh
python3 -I benchmarks/agentic/cohort/harness/selftest.py --require-native
python3 benchmarks/agentic/cohort/harness/experiment.py freeze design.json plan.json
python3 benchmarks/agentic/cohort/harness/experiment.py schedule plan.json > development-schedule.jsonl
python3 benchmarks/agentic/cohort/harness/experiment.py report plan.json development-result receipts.jsonl
python3 benchmarks/agentic/cohort/harness/experiment.py schedule plan.json --partition holdout > holdout-schedule.jsonl
python3 benchmarks/agentic/cohort/harness/experiment.py report plan.json holdout-result holdout-receipts.jsonl --partition holdout
```

`freeze` creates `plan.json` exclusively. Retain it in Git or another immutable
experiment record before launching trials. `schedule` emits metadata in frozen
order; it does not run a provider and its records are not outcome receipts.
The default partition is `development`; holdout access is explicit.

`report` writes a new `<prefix>.json` and `<prefix>.md`. It refuses an existing
output path. With no receipt files it emits every selected planned trial as
missing. A provided empty file has the same coverage meaning. Invalid input
produces no report. Use a new output prefix to retain each comparison.

| Report exit | Meaning |
|---|---|
| `0` | All selected model/arm comparisons have complete planned populations, verified quality and component-derived token usage. This does not mean the candidate improved. |
| `1` | A report was written, but planned population, verified quality or usage is incomplete. |
| `2` | Invalid plan/receipt, mismatched identity, or input/output error. |

Dollar cost and wall-time measurements have independent availability. An
otherwise comparable token/quality experiment can retain unknown dollar or
time estimates. `cost_usd` remains the existing SDK estimate, not billed cost.

## Design schema v1

Every field below is required; unknown fields, duplicate JSON keys and nonfinite
numbers are refused. SHA-256 values are 64 hex characters and canonicalize to
lowercase. A RIPR revision is a full lowercase 40-character Git SHA. Identifier
fields cannot contain whitespace or control characters.

| Object | Required fields and meaning |
|---|---|
| Design | `design_schema_version: 1`, `experiment_id`, nonnegative integer `seed`, positive integer `repetitions`, `max_turns`, `trial_timeout_s`, `protocol_sha256`, `evaluator_sha256`, `runner`, `control_arm`, `models`, `arms`, `tasks` |
| Runner | `id`, `environment_sha256` for the frozen relevant execution environment/toolchain |
| Model | `id`, `provider`, `revision`, `settings_sha256`; retain the provider's actual immutable model identifier where available and the complete sampling/client settings |
| Arm | `id`, `configuration_sha256`, `ripr_revision`, `ripr_binary_sha256`; both RIPR fields are null for an arm without RIPR, otherwise both are present |
| Task | `id`, `family`, `stratum`, `partition`, `task_sha256`, `oracle_sha256`, positive `mutants`, and ordered `mutant_sha256s` |

Models, arms and tasks are nonempty arrays with unique IDs. At least two arms
are required, and `control_arm` names one. Canonical IDs are used; the old
`A0-none`, `A1-diagnostics`, and `A2-packet-loop` aliases are refused. The control
can be `none` for an intervention experiment or a named baseline build for a
candidate comparison. Other arms can describe diagnostics, repair packets,
optional intervention or a candidate binary, with their configuration bytes pinned.

Each task has one `stratum`: `routine`, `no_op`, `negative_control`, or
`challenging`. Its `partition` is `development` or `holdout`. Both partitions
must exist in the design. A family cannot cross partitions, and an identical
task digest cannot add another subject even within one partition. Use one family
for related tasks from the same underlying code/behavior source. Naming different
families does not prove independence; that remains part of dataset review.

Every task/model/arm/repetition combination is planned, with repetitions numbered
from one. Expansion is bounded at 100,000 trials. The instrument permits a small
smoke design with one provider or missing strata; the report discloses those
limits. Such a design cannot satisfy the broader multi-provider efficacy goal.

### Derive identities from retained bytes

Use the existing `scoring.canonical_hash(frozen_task)` for `task_sha256`.
It identifies the complete task definition, including source, tests and selected
mutations. Compute each mutation's digest with the same function, preserving
the scorer's declared mutant order. Then use
`measurements.digest(mutant_sha256s)` for `oracle_sha256`. The list must be
nonempty, unique, and have exactly `mutants` entries.

`protocol_sha256` should identify the common trial protocol, prompts, collection
entrypoint and rules. `evaluator_sha256` should identify the retained scoring,
measurement and report source bytes. Hash the actual RIPR executable separately
from its source revision. Retain the bytes/manifests behind runner, model-settings
and arm-configuration digests; a meaningful-looking version string cannot stand
in for those bytes. This tool checks declared agreement, not the authenticity
of an external execution or whether a model alias is immutable.

The frozen plan records canonical design and schedule hashes. A seeded hash order
selects trial blocks. Within each task/model block sequence, a seeded permutation
of arms is rotated across repetitions, so each arm's position counts differ by
at most one. All arms in one repetition remain adjacent. `position` is a global
zero-based schedule position. Plan validation recomputes the entire schedule;
rehashing an edited order does not make it valid. A deliberately new design is a
new experiment, not a continuation that can reuse old trial IDs.

## Collector integration and admission

The existing collector remains the execution owner. It should consume the
selected schedule before launching work, enforce the frozen budgets and order,
independently observe the installed/model/configuration identities, then write
the planned metadata plus its actual terminal usage, process outcome and scorer
evidence. Copying plan metadata alone is not proof those inputs were executed.
This layer does not change the pending collector's process or reference behavior.

The schedule metadata supplies these required joins:

- `receipt_schema_version: 3`, `cohort_id`, `experiment_plan_sha256`, `trial_id`,
  `schedule_position`, `task`, `model`, `arm`, `rep`, `max_turns`, `trial_timeout_s`;
- `task_sha256`, `oracle_sha256`, `mutants`, `protocol_sha256`, `evaluator_sha256`;
- `runner_id`, `runner_environment_sha256`, `provider`, `model_revision`,
  `model_settings_sha256`, `arm_configuration_sha256`, `ripr_revision`,
  `ripr_binary_sha256`.

All joins must match the frozen plan exactly, including explicit nulls for a
no-RIPR arm. A receipt must also record `rc` (an integer, or null with a nonblank
failure `error`), boolean `result_observed`, and `is_error` (boolean or null when
unavailable). Schedule metadata alone cannot claim an observed attempt.
Available `score.mutant_results` also must name the planned mutant
ID and digest. Copying a top-level oracle hash cannot hide a changed or shortened
scored population. The shared `quality_verified` owner still decides whether
the full pristine/mutant/restored runtime evidence qualifies.

Unplanned, duplicate, relabelled or mismatched receipts fail admission rather
than being silently discarded. A missing terminal result, partial score or
missing usage remains an observed attempt with unavailable evidence; it cannot
establish a verified success. A missing receipt remains an explicit missing
planned trial. New planned trials require component-derived usage: a legacy
`tokens` total without the terminal `usage` object stays unknown.

The legacy `scoreboard.py OUT receipts.jsonl` command retains its existing input
and report behavior. It remains useful for descriptive historical accounting;
old receipts are not retroactively upgraded into planned trials.

## Report schema v1 and estimation

The report retains experiment/plan identity, selected partition, provider/model
counts, missing strata and total/selected planned counts. Each model/arm appears
even when it has zero receipts. Every present stratum has its own rows, as well
as an aggregate over that partition. The other partition's outcome data is not
pooled into the selected report.

Group summaries distinguish planned, observed, completed, quality-verified and
verified-success counts. Measurement rows retain measured/missing/unobserved
counts and observed totals. A mean or cost per verified success cannot appear
complete when planned spend is missing. Failed-attempt spend remains in the
numerator of cost per verified success.

Comparative estimates require the full planned paired population and verified
quality on both arms. Token comparisons also require complete terminal usage;
dollar and agent-wall-time estimates require their own complete measurements.
An omitted entire task, arm, repetition or model cannot shrink these denominators.
The report separately names tasks with worse mean verified success or higher
mean escaped fraction, even when improvements on other tasks cancel them out.

For each metric, the paired point delta is arm minus control, averaged over
repetitions within each task and then equally over tasks. Resource ratios use
the sum of all arm measurements divided by the sum of all control measurements;
they are not averages of per-trial ratios. A zero control denominator leaves
the ratio unknown. Quality and cost retain distinct units and meanings.

The deterministic bootstrap resamples whole declared families with replacement,
preserving task membership, all repetitions and arm/control pairing. It reports
2,000 draws and a linearly interpolated 2.5th/97.5th percentile interval. This is
descriptive sensitivity to the composition of a curated suite, not a confidence
claim about unseen production tasks or within-task stochastic variation. One
family cannot produce an interval, however many repetitions were run. Undefined
ratio resamples withhold its interval. A bounded work budget may withhold optional
intervals while retaining the exact point estimates; the reason is explicit.

## Proof and limits

The existing selftest discovers the new plan, report and statistics controls.
They cover omitted whole populations, failed-attempt spend, unknown usage,
identity and oracle substitution, duplicated evidence, split leakage,
reproducible/balanced schedules, paired weighting, one-family uncertainty refusal,
quality regressions hidden by pooled effects, and the actual CLI/Markdown outputs.
Synthetic report evidence is identified as such; it is not an agent experiment.
The native bridge still requires its six existing Cargo controls and zero skips.

Report and plan hashes establish consistency. They do not establish precollection
timing, execution order, hidden-oracle secrecy, real family independence,
provider provenance, causal benefit, or fresh analyzer correctness. Commit the
plan before collection, keep grading material outside the agent's workspace,
retain actual execution artifacts, and perform independent dataset review.
A repository-visible holdout is a reserved assessment partition, not a secret.

This additive contract advances #13560 under #12902/#12912. Collector integration,
representative qualified task catalog, a fresh multi-provider run and broader
efficacy acceptance remain separate. Revert the additive experiment owner and
its tests/docs to roll back; historical receipts need no migration or deletion.
