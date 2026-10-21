# RIPR-SPEC-0260: Python guarded-arm activation gate for return-value credit

Status: proposed

Owner: product-analysis

Created: 2026-10-14

Linked issues: [#7392](https://github.com/EffortlessMetrics/ripr-swarm/issues/7392),
[#9002](https://github.com/EffortlessMetrics/ripr-swarm/issues/9002).

Support-tier impact: Python preview adapter output only; no support-tier
change. See: [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md).

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

A Python return-value change inside a guarding `if` arm executes only when a
call's inputs satisfy the guarding comparisons. Before this gate, a strong
exact-value oracle on inputs that all fail the guard read `exposed`: the
assertion pins the returned value, but the changed line never runs under any
observed input, so the oracle passes either way. Reach plus a strong oracle
is not exposure when the oracle never sees the new line run; the verdict was
a false `exposed` that told the user the change was already pinned.

## Behavior

For a Python `return_value` probe on an added line inside one or more
guarding `if`/`while` arms, the classifier reads the enclosing conditions
from the new-side source and the literal arguments the strong related tests
bind:

- When every strong related call provably fails at least one guarding
  comparison, the changed line never executes under an observed input. The
  finding fails closed to `weakly_exposed`, naming the guarding condition as
  the missing discriminator and surfacing the literal inputs the strong
  calls bind as observed values, the way the predicate boundary rule does.
- A row with an unevaluable guard is never excluded: one unreadable input
  keeps the existing verdict. Shapes that fail open (keep the pre-gate
  verdict) include `else` arms, `elif` chains read by their own condition,
  guards containing `or`/`not` boolean syntax anywhere in the WHOLE
  condition (the check runs before any `and` conjunct is evaluated, so a
  false leading conjunct cannot exclude a row a later `or` lets in,
  #7874), a parameter the owner rebinds before the guard (plain,
  single- or multi-character augmented, annotated, destructuring-target,
  or `for`-target assignment), computed operands, non-literal call rows, a
  strong related call that binds no row (a `*args` unpack or unreadable
  argument list), and entries without source (repo mode, deleted lines,
  the old test harness).
- Fallthrough (#9002): a changed return that follows a simple `if X:` in its
  own block whose body ends in a top-level `return`/`raise` (no `elif`/`else`
  chain) runs only for inputs that fail `X`. When every strong related call
  provably satisfies at least one such preceding early-exit condition, the
  finding fails closed to `weakly_exposed` with missing discriminator
  `not (X)`; the reason says each call satisfies at least one early-exit
  condition, since different calls may take different exits. Lines inside
  triple-quoted strings are not code and are never read as early exits. A
  call that misses every exit, a non-terminating or nested-only `return`
  body, a chained `elif`/`else`, and unreadable guards keep the existing
  verdict.
- Equality follows Python semantics for scalar literals: `True == 1` and
  `False == 0` hold, and `2 == True` does not, so a bool call argument is
  not excluded by a numeric guard (or vice versa).
- Ordering comparisons never order across kinds (a string against a number
  is unreadable, which fails open). Integer literals order exactly as
  Python does beyond f64 precision (`9007199254740993 > 9007199254740992`
  holds): both integer-form operands compare as `i128`, refusing on
  overflow, while float literals keep the f64 path because Python rounds
  them at parse time too (#7874).

## Required Evidence

Unit tests cover the fail-open shapes (`or` and `not` guards whole-condition
and after a false conjunct in both operand orders, a rebound parameter
including multi-character augmented and destructuring-target assignments,
comparison-line negative controls, bool/numeric equality, and exact
large-integer ordering) and the end-to-end classification: an
arm-entering input, an unreadable strong call, a rebinding owner, and each
#7874 counterexample keep `exposed`; only the all-rows-excluded case
downgrades. Fixture goldens record the emitted missing-discriminator text
and observed values.

## Non-Goals

This spec does not change the predicate boundary rule, the boundary rule's
own literal equality (its mixed-kind inequality stays conservative for
credit), or any non-Python adapter. It does not evaluate arbitrary Python
expressions: only simple comparisons over literal bindings are read, and
everything else fails open. It does not cover changes without a readable
guard predicate (a pure-expression change).

## Acceptance Examples

1. `def quote(total): if total > 1000: return round(total * 1.02, 2)` with
   the only strong test asserting `quote(100.0) == 105.0` reads
   `weakly_exposed`, naming `total > 1000` with the observed `total = 100.0`.
2. The same owner with a test asserting `quote(2000.0) == 2040.0` stays
   `exposed`.
3. `if total > 1000 or force:` with a test calling `quote(100.0, True)`
   stays `exposed`: the `or` makes the guard unreadable and the gate fails
   open rather than excluding the row.
4. `def quote(total): total = 2000; if total > 1000: ...` keeps `exposed`
   for `quote(100.0)`: the rebinding means the call binding cannot exclude
   the row.
5. `if flag == 1:` with `quote(True)` stays `exposed` (`True == 1` in
   Python).

## Call-Row Excerpt

The strong call rows the gate reads carry display text (#8777, #8788). The
text is display only and never feeds a decision:

- A physical line of at most 240 bytes is shown whole (trimmed).
- A longer line is cut to a window of at most 240 bytes that starts up to 60
  bytes before the call name, on character boundaries.
- A call that fits the window is kept whole: the lead shrinks by the exact
  call extent (name through the closing parenthesis, whitespace before `(`
  included), and the start rounds toward the call so a multibyte character
  never costs the call its trailing bytes.
- `...` marks a side only where visible text was cut, never for indentation
  or trailing whitespace alone.

## Test Mapping

- `analysis::language::python::guard_activation::tests` covers condition
  reading, the `or`/`not` fail-open, rebound-parameter detection, and
  bool/numeric equality.
- `analysis::language::python::classify::tests::guarded_arm_*` and the
  `or_guard_*`, `unreadable_strong_call_*`, `rebound_parameter_*`, and
  `bool_numeric_guard_*` cases cover the end-to-end verdicts.

## Implementation Mapping

- `crates/ripr/src/analysis/language/python/guard_activation.rs` owns the
  guard reading, row exclusion, and fail-open shapes.
- `crates/ripr/src/analysis/language/python/boundary.rs::strong_owner_call_rows_with_gaps`
  supplies the strong rows and the unbound-call flag the gate refuses on.
- `crates/ripr/src/analysis/language/python/classify.rs` engages the gate
  for `return_value` probes with new-side source.

## Metrics

Report the count of Python return-value findings the gate downgrades per
run; the fixture set carries the expected counts. A false downgrade (a
fixture reading `weakly_exposed` whose observed input enters the arm) is a
release-blocking honesty defect, not a workload measurement.
