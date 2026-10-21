# RIPR-SPEC-0280: covered_by enumeration instrument-failure contract

Status: proposed

Owner: test-infra

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8956 (this spec and its traceability)
- #8793 (the lock-wait instrument failure and `precommit` deferral)
- #8939 (the lock-wait wall-clock ceiling)

Linked PRs:

- #8905 (lock-wait retry and the `covered_by_lock_wait_timeout` class)
- #8941 (90-minute ceiling and the `precommit-v2` composition tests)

Support-tier impact:

- None. This is repository automation (`xtask`), not product analysis.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No behavior change. The spec states the contract that #8905 and #8941
  implemented and maps it to its tests and code. It does not bolt onto
  RIPR-SPEC-0191, which governs host-scoped `covered_by` selection only.

## Problem

`check-file-policy` validates each test-valued `covered_by` pointer in
`policy/non-rust-allowlist.toml` by enumerating `cargo test ... -- --list`.
That enumeration can fail for reasons that say nothing about the pointer:
Cargo may sit blocked on a build or package lock another process holds, or
the spawn may exceed its time budget. Reporting such a failure as an
unresolved pointer sends the author to repair a correct pointer. Passing it
silently hides a pointer that was never checked. Before this contract the
behavior (retry bound, ceiling, classification, deferral) was implemented but
governed by no spec, so it sat outside the spec-test-code chain.

## Behavior

### Rule 1: lock-wait retry is bounded

A spawn that timed out is a lock wait only when its stderr's last non-blank
line contains `waiting for file lock`. Cargo keeps that line after it
acquires the lock and goes on to compile, so a lock line followed by any other
output is a stale mention and not a lock wait.

A lock-wait timeout is re-run at most `LOCK_WAIT_RETRIES` (6) times, so a
sequence is at most 7 spawns. Time queued behind another process's build does
not consume the enumeration budget. A timeout that is not a lock wait is never
retried. A lock wait that clears within the bound completes normally. The
retry history (`xtask: retried N time(s) while blocked on a cargo lock ...`)
is kept ahead of cargo's own output in the retained stderr.

### Rule 2: the lock-wait ceiling

One lock-wait retry sequence has a total wall-clock ceiling,
`LOCK_WAIT_CEILING` (90 minutes). The attempt bound alone permits 7 spawns of
the 30-minute build timeout (about 3.5 hours). The ceiling is the sum of the
recorded durations of the lock-wait attempts, checked after each attempt; no
clock is read. The sequence may therefore overshoot the ceiling by at most
one spawn timeout. A sum equal to the ceiling is exhausted. Retrying stops at
whichever bound is reached first. When the ceiling stops it, the retained
stderr carries a history line starting `xtask: lock-wait ceiling of <ceiling>
exhausted after <waited>`, and the reported failure names the ceiling. A
sequence below the ceiling retries to completion.

### Rule 3: classification is an instrument failure, not a source failure

An enumeration that ends timed out is classified by its stderr, never as an
unresolved pointer:

- blocked on a cargo lock at the end: `covered_by_lock_wait_timeout`
  (lock wait, including a ceiling stop);
- any other timeout: `covered_by_instrument_timeout`.

Both produce a validator error whose first line contains `enumeration did not
finish (instrument timeout, not an unresolved pointer)`. That error is an
instrument failure. A pointer that selects zero tests, a non-cargo-test
command, or a failed (non-timed-out) enumeration remains a source or policy
failure and does not carry the phrase.

`is_covered_by_instrument_failure` recognises an instrument failure only by
that phrase on the error's first line, which the validator formats itself.
Later lines embed arbitrary cargo stdout and stderr and cannot turn a real
failure into an instrument one; a spoofed phrase in embedded output does not
classify.

### Rule 4: `precommit` defers the instrument failure

Under `precommit` and `precommit-v2`, a `check-file-policy` instrument failure
is stored instead of returned, and the remaining policy gates run. If no later
gate fails, the run fails at the end with an error beginning
`precommit instrument failure (no source or policy gate failed): `. In
`precommit-v2` the remaining stages (the diff checks and selected Clippy) run
and are recorded, the instrument failure is retained in the report's
limitations, and the published report has status `infrastructure_failure` and
failure kind `infrastructure`.

### Rule 5: fail closed

