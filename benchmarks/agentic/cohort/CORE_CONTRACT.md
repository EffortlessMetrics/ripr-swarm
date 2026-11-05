# Cohort scoring and accounting contract

This is the independently runnable core extracted from #13071, including its
#13146 accounting repair and #13192 runtime-quality repair. It owns the question
of useful test outcomes per total agent effort, not an automatic efficiency win.
RIPR's static classifications and product code are unchanged.

## Execution and source authority

`workspace.py` constructs trusted Rust task crates and delegates scoring to
`scoring.py`. The scorer reconstructs the frozen task in an evaluator-owned
workspace and compares production/configuration files with the submitted tree.
An agent commit cannot redefine the pristine source. Only regular files under
`tests/` are transferred, bounded by 1,000 files and 10 MiB; symlinks, special files
and an empty transfer are refused. The exact transferred tests may be archived
and their digest retained. Submitted source, manifest, `.cargo` or build-script
changes cannot silently alter grading.

Default and custom manifests receive an empty `[workspace]` table when none is
present; an explicit workspace table is preserved. This prevents temporary task
crates from accidentally joining an enclosing Cargo workspace. Python 3.11's
TOML parser distinguishes tables from comments and retains custom package options.
The task and test bytes, not an agent's mutable Git HEAD, define the subject.

The execution protocol is `cargo-libtest-pass-fail-pass-v1`. Each invocation has
separate offline compile-only and stock-libtest phases under a common timeout.
Compiler JSON errors are distinct from dependency/build errors. Complete nonempty
libtest summaries and matching native exits establish pass or test failure;
timeout, process failure, cleanup failure, zero tests, incomplete output or
changed inputs cannot establish a catch. Phase, exit, counts, elapsed time,
raw stdout/stderr hashes and lengths, and bounded diagnostic tails are retained.
Owned process trees are terminated on timeout; cleanup failure remains visible.

The initial pristine tests must pass with nonzero execution. For every declared
mutant, retain its identity, run it, restore the original bytes and rerun pristine
with the same transferred tests. A catch requires a mutant test failure and a
restored pass matching the original suite/test counts. A survivor requires a
complete mutant pass and matching restoration. Failed restoration stops later
execution and records the remaining intended mutants as not run. Invalid,
duplicate, no-op and unresolved mutations do not shrink a frozen denominator.

Complete quality requires a nonempty fully accounted population containing only
caught or escaped outcomes. Retained-evidence validators check the joins between
labels, phases, native exits, test counts, identities and aggregate counters.
A fabricated receipt can still be internally consistent: this is not independent
producer authentication or a security sandbox. Custom harnesses, doctests,
untrusted submitted code and arbitrary repository graphs are outside this scope.

## Token and cost accounting

`measurements.py` reads cumulative terminal-result usage once, preserving input,
output, cache-creation and cache-read counters. A total exists only when all four
are measured nonnegative integers. Explicit zero is valid; absent, null, negative,
boolean or malformed counters leave the total unknown. Assistant-message echoes
are not added again. No tokens are inferred from bytes or characters.

Provider `cost_usd` is an SDK estimate, not billed cost. Token volume includes
cached input; token and dollar ratios are different quantities. Error results
may still have incurred measurable spend. Legacy totals cannot reconstruct
missing cache components or prove historical completeness.

Every supplied receipt remains an attempt, including failures. Each metric
reports measured/missing counts and its observed total; complete-cohort means
are null unless all attempts were measured. Cost per success includes all attempt
spend, including failures, and is null for missing spend or zero successes.
An unknown measurement must never make an incomplete trial look free.

## Quality, compatibility and comparisons

Report schema v3 distinguishes reported historical success from verified success
that satisfies the runtime protocol. It preserves compatibility fields such as
`successful_runs`, `fix_rate` and `escape_rate` as reported observations. The
headline uses `tokens_per_verified_success`; verified quality and success counts,
verified escape rates and paired-quality coverage are separate fields.
Unscored failures are not assigned invented escape percentages. Escape means
always disclose their measured-run denominator. A Wilson interval for reported
success is descriptive; it does not account for task clustering or prove an arm
comparison. Agent, scoring and combined evaluation time remain separate metrics.

Pairs require the same cohort, requested model, turn cap, task and replicate,
plus agreement of recorded task/protocol digests and mutant denominators. Legacy
files without explicit cohort IDs remain separate file-hash cohorts; missing
identity is disclosed, not inferred. Duplicate attempts, malformed identities,
unsupported receipt versions and mixed reported RIPR versions fail explicitly.

Unmatched arm/control keys and subject mismatches remain visible. Headline token
ratios require complete pairing, complete measured usage and a nonzero control
denominator. They compare paired sums, not averages of per-run ratios. Verified
quality deltas require verified evidence for the whole paired population.
`comparison_status` exposes missing usage, partial pairing and unverified quality.
Even fully matched observations are descriptive, not a significance test or an
automatic efficiency win. Lower token use with worse quality is a tradeoff.

## Qualification and remaining experiment

The selftest runs the real loader, scorer, report generation and process controls,
using boundary stubs only where a native process is not needed. Six named Cargo
controls establish the stock-libtest and workspace behavior; the Rust integration
test requires all six and zero skips. Ordinary tests invoke no model or network.

The collection/reference tools and retained datasets from #13071 build on this
core; this safe prefix does not claim that those tools or cohorts have landed.
Model/provider/executable pinning, harder representative tasks, planned-attempt
accounting, randomized order, a second provider and a fresh qualified experiment
remain separate acceptance. Historical receipts are not rewritten or upgraded by
reading them. Passing instrument tests does not establish that RIPR saves tokens.
