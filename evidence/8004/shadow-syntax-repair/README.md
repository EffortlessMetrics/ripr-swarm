# Draft #8004 immutable shadow/fragment syntax repair

Source `8e0444b1a62ead18584d40b18278c6e8716dc56c`, tree `da4222d49d37aa82662003e0b6ef4ca8715dbac3`; baseline `77474e347bbdd8ef2101b7f9865e65f094df44c1`. Source basis `f2ba9522267e88dfb03e3006e83129dfd2a8bb72` remains distinct from the observed main/composition subjects in `receipt.json`. Exact compiler artifacts, source patches and original failures are in the archive. Keep Draft; fresh full exact-head review remains required.

## Change and retained semantics

`test_module_shadows_item` now shares immutable complete-source parse, line and scope facts and fully keyed shadow answers. The snapshot supplies exact file/source identity; the query retains test name/start/end, base, type-or-trait and full available owner file/name/start. A foreign source uses a standalone snapshot, source replacement invalidates, clones start empty. Owner-module exemption, raw names, aliases, same-line ambiguity and macro-shaped fallback stay conservative. Caller and assertion-operand content shares immutable green parses only; per-call lexical anchors, current owner predicates, async/item/closure/scope refusals still execute.

## Measured retained release inputs

Locked Rust 1.95.0/default features/unchanged opt3,LTO,codegen-units1 profile, Linux four-CPU quota,16GiB, Rayon4. Three alternating paired repetitions, analyzer-cache-cold plus immediate warm partner,120 processes, zero timeouts. No build/setup/profile overlaps timed samples. Cold is not OS-cold. Full medians/MAD/ranges, paired ratios, CPU/RSS process receipts and all raw reports remain in the archive.

| Workload / cold mode |774 median [range] s|8e median [range] s|Median paired8e/774|
|---|---:|---:|---:|
|fixture-mix-16 / full|1.273 [1.145, 1.584]|0.081 [0.075, 0.085]|0.059|
|fixture-mix-16 / compact|3.468 [3.361, 3.560]|0.063 [0.063, 0.065]|0.019|
|sync-16-single / full|0.498 [0.483, 0.545]|0.109 [0.099, 0.110]|0.206|
|sync-16-single / compact|1.237 [1.148, 1.270]|0.147 [0.124, 0.151]|0.116|
|sync-16-split / full|0.076 [0.066, 0.085]|0.048 [0.048, 0.050]|0.628|
|sync-16-split / compact|0.146 [0.146, 0.148]|0.063 [0.054, 0.071]|0.434|
|sync-64-single / full|33.695 [31.991, 34.931]|3.419 [3.352, 3.529]|0.101|
|sync-64-single / compact|88.640 [87.122, 89.065]|9.615 [9.229, 9.762]|0.108|
|sync-64-split / full|1.204 [1.177, 1.358]|0.744 [0.720, 0.829]|0.611|
|sync-64-split / compact|3.608 [3.446, 4.351]|2.094 [2.081, 2.301]|0.580|

All 120 complete reports have zero within-variant, cross-variant or full/compact metric-control failures after only top-level artifact removal. No root rebasing is needed for these same-root benchmarks. Warm results and raw resource counters are in `receipt.json`; these observations are descriptive, not a general no-regression claim.

## Isolated bounded profiling

Experiment `88ee314a0e14ef9a768032d4587ce8d26adadc2e`, tree `1cf613b295cf8883aa6e9a8773d94dc5efd44e75`, instrumentation-only patch against 8e. The composition parents and patch hash are explicit; this is not a main integration. All 10 cold instrumented reports equal the unmodified candidate after only artifact removal. Prior compact sync64 profile counted 12,288 complete-file shadow reparses/39.153s inclusive, caller 1,292,640 reparses/four contents and operand 516,160/two contents. New compact sync64 observations:

- `executable_query` `crates/ripr/src/analysis/classify/related_tests.rs:3126`: 1,292,640 calls, 1 fingerprint groups, 7.512868s inclusive.
- `shadow_answer_miss` `crates/ripr/src/analysis/classify/owner_pin.rs:9924`: 12,288 calls, 1 fingerprint groups, 0.846192s inclusive.
- `fragment_query` `crates/ripr/src/analysis/syntax/source_snapshot.rs:73`: 1,808,800 calls, 6 fingerprint groups, 0.099780s inclusive.
- `parse_clean_source_file` `crates/ripr/src/analysis/syntax/source_snapshot.rs:129`: 1 calls, 1 fingerprint groups, 0.002919s inclusive.
- `shadow_scope_build` `crates/ripr/src/analysis/syntax/source_snapshot.rs:144`: 1 calls, 1 fingerprint groups, 0.000580s inclusive.
- `fragment_miss` `crates/ripr/src/analysis/syntax/source_snapshot.rs:79`: 6 calls, 6 fingerprint groups, 0.000092s inclusive.
- `parse_clean_source_file` `crates/ripr/src/analysis/syntax/source_snapshot.rs:80`: 6 calls, 6 fingerprint groups, 0.000090s inclusive.

All 12,288 shadow-answer entries are distinct misses in this selected compact workload: do not credit shadow-answer hits for its timing recovery. Repeated-query unit/removal controls establish answer reuse separately. Executable queries remain 1,292,640/7.513s inclusive; shared complete-file parse/scope construction occurs once, fragment misses six across 1,808,800 queries. No UnitStructFacts scan is observed in these selected profiles; this does not clear other workspace workloads.

