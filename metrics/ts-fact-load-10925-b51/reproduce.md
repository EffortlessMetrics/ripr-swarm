# Reproduce the unselected fact-load experiments

**Disposition: NO_PRODUCT_PATCH_RECOMMENDED.** These source pins and patches support reproduction; neither is selected for central integration. The composed owner-call reorder is outside this comparison.

| Source-built subject | Commit | Tree | Binary SHA256 |
| --- | --- | --- | --- |
| Frozen baseline | b51aebc7e4735529befd677ebf14071c7f09e477 | d3142dd59d4dcfea5e6b4981732f61c8516f3fa0 | ca2cedb6555259a2eaffb8864062de4cd990a3a01f48118e7aa13fab5298352f |
| Smaller load-only experiment | c7e123521d500ffe411d8d9dbafbf61d2404f1a3 | a09f9dc194231a51d344d36f6d14579e58e4f838 | 292bd1f450d0531c5883e4250552ef5de279fba552442ef6b40e80a686e30029 |
| Framing experiment | 9c64ecada5fb4780aa4406eaf71f28b6e6ba4905 | 4acad449937c906afe06ad358bca505115b0e36a | 6916a0fd95819a33e5bd43e8e1d36b7375db4445171b8c8c767b74b906828817 |

Source refs are `archive/perf-10925-fact-byte-c7e123521` and `archive/perf-10925-fact-frame-9c64`. Baseline remains the published frozen successor source. Use exact commits, not current main, this evidence commit, installed artifacts or the newer central candidate. The smaller patch is one standalone commit; framing has three ordered commits from b51. Complete patches/diffs are included.

Build clean subjects serially using the repository release profile, default features, locked dependencies and Rust 1.95.0. Original builds reused one isolated target and worktree path under the recorded 4-vCPU/16 GiB admission; commands were:

```sh
cargo build --locked -p ripr --bin ripr --profile release -j 3
cargo test --locked -p ripr --lib analysis::language::typescript::fact_store -j 2 -- --test-threads=1
cargo clippy --locked -p ripr --lib -j 2 -- -D warnings
```

Preserve each binary separately, record `--version` and SHA256, and assert a clean source before/after. Each build's identity is a cache input; do not port cached analysis between builds. Binaries are retained locally by their original owner and omitted from Git archives.

The raw archives contain controllers, every stdout/stderr, wait4 resources, correctness receipts, source/build/test/lint logs and useful fixture bundles. Internal manifests hash all included files. The pinned zod workload is HEAD 680d19f7d1e042e55934209caeb79bc843b94f44, parent 40a5dba080b065cc430e5b0e64a8058d68c1660d: two changed files, 23 changed lines. Its verified remote is https://github.com/colinhacks/zod.git. `workloads.json` records all retained fixture trees. Bundles recreate namespace-1, namespace-2680 and alias fixtures; adjust absolute `/workspace` paths consistently if replaying elsewhere. Keep a subject root fixed within its pair because root identity is checked.

Every ordinary measured invocation is:

```sh
RIPR_CACHE_DIR=unique-cache RIPR_PROGRESS=1 RAYON_NUM_THREADS=4 \
  /absolute/source-built/ripr check --base HEAD~1 --format json --mode draft
```

`measure.py` execs the child directly, bounds its process group to 45 s, and records wall, wait4 CPU and per-run peak RSS. Wall includes a 5 ms polling interval. Application-cold means an empty private cache; OS pages are not flushed. `paired_runs.py BASELINE CANDIDATE` alternates binary order by trial and runs cold/first-warm/unchanged-warm, N=3, for zod and two useful sizes. It checks complete nonempty output and full stdout equality. Source/config/cache state and analyzed populations are stable.

`edit_loops.py BINARY baseline` and then `... CANDIDATE candidate` each seed a fresh clean state for every independent trial. Production edits change `>100` to `>101`; assertion edits change the tested public call from 200 to 100. Every edited warm state must equal an independent cold same-state run, then restoration must equal clean output. Binary timing is block-ordered here; it is not a randomized crossover. `config_control.py` independently redirects/restores aliases. Its no-static-path states receive no useful-reach gain claim. `compare_edits.py` checks all 45 corresponding states across binaries, including every semantic field and origin.

The smaller `corruption_control.py` changes a valid served field without updating its checksum, requires recomputation and byte-identical repaired cache/output, and verifies subsequent reuse. It then puts an intact older-build body at the current cache filename and requires an identity miss/recomputation. Verbose receipts show 2 reused/1 recomputed for both; only actual corruption reports 1 corrupt ignored. Neither refusal/recompute control counts as a speed gain. Both experiments preserve the helper's static_unknown finding; the public finding becomes exposed under the assertion edit.

Attribution is separate from speed measurement. The b51 instrumentation patch/binary identity and 12 runs are retained in the framing archive: decode, streamed checksum reserialization, reads, reshare and whole-load timers. Its binary SHA256 is 008cf748a0e773fb7e12772bdee70485538c9101ec0dee7b05247158b22a27aa. `stored_body_parity.py CACHE/typescript-file-facts/6` independently checks all 501 old producer digests. The retained red executable is b51 plus test instrumentation/oracles, SHA256 46282ab0dfc6ac5509377b465b3e09dc99a43c48b45ed796c6d85ff28c5a6609; it serves real nonempty facts before failing 1 versus 0 checksum reserializations. `work-counter-linkage.json` proves the smaller oracle is equivalent after identifier/formatting changes.

Framing memory diagnostics are 18 complete map-sampling controls, separate from timing. An initial ancillary controller Path-join error and its partial outputs remain under `memory-attempt1`; none supports a speed claim, and the main 160 records are unaffected. No mapping attribution is transferred to c7. No further variant was run.

The default check-fast merge-base selects 18,272 inherited paths rather than this explicit b51 delta. Scoped proof and base-aware path sets are retained. Clippy's three untouched cmp_owned errors and fragment 12010's missing reference independently reproduce on exact b51. There is no passing aggregate/hosted/installed or central qualification claim. Maintain the 8 GiB disk reserve and bounded parallelism; do not raise a ceiling to pass. Public/release holds remain unchanged.

## Raw archives

| Archive | Bytes | SHA256 |
| --- | ---: | --- |
| [byte-admission](byte-admission/raw-evidence.tgz) | 375596 | 62f0adf122ba1d8970be4f1b1d59b982b69c11d42844819f6a7c751716627960 |
| [framing](framing/raw-evidence.tgz) | 512685 | 36ce54fc1bf15ffdb3615076740aca6bcc7421f2bf26fe802cbbb7ebe2835c6b |

Each archive includes RAW_MANIFEST.json. Validate every included member against it after checking the archive hash. Complete source/binary metadata is also readable in each experiment’s identity.json.
