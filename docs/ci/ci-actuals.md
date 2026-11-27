# CI Actuals

This is the **proposed accounting contract**, not an installed metrics schema,
lane emitter, PR Plan rollup or budget gate. The
[detailed qualification design](QUALIFICATION_REBUILD.md) owns the target
execution and acceptance model; this page specifies the evidence its cost
claims need. [Current state](current-state.md) records what is installed.

The acceptance denominator is a normal accepted submission or stack:
**under 20 total raw runner-minutes and under 20 minutes complete qualification
elapsed across all associated merge-group attempts**. Qualification elapsed
runs from the first qualification allocation to the complete result. Count
runner-allocating prefix dispatches, retries, replacements and all causally
required preparation, transport and
finalization, including off-run work. This follows the
[#4841 acceptance clarification](https://github.com/EffortlessMetrics/ripr-swarm/issues/4841#issuecomment-6999978169).

Exact-G totals remain diagnostic and cannot independently pass the stack
objective. Report each stack total and its landed-PR count; a per-PR division
must not conceal an expensive stack. Keep enqueue-to-result and
enqueue-to-landing separate from qualification elapsed; the target does not
establish a universal queue-delay SLA. Cheap H admission and control work remain
explicit additional lifecycle costs, not omitted work or hidden allocation to
a G.

## Existing owner and current limits

Extend `xtask/src/reports/ci_budget.rs`, the existing read-only
`cargo xtask ci-budget` audit/report owner. It writes
`target/ripr/reports/ci-budget.json` and `ci-budget.md`, can consume captured
JSON through `--input`, and does not dispatch, retry, cancel or qualify work.

The current implementation summarizes route-specific job durations, run
attempt counts and failure signals. Its live job read is not an all-attempt
collector; its duration table selects implementation lanes by job name rather
than summing every allocated qualification job. Missing job reads and unknown
failure classifications do not establish complete accounting. A successful
rerun alone does not establish an earlier infrastructure failure.

Keep that report's current scope honest while extending its typed model and
collectors. Do not introduce another registry, resident service or automatic
development-event workflow. The future format and version must be implemented
and qualified before any caller may rely on them.

## Proposed record requirements

Record identities and raw observations before computing summaries. One logical
obligation may share a producer with another; one job may contain many phases.
Neither creates additional physical execution cost.

| Record | Required information |
| --- | --- |
| Submission / stack | Existing request/admission identity, ordered submitted membership, all associated G lifetimes and prefix dispatches, retry/replacement relationships, first qualification allocation, enqueue time, terminal result and protected landing times, landed-PR count and observation window. |
| Exact-G diagnostic | Repository, exact G/tree, frozen B0, ordered PR/H membership, protected control generation C, plan/schema identity, selected obligation/inventory digests and relationship to the accepted submission/stack. |
| Native job execution | Repository, run ID, run attempt, job ID, workflow path/source identity, authoritative status/conclusion, start/completion timestamps, runner labels and observed platform. Link the job to its submission/stack, G where applicable, and producer/consumer or H/control role. |
| Preparation and reuse | Compatible build-family identity, producer execution reference, exact input/output manifest identities, consumer decision and reason, actual preparation/rebuild count, and any original producer whose evidence was reused. |
| Phase observation | Owning job execution, phase name, start/end or measured elapsed seconds, command count, outcome and collection method. Identify overlap instead of adding concurrent phase durations to job time. |
| Resource observation | Available CPU user/system seconds; attributed compiler peak, simultaneous owned-process-tree memory, swap, scratch peak/minimum free space and transfer bytes; units, method and sampling scope. |
| Collection completeness | Source endpoint/report, capture time, stack/G association and pagination/attempt coverage, missing records, unavailable fields and explicit reasons. Unknown values are not zero. |

Use stable native IDs for attribution; a mutable job name is a display label,
not an identity. Keep producer, verifier and finalizer attempts distinct.
An artifact receipt is a correlation input, not independent proof that a job
executed, remained eligible or passed.

Use explicit units: elapsed/CPU seconds and bytes for normalized memory, disk
and transfer fields. Retain the original units and method when converting
KiB/MiB observations. An individual compiler's maximum RSS is not simultaneous
aggregate memory. CPU inferred from elapsed time or summed per-process peak
RSS presented as a simultaneous peak is not a measurement.

Record observed runner class, architecture and capacity when available;
unavailable capacity is disclosed. A runner label alone does not establish
hardware-normalized comparison. Do not collect secrets or an unrestricted
environment dump as configuration evidence.

## Aggregation and completeness

The proposed accounting rules are:

1. Enumerate all G lifetimes, prefix dispatches, runs and attempts associated
   with the accepted submission/stack, plus every causally required producer
   outside its primary workflows. Retain retry/replacement relationships and
   read complete native job pages. Preserve source records so the stack rollup
   and exact-G breakdowns can be reproduced. Unknown association is incomplete
   accounting, not permission to omit the work.
2. Deduplicate by native execution identity. Charge each actual job once, even
   when it satisfies multiple obligations or feeds several consumers. A native
   job ID encountered through multiple attempt pages is still one execution;
   retain its original execution attempt, not the verifier's attempt. Conflicting
   identity metadata is unresolved, not another billable job. Distinct rerun jobs
   are distinct cost.
3. For a completed allocated job, derive raw seconds from its native
   `completed_at - started_at` interval. Include setup, action post-steps and
   cleanup within that interval. A non-success conclusion does not erase
   occupied work. A prefix dispatch that allocates a runner costs time even if
   it starts no product tests.
4. Sum raw seconds before dividing by 60. Do not round every job up to a minute,
   sum overlapping test/phase durations, or substitute the longest job for the
   total. These native job spans define this accounting measure; they are not
   CPU time, invoice minutes or independently instrumented physical occupancy.
5. Count a skipped or never-allocated job as zero only when native state
   establishes that fact. Missing endpoints, incomplete pages, a still-running
   job, or an unavailable completion time leave an incomplete total; record
   any observed lower bound separately.
6. Include every associated merge-group attempt: failed/retired predecessor Gs,
   replacements, producer retries and finalizer retries. Break these costs out
   without removing them from the stack total. Record reused evidence's
   original execution and cost once; do not charge its compile again merely
   because another finalizer reads it, or hide it by considering only the final
   successful G or attempt.
7. Report the complete stack total together with its landed-PR count. A
   supplemental per-PR ratio cannot establish acceptance or replace that total.
   Zero landings yields an unavailable ratio, not zero cost. Retain H admission
   and control work as separate explicit lifecycle costs; broader lifecycle
   reports also retain unsuccessful submissions and reconcile shared executions
   without double-counting them.
8. Derive complete qualification elapsed from the first qualification allocation
   to the complete stack result. Also derive enqueue-to-result and
   enqueue-to-landing from their own native timestamps and report both
   separately; they do not inherit the qualification threshold. A new G,
   replacement or retry does not restart any stack clock. Keep queue wait and
   exact-G elapsed diagnostics separate. Missing allocation, enqueue, result or
   landing timestamps leave the corresponding measure unknown; the latest
   successful job's wall time is not a substitute.

Causally required preparation cannot be moved into a separate workflow or an
earlier timestamp to make the target pass. For reusable generation-level work,
retain its original cost and consumers and state the attribution method.
Do not invent a free producer or spread a known cost over hypothetical future
consumers. Distinguish an existing opportunistic cache from preparation
explicitly required to make the selected qualification work.

The accounting report remains advisory. Incomplete telemetry cannot support a
cost-target claim, but it does not invent a new required product check.
Qualification still depends on its separately authenticated producer evidence.

## Cache and preparation states

A single `cache_hit` boolean cannot describe a qualification. Observe each
relevant layer separately: toolchain/image, registry or source dependencies,
compiled Cargo outputs, prepared CLI/test artifacts, and fixture dependencies
or generated fixture state.

For each layer, preserve these independent dimensions:

| Dimension | Proposed states / evidence |
| --- | --- |
| Lookup | Not requested, miss, exact-key hit, fallback/prefix hit, error or unknown; identify the actual requested/restored generation. |
| Compatibility validation | Not evaluated, accepted, rejected or unknown, with the input/configuration or integrity reason. |
| Actual use | Not used, reused, rebuilt or unknown; identify the producer/consumer and observed avoided or performed preparation. |
| Transfer and preparation | Bytes restored/uploaded, extraction and validation time, fresh-build work and writer identity, when measured. |

A restored cache can fail compatibility and rebuild. A cache can restore
successfully without the real consumer using it. Neither is an exact prepared
artifact reuse. Retain cold, partial and warm layer states rather than forcing
the whole transaction into one optimistic label.

A cold proof names the cleared reuse layers and the preparation that remained
available, including runner images or installed toolchains. Fresh processes with
a warm filesystem are not a fully cold build. Undeclared fixture dependencies
or an inherited target directory cannot silently supply the cold route.

The selected CI design has one rebuild owner per compatible preparation family.
A consumer miss or refusal returns through that owner instead of starting an
independent Cargo build in every consumer. Local standalone fallback remains a
separate route. A rejected cache preserves correctness through bounded owned
recomputation or its real failure; no scheduled warmer is a prerequisite.

## Comparison and acceptance

Choose the accepted-submission/stack cohort, obligation/configuration identity,
observation window, cache-state mix and comparison rule before reading the
result. Keep individual stack totals, landed-PR counts, associated native IDs,
qualification elapsed and both enqueue-based views, not only an aggregate or
per-PR average.

Report cold/warm strata, sample count, p50, p90 and maximum, with the percentile
method stated. These are descriptive statistics; a small sample does not
establish stable tail behavior. Retain failed, confounded and unfavorable
measurements with their reasons. A favorable median alone cannot establish the
two under-20 stack targets; disclose every observed exceedance in total runner
work or qualification elapsed. Exact-G figures can locate a saving or failure
but cannot establish stack acceptance independently.

An enabling implementation can satisfy its bounded claim before the complete
cost target is met. A claimed optimization needs a reached consumer and
whole-boundary evidence with unchanged required meaning. A parent-target claim
needs the declared accepted-submission/stack cohort, complete accounting and
the resource and correctness acceptance in
[the qualification design](QUALIFICATION_REBUILD.md).
No fixed quota of five full CI runs is introduced for each candidate.

Useful collector fixtures include duplicated pages, missing pages, absent
timestamps, allocated cancellation, never-started cancellation, two attempts
with reused producer evidence, a later failed producer with no artifact, and
two obligations sharing one preparation job. Include an allocated prefix that
runs no product tests, a replacement G whose isolated cost passes while the
stack total fails, and a multi-PR stack whose per-PR average hides an overrun.
Reject clock-reset accounting after a replacement. These cases challenge the
totals; native producer eligibility still belongs to the qualification verifier.

## Historical lane-format proposal

The `schema_version: "0.1"` example in
[CI strategy](../CI.md#ci-actuals) is the earlier planned lane-level shape:
`workflow`, `job`, `status`, `duration_seconds`, `runner`,
`estimated_lem`, `actual_lem` and `cache_hit`. Its proposed
`ceil(duration_seconds / 60)` LEM estimate is not the raw stack sum.
It lacks the identities, attempt coverage and cache dimensions above.

Neither that example nor this document establishes a deployed
`target/ci/ci-actuals.json` emitter, PR Plan aggregator, runner multiplier
table, Test Analytics retention path or hard budget guard. A schema migration
must update the typed owner and its actual consumers together, with explicit
compatibility and rejection behavior; do not silently reinterpret old records.

Retain existing report/artifact surfaces and name their actual retention.
Accounting captures needed for accepted evidence must remain retrievable for
that acceptance; a later rerun is a new observation. Diagnostic retrieval
windows are not automatically qualification-evidence retention. No new
persistent storage service is required by this design.
