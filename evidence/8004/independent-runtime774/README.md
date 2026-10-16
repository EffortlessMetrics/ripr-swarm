# Independent runtime supplement — Draft #8004

**MATERIAL_RECOVERY_CONFIRMED / RESIDUAL_COST_VERIFIED / REVIEW_INCOMPLETE. Keep Draft.**

Source head `77474e347bbdd8ef2101b7f9865e65f094df44c1`, tree `6aafd27b5ee2971dcfa052bfc911bae74b69a4d8`; integrated basis `f2ba9522267e88dfb03e3006e83129dfd2a8bb72`. This completes the bounded runtime supplement to [the source-only review](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5541396381). It does not provide full merge qualification. No new correctness defect was identified in these controls.

## Observed source-runtime evidence

- Pinned Rust1.95.0, locked/default-feature unchanged release profile (LTO, one codegen unit, stripped), Linux, four-CPU quota/five-CPU affinity,16GiB memory, Rayon4. Both independently built executables exactly reproduce packet digests: candidate `14f33ad70bc5faa922663d8136eeda8ac386db6106bf09cce1b85ab266091e78`; a5 `0aa3bb1e350f866e538f3e195763660b7c4740d1227b38bef2d0369f843be7a4`.
- **884 distinct units and14 public-path integrations pass at literal final head.** This refreshes the predecessor-only observations, including all four source-snapshot sharing/identity/ambiguity/replacement/clone/removal tests.
- **63 originals pass;28 native owner mutants fail and35 survive**, exactly matching retained native truth. The35 native negatives receive no high direct/helper attribution. Four native-positive unsupported shapes intentionally continue withholding it; no accuracy labels were changed.
- **189 candidate CLI reports pass** across full/fresh-compact/diff.189 a5 reports agree in every field after artifact removal, except the expected full cache-schema1.78→1.79 provenance transition.12 additional same-path sync→async→sync reused/fresh-cache reports agree completely after artifact removal.
- Selected workload native controls:176 actual tests pass. The concatenated split module bodies equal the corresponding single-file bytes for both16 and64; derived sync16-split uses retained modules0..15. Manifests retain this correspondence.
- Honesty corpus remains the identical a5 Git subtree `3d4cba61b83bc67620a14c3dc7a516c148801986`. This is preservation, not fresh general accuracy qualification.

## Fresh bounded release repeats

Three alternating AB/BA pairs for fixture/sync16 single/split; three repaired-only repeats for sync64 single/split. Both full/compact and immediate cold/warm partners: **96 successful processes**, zero timeouts, complete nonempty expected inventories, no within-variant output drift or full/compact metric mismatch. All paired output differences are only the declared full cache-schema transition. No build, native setup or profiling overlaps timing. Each process has a120-second bound. Cold is analyzer-cache-cold, not OS-cache-cold. n=3 is descriptive; warm/no-regression/general qualification is not inferred. Individual times, ratios, MAD/ranges, resource/phase traces and raw reports are retained.

| Workload | Mode | a5 cold median s | Repair cold median s |
|---|---|---:|---:|
| fixture-mix-16 | full | 4.127 | 1.266 |
| fixture-mix-16 | compact | 11.717 | 3.530 |
| sync-16-single | full | 7.957 | 0.457 |
| sync-16-single | compact | 21.247 | 1.153 |
| sync-16-split | full | 0.114 | 0.064 |
| sync-16-split | compact | 0.320 | 0.127 |
| sync-64-single | full | not run | 29.588 |
| sync-64-single | compact | not run | 84.703 |
| sync-64-split | full | not run | 1.007 |
| sync-64-split | compact | not run | 2.968 |

Sync64 compact single-file range is81.909–85.050s, median84.703s; split median2.968s. The prior87.728s observation is independently confirmed as a substantial remaining cost, not erased by the recovery.

## Measured residual and next repair

An isolated seven-file instrumentation experiment at tree `9cda8cee2a9137935f6c4571feec4b7b41a243c7`, patch SHA256 `d12f0c9e2adc3d317147f1de60311b2528619a593eeb590517b5a843643b1a81`, uses the same release profile. Its binary is `da42c086c614eea0ef5e2159874b68a3cc002ac53de7b0bae16ffb70b812b685`. All10 cold profile reports equal the unmodified candidate after artifact-only removal. Diagnostic processes have a180-second bound and are separate from benchmark samples.

In sync64 single-file compact (instrumented wall88.261s):

