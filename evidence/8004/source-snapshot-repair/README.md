# Source-snapshot repair — Draft PR #8004

**CANDIDATE_REPAIRED / MATERIAL_RECOVERY_OBSERVED / REVIEW_INCOMPLETE. Keep Draft.**

Source: `EffortlessMetrics/ripr-swarm`, owned branch `rook/7415-private-target-attribution`.
Published head `77474e347bbdd8ef2101b7f9865e65f094df44c1`, tree `6aafd27b5ee2971dcfa052bfc911bae74b69a4d8`.
Original expected head `a5c69b1cd2ef73d47a11163bca0432f9fe285c3d` was verified before work and immediately before the normal history-preserving push. Remote readback confirms the new head, open and Draft. No other writer advanced it.

## Repair and boundary

`RustIndex` now owns immutable source-snapshot syntax storage. Complete-source declaration answers retain symbol ID, name and both coordinates within the exact file/source snapshot. Unknown remains `None`, distinct from synchronous `Some(false)`. Declaration, function nesting and lexical name-scope queries reuse the source parse and facts; owner-pin file-scope tables are also shared across short-lived syntax owners. Retained source Arcs prevent allocation-address reuse; source replacement selects a fresh slot and index clones start empty. Poisoned memo locks rebuild facts rather than inventing an answer.

The existing entry/intermediate/last-hop async gate, executable-body filtering, lexical ambiguity, cfg/module refusal and owner-return-pin fallback remain. No polling, type/value-flow, Cargo-target resolver or macro expansion is added. Same-line ambiguous definitions stay unknown. The sparse one-line synchronous-chain documentation now distinguishes missing candidate routing from the async declaration gate, retaining named/multiline positives and the existing bridge limitation.

Pure cache repair commit `9c6553ac6`; conflict-required main integration `012d3cd94` includes frozen main `f2ba9522267e88dfb03e3006e83129dfd2a8bb72`. The actual conflict was the classifier export list; both exports were preserved. Final `77474e347` adds only verified Hidden-row path goldens/reasons to the preceding code head. Main's formatter is unchanged. All 140 honesty-corpus blobs, complete rows and metadata equal a5; no truth labels or JSON expectations were changed.

## Paired release comparison

Both actual executables use locked Rust 1.95.0, unchanged default features, optimized release/LTO, codegen-units=1 and stripping. RUSTFLAGS/wrappers are unset. Host quota is four CPUs, affinity five, memory 16 GiB; Rayon is fixed at four. Every process has a 120-second bound, separate variant/cache paths and raw stdout/stderr. Cold means analyzer-cache-cold, not OS-page-cache-cold. No build or setup execution overlaps timing.

Nine alternating AB/BA pairs per primary workload, each full/compact and cold/warm:

| Cold workload | a5 median | Repair median | Median paired repair/a5 ratio [95% paired bootstrap interval] |
|---|---:|---:|---:|
| Retained fixture mix, full | 4.763 s | 1.306 s | 0.279 [0.237, 0.282] |
| Retained fixture mix, compact | 11.406 s | 3.469 s | 0.303 [0.283, 0.311] |
| Unique synchronous helpers, full | 0.131 s | 0.073 s | 0.555 [0.481, 0.648] |
| Unique synchronous helpers, compact | 0.252 s | 0.082 s | 0.330 [0.307, 0.356] |
| Direct calls, full | 0.038 s | 0.013 s | 0.348 [0.316, 0.385] |
| Direct calls, compact | 0.085 s | 0.008 s | 0.096 [0.085, 0.111] |

No primary warm group meets the prespecified regression screen (ratio interval wholly >1.10 plus >5 ms median overhead). This is bounded evidence, not blanket warm clearance. MAD, ranges, individual ratios and 20,000-resample intervals are retained.

All 17 workloads/both modes complete: **384 successful processes**, zero timeouts, zero within-variant normalized mismatches and zero full/compact inventory/grip mismatches. Remaining workloads have three repaired-only descriptive cold/warm repetitions. Previously censored async-64 single-file medians are now 0.400 s full / 0.287 s compact. Synchronous-64 single-file remains expensive: **31.726 s full / 87.728 s compact**, versus split-file 1.128 s / 3.065 s. These are not paired a5 scaling ratios; prior four censored/interrupted a5 workload/mode entries stay censored.

Fixture cold self-observed CPU medians fall from 18.73 s to 5.03 s full, and 12.36 s to 3.71 s compact. Median peak RSS falls from 62,763,008 to 57,425,920 bytes full and 30,302,208 to 28,323,840 bytes compact. All 384 resource receipts are retained, including clock-tick granularity; warm zero CPU ticks do not mean zero work.