- An instrument failure is never a pass: the run always fails, and the failure
  is never reported as a clean `covered_by` result.
- A real policy failure alongside an instrument failure wins. Both are
  reported, the error does not carry the instrument prefix, and under
  `precommit-v2` the report is `source_or_policy_failure`, later stages do not
  run, and the run is not classified as infrastructure.
- A later diff-check or Clippy failure after a deferred instrument failure is
  reported with that failure's own kind (`fail_with_diagnostic_report`), not
  forced to `source_or_policy_failure`; the instrument failure text survives
  only in the report's limitations.
- Only an instrument failure is deferred past later gates. A genuine policy
  error stops at the first one, as before.
- Exhausted retries or an exhausted ceiling stay failures; they are never
  converted to a pass or to an unresolved-pointer finding.

## Decisions

1. **Bound by attempts and by summed duration.** Adopted: both
   `LOCK_WAIT_RETRIES` and `LOCK_WAIT_CEILING`, with the sum taken from
   recorded attempt durations. Rejected: an attempt bound alone (about 3.5
   hours of waiting) and a wall-clock read (untestable without a clock seam).
2. **First-line anchoring.** Adopted: classify only the validator's own first
   line. Rejected: searching the whole error, which lets quoted cargo output
   mislabel a real failure as infrastructure.
3. **Defer, do not skip.** Adopted: run the remaining stages so one lock wait
   does not hide later findings, then fail. Rejected: passing, or stopping at
   the lock wait.

## Required Evidence

- Unit tests pin the retry bound with a negative experiment (a lock that never
  clears fails distinctly after `LOCK_WAIT_RETRIES + 1` calls), that a real
  timeout and a stale lock line are not retried, that every attempt and its
  duration are recorded, and that a spoofed phrase and an unresolved pointer
  do not classify as instrument failures.
- The ceiling test pins an early stop that names the ceiling, the exact-ceiling
  boundary, a sub-ceiling sequence that retries to completion, and a single
  over-ceiling non-lock timeout (stderr shows `Compiling`, no lock line) that
  is neither retried nor labeled as the ceiling. A lock-wait attempt that
  reaches the ceiling is labeled a ceiling failure, per Rule 2.
- Composition tests drive `run_composition_in` and pin the deferred failure as
  `infrastructure_failure` after the diff checks and selected Clippy run, and a
  real policy failure alongside as `source_or_policy_failure` with no later
  stage.

## Non-Goals

- Changing any retry bound, the ceiling, a classification prefix, or the
  deferral behavior.
- Enumeration budgets for non-lock timeouts, or the host-scoped selection
  rules (RIPR-SPEC-0191).
- Killing or inspecting the process that holds the lock.
- A claim about product analysis, runtime mutation outcomes, or gate sufficiency.

## Acceptance Examples

1. A lock that clears on the fourth attempt: enumeration completes and is not
   timed out.
2. A lock that never clears: 7 spawns, then `covered_by_lock_wait_timeout`,
   an instrument failure, not a pass.
3. Three lock-wait attempts whose durations sum past 90 minutes: retrying
   stops after three, and the failure names the ceiling.
4. A timeout whose stderr shows `Compiling` after the lock line: one spawn,
   `covered_by_instrument_timeout`.
5. `precommit-v2` with only a deferred instrument failure: the diff checks and
   Clippy run; the report is `infrastructure_failure`.
6. The same with a failing later policy gate: `source_or_policy_failure`, not
   infrastructure. A later diff-check or Clippy failure instead carries its own
   kind, with the instrument text only in limitations.

## Test Mapping

- `xtask/src/policy/file_policy.rs`: rules 1 to 3 and the validator side of
  rule 5.
- `xtask/src/precommit_v2.rs`: rules 4 and 5.

## Implementation Mapping

- `xtask/src/policy/file_policy.rs`: `LOCK_WAIT_RETRIES`,
  `LOCK_WAIT_CEILING`, `lock_wait_note`, `capture_past_lock_wait`,
  `enumeration_result`, `validate_test_covered_by_with`,
  `is_covered_by_instrument_failure`.
- `xtask/src/main.rs`: `check_file_policy_deferring`,
  `finish_precommit_policy`, `PRECOMMIT_INSTRUMENT_PREFIX`.
- `xtask/src/precommit_v2.rs`: the deferred-instrument handling in
  `run_composition_in`.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
