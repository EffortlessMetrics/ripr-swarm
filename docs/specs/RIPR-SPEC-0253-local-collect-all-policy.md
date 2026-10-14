# RIPR-SPEC-0253: Local collect-all policy scans

Status: accepted

Owner: xtask / repo-infra

Created: 2026-10-12

Linked issues: #5073, #7826

Linked PRs: #7521

Standalone reason: #5073 defines this bounded local command over existing
independent policy scans; it does not compose product policy artifacts.

Support-tier impact:

- None.

This local command changes no product, language, platform, packaging, or
support-tier promotion.

Policy impact:

- Register this contract in the spec index, document-artifact ledger, and
  traceability manifest. The original standalone command changes no leaf
  predicate, policy allowance, hosted workflow, required gate, or CI cadence.
  The later #7826 composition reuse is described below.

## Problem

Existing local policy composites return at the first policy error. A developer
with two independent violations needs another invocation after fixing the first
to discover the second. The local command needs an explicit behavior contract
connecting its acceptance cases to the actual driver tests and owning code.

## Behavior

`cargo xtask check-policy-all` invokes these existing checks exactly once in
this precommit-relative order:

1. `check-static-language`
2. `check-no-panic-family`
3. `check-allow-attributes`
4. `check-local-context`
5. `check-covered-by`
6. `check-executable-files`
7. `check-process-policy`
8. `check-network-policy`
9. `check-lint-policy`

Every selected check runs even when an earlier check returns an error. The
composition preserves each leaf's predicate, report, success output, and
returned error text. It does not infer a result from a previous report file:
a leaf error before report publication remains an error even if an old report
declares success.

After all nine checks return, the command writes
`target/ripr/reports/check-policy-all.md`. The report lists every selected
command and its returned pass/fail outcome in order. Each failure includes
the original error text and its individual rerun command. The command returns
nonzero when any leaf returns an error or when aggregate report publication
fails. If both happen, the returned error retains the leaf failures and adds
the aggregate write error. Successful leaves cannot mask a reporting error.

The selected leaves inspect filesystem, policy, or Git-index inputs
independently. They do not consume earlier leaf reports, invoke Cargo
build/test/Clippy children, or run the compilation-bearing `check-file-policy`.
Git queries remain permitted. `cargo xtask` can compile the driver before
executing this command; the existing Windows bootstrap may reexecute it.
The catalog exposes the command as a local non-mutating check with
`ci_enforced = false`.

## Required Evidence

- Invoke the actual xtask driver against a tiny Git-indexed workspace with a
  nonempty static test inventory. Require all nine standalone leaves to pass
  before introducing deliberately invalid input.
- Reject a deliberate fail-fast implementation using two independent
  violations in the first and sixth checks. Require both original errors,
  overall failure, and fresh reports from later selected leaves.
- Compare aggregate-run leaf reports and success/error output with separate
  invocations of the same existing leaves; pin membership and actual success
  execution order.
- Exercise early I/O errors despite stale reports and aggregate write errors
  with both failing and otherwise healthy leaves.
- Inspect selected call trees and retain native process-trace evidence where
  available for the absence of Cargo/compiler children. Linux execution does
  not establish native Windows execution.

Focused proof route:

```bash
cargo test -p xtask --test policy_all
cargo xtask check-policy-all
cargo xtask check-spec-format
cargo xtask check-spec-numbering
cargo xtask check-traceability
cargo xtask check-doc-artifacts
```

## Non-Goals

- No leaf-policy relaxation or changed precommit/check-fast failure behavior.
- No generic scheduler, new registry, parallel execution, or new hosted step.
- No Cargo compilation/testing/Clippy in the selected leaf scans.
- No product policy-artifact composition or product API/schema change.
- No rustc speedup, cold/warm elapsed-time ceiling, or CI-equivalence claim.

## Acceptance Examples

1. Given nine healthy leaves, the command returns success, reports all nine
   passing outcomes, and preserves original leaf reports and success order.
2. Given an invalid static-language statement and an unregistered executable
   Git-index entry, the first and sixth checks both fail. The command returns
   failure, includes both diagnostics, and executes the later selected checks.
3. Given a missing allow-attribute ledger and a stale passing leaf report, the
   early I/O error still fails the aggregate while the executable violation
   and later successful lint-policy result remain visible.
4. Given an executable violation and an unwritable aggregate report path, the
   returned failure retains the executable diagnostic and the write error.
5. Given healthy leaves and an unwritable aggregate report path, the command
   returns failure despite all nine leaf successes.

## Test Mapping

All cases invoke the real driver through `xtask/tests/policy_all.rs`:

- Example 1: `policy_all_healthy_fixture_passes_with_original_leaf_reports`.
- Example 2: `policy_all_reports_two_independent_violations_and_runs_later_leaves`.
- Example 3: `policy_all_keeps_early_io_error_and_does_not_trust_stale_leaf_report`.
- Example 4: `policy_all_report_write_error_preserves_leaf_failures`.
- Example 5: `policy_all_healthy_leaves_still_fail_when_aggregate_report_cannot_be_written`.

`.ripr/traceability.toml` registers the exact file/symbol references. A
traceability-check pass validates registered paths; it does not itself show
that a test symbol exists or ran. See [Spec-test-code traceability](../SPEC_TEST_CODE.md).

## Implementation Mapping

- `xtask/src/policy/all.rs::check_policy_all` owns explicit sequential
  composition, returned outcomes, aggregate report, and final failure semantics.
- `xtask/src/policy/mod.rs` exports that existing policy owner.
- `xtask/src/command.rs::XtaskCommand::parse` and the command catalog expose
  `CheckPolicyAll`, its local status, report path, and compilation boundary.
- `xtask/src/dispatch.rs::execute` routes that variant to `check_policy_all`.
- Existing selected leaf owners and `tools/repo-policy/src/lib.rs` retain
  policy predicates, reports, Git helpers, and the shared report writer.
- [PR automation](../PR_AUTOMATION.md) documents local use;
  [Contributing](../../CONTRIBUTING.md) provides the focused test route.

## Metrics

- `unit_test_pass_rate` for the five named actual-driver cases, with the
  executed/failed/ignored counts retained in the test output.
- Literal nine-check membership and first/sixth failure counts are contract
  assertions, not new telemetry counters or timing budgets.

## Composition reuse (#7826)

Hosted `cargo xtask ci-policy` reuses this producer before compilation-bearing
file policy, preserving these nine predicates and attributable errors. The
`ci_policy_collects_real_cheap_failures_before_expensive_work` actual-dispatch
test retains two independent failures, later leaf output, stale-pass replacement
and local-receipt isolation. This reuse changes no standalone command semantics;
the complete hosted composition remains in the canonical product-gate plan.

CI publication also preserves both legacy `precommit.md` and structured local
precommit reports. `ci_policy_publication_preserves_local_report_and_local_io_failures`
checks existing-report preservation, unchanged local content and local write-error
propagation through the shared publication owner.