Inclusive spans overlap, including parser wrappers and concurrent full-mode work. Do not sum or interpret them as exclusive wall attribution. Profile unique_inputs fields count hash/length fingerprint groups and can collide; production fragment keys compare complete bytes. Instrumentation overhead is outside benchmark results. Remaining per-query traversal and owner syntax work remains visible in the full profile.

## Correctness, invalidation and review

Exact 8e:888 distinct affected units,14 integrations,189 release CLI reports pass; development and baseline 189 replays also pass complete-report comparison after only top-level artifact removal and exact replay-root rebasing. Native 63 originals pass;28 owner mutants fail/35 survive, one real test each. Retained five workload native inventories total176 real tests. Same-path sync→async→sync source replacement passes12 reused/fresh complete-report controls.

15-gate check-fast and typed precommit pass on173 exact changed paths, zero precommit skips/limitations. All 465 fixtures/1,157 golden surfaces pass with zero drift; complete fixtures tree `9cef3bf61fb43d24b0ac728bb4ca3cff53bb724d` is identical to774. No truth/golden refresh. Honesty 324 pass/0 fail/1 pinned-external not run (explicit opt-in required); advisory dogfood passes. Configured judged-panel departures remain visible, not relabeled zero accuracy errors.

Independent distinct-context source and retained-data audit found no blocking finding. It independently executed eight candidate sharing controls and eight isolated memo-removal controls on author-built frozen binaries; two removal controls fail at intended runtime assertions while candidate twins pass. It did not independently build or rerun888/14/189. Broader review remains incomplete. Removal experiment `13a423c05a5eaba2bc9dc0d1c7d91a10aa50eb39` is isolated, not canonical source.

## Recovery, failures and remaining limits

Both requested immutable packets were recovered and checksum/all-member verified:4,660 plus5,464 files, full bytes/sizes/modes. Prior timestamp-rounding limits remain; no nanosecond-preservation claim. Prior accuracy costs and censored old runs stay in those immutable packets.

- initial local publication staging copied archive before packaging completed; verifier rejected checksum, failed local commit4e815 retained without push; final packet staged only after terminal verification
- baseline worktree doctor behind-main failure retained; no material main integration prerequisite identified
- baseline parallel release build interrupted at memory pressure; serial exact 774 build reproduced retained digest
- initial workload --locked attempt lacked lock; preserved and generated offline lock before native replay
- new use-alias test oracle corrected to existing file_renames_to authority before execution
- 246 test-only green.as_ref API compile failure; fixed8e
- test build begun during test-only commit had246 dirty stamp; deliberately interrupted and clean8e rebuilt

- Three paired descriptive repeats, no general no-regression/full performance qualification, no new a5/base98 equivalence.
- Inclusive profile spans overlap: never sum or claim exclusive wall/CPU attribution. Instrumented elapsed time is not release benchmark time.
- Broader runtime evidence author-executed and independently data-audited; independent reviewer reused author-built frozen binaries for eight controls and memo-removal failures, not an independent build/full rerun.
- Latest-main/hosted actual checkout, native Windows and full workspace runtime qualification remain separate.
- Honesty corpus 324 pass/0 fail/1 pinned-external not run; precommit zero skips is a separate denominator.
- Existing unsupported async, closures/items, lexical/module ambiguities and bounded resolver limits remain. Index-local memo memory follows unique source/fragment/query cardinality; per-query AST walks and short-lived owner syntax scans remain.
- Prior censored runs, accuracy costs, setup/compiler failures and resource interruptions retained, no corpus rebless.
- Open Draft REVIEW_INCOMPLETE; no Ready/merge/thread resolution/release/deploy/bot contact. Issue8005 not read or retried.

## Reproduce and inspect

Recover this immutable commit with Git, run `sha256sum -c SHA256SUMS`, then `python drivers/verify-new-packet.py <packet-directory>` using the archived driver after extraction. `MANIFEST.json` inventories every raw member byte/size/mode. Archive mtime/uid/gid are explicitly normalized to zero; compiled executable witnesses are excluded but exact hashes, compiler receipts, native inventory and build/run drivers are retained. Task-owned executable witnesses and worktrees remain local for review; inherited target storage was not cleaned.

Extract `raw.tar.xz` to a new directory. Review `drivers/release-validation.sh`, `replay.py`, `bench.rs`, `analyze-bench.py`, `profile-validation.sh`, `profile-run.py`, source/profile/removal patches and logs. Drivers deliberately use recorded absolute task roots; adapt roots explicitly rather than silently changing normalization. Build baseline 774 and candidate8e separately with unchanged locked release/default-feature profile; verify compiler artifacts/version/source stamp before using them. Native setup lock failure and corrected offline lock receipts are retained. Do not reuse existing cold-cache roots on rerun.

## Handoff

Owner: Rook / Draft #8004, governing7415. Candidate 8e and all exact subjects above. Dependency: fresh full exact-head independent review; hosted/latest-main, native Windows and full-workspace runtime remain separate. Next action: reviewer consumes immutable packet and current source, evaluates remaining single-file residual and correctness boundaries; resume delivery only within existing Draft scope. No merge or Ready authority is supplied.
