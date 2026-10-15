# Bounded all-suppressed GitHub CLI smoke input

`check_github_over_broad_suppression_policy_emits_denominator_notice` retains
its symbol and two real CLI executions. Its original nonempty unfiltered
annotation count, no-policy baseline, success exits, exact relative denominator,
non-silent filtered output, no leaked annotations and no false-empty notice
assertions now consume the owned sample fixture introduced by #7923. The fixture
also pins four findings. Production code, output contracts, workflows and the
six #7923 test bodies/oracles are unchanged.

The new policy writer stores its file in the fixture's exclusive scratch root;
the existing Drop owner removes it after success, setup failure or assertion
unwinding. The existing fixture embeds the sample source, tests and diff at
compile time, supplies an owned Cargo workspace, and launches the actual
`CARGO_BIN_EXE_ripr` from a separate Git decoy with invalid config. Explicit
absolute root/diff/policy paths and private cache/temp state bound analysis.
No product workspace is analyzed by this case. The existing 30-second owned
CLI process route and its platform containment limitations are retained.

## Actual warm observations

Base: `d736c39d749f4fabde4e8fe11456ec340bb88e90`.
Measured behavioral tree: `5c7c81bd5a146b6389b46a49c7813e5883da3353` (base plus the three changed Rust
test files; traceability and this receipt do not affect compiled/runtime inputs).
The final PR body binds this evidence to its committed head.

Native Windows/MSVC, Rust 1.95.0, locked default features
`default,lang-python,lang-rust,lang-typescript`, unoptimized test/dev profile,
line-table debug information, debug assertions and overflow checks enabled.
Cargo jobs: two. Build/filesystem caches were warm; no OS cache flush, affinity
or host-load control was applied. Other repository builds and the separate
recall-lane compilation were active. These are local observations, not hosted
CI savings or an isolated statistical performance qualification.

Each value brackets one direct `cli_smoke --exact <symbol> --nocapture
--test-threads=1` process with a monotonic Stopwatch, including fixture setup,
both CLI calls, assertions and cleanup. Compilation/artifact hashing/copies are
outside the measured interval. Baseline used an external task cache, populated
by the preceding warmup; every candidate fixture has fresh private analysis
state. The two approaches are compared as actual tests, not as identical
analyzer-cache experiments. Timing instrumentation prints fixture setup/CLI spans
only for the candidate and does not impose a time threshold.

| Subject | Warmup wall (s) | Measured warm wall (s), all attempts | Outcomes |
| --- | ---: | --- | --- |
| Original full product workspace | 161.9970071 | 125.3190586 | 1/1 passed per run |
| Owned sample fixture | 6.1352323 | 4.4080149, 4.5916419, 5.8939076 | 1/1 passed per run |

The observed candidate warm median is 4.5916419 seconds. Libtest's own elapsed
values exclude part of process startup/driver overhead; the table consistently
uses the enclosing Stopwatch values. Historical 163.437 seconds is context,
not this session's measured warm baseline. The baseline test inventory was 480;
the candidate inventory is 481 because one oracle-control test was added.

## Artifact and input identities

Both measured subjects executed the **same CLI bytes** selected by Cargo's
compiler-artifact rows: 75,096,576 bytes, SHA-256
`30ae533e65bd9bc0909aaaf0719ec0d086346771c7a0e2f508d8dd2f0c05fb67`.
The candidate CLI row was fresh; only the test harness recompiled (31.14 s,
excluded). The original focused build took 6m13s, also excluded.

| Retained executable | Bytes | SHA-256 |
| --- | ---: | --- |
| Original cli_smoke | 49,890,304 | `f7f147bd8dd361786e85974d95d8679c5f4671e2d5586f2d9a22e18f09ed6220` |
| Candidate cli_smoke | 49,897,984 | `2872e29d98e80c6531a80d5430d69bfb08f8346977c7e415f7c7b72a05229f52` |

Embedded sample source/test/diff: 598 / 252 / 693 bytes (1,543 bytes total),
plus small owned manifests, Git metadata, the decoy and suppression policy.
These figures describe source inputs, not peak cache or process memory.
No native Windows file-I/O trace was captured; input independence follows the
existing explicit fixture path/decoy implementation and retained #7923 controls,
not a new claim of an observed Windows read set.

Raw Cargo artifact receipts, native terminal logs, timing JSONL and ordinary
copied witnesses are retained at `H:\Temp\ripr-task5-evidence`.
`baseline-ripr.exe`, `baseline-cli-smoke.exe` and `suppression-cli-smoke.exe`
are actual copied bytes, not hard links. The runner embeds its original absolute
CLI path; retain the task target while replaying it, or restore the retained CLI
there only after all target consumers release it. This is local reproducibility
evidence, not a portable installed-package qualification.

## Discriminating controls and coverage

`suppression_notice::retained_oracle_rejects_silence_wrong_counts_and_leaked_annotations`
first executes the same positive public path, then requires the unchanged oracle
to reject nine seeded wrong outcomes: empty baseline, false baseline policy,
wrong total, wrong suppressed count, leaked annotation, false-empty notice,
silence, failed baseline exit, failed filtered exit. Output mutations change
actual emitted bytes; failed statuses come from an actual invalid-format CLI
refusal. Caught assertion panics are expected control witnesses. These controls
do not replace either original smoke-test CLI call and do not claim mutation
adequacy of the analyzer.

Focused execution additionally covers all six #7923 original exact tests and
its two shared fixture/oracle controls. Validation outcomes and independent
exact-head inspection are recorded in the PR. Rollback is this test/helper,
controls, traceability and receipt together; no production change is required.

The commit carries `[skip ci]` to honor the user's no-broad-CI limit: current
wheel/advisory workflows otherwise trigger even for Draft creation. The PR
stays Draft without manual dispatch, Ready transition or broad-CI labels.
Hosted qualification, merge readiness, release and deployment are not claimed.
