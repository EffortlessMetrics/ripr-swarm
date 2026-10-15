# Frozen performance protocol

Subject: EffortlessMetrics/ripr-swarm Draft PR #8004; candidate
`a5c69b1cd2ef73d47a11163bca0432f9fe285c3d`, tree
`cc498e204e08facb92cab0d1dad36b36bddd428d`; frozen published base
`98c8f172bfd6aa2b808f63431ed4eac9c224c102`.

Written before the first timing. This is comparison evidence, not correctness,
hosted, current-main, Windows, Ready or merge qualification. No issue #8005 read.
No candidate edits, instrumentation or competing repair.

## Build and machine

Rust 1.95.0, locked default features, `cargo build --locked --release -p ripr
--bin ripr --target-dir /workspace/perf8004/target-base -j 4` in each detached
checkout sequentially. Copy the base executable before rebuilding candidate.
Release manifest: opt-level=3 (Cargo default), lto=true, codegen-units=1,
strip=true, debug-assertions=false, overflow-checks=false, panic=unwind;
default features lang-rust/lang-typescript/lang-python. No RUSTFLAGS override.
Toolchain lives under `/workspace/perf8004/toolchain`; build logs, binary digests,
source/tree identities, machine and cgroup resources retained separately.

## Workloads

`bench.rs generate /workspace/perf8004` generates three families at 4/16/64
independent inline modules, plus split-file 64-module versions. Sync chains use
the retained named synchronous three-helper shape; async chains call and drop
an unpolled async entry. Each chain has three helpers and an inherent receiver
method. Repeated `as_object` names model common accessors and exercise candidate
fanout and identity filtering. The `direct` family has no receiver or helper
chain and is the unchanged-semantics control. Each module has one test.

`fixture-mix-16` wraps the exact candidate async-function input and all 15
controls in distinct modules, preserving each case's local source and semantics.
Origins and byte digests retained. This is a representative bounded workload,
not a whole-repository scan or universal workload distribution.

Native `rustc --edition=2024 --test` compile and test execution are workload
setup controls only. They do not duplicate the independent correctness review.
All inputs must be nonempty and analyzer output must enumerate expected seams.

## Execution

Run nine rounds per workload and each public production mode:
`ripr check --root <same absolute workload path> --format repo-exposure-json`
(full), and `--format repo-exposure-summary-json` (compact, no gap ledger).
Even rounds run base then candidate; odd rounds reverse. Each variant gets
a unique empty `RIPR_CACHE_DIR` per workload/mode/round, then runs cold followed
by warm using that same cache. Never share a cache between variants.
`RIPR_REPO_EXPOSURE_LATENCY_TRACE=1`, `RAYON_NUM_THREADS=4`. All other analyzer
limits use unchanged defaults; subjects are below the default 10,000 seam cap.
Warm cache status must be verified from traces, not inferred from order.
Cold means analyzer-cache-cold, not OS-page-cache-cold. There is no cache purge
or system tuning. Build/setup activity finishes before timing starts.

Each invocation runs under `timeout 120s`; its overhead is included equally.
Wall elapsed time comes from Rust's monotonic Instant around process execution
and file-backed stdout/stderr. Retain every raw report, stderr, exit status,
stdout byte count and sample. Stop and retain setup/process/timeout failures;
revise protocol explicitly rather than erasing failed measurements.

## Analysis and bounded decision

Report medians, MAD, min/max, paired candidate/base ratios and deterministic
paired bootstrap 95% intervals for their median (20,000 resamples, seed 8004).
Nine samples limit precision; confidence intervals do not remove machine noise.
A prespecified material cold regression requires a paired-ratio interval wholly
above 1.10 and median absolute overhead above 20ms. Warm materiality uses 1.10
and 5ms. State actual effects and scaling even below these decision cutoffs.
Scaling comparisons keep family and layout explicit; split-versus-single is
a workload control, not a source-code optimization.

Check full/compact counts and within-variant cold/warm normalized output parity;
record cross-revision semantic differences. Base may over-credit dormant/async
or wrong-owner helper paths. Candidate's withholding is intended behavior and
can change classifications, relation payloads and canonical actionable counts.
A speedup accompanied by reduced/different analysis output is not a timing-only
win. Conversely equal top-level counts alone do not prove equal evidence.
Disclose normalization fields and preserve unnormalized reports.

Recommendation: accept only the bounded performance concern if representative
controls show no material regression with completed work and retained semantic
boundaries. Otherwise recommend repair or narrower follow-up, naming the
measured cases, absolute impact and unmeasured limits. No implementation in this
lane. Publish evidence on its own non-protected branch and comment on #8004.

Pre-measurement correction: source inspection at `output/render.rs:185` showed
that `repo-exposure-summary-json` reaches the compact classified producer.
`repo-badge-json` reaches the full producer for canonical actionable parity
(`ripr_repo_canonical_actionable_summary`, lines 529ff), so it would not measure
compact analysis. The protocol and driver were corrected before any runs.
