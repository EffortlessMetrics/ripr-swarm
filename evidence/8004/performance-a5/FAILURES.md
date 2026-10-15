# Retained setup and instrument observations

* The environment initially started without callable shell tools. Startup wait
  completed, then `/workspace/ripr-swarm` was found clean at
  `08507c1ab81c4791c933d19bcb3abb74c22e8f26` (not the benchmark subject).
* Initial `rustc -Vv` and `cargo -V` returned `command not found` (127) under
  the original PATH retained in `environment.txt`. Standard home/opt/local
  locations contained no Rust toolchain. This is a setup failure, not a
  candidate failure. Rustup installed exact 1.95.0 locally under the benchmark
  workspace; install log retained. No credentials or security settings changed.
* `git cat-file -t a5c69b1cd2ef73d47a11163bca0432f9fe285c3d` initially failed:
  `fatal: git cat-file: could not get object info`. Narrow fetch of that exact
  SHA succeeded. The base object was already present. Detached worktrees retain
  both subjects; candidate tree matches the requested hash.
* Initial path lookups for `xtask/src/reports/repo_exposure*`,
  `crates/ripr/src/cli/check*`, `analysis/repo_seam_cache*`, and a fixture-root
  Cargo.toml missed. Actual owners were found with `rg`: `xtask/src/main.rs`,
  `cli/commands/check.rs`, `analysis/seam_cache.rs`, and fixture `input/`.
  These lookups establish no analyzer failure.
* No `/usr/bin/time`, perf, or gdb is installed. The harness uses Rust's
  monotonic wall clock. Exact runtime parse-call counts and an independent
  external RSS measurement are unavailable. Correction after the pilot:
  the unmodified product emits `ripr_resource_cost_receipt` with self-observed
  Linux CPU ticks and peak resident bytes. These receipts are retained and
  extracted separately. CPU source resolution is 100 ticks/second; a zero
  quantized CPU observation is not proof of zero CPU work. Machine/cgroup
  resources are recorded independently.
* Around 21:12 UTC a new shell read failed with
  `exec_command failed: CreateProcess { message: "Rejected(\"Failed to create unified exec process: exec-server transport disconnected\")" }`.
  This was transport failure, not approval denial. The existing base build
  session remained active; a delayed patch/read returned successfully, and
  subsequent read-only probes recovered. `PARSE-WORKLOAD.md` was verified
  present (3,781 bytes). No timing process had started, no timing was lost,
  and no interrupted sample was admitted. No redundant build was launched.
* Compact entry-point selection was corrected by source inspection before
  timing: `repo-badge-json` calls full inventory for canonical actionable
  projection; `repo-exposure-summary-json` reaches compact inventory. The
  original choice was not executed or counted as compact evidence.
* A Cargo version probe used the rustup shim without the local RUSTUP_HOME and
  reported `could not create home directory: '/home/agent/.rustup': Read-only
  file system (os error 30)`. The original output is retained in environment.txt;
  direct `.../toolchains/1.95.0-x86_64-unknown-linux-gnu/bin/cargo -V` then
  recorded the installed Cargo version. Builds already used explicit local
  CARGO_HOME/RUSTUP_HOME; no home-directory permissions were changed.
* At about 21:15 UTC, polling base build session 94619 failed with
  `write_stdin failed: Unified exec process failed: exec-server transport disconnected: pong_timeout; failed to resume exec-server session: exec-server rejected request (-32600): unknown session id 03cdbe4b-1c60-4423-8f3b-d2b7747a7c8c`.
  Read-only recovery found owned Cargo PID 873 and rustc PID 3860 still running,
  with the build log intact and no binary yet. The old session's native exit
  result is unavailable. Do not equate this with product failure or start an
  overlapping replacement. After the process terminates, obtain a new no-op
  Cargo build receipt and verify executable identity before timing.

The user-provided denial of issue #8005 was honored. There was no read,
alternate-route retry, bot contact, or candidate-source write. Later failures,
if any, are retained in raw process output and the final report.

## Measurement setup details

* After the candidate build, a process-absence check `ps -C rustc -C cargo`
  returned 1 because no processes matched, causing a `set -e` wrapper to exit.
  Both builds had already completed; candidate file-backed status was 0 and
  version/digest checks succeeded. Repeating the check with explicit acceptance
  of the empty process list established the intended no-build boundary.
* The first main invocation filtered with substring `sync-4-single`, which also
  selected `async-4-single`. Both are planned workloads, fully labeled in raw
  files/TSV and independently completed at nine pairs. They remain valid; do
  not repeat async-4 samples. The driver was corrected to exact workload names,
  retaining suffix matching only for the explicit pilot selector `-4-single`.
* At 21:47 UTC the parent reported another environment disconnect. Direct
  recovery at 21:47:49 UTC succeeded: 570 completed-process TSV rows were
  present (including 32 pilot), the existing benchmark driver and candidate
  child remained active, both build receipts were terminal 0, and no Cargo or
  rustc process existed. The original benchmark session continued responding.
  No interrupted sample was observed or admitted and no replacement run was
  started. Later raw/row membership checks determine any orphaned output.
* A protocol documentation patch missed its expected text context and was
  rejected atomically. After reading the actual tail, the intended prospective
  sample-count amendment was applied. This establishes no product/test failure.

## Terminal failure envelope and instructed stop

Three candidate cold processes hit the unchanged 120-second bound, native
timeout status 124: async-64-single full and compact, and sync-64-single full.
Each raw stdout/stderr and driver error is retained; dependent warm paths were
not run, no timing median/CI is claimed for those incomplete groups, and the
driver did not retry them. The base cold/warm processes completed separately.

At 22:03 UTC the parent instructed this task to prioritize the durable bounded
report and stop spending repeated long timeouts once the failure envelope was
established. By recovery inspection the final sync-64-single compact candidate
was already active. At 22:04:31 UTC SIGTERM was sent only to verified owned PID
14556 with that exact argv. It terminated at 18.957066504 seconds; Rust reported
signal 15 and the TSV encodes no numeric exit code as -1. This is an instructed
interruption, not a product assertion failure or 120-second timeout. No completed
candidate output, warm observation, median or CI is claimed for that mode.
`controlled-stop.log` retains the action/target, and the wrapper's four stdout
lines plus both driver/status logs are retained. A post-stop process check found
no candidate, base, timeout, or benchmark driver process. Remaining unrun planned
rounds stay unrun; the already established envelope was not repeated.

## Final archive readback

The 2,725,700-byte XZ archive's connected Contents read returned the expected Git blob SHA and empty base64 content. The supported blob reader then failed with `UnicodeDecodeError: 'utf-8' codec can't decode byte 0xfd in position 0: invalid start byte`. This is an encoding/tool limitation, not corruption or an authorization denial. A fresh filtered no-checkout Git clone and binary `git show` readback downloaded the intended archive bytes into an independent object store. All 1,681 remote archive member hashes and sizes matched; the complete payload packet's 368 Git blob identities matched the fresh remote tree. No credential/security settings or issue read routes were changed. Publication receipt records this success and the failed reader observations separately.
