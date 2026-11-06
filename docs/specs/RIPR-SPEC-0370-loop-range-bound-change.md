# RIPR-SPEC-0370: A changed range bound in a `for` header is credited only by an exact pin whose argument makes the old and new ranges iterate a different number of times

Status: proposed

Owner: product / analysis

Created: 2026-10-31

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #11399 (burn-down rank 2 of #11111, audit case 16 `loop-accum`)
- #6485 (RIPR-SPEC-0360, the loop accumulator this extends)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory. Until the producer
  below exists, a changed range bound stays `static_unknown`
  (`static_probe_unknown`); this spec pins that fail-closed floor and the
  contract the producer must meet.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0370.toml`, add
  `.ripr/traceability.d/loop-range-bound.toml`, add a
  `docs/spec-index.d/RIPR-SPEC-0370.md` row, and give
  `fixtures/*-verdict-corpus/spec-scopes/RIPR-SPEC-0370.toml` its scope.
- The producer advances the seam cache schema generation because probe and
  seam production change. This spec-and-proof slice does not.

## Problem

```rust
pub fn sum_to(n: u32) -> u32 {
    let mut acc = 0;
    for i in 1..n {              // edited from `1..=n`
        acc += i;
    }
    acc
}

#[test]
fn zero_sums_to_zero() { assert_eq!(sum_to(0), 0); }
```

`1..=0` and `1..0` are both empty, so the only test cannot notice the change.
The parser emits no probe shape for a `RangeExpr` in a `for` header
(`probe_shapes.rs` collects `if`/`while` conditions, predicate `BinExpr`s,
returns, calls, record fields and match arms) and the repository seam
inventory has no seam there, so `check` reads `static_unknown`
(`static_probe_unknown`) and `repair` offers no start. Both are accurate.

The tempting repair is wrong. Treating the header as a return-value or
boundary seam would let `assert_eq!(sum_to(0), 0)` grade the change as
checked, a false `exposed`, because nothing evaluates that `n = 0` makes both
ranges empty. Declining to start also contradicts a test that does notice:
`assert_eq!(sum_to(5), 15)` fails under `1..n` (10 instead of 15).

## Behavior

### The shape

A changed `for` header has the **range-bound shape** when the owner has the
RIPR-SPEC-0360 accumulator shape except that the walked expression `P` is a
range `A..B` or `A..=B` where each of `A` and `B` is a decimal integer
literal or the name of one integer parameter of the owner (used nowhere else
in the body), and the changed text differs from the removed text only in the
range operator (`..` vs `..=`) or in one bound. Any other owner keeps its
existing reading. The loop body is the single accumulator update, so the
accumulator is the owner's returned tail.

### Probe and flow

A changed range bound in an owner with the range-bound shape is a
`return_value` probe on the `for` line; its flow sink is the returned tail
`acc`, with the propagation edge established by the shape itself.

### Credit

The producer evaluates the **old range** (from the removed line) and the
**new range** (from the added line) for each exact pin's argument. It credits
the change (`exposed`) only when all of these hold:

- the standard owner-return pin gates hold (a direct call, one plain
  `assert_eq!` whose other operand does not call the owner, no
  `should_panic`, eager execution);
- the argument of the pinned call is a decimal integer literal (or constant
  table row) for every range parameter;
- the old and new ranges iterate a different number of times for that
  argument, and the folded old and new accumulator values differ;
- the pinned expected value is a decimal integer literal that equals the
  fold of the **new** range through the update.

An exact pin whose argument leaves the two ranges equal in iteration count
(`sum_to(0)`, both empty) is **non-discriminating**: the finding is a gap
(`weakly_exposed`) whose start asks for a case with an argument for which
the ranges differ, never `exposed`. An exact pin the producer cannot read
(bound input, computed expectation, helper) is withheld as `static_unknown`
under `rust_pinned_shape_unresolved`, never a gap. A related smoke call or
relational check keeps the gap.

### Fail-closed floor (holds before the producer exists)

Until the producer lands, no pin of any argument may report a changed range
bound as `exposed`. This is the invariant an implementation must never break
in either direction: a wrong `exposed` is worse than several missed
advisories.

## Required Evidence

- The probe, flow sink and established propagation edge for the shape.
- The old and new range, the pinned argument, and both iteration counts.
- The refusal evidence for every shape outside the list above.

## Known Limits

- Bounds that are expressions, fields, method calls, `.rev()`, `.step_by(..)`
  or `.len()` keep `static_unknown`.
- A range parameter used elsewhere in the body, a non-zero accumulator start
  and bodies other than the single update are outside the shape.
- Overflow is not modelled (as in RIPR-SPEC-0360); panic counts as a kill.
- The neighbours of the update itself remain RIPR-SPEC-0360's concern.

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- General loop-count or termination reasoning.
- Changing the RIPR-SPEC-0360 accumulator credit rules.

## Acceptance Examples

1. `for i in 1..n { acc += i; }` from `1..=n`, pinned only by
   `assert_eq!(sum_to(0), 0)`, is never `exposed`: both ranges are empty. With
   the producer it is a gap asking for an argument of at least 1.
2. The same owner pinned by `assert_eq!(sum_to(5), 15)` is `exposed`: the old
   range sums to 15, the new to 10.
3. The same owner pinned on a bound input or against a computed expectation
   is `static_unknown` under `rust_pinned_shape_unresolved`, never a gap.
4. A range whose bound is an expression, method call or `.rev()` keeps its
   `static_unknown` probe.

## Test Mapping

- `crates/ripr/tests/loop_range_bound.rs`: the fail-closed floor tests run
  today; the producer tests are `#[ignore]` until the producer lands and must
  be un-ignored by it.

## Implementation Mapping

Planned, not part of the spec-and-proof slice:

- `crates/ripr/src/analysis/syntax/loop_accumulator.rs`: range-bound shape
  reader.
- `crates/ripr/src/analysis/syntax/ra/probe_shapes.rs`: the probe.
- `crates/ripr/src/analysis/classify/owner_pin.rs`: old-versus-new range
  evaluation per exact pin.
- `crates/ripr/src/analysis/seam_cache.rs`: schema generation bump.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
