# Cargo preparation measurements: 2026-10-10

This reproducible local experiment informs [CI cost issue #5090](https://github.com/EffortlessMetrics/ripr-swarm/issues/5090). It measures development-profile preparation of the current `xtask` driver. It does not qualify a cloud configuration or replace the issue's acceptance history. Exact observations and the edit recipe are in the [measurement data](cargo-cloud-preparation-2026-10-10.json).

## Inputs and admission

Source: `331176f6cf6405ba9688b7a5dbca6654453eaad1`, Rust/Cargo 1.95.0, Linux, four CPU cores of cgroup quota, 16 GiB memory limit, two Cargo jobs. Each configuration used a separate fresh target directory and the same existing offline registry. All compiled 289 Cargo units with the default Rust, TypeScript and Python language features. Cold refers to target state, not network or registry installation.

The admission reserved 12 GiB free disk, bounded additional storage to 12 GiB, and capped each command at 600 seconds. Existing shared debug output was approximately 9.0 GiB, including 5.8 GiB incremental state; it was left intact. Experiments were serialized after an overlapping sample was excluded. Owned expanded experimental caches were removed only after the frozen drivers and timing reports were archived and restoration verified, reclaiming 9,092,276,224 bytes.

The checked-in dev profile used `debug = "line-tables-only"`. Release used fat LTO, one codegen unit and stripping. Neither profile, release artifacts, dependencies nor product source was changed. Variants were process-local environment overrides:

| Variant | Incremental | Codegen units |
| --- | --- | --- |
| baseline | Cargo dev default, enabled | Cargo default |
| lean | `CARGO_INCREMENTAL=0` | Cargo default |
| lean256 | `CARGO_INCREMENTAL=0` | `CARGO_PROFILE_DEV_CODEGEN_UNITS=256` |

## Observations

All nine build commands exited zero. Disk values below are decimal GB and reflect target directory apparent bytes (`du -sb`), not compressed image size. RSS is the largest waited child's high-water mark, not aggregate process-tree peak. Warm RSS/CPU counters are omitted because inherited helper usage could contaminate them.

| Variant | Condition | Wall seconds | Target GB | Max child RSS GiB |
| --- | --- | ---: | ---: | ---: |
| baseline | cold | 244.065 | 3.163 | 3.597 |
| lean | cold | 213.296 | 1.563 | 3.602 |
| lean256 | cold | 223.082 | 1.592 | 3.520 |
| baseline | unchanged warm | 0.261 | 3.163 | — |
| lean | unchanged warm | 0.261 | 1.563 | — |
| lean256 | unchanged warm | 0.260 | 1.592 | — |
| baseline | comment-only edit | 11.767 | 3.579 | 1.685 |
| lean | comment-only edit | 59.444 | 1.563 | 2.620 |
| lean256 | comment-only edit | 65.020 | 1.592 | 2.502 |

Disabling incremental saved 30.769 seconds (12.6%) and 50.6% target bytes in this cold cohort, but added 47.677 seconds to a single comment-only rebuild. One such edit erases the measured cold saving. Unchanged warm runs were fresh and practically identical. Explicit 256 codegen units was slower than lean with default units. These observations do not justify a global incremental-off setting.

Baseline and lean shared 219 matching third-party library fingerprints; the local `ripr` fingerprint differed. Explicit codegen units changed dependency fingerprints. Compiler traces retained line tables and omitted the incremental flag only in the disabled variants. Setup and start should use identical effective toolchain, target, features, profile, flags and private target path; changing incremental mode can rebuild workspace units even when dependency fingerprints match. Cache keys must distinguish configurations that change emitted compiler flags. Nextest's `--profile ci` selects its runner configuration, not a Cargo build profile.

The baseline library and driver units took 100.28 and 72.37 seconds respectively, about 71% of cold wall time. The separately owned [policy-driver extraction #7158](https://github.com/EffortlessMetrics/ripr-swarm/issues/7158) addresses this preparation graph. That is a reason to measure the extracted graph, not an estimate of whole-job savings.

## Reproduction

Use a clean disposable checkout of the exact source SHA and an admitted private storage root. Select the existing 1.95.0 toolchain and registry without installing software. Run the three configurations sequentially with distinct target directories, two jobs, a 600-second watchdog and a 12-GiB reserve. Abort rather than consume that reserve. Capture command exit status, monotonic elapsed wall time, child resource usage and `du -sb` target bytes after each command. Preserve each cold timing HTML and driver before the edit.

```bash
export CARGO_BUILD_JOBS=2
# Set bench_root to an admitted, task-owned directory before these commands.
CARGO_TARGET_DIR="$bench_root/baseline" cargo build --locked --offline -p xtask --timings
CARGO_TARGET_DIR="$bench_root/lean" CARGO_INCREMENTAL=0 cargo build --locked --offline -p xtask --timings -vv
CARGO_TARGET_DIR="$bench_root/lean256" CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_CODEGEN_UNITS=256 cargo build --locked --offline -p xtask --timings -vv
```

Repeat each command with `--timings` omitted and `-vv` enabled to measure unchanged warm runs. Then append exactly `\n// RIPR build-preparation benchmark: harmless comment-only edit.\n` to `xtask/src/main.rs`, repeat all three warm commands, and restore the original bytes in a finally/trap handler. The adjacent JSON records both source SHA-256 values. The original and edited executable hashes were identical within each configuration.

For meaningful checker parity, run each frozen driver with `check-workflows` and `check-agent-skills`: six positive cases exited zero over 31 workflow files and seven canonical skills. Temporarily change the `prepare-proof` skill's frontmatter name to `invalid-proof`; all three frozen drivers returned exit one and reported invalid frontmatter for both Codex and Zcode skill paths. Restore exact skill bytes and require a final clean check. The initial negative harness inspected stdout rather than the generated report; that instrumentation error was corrected before recording the nine passing controls.

## Limits and next acceptance

These are single runs, with baseline polling resolution up to two seconds and later resolution up to 0.05 seconds. The edit is deliberately bounded and does not represent a product-body edit. No full test-feature/all-target/release/package matrix, actual cloud snapshot transfer or new-session resume was measured. No live setup/start definition was changed. The intentionally detached experiment failed worktree readiness. `check-fast` was stopped after its format/static-language/catalog/no-panic stages because its selector included 18 inherited paths; that partial execution is not an aggregate pass. A 44.753-second overlapping lean attempt was excluded and repeated with a fresh target.

Before selecting an environment default, measure actual snapshot bytes, transfer time, resume time and a representative iterative writer workload on the same cloud budget. Distinguish one-shot/read-only preparation from iterative writers and preserve release proof and cache identities. Compare the extracted policy driver under the same conditions when its owner completes it. Keep existing codegen defaults and release configuration until separate discriminating runtime/output qualification supports a change; [release profile issue #6661](https://github.com/EffortlessMetrics/ripr-swarm/issues/6661) owns that question. After integration and required local checks are green and review-ready, perform the required hosted qualification once; this experiment did not launch hosted CI.

The task-local frozen-driver/timing archive was 211,727,176 bytes with SHA-256 `14b369427fb67859e9c3dc3b70209e9bfb30b082122dbcd809c1394df04e2f62`. Its restore was verified before expanded caches were removed. It remains a local supporting witness, not an uploaded binary receipt; this note and its compact JSON preserve the durable reproducible measurements.
