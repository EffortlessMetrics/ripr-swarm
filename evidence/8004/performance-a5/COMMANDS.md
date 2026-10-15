# Executed command routes and replay

All paths below are this task's local paths. The frozen workloads are also
committed under this evidence packet, so replay does not require regenerating
fixture expectations or retrieving issue #8005.

## Source and toolchain

```bash
git -C /workspace/ripr-swarm fetch origin a5c69b1cd2ef73d47a11163bca0432f9fe285c3d
git -C /workspace/ripr-swarm worktree add --detach /workspace/perf8004/base 98c8f172bfd6aa2b808f63431ed4eac9c224c102
git -C /workspace/ripr-swarm worktree add --detach /workspace/perf8004/candidate a5c69b1cd2ef73d47a11163bca0432f9fe285c3d
curl --fail --location https://sh.rustup.rs -o /workspace/perf8004/toolchain/rustup-init.sh
CARGO_HOME=/workspace/perf8004/toolchain/cargo RUSTUP_HOME=/workspace/perf8004/toolchain/rustup bash /workspace/perf8004/toolchain/rustup-init.sh -y --no-modify-path --profile minimal --default-toolchain 1.95.0
```

The toolchain install was redirected to `toolchain-install.log`. Neither the
candidate nor base tracked source was edited. `*-identity.txt` and build-input
digests bind the actual checkouts. Existing bootstrap checkout was preserved.

## Drivers and input setup

```bash
/workspace/perf8004/toolchain/rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc --edition=2024 -O /workspace/perf8004/evidence/bench.rs -o /workspace/perf8004/bench-driver
/workspace/perf8004/bench-driver generate /workspace/perf8004
```

`fixture-mix-16/src/lib.rs` wraps each exact file in `fixture-origins.json` as
`mod case_<ordinal> { <original file bytes> }`, in that recorded order. All
frozen workload source and Cargo.toml files are committed; their SHA-256 and
byte lengths are in `workload-manifest.json`. Four uniquely named controls were
added before timing; `workload-manifest-initial.json` retains the first 13.

For every workload, setup ran and retained compiler diagnostics plus test stdout:

```bash
/workspace/perf8004/toolchain/rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc --edition=2024 --test /workspace/perf8004/workloads/<name>/src/lib.rs -o /workspace/perf8004/native/<name>
/workspace/perf8004/native/<name>
```

The setup shell stopped at the first nonzero result. All 17 succeeded (608
native test executions). These are valid-input controls, not new correctness
or mutation-review clearance.

## Product builds

Initial base command, with stderr/stdout redirected to `build-base-release.log`:

```bash
cd /workspace/perf8004/base
CARGO_HOME=/workspace/perf8004/toolchain/cargo RUSTUP_HOME=/workspace/perf8004/toolchain/rustup /workspace/perf8004/toolchain/cargo/bin/cargo build --locked --release -p ripr --bin ripr --target-dir /workspace/perf8004/target-base -j 4
```

After completed base dependencies/library were copied to a separate target:

```bash
cp -a /workspace/perf8004/target-base /workspace/perf8004/target-candidate
BENCH_TARGET_DIR=/workspace/perf8004/target-candidate /workspace/perf8004/bench-driver build /workspace/perf8004 candidate
/workspace/perf8004/bench-driver build /workspace/perf8004 base confirmation
cp /workspace/perf8004/target-base/release/ripr /workspace/perf8004/bin/base
cp /workspace/perf8004/target-candidate/release/ripr /workspace/perf8004/bin/candidate
```

The `build` subcommand records native exit status and elapsed time to files.
The base confirmation is a no-op after the first build finishes. The first
base terminal handle was lost; its log and later successful confirmation
are separate receipts. Candidate has a file-backed exit receipt. Actual rustc
commands are retained as well as manifest profile and lockfile digests.

## Untimed setup probes

Four untimed base probes used unique cache paths prefixed `diagnostic-`,
RAYON_NUM_THREADS=4, timeout 120s, the 4-module unique and 64-module common-name
synchronous inputs, and the two production formats below. They ran during
candidate build to verify output schema, nonempty subjects and parity. They
are in `diagnostics.tar.gz`, outside the timing TSV. No observed probe duration
is admitted to performance results.

## Paired timings

After every build/setup process ends:

```bash
BENCH_RUN_LABEL=pilot /workspace/perf8004/bench-driver measure /workspace/perf8004 1 -4-single
/workspace/perf8004/bench-driver measure /workspace/perf8004 9 <exact-workload-name>
```

The second command is repeated once for each of the 17 workloads; each batch
finishes before analyzing it or starting another. No overlapping timing jobs.
Within each workload, nine AB/BA rounds each run full and compact, both variants,
and cold/warm. The main matrix therefore has 1,224 processes, 612 paired
observations, 68 groups of nine pairs. Pilot: 32 processes, excluded from main
statistics but retained. An explicit timeout or failure is retained and stops
that batch; it is never a zero-time sample.

Actual child commands are:

```bash
RIPR_CACHE_DIR=<unique-per-variant-mode-round-cache> RIPR_REPO_EXPOSURE_LATENCY_TRACE=1 RAYON_NUM_THREADS=4 timeout 120s /workspace/perf8004/bin/<base-or-candidate> check --root /workspace/perf8004/workloads/<name> --format repo-exposure-json
RIPR_CACHE_DIR=<unique-per-variant-mode-round-cache> RIPR_REPO_EXPOSURE_LATENCY_TRACE=1 RAYON_NUM_THREADS=4 timeout 120s /workspace/perf8004/bin/<base-or-candidate> check --root /workspace/perf8004/workloads/<name> --format repo-exposure-summary-json
```

Each cold process is immediately followed by the same variant/mode warm
process using that cache. Stdout/stderr are file-backed per sample. The driver
appends and flushes one measurements.tsv row only after process exit, recording:
label, workload, mode, round, variant, path, monotonic seconds, native exit
code, stdout bytes, and start UNIX nanoseconds. Rows have no header. Startup,
rendering, tracing, writing reports and timeout wrapper overhead are included.

## Analysis

```bash
/workspace/perf8004/toolchain/rustup/toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/rustc --edition=2024 -O /workspace/perf8004/evidence/analyze.rs --extern serde_json=/workspace/perf8004/target-base/release/deps/libserde_json-9516ec27e64ad406.rlib -L dependency=/workspace/perf8004/target-base/release/deps -o /workspace/perf8004/analyze-driver
/workspace/perf8004/analyze-driver /workspace/perf8004
```

The analyzer reads raw JSON and stderr, not just exit status. It keeps every
semantic output field and array order, removing only the top-level timestamped
artifact envelope for repeatability comparison. It checks within-variant
cold/warm/repeat parity, traces for classified-cache hits/misses, nonzero seam
totals, full report array length, cross-revision inventory identity, rendered
relation counts, full/compact inventory/grip parity, and cross-revision output
differences. Full/compact canonical actionable projections may intentionally
differ; neither equal aggregate counts nor an empty headline denominator proves
equal evidence. Raw reports remain available for inspection.

Statistics and prespecified materiality are in PROTOCOL.md. Analysis runs
between completed workload batches, never alongside timed processes. Raw
archive creation, compression, manifests and publication happen afterward.
