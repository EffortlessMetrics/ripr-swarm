# Recall warning smoke fixture

The original `check_no_unchanged_tests_restates_the_recall_tradeoff_on_stderr`
test still executes the real Cargo-built CLI twice: once with
`--no-unchanged-tests` and once with the default setting. It now uses the
existing `sample_output::Fixture` from #7923 instead of analyzing the entire
product workspace. The fixture module and its six existing smoke consumers
are unchanged by this repair.

The retained oracle requires successful exits, the active exclusion notice,
the exact Reach-recall explanation, and absence of the exclusion notice on the
default run. Both runs must analyze the sample's four probes. A second test
runs the real CLI and independently rejects seven wrong-output variants:
missing exclusion notice, missing recall explanation, a false notice on the
default run, empty active/default analysis output, and failed active/default
statuses. Failure statuses come from a real invalid-format refusal.

The shared fixture embeds two Rust input files and their existing diff at
compile time. It constructs an exclusively owned external workspace and Git
repository, executes from an invalid-config decoy, isolates cache/temp state,
and uses the existing drained 30-second process route and RAII cleanup.
No production, output, schema, workflow, dependency, process-launch or global
configuration change is involved.

## Measurement receipt

Measured on native Windows x86_64/MSVC with Rust 1.95.0, default features,
the unoptimized test profile, and at most two build jobs. Baseline used the
parent task target; candidate used its own private target. The baseline left `CARGO_INCREMENTAL` unset (profile default enabled);
the candidate build sets `CARGO_INCREMENTAL=0` to bound its private target.
This compile-cache difference and distinct checkout paths mean the complete
build configuration and CLI bytes are not claimed identical. Base source: `d736c39d749f4fabde4e8fe11456ec340bb88e90`,
tree `f1d9ce0db1ef61c39201626b1e6dae335a55567d`.
Prepared behavioral tree (only the two test-source changes):
`1c209091ebbea62e2b19782d3d8643dfba79a8a9`.
The unchanged shared fixture blob is
`38c51a75d725f7e177f4c2a8bbea25bdb40607f0`.

A warm filesystem/build comparison uses direct exact-case libtest processes;
compilation is outside the timings. Baseline followed the suppression-test
warmup and measured run, with no OS cache flush, affinity, or host-load
control. Other task processes and formatting may have overlapped; no claim
of isolated-machine timing or hosted CI savings follows. Parent precommit
static checks and unrelated host work could run during candidate timings;
no new parent compilation or heavy CLI scan was started in the window.
Each candidate case gets fresh private analysis-cache state; the paired
invocations share that fixture. All three measured warm outcomes are retained,
with median 4.8775304 seconds; the baseline is one warm observation.

| Subject | Wall seconds | Libtest seconds | Outcome |
| --- | ---: | ---: | --- |
| Unchanged base, 2026-10-15 05:14:36 UTC | 98.9471032 | 97.90 | 1 passed, 479 filtered |
| Owned fixture warmup | 9.8326890 | 8.14 | 1 passed, 480 filtered |
| Owned fixture warm 1 | 10.6176470 | 9.41 | 1 passed, 480 filtered |
| Owned fixture warm 2 | 4.8775304 | 4.19 | 1 passed, 480 filtered |
| Owned fixture warm 3 | 3.7307245 | 3.69 | 1 passed, 480 filtered |
| Seven wrong-output controls | 4.0488070 | 3.61 | 1 passed, 480 filtered |

Compiler-artifact-selected witnesses, independently rehashed. Candidate hashes
were unchanged across all runtime runs:

| Executable | Bytes | SHA-256 |
| --- | ---: | --- |
| `ripr.exe` (base) | 75,096,576 | `30ae533e65bd9bc0909aaaf0719ec0d086346771c7a0e2f508d8dd2f0c05fb67` |
| `cli_smoke-3fbbfebcb5614937.exe` (base) | 49,890,304 | `f7f147bd8dd361786e85974d95d8679c5f4671e2d5586f2d9a22e18f09ed6220` |
| `ripr.exe` (candidate) | 74,897,920 | `4e3fc39d5756e5886385b2c382b674e95ed92a2fc010b59337231ffcd174240f` |
| `cli_smoke-9cb719be523a85c0.exe` (candidate) | 49,795,072 | `99b721b46365cc9542650a506b05bfe715e4bf58efd9b3c79204322ca9eaa989` |

The embedded source is 598 bytes, tests 252 bytes, and diff 693 bytes
(1,543 bytes total before manifests/Git/decoy/cache state). The new oracle
module is 2,911 bytes. The candidate target held 2,230,385,450 bytes after the
focused build, excluding the two retained executable copies (124,692,992 bytes).
This is observed size, not a peak-memory or peak-disk measurement.

Focused compile and formatting pass. The original test passed four runs;
the seven wrong-output controls passed in one separate exact test. The
unchanged production source, default Rust/Python/TypeScript features,
optimization level 0, line-table debug information, debug assertions and
overflow checks match the base; the documented build-path/incremental
configuration differences remain. Clone-bound aggregate policy evidence
is pending and will be recorded on the draft PR.

Raw receipts are retained in the task-owned evidence directory (`$EVIDENCE`):
`baseline-artifacts.jsonl`, `baseline-artifacts.json`,
`baseline-timings.jsonl`, and `recall-warm-before.log`, plus ordinary copied
`baseline-ripr.exe` and `baseline-cli-smoke.exe` witnesses. These paths
supplement the durable values above. Candidate receipts live under the
`recall` subdirectory: `candidate-build.jsonl`,
`candidate-artifact-selection.json`, `candidate-artifacts.json`,
`candidate-timings.jsonl`, all five exact-test logs and both verified witness
copies. No elapsed-time assertion controls test success.

This test-only fixture refactor is exempt from a changelog fragment under
`docs/CHANGELOG_POLICY.md`: it changes no product behavior, public contract,
documentation system, or workflow. Traceability maps the nonempty CLI
analysis and wrong-output controls to RIPR-SPEC-0001.

## Publication boundary

The draft publication commit carries `[skip ci]` because this task explicitly
excludes broad CI. Draft status alone does not skip every advisory pull-request
workflow. No Ready transition, full-CI label, manual workflow dispatch, merge,
release, or deployment is requested. Skipped checks may remain pending, so
these local measurements and controls do not establish merge readiness.