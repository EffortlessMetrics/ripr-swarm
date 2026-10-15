# Commands and identities

Paths are the original selected-executor paths. Recreate isolated checkouts and
adapt the workspace prefix when replaying elsewhere; do not infer shared files.
The A5 sources were later byte-verified and retained under
`/tmp/repair8004-retained/{a5,red-a5}`. Their original paths and relocation receipt
are retained; benchmark input and executable paths stayed unchanged.

## Build

Both clean source subjects use Rust 1.95.0 and the committed lockfile/profile:

```
cargo build --locked --release -p ripr --bin ripr --target-dir /workspace/repair8004/target-release -j 4
cargo build --locked --release -p ripr --bin ripr --target-dir /workspace/repair8004/target-release -j 2 --message-format=json
```

The first command ran on A5. Its actual binary was frozen before reusing mutable
release build state for the repair. The final command ran on
`77474e347bbdd8ef2101b7f9865e65f094df44c1`. The retained JSON compiler-artifact,
version output and SHA256 bind the chosen executable; intermediate successful
code producers and the paused/resumed final build remain separate receipts.
`CARGO_HOME=/workspace/.cargo`, `RUSTUP_HOME=/workspace/.rustup`, and the matching
Cargo/rustc directory was prepended to PATH for builds. No RUSTFLAGS override.

Actual timing executables:

- `bin/base`: A5, SHA256 `0aa3bb1e350f866e538f3e195763660b7c4740d1227b38bef2d0369f843be7a4`.
- `bin/candidate`: final head, SHA256 `14f33ad70bc5faa922663d8136eeda8ac386db6106bf09cce1b85ab266091e78`.

## Workloads, correctness and timing

All 290 workload files equal the immutable prior performance packet. All 63
stimuli in `evidence/immutable-stimuli.json` equal the independent review packet;
`replay-driver.py` records the executed production-path assertions. Its original
executor paths locate the recovered prior packet under `review-raw`; adapt that
source path to the included immutable stimuli for a fresh replay. Preserve all
native/static truth labels and the case59 extra-cli / cases60-62 operand-cli prior
locations when comparing old reports.

```
python /workspace/repair8004/replay-final.py cli /workspace/repair8004/bin/candidate
/workspace/repair8004/bench-driver measure /workspace/repair8004 9 fixture-mix-16
/workspace/repair8004/bench-driver measure /workspace/repair8004 9 unique-16-single
/workspace/repair8004/bench-driver measure /workspace/repair8004 9 direct-16-single
BENCH_VARIANT=candidate /workspace/repair8004/bench-driver measure /workspace/repair8004 3 <each-other-workload>
python /workspace/repair8004/compress_completed.py <completed-workload>
/workspace/repair8004/analyze-driver /workspace/repair8004
python /workspace/repair8004/verify_final.py
```

The driver's only measurement-route extension is the explicit variant filter for
repaired-only descriptive scaling. It retains timeout120, alternating pair order,
file-backed stdout/stderr, four Rayon threads, unique caches, monotonic wall time,
resource/phase traces and immediate warm partners. The final reader adds exact
xz restoration of archived files; the statistical algorithm is unchanged.

Build the small Rust drivers from the included `bench.rs` and `analyze.rs` with
Rust 1.95.0. `analyze.rs` uses the same locked serde_json rlib as the checked
workspace (`--extern serde_json=... -L dependency=...`). All input manifests,
process TSV rows, raw sample hashes, restored-file hashes, complete outputs,
statistics and resource receipts are included.

## Local checks

Actual commands are retained in logs:

- `cargo xtask worktree doctor`; `cargo check --locked -p ripr --lib -j 2`.
- `cargo test --locked -p ripr --lib source_snapshot -j 2`, plus the five named
  affected library selections in `library-suite-statuses.tsv`.
- `cargo test --locked -p ripr --test private_inherent_receiver --test native_path_roots -j 2`.
- Actual `check-fast` and `precommit` on the final head; invoking the already
  built xtask binary avoids a redundant Cargo wrapper build while preserving
  its own real Cargo Clippy/build subcommands and emitted Git/head inventory.
- `goldens check`, canonical per-fixture `goldens bless <fixture> --reason
  "RIPR-SPEC-0122: ..."`, `check-evidence-promotion-honesty`, `dogfood`.

The golden refresh updates only eleven human Hidden-row path prefixes and
expected-output reasons. Initial drift, final full comparison and actual renderer
identity are retained; source lines, classes and every JSON observation are equal.
The exact source/currentness distinctions are in `validation-currentness.json`.
