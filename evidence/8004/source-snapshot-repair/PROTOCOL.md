# Bounded source-snapshot repair comparison

Baseline is exact a5c69b1cd2ef73d47a11163bca0432f9fe285c3d. The repaired
candidate will be frozen before measurement. Both use locked Rust 1.95.0
default-feature release builds with the unchanged Cargo release profile.
No builds or native setup runs overlap timing. Retain both actual executables,
build logs, source/tree identities, binary digests, machine and memory receipts.

Reuse the immutable performance-a5 workload bytes and original bench.rs driver.
Here its `base` label means a5 and `candidate` means the repair. Nine alternating
AB/BA pairs each for fixture-mix-16, unique-16-single and direct-16-single,
both full/compact and analyzer-cache-cold/hit-cache paths, anchor the recovery
decision. Cold is not OS-cache-cold. Each process keeps the original 120-second
bound, four Rayon threads, latency traces, dedicated per-variant caches, and raw
stdout/stderr. Stop a batch on failure; preserve censored and missing outcomes.

Replay the remaining retained workloads as three descriptive repaired-only
cold/warm pairs in both modes, including previously timed-out single-file 64
cases. Compare complete normalized repaired output against a5 for every
completed a5 workload/mode, supplementing with the same-machine primary pairs.
The prior censored a5 cases remain censored, not invented complete comparisons.
Retain prior failures, native truth, unsupported boundaries and complete corpus
rows. Normalize only the top-level artifact; inspect any other differences.

Report medians/MAD/ranges and paired repair/a5 ratios with deterministic paired
bootstrap intervals (20,000 samples, seed 8004). Retain absolute wall/CPU/RSS and
warm cache-hit observations. Use the original 1.10 and 20 ms cold / 5 ms warm
materiality screen; small descriptive samples cannot establish no regression.
Material improvement with semantic parity establishes bounded recovery only.
It does not establish full qualification, current-main/hosted/Windows clearance,
Ready or merge acceptance. Any residual measured regression remains explicit.

Correctness replay uses all 63 immutable independent-review stimuli through
full/fresh-compact/diff consumers and retained native original/mutant controls.
Focused source-snapshot/identity/invalidation tests and affected owner/scope
suites supplement that replay. Actual check-fast/precommit, selector integrity,
golden/honesty and dogfood observations remain separate evidence.

## Prospective integration metadata clarification (before timing)

The conflict-required main integration includes #7063's unrelated error-variant
repair and classified-cache generation bump: full 1.78 -> 1.79, compact/sharded
0.84 -> 0.85. Complete normalized reports (artifact removed only) must preserve
and report any resulting source_subject.corpus.cache_schema_version difference.
For the separate semantic parity projection, permit only that exact generation
metadata transition; require file digests/content hash, all remaining report
fields, full seam/relationship payloads and metrics to agree. Any additional
cross-revision difference must be inspected and reported, never silently removed.
The diff replay also retains its explicit input-root difference separately from
finding/decision parity. These are public provenance fields, not row semantics.
The paired result measures the integrated repair against a5; it does not isolate
all runtime effects of main from the source-snapshot delta.

## Prospective storage preservation amendment (before first timing)

After the final workload invocation in a completed batch, losslessly xz-pack
that batch's stdout/stderr and derived cache files, one file at a time. Restore
each into a separate staging file and compare complete bytes, mode and mtime
before releasing its expanded copy. All cold/warm processes finish first; no
packing overlaps any timing sample, and cache inputs are untouched between a
cold invocation and its warm partner. Retain both raw/compressed hashes and
lengths. The analysis reader restores exact sample bytes through xz when an
expanded file is absent; all statistics and parity rules remain unchanged.
This controls storage while preserving the 8 GiB reserve. The earlier final
release-build estimate overran it, was paused, and is separately retained in
the storage incident receipts; it is not a successful admission claim.
