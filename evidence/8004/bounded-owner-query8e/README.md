# Bounded remaining owner-query profile — frozen8e / Draft8004

**PROFILING_COMPLETE; no optimization implemented.** Next target: reuse fully scoped, owner-independent `free_helper_call_resolves(caller, helper, index)` results, including false answers, across the repeated production-owner searches in `inherent_helper_path_reaches_owner`. Existing `(test, production owner)` reach memoization already exists; adding that again misses the measured duplication. Keep the DFS depth/visited semantics and every refusal unchanged.

At frozen source8e, sync64-single makes **1,292,448 helper queries but only20,352 complete caller/helper requests**: **1,272,096 repeat checks (98.425%)**, up to64 total queries per request (63 repeats). These are actual complete-key counts in one immutable index, not hash/length fingerprints or asserted cache hits. This establishes a reuse opportunity; no cache, speedup or correctness qualification of a proposed cache is claimed. Inherent receiver queries are only192, each distinct; receiver-scope work is64 calls /0.000190s inclusive. Further fragment parsing repair is not the primary target.

## Subject and currentness

Requested review [5543295375](https://github.com/EffortlessMetrics/ripr-swarm/pull/8004#pullrequestreview-5543295375) and requested packetf975 were read first. Independently restored and verified all2169 independent-review packet members and8985 author packet members: checksums, complete bytes/sizes/modes/inventory. No original timeout/censoring result is erased.

Executed source **8e0444b1a62ead18584d40b18278c6e8716dc56c**, tree **da4222d49d37aa82662003e0b6ef4ca8715dbac3**, basisf2ba9522267e88dfb03e3006e83129dfd2a8bb72. Independently built clean release exactly reproduces retained binary **9a4674f89a81c5ff153e1422946e1425cf1491cc578f5e478aa43a420b3c3d84**. Instrumentation-only local commit **dbea701084ed6c6ded6cf35dfed3bb0f0e065af4**, tree **c055237af4cbd0cc570f9d62baaebbdafa241b97**, binary **599e117aee42e63a460c141a74a5639502514d64a60825fe09ec2568bbd09d28**; full seven-file patch is retained. No source optimization, candidate ref write or experiment-source push.

The existing owner advanced live #8004 to **36dbb87430ef858aa143b5fc3205b376b0caee11**, tree024d04e46de6ac531ca7485d9b7b6b4e7dafc2e4, by merging landed maind08c77d… into8e. It remains open Draft. Five complete measured-path blobs are identical; six selected `owner_pin` function bodies are byte-identical. That file gains196 additive lines for element-field support/fields, while Cargo, classifier and cache schemas also change. Raw path inventory/diff and exact blob/function hashes are in `logs/source-currentness.json`. The target survives **source inspection**; all executions/timings remain **frozen8e**, with no new-head runtime/integration qualification. No competing edit or duplicate dependency repair.

## Bounded observations

Rust1.95.0, locked/default features, unchanged release opt3/LTO/codegen-units1, four-CPU quota/16GiB Linux, Rayon4, jobs2. Two sequential builds; no build/setup/profile overlap with measured processes. Four compact clean/profile cold pairs, two immediate warm profile controls, one separate cardinality pass, and two small full lexical-control reports: **13 successful analyzer processes**, zero timeouts or output-control failures,120s cap each. No predecessor, full workload mode, fixture mix, or full benchmark matrix. n=1 observations are descriptive; cold means analyzer-cache-cold, not OS-cold. Instrumented elapsed time is not a clean release benchmark.

|Workload|Clean compact wall n=1|Helper queries|Inherent queries|Outer AST visits|Fragment misses|
|---|---:|---:|---:|---:|---:|
|sync-16-single|0.164s|19,368|48|813,200|6|
|sync-16-split|0.065s|768|48|17,120|5|
|sync-64-single|9.041s|1,292,448|192|55,043,648|6|
|sync-64-split|1.920s|12,288|192|252,800|5|

Growing16→64 units increases helper queries66.731× single-file versus16× split. At64, single/split query count ratio is105.180; it does not imply a105× speedup. Every report is completely equal to retained8e after only top-level artifact removal and exact workload-root rebasing. Inventories are112/448 seams. Ordered split-unit bytes equal single-file bytes; the physical module layout differs, so reports are checked against their own retained layout rather than asserted cross-layout equal.

Measured nested spans (inclusive, overlapping; never sum or treat as exclusive wall percentages):

- Single64 helper queries:7.885430s; executable queries7.671344s /1,292,640 calls;55,043,648 outer AST visits. Macro binding260,128 calls /2.963578s; same-file helper binding780,384 /1.034667s; operand admission260,128 /1.466592s. Six fragment parse misses take0.000075s;1,808,800 fragment lookups take0.083538s.
- Split64 helper queries:1.548272s /12,288 calls. Macro binding4096 /1.386663s includes source-provenance work4096 /0.984161s, itself including **4096 parent-source `owner_pin_assertions` builds /0.973635s**. This is the distinct split-layout secondary target: share immutable source/trusted-policy facts while keeping invocation anchors and current binding decisions. Simply optimizing receiver AST predicates misses it.
- Single64 shadow misses:12,288 /0.804249s, including complete-source masking12,288 /0.597880s. Split has the same miss count but masking0.010723s. Sharing immutable masking/declaration facts is a later bounded target; weakening shadow answer keys is not justified. No selected shadow answer hit is inferred.

## Next implementation boundary and negative controls

Canonical owner: **Rook**, existing #8004 / #7415. No implementation request is executed here. If the owner selects a follow-up, attach an index/snapshot-scoped result memo to the existing semantic owner for **caller/helper admission**, not to recursive reach and not a body-only syntax memo. Preserve caller file/name/both coordinates/body identity, actual helper symbol/declaration/source, complete lexical/module/macro policy and provenance generation. Other files can alter macro admission; reset on dependent source/provenance changes and clone, not merely when the caller body changes. Keep the async-declaration gate before traversal and existing depth/visited handling. Do not key by helper spelling, body text, or file/start line alone.

Executed negative/alternate controls:

1. Same-index identical-body trusted/shadowed macro twins: clean and profile each retain4positive direct/high edges and0negative direct/helper edges across4owner seams. Four retained native-control results: originals pass both tests; owner mutant fails only trusted positive(exit101), shadowed twin survives(exit0). This rejects a body-only semantic cache oracle.
2. Warm-cache bypass on single64 and split64: complete448-seam report parity, **zero instrumented spans/owner queries**. Counter absence is accompanied by nonempty correct output.
3. Sync16 scaling and split-layout alternate: full output parity, exact source-unit byte controls and seam inventories; selected profile changes neither admission nor output. All13 complete reports and raw counters remain retained.

Required discriminators for a future cache implementation (not newly rerun here): negative edge reuse and removal/red control; caller/helper same-name sibling modules and same-line distinct names/coordinates; positive/negative identical-body macro anchors; unrelated-file macro binding or parent module/provenance change with unchanged caller bytes; same-path sync→async→sync, replacement and clone; owner-module exemptions, aliases/raw names, ambiguous/malformed cases; dormant item/closure/async and unpolled async-function helper refusals; bounded recursion/cycles and remaining-depth/visited distinction. Reuse the existing tests/fixtures and run only affected controls plus compact single/split profiling, rather than restarting the old full matrix. Source-fragment sharing alone must continue to share immutable syntax without semantic permission.

## Replay and handoff

Restore `raw.tar.xz` into a new path; verify every `MANIFEST.json` member's bytes,size,mode and complete inventory. `instrumentation.patch` applies to exact8e and contains no optimization. `env.sh`, `freeze.py`, `instrument.py`, `run.py`, `analyze.py`, `check-inputs.py`, native controls and all raw outputs are retained. Drivers use explicit recorded roots under `/workspace/profile8004`; rebase these explicitly when relocating. To replay, materialize exact8e in an owned checkout, build/freeze clean, apply the retained instrumentation patch (or run the driver), build/freeze profile, then run `run.py` and `analyze.py` using fresh cache destinations. Preserve source identity separately when rebuilding instrumentation; a different local commit changes its version stamp/artifact bytes. Compiler executables are excluded from this compact packet; exact hashes, compiler-artifact JSON and build-script stamp rows are retained. Native tests and instrumented executions are not hosted or new-head qualification.

Own1.4GiB workspace/build/source/binary witnesses are retained for owner replay, with29GiB free observed against8GiB reserve and12GiB conservative extra budget. Shared checkoutb772… stays clean/unchanged; inherited19GiB target is untouched. Setup failures and corrected verifier invocation remain in `logs/setup-failures.json`. No remaining execution blocker for this bounded profiling task. New-head review/qualification remains parent-owned and separate.

No issue8005 read/retry, source optimization or submitted candidate push, Ready/merge/thread resolution, bot contact, release/deploy/spending, credential/security changes, destructive cleanup or workflow weakening.