## Semantic and correctness evidence

- Exact final release: all 63 immutable independent-review stimuli, **189 full/fresh-compact/diff reports**, pass. Their complete findings, seam/relationship payloads, metrics and decisions agree with a5.
- All **30 completed prior-a5 benchmark workload/mode outputs** agree in semantic projection. Four earlier censored a5 entries have no completed-output equivalence claim.
- Initial normalization removes only top-level artifact. Every remaining difference is retained: full source-subject cache generation 1.78 -> 1.79 from main #7063, and cross-location root metadata/canonical explain-route rebasing. Only those exact provenance transitions are admitted; file digests/content hashes and every other field must match. Same-path paired compact reports are wholly equal after artifact removal.
- Replayed native controls: 63 originals pass; 28 owner mutants fail and 35 survive. Each has one real test and CLI/native input digests match. All 17 benchmark setups also compile/run, totaling 608 native test executions.
- Focused source-snapshot sharing, concurrency, qualifiers, identity collision, ambiguity, malformed source, replacement/clone/removal and scope-refusal controls pass. The owner-lexical sharing regression fails at its intended assertion on isolated a5; its actual executable is preserved as a verified restorable archive.
- **884 distinct relevant units** pass at code predecessor `0c31940f0cb6dae6af01372cbff2626c9861e6e0`; the later test-only join-error diagnostic fix preserves failure propagation/stimuli/assertions and leaves production prefix bytes unchanged. **14 public-path integrations** pass at `757da24c898e9daf34e11caba9330209fa55bfbc`; final changes are only the path goldens/reasons, with all code/build-input blobs identical. These are precise predecessor observations, not relabeled literal final-head unit executions. Final-head workspace/all-targets Clippy compiles the final tests.
- Exact final head: actual check-fast passes all 15 gates; actual typed precommit passes, complete 173-path change set, zero skips/limitations. Git diff, independent tree inventory and typed policy inventory agree.
- All 465 selected/actual golden fixtures and 1,157 surface comparisons pass after the verified path-only refresh. Honesty and nonempty advisory dogfood pass. `validation-currentness.json` binds subjects and unchanged dimensions.

## Recovery, failures and retention

Both prior immutable packets were fully recovered and verified: independent review `4f7f4a150f9ab5bc23a604ff1bcaabd00dff1a53` (2,386 archive members), performance `a31c284a45a7966b54266ba440b32684d1bbb9e0` (1,681 archive members). Their failures and original benchmark method remain authoritative; this packet supplements them.

`FAILURES.md` retains the initial Clippy test-diagnostic failure, root-only golden drift, environment/reader mistakes and storage incident. The final release build exceeded its storage estimate and crossed the 8 GiB reserve, then was paused. Only task-owned artifacts were recoverably archived/relocated with complete byte/mode/nanosecond-mtime verification. Both A5 worktrees remain materialized under `/tmp/repair8004-retained`; their full tracked source and the unique red patch were verified before the old checkout copies were released. Native executables remain in 144 verified archives there (1,401,814,400 original bytes / 234,651,112 archived bytes). No inherited target cleanup or reserve reduction occurred. The resumed final link completed under a sampled storage guard; earlier overrun is not described as successful admission.

Completed benchmark stdout/stderr and derived caches are losslessly xz-packed only after a batch finishes. Every file is fully restored and compared before releasing its expanded copy. `compressed-storage-manifest.json` retains raw and compressed identities. `analyze.rs` reads exact restored bytes. No packing overlaps timing or changes cold/warm cache inputs between partners.

Actual base/final release binaries and historical code witnesses remain retained in the selected executor; this packet contains their identities, source recipes, inputs and observations, not executable-byte release qualification. Task-owned verified native archives, source copies, targets and receipts remain retained for independent consumers. No session archival or destructive cleanup was performed.

## Handoff and non-claims

Independent exact-new-head review follows this author handoff. Performance has materially recovered within these controls, but the synchronous single-file residual remains substantial. Caller-body/assertion parsing and fresh owner-syntax workspace work remain visible; exact parse counts and exclusive attribution to one query are not claimed. No same-machine repair/base98 equivalence or full-repository performance qualification is established.

Hosted/latest-main, native Windows and full-workspace runtime qualification remain separate. Prior accuracy costs and unsupported native-positive polling/nested shapes remain visible; small/local checks are not full qualification. Keep Draft. No Ready, merge, review-thread resolution, release/deploy, spending, credential/security change or bot contact. Issue8005 was not read or retried through another route.
