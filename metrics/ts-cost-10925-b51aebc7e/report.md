# #10925 frozen-candidate finding

**Retain the current implementation. No worthwhile scoped repair was demonstrated.** The proposed next `named_limitations_*` slice costs only about **1.4 ms total** on the warm representative workload. A new per-owner cache or retained candidate list is not warranted by this evidence. No product patch, central-ref change, CI request or release gate was introduced.

Source: `b51aebc7e4735529befd677ebf14071c7f09e477`, tree `d3142dd59d4dcfea5e6b4981732f61c8516f3fa0`. Subject: zod `680d19f7d1e042e55934209caeb79bc843b94f44`, base `40a5dba080b065cc430e5b0e64a8058d68c1660d` (2 files, 23 changed lines). Linux x86_64 AMD EPYC 9V74, 4-vCPU cgroup, 16 GiB RAM; Rust 1.95.0; repository release profile/default features, locked dependencies.

## Exact baseline observations

Three repeats per row, serialized without a concurrent compiler. Application-cache cold is not OS-page-cache cold. Command: `RIPR_CACHE_DIR=<isolated-cache> RAYON_NUM_THREADS=4 <absolute-binary> check --base HEAD~1 --format json --mode draft`. Each run had a 45-second process-tree limit; all completed with exit 0, no timeout.

| Path | Wall median (s) | CPU median (s) | Maximum RSS (KiB) |
| --- | ---: | ---: | ---: |
| Cold | 1.999 | 1.300 | 69,516 |
| First unchanged warm | 0.410 | 0.354 | 71,044 |
| Second unchanged warm | 0.432 | 0.368 | 70,896 |
| Source-comment edit | 0.469 | 0.396 | 71,180 |
| Related-test-comment edit | 0.481 | 0.399 | 70,688 |
| Explicit TypeScript config | 0.471 | 0.395 | 71,716 |

These are resource observations, **not a speedup claim** against the historical 2.6-second Xeon measurements or proof of that runner's 2-second target. Zod's six findings are all `no_static_path`, with no related-test rows; that abstention boundary is retained. Cold/warm/profiling JSON is byte-identical, SHA-256 `0560b971b1c6571ebc92b9f0e5843b1ca6453236e67e57a6383c524d1cd163eb`, complete analysis, 6 probes/6 findings, no outcome limitations. It does not establish useful reach quality.

Edited-state rows contain one actual transition followed by two same-content repeats; they are not three independent edit transitions. The source edit changes a disk-dependent file already recomputed on every run. These controls establish bounded edited-state behavior, not exhaustive invalidation coverage.

## Profile and work discriminator

A separately identified disposable instrument adds timed spans; it is not a repair candidate. Three cold profiles and six unchanged warm profiles retained identical output. Timings below are inclusive and overlap.

| Consumer | Cold median | Warm median | Calls cold → warm |
| --- | ---: | ---: | ---: |
| Test extraction | 824.7 ms | 0 ms | 208 → 0 |
| Module export facts | 353.1 ms | 23.9 ms | 522 → 21 |
| Enum-member code | 273.1 ms | 10.5 ms | 730 → 21 |
| Fact-store load | 3.3 ms | 79.2 ms | 522 → 522 |
| Owner-line context | 156.4 ms | 165.2 ms | 1 → 1 |

Warm limitation medians: unresolved ownership 0.864 ms; spy fabrication 0.406 ms; relative-import disclosure 0.151 ms across six calls. The already-landed #11949 single owner-context computation is still material; reimplementing its removed duplicate scan would be a rival repair.

Scaling controls increase unrelated tests 1 → 64 → 512, retaining one real related test and one `weakly_exposed` finding throughout. Total extracted tests are 2/65/513. Warm relative-import disclosure grows 0.00042 → 0.00307 → 0.01981 ms, while owner context stays about 0.06–0.07 ms. This confirms the remaining linear scan but not a material optimization opportunity at these sizes. Controls are small, synthetic, and one run per cold/warm state, not a statistically qualified scaling benchmark.

## Correctness and currentness evidence

- Every zod cold/warm instrumented output equals the exact baseline bytes. Source-comment, related-test-comment and config edits retain identical findings; restoration restores exact baseline output.
- An independent semantic invalidation control changes `expect(price(200)).toBe(180)` to `expect(price(100)).toBe(100)` on a `>= 100` → `> 100` diff. The same one finding becomes `exposed` in three warm runs; restoring test bytes restores the original `weakly_exposed` output. A stale extraction cannot pass this control.
- A checksum-corrupted test envelope is ignored: verbose receipt says 500 reused, 22 recomputed, 1 stored, 1 corrupt entry ignored. Output and original envelope bytes are restored exactly. Integrity/currentness rules were not changed.
- Zod warm receipts report 501 reused/21 recomputed/0 stored. Persisted extraction contains 208 test-file entries and 2,680 tests. The 21 recomputed files include disk-dependent re-export facts deliberately excluded by `export_facts_are_disk_independent`; persisting those unchanged would violate currentness.
- Latest #10925/#11676 continuations and all-state finding-specific PR searches were inspected. #11320 concerns Rust masking; TypeScript extraction persistence, alias memoization, one owner scan, shared facts and #13300 adjacency work already exist in the frozen source.

## Identities, artifacts and next action

Baseline executable SHA-256: `ca2cedb6555259a2eaffb8864062de4cd990a3a01f48118e7aa13fab5298352f`; observed version `ripr 0.11.0 (b51aebc7e4735529befd677ebf14071c7f09e477)`. Profiling executable SHA-256: `57b3aac1bbc1b1fc762ca8b782b77392e0df42af888af2cae12b524bcf1411a7`, observed version explicitly `-dirty`; profiling patch SHA-256 `7dc40880f2bb05bd3f10115a532bc62db1cf4dd5e6c031533272c94716bdea04`. Both are source-built artifacts, not installed distribution qualification.

The raw archive contains all 82 measured invocations, stdout/stderr, native build logs, commands, identity/admission records, profiling patch, correctness assertions and exact synthetic Git bundles. Preliminary `baseline-*` runs overlapped compilation and are explicitly excluded from isolated timing summaries. `summary.json` and `correctness.json` expose the denominators and assertions; `validate_controls.py` reruns the retained-output checks.

**Next action for the performance owner:** profile inside the existing single owner-context relation/fallback path (about 165 ms here), using a namespace/module-entry discriminator with retained useful reach, before selecting a new repair. Fact-store decoding/integrity is the other measured warm bucket (~79 ms); preserve its semantic digest contract. Neither observation authorizes a new cache or broad release gate. #10925 remains open; no issue closure, release readiness or historical-runner comparison is claimed. Public #1889 draft/DO NOT MERGE and #1893 trust hold remain intact.
