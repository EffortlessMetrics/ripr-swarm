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
  monotonic wall clock. Per-process peak RSS and exact runtime parse-call
  counts are not measured; machine/cgroup resources are recorded.
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
