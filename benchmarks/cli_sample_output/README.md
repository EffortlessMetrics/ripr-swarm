# Six sample-output CLI cases (#7825)

The six named smoke tests retain their symbols and every original output,
exit and refusal assertion. Their input is now a private sample repository,
constructed by `crates/ripr/tests/cli_smoke/sample_output.rs` from the existing
sample source, tests and diff embedded at compile time. The sample paths and
probe IDs stay unchanged. Its two changed owners (`price`, `validate_currency`),
two real weak test oracles, four findings and three probe families are pinned.
No production analyzer, output renderer, schema, workflow or gate changes.

Each invocation owns an exclusively created external scratch directory, its
Git index/commit, reports/root, cache and temporary state. The child executes
from a separate Git decoy with unrelated source and invalid configuration.
Explicit absolute root/diff arguments and the owned workspace manifest keep
checkout/config ancestor discovery bounded. Child-only environment settings
avoid global mutation. Fixture Git uses the existing hardened helper; CLI
execution uses the existing owned, drained, 30-second stdin-probe route. RAII
cleans partial population, successful completion and assertion unwinding.
The shared route's native platform containment limits are unchanged.

The original six functions remain in `cli_smoke.rs`; shared output oracles let
the negative test mutate actual rendered output after a positive analyzer run.
It rejects missing/wrong contract fields/schema/classification, empty findings,
refusal keys on success, native/Shields shape and count errors, a wrong exit,
extra Shields fields, and missing human count/discriminator guidance. The
parallel control runs two roots together and verifies independent writable
sentinels and cleanup. No canned analyzer result replaces a smoke execution.
Traceability retains the existing success symbol and records the owned fixture
and controls under existing specs 0001, 0122 and 0241.

## Matched local observations

[measurements.json](measurements.json) retains all nine family attempts and all
54 terminal case outcomes, identities, setup/CLI spans and trace digests.
Base: `4ad9553adc77ef63f6cb8a134d0cfc30ce638158`, tree
`6a9a37d0482ad6b61327bdd82b2ffe969b133b40`. Measured candidate test tree:
`cd221e46274f1b586990901d23b97b947cc1553e` (the two test files only).
Subsequent wrapper whitespace and evidence/traceability additions are outside
the measured behavioral dimensions; the final frozen candidate is checked again.

Both subjects used the **same source-built CLI bytes**, SHA-256
`dde62baeabd0afedae2c9ef2d87a4bef648564e66331bda80e344eea394c31c3`, selected by
Cargo's compiler-artifact message. Linux x86_64, the same container/4-CPU quota,
16 GiB memory ceiling, Rust 1.95.0, locked default Rust/TypeScript/Python features,
test/dev profile with line-table debug information. Analysis caches were fresh
and private per case. Existing build/filesystem caches were retained, with no
OS cache flush or CPU affinity/load control. Compilation is outside the timings.

Six independent `--exact` libtest processes launch concurrently; wall starts
before dispatch and ends after all six terminal outcomes are collected. Two
untraced attempts precede one traced attempt per subject. Polling has 10 ms
resolution. These are family wall observations, not summed testcase seconds.

| Subject | Untraced family wall (s), all attempts | Traced family wall (s) | Rust read set / case | Successful execs / family |
| --- | --- | --- | --- | --- |
| Base | 136.661462, 130.299345 | 139.066216 | 1,193 | 12 |
| Owned fixture | 0.131023, 0.124948 | 0.455903 | 2 | 48 |
| Owned + 64 checkout files | 0.126014, 0.131564 | 0.478047 | same 2 | 48 |

All traced subjects execute six actual CLI processes plus six case processes.
Candidate adds 24 hardened fixture-Git invocations and 12 CLI Git probes.
Candidate untraced fixture setup takes 52.8–63.1 ms per case; CLI spans take
34.4–57.4 ms. Baseline creates no fixture; root/diff path lookup is not timed
separately, and its libtest execution durations include the CLI and assertions
(the receipt does not invent a precise baseline CLI-only span). Traced times
include instrumentation overhead and are not mixed with untraced observations.

The read checker reconstructs unfinished/resumed syscalls and includes analyzer
worker-thread reads after the one actual CLI exec. It admits exactly the owned
`src/lib.rs` and `tests/pricing.rs`, and verifies the CLI's decoy CWD. All six
normal and all six grown-checkout traces pass. The growth control added 64 valid
Rust files under the actual product `crates/ripr/src/` tree, verified the untracked
Git inventory, repeated all three attempts, then removed only that owned directory.
No elapsed ceiling decides independence. Input hashes are in the receipt.

Local checks establish this bounded mechanism, not hosted nextest/critical-path
savings, CPU time, native Windows acceptance or release qualification. Historical
654.826 summed concurrent testcase seconds is not a savings promise. Raw traces,
Cargo JSON selection receipts and all logs remain under `target/7825-evidence`;
the measured CLI has an ordinary copied frozen witness there. Original baseline
runner bytes were overwritten during the candidate build; its source, artifact
selection/digest and terminal logs remain, rather than claiming byte retention.

## Executable proof routes

```bash
cargo test --locked -p ripr --test cli_smoke sample_output::
cargo test --locked -p ripr --test cli_smoke --no-run --message-format=json > build.jsonl
# Select the cli_smoke executable and ripr executable from compiler-artifact
# rows with executable != null. Use absolute paths from this receipt.
# For each of the six named cases, capture one isolated exact-case trace:
strace -f -qq -yy -e trace=execve,openat,chdir -o case.trace \
  "$SMOKE_EXE" --exact check_json_output_has_stable_contract_fields --nocapture
rustc --edition 2024 benchmarks/cli_sample_output/check_reads.rs -o check-reads
./check-reads owned "$RIPR_EXE" case.trace
```

Repeat the trace command for the six names in `measurements.json`. Repeat with
unrelated source files added under the surrounding checkout and require the
same read checker result. Run this growth experiment serially outside other
checkout consumers, then remove only its owned files. Baseline uses the exact
base test source and `check-reads base`. For matched cold-cache family runs,
launch the six exact cases concurrently with an empty distinct `RIPR_CACHE_DIR`
per process; enable `RIPR_SAMPLE_OUTPUT_TIMINGS=1` for candidate setup/CLI spans.
Retain every outcome and bracket dispatch-to-all-exit with a monotonic timer.

Required focused consumers: `check-process-policy`, `check-traceability`,
`check-fixture-contracts`, `check-output-contracts`; then committed-tree
`check-fast`, independent exact-head review around `precommit`. Draft remains
in flight for any normal hosted qualification. Rollback is this test/fixture,
traceability and evidence package alone.
