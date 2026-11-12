# RIPR-SPEC-14525: Advisory runner occupancy

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