| Candidate site | Calls | Distinct input contents | Inclusive parse duration |
|---|---:|---:|---:|
| `owner_pin.rs:9912`, `test_module_shadows_item` complete-source parse |12,288|1|39.153s|
| `related_tests.rs:3126`, executable caller body |1,292,640|4|11.505s|
| `related_tests.rs:3271`, synthetic assertion operand |516,160|2|3.533s|

The complete-source shadow parse is the largest measured parse site in fixture and sync16/64 single-file profiles. SourceSnapshot declaration/nesting/name-scope first-miss durations are small by comparison; the existing UnitStructFacts workspace-collector counter is zero on these workloads. The new evidence therefore prioritizes `test_module_shadows_item`, beyond the previous caller/operand-only source hypothesis.

**Next bounded repair:** route that shadow authority through the existing exact-source snapshot parse, line index and reusable scope/declaration facts; memoize shadow answers only with complete source/test identity, type-versus-trait/base and owner-scope identity. Preserve owner-module exemptions, aliases/raw identifiers, same-line ambiguity, malformed-source refusal and fallback behavior. Then reuse immutable caller/operand parses with original lexical anchors and owner-dependent resolution. Retain and re-run every async/scope/native/CLI control and the same timing/profile workloads. No such repair was implemented here.

Durations are accumulated inclusive wall spans. Nested parser/query spans overlap, and full mode runs concurrently; totals are not exclusive CPU attribution and must not be summed into wall percentages. Fingerprints group byte contents, not lexical caller identities. Remaining uninstrumented work is not attributed to one query.

## Separate current-main composition

Frozen observed main `20022bbfeb0d29882f97f51dc5f83ead26919dd4` +candidate77474e347 produces clean local tree `20c28e245b806ada153fe49c1c78eab205d421c3`, local two-parent commit `f8553783808d8ffb6db288e8427a301bafe23411`. Its dev/debug0 binary independently passes189 CLI reports. Compared on identical paths with the source release reports,126 are equal after artifact removal and63 full reports differ only in cache-schema1.79→1.80. Literal source unit/integration and performance results above are not relabeled composition runs.

Separately observed GitHub merge object `2be47f8c5e12c17efc2b996d50c7f627f7ee9322` has parents6239039f80a71d6a2dd12b551118fd48e3b6a551/77474e347 and tree `3e82855ea311d9f4eb9925a91c5a090419ac13bc`. It is a different, unexecuted subject; no hosted job checkout is established. PR metadata's base SHA and the main ref are preserved separately in the receipt.

## Recovery and limits

Recovered [packet525d1e7d](https://github.com/EffortlessMetrics/ripr-swarm/tree/525d1e7d489710bb4cabb5b7a1cf2047eaf993ad/evidence/8004/source-snapshot-repair): all five checksums and all5,464 archive member bytes/sizes/modes/inventory pass independently. Archive timestamps retain rounding differences versus the manifest (5,149 members,−226..225ns); no exact-nanosecond claim is made for that archive. Independently decoded all384 retained reports, recomputed published medians and confirmed40 within-variant groups/full-compact metric controls. Prior failures, censored samples and storage history remain in the original packet; none are rewritten as new passes.

All13 returned review threads were read; one already resolved,12 unresolved, none changed here. The residual performance concern remains. Hosted required-check/actual-checkout, native Windows, full-workspace runtime, base98 equivalence and new a5 sync64 scaling ratios remain outside this proof. Keep Draft; no Ready/merge clearance.

Candidate/root source stays untouched; only isolated reviewer instrumentation differs. No candidate source push, Ready, merge, thread resolution, release/deploy, spending, credential/security change, destructive cleanup, bot contact or issue8005 read/retry. All local witnesses remain retained.

## Replay and artifacts

`receipt.json` is the compact typed receipt; `MANIFEST.json` binds every member of `raw.tar.xz`; `SHA256SUMS` binds the public packet. The raw archive includes exact selected inputs, original bench.rs, instrumentation patch, command drivers, compiler/native/unit/integration logs, all CLI/benchmark/profile outputs, statistics and relevant cache states. Executables are retained in the selected executor with hashes/recipes; executable bytes are not distributed by this packet.

The raw `review/PERFORMANCE-PROTOCOL.md` and scripts record actual commands. Recover original packet525d1e7d, create review-owned exact-head/a5 checkouts, use Rust1.95.0 and the unchanged profile; adapt workspace prefixes. Run `cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml` in the same Cargo home before owner-pin tests. Build the retained Rust bench.rs driver, use the verified input bytes, and run the stored three-round commands without overlapping builds. Apply the diagnostic patch only in a separate exact-head checkout; its complete tree/hash and output-parity rules are retained. Original selected-executor absolute paths are evidence locations, not assumed shared filesystem transfers.
