# RIPR-SPEC-0351: A test that asserts on the local it passed as `&mut` observes the changed method

Status: proposed

Owner: product / analysis

Created: 2026-10-23

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8999

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory; this only stops one
  call shape from being reported as unobserved.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No output schema change.

## Problem

`OffsetFormat { .. }.format(&mut s, off).unwrap(); assert_eq!(s, "+09:30");`
passes a local to the changed method as `&mut` and asserts on that local. The
assertion never names the method, so lexical matching left the builder
statement as the only apparent oracle and the finding stayed
`reachable_unrevealed` even though the test reads exactly what the method
wrote.

## Behavior

An `assert_eq!` is admitted as an oracle for a direct call of the changed
method when every link is syntactic and exact:

1. The changed method has a receiver and a `&mut` parameter its body visibly writes (a mutating
   method on it, `write!`/`writeln!` into it, or an assignment through a
   dereference, field or index) and never rebinds, shadows or reassigns.
2. One operand of the assertion is a bare local. The other operand does not
   mention it.
3. In the same test body, that local is bound exactly once by
   `let mut local = <call or record literal>;` (no borrow in the
   initializer, no `let ... else`), and no statement between that binding and
   the call mentions it.
4. The next mention is a top-level statement that calls the method, possibly
   through `?`, `.unwrap()` or `.expect(..)`, passing exactly `&mut local` at
   an out-parameter position, with the local named once in the statement.
5. The call and the assertion are top-level statements of the test, in that
   order, and the call's receiver resolves to the owner's impl type by the
   existing relation-layer receiver rules. No new receiver type inference is
   added.
6. When the changed line itself names that parameter, the assertion also
   confirms observation of the change; otherwise it is only a candidate
   oracle.

Anything else (a rebinding or shadow, a second pass of the local, a use
inside a loop, closure or macro, an assertion before the call, a call in
another test, a different or longer-named local, a `should_panic` test, an
unresolved receiver) stays uncredited. The helper decides only that the
assertion reads the call's out-parameter; it grants no strength or reach.

## Required Evidence

- Positive: asserting the local passed `&mut` exposes the changed line.
- Negatives: another local, a whole-word near miss, a non-local assertion
  operand, an assertion before the call, a rebinding, an intervening write,
  a second pass and a call in another test are each not credited.

## Known Limits

- An expected value equal to the local's initial state or an empty literal
  is not credited (it cannot notice a write). Other expected values are not
  evaluated: like every exact-value oracle, the helper decides that a
  discriminator exists, not that the test passes.

- A parameter that is only read, or only passed onward to a helper, is not an
  out-parameter here; the writer list is fixed and conservative.

- Only `assert_eq!` and only a bare local operand are recognised.
- Out-parameters reached through fields, helper wrappers or returned
  references are not followed.

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- Receiver type inference beyond the existing relation-layer rules.

## Acceptance Examples

1. A test builds `let mut s = String::new();`, calls
   `OffsetFormat.format(&mut s, 570);` and asserts `assert_eq!(s, "+570");`;
   the changed push line is `exposed`.
2. The same test asserting a different local, `s2`, `s.len()`, or asserting
   before the call is not `exposed`.
3. A rebinding of `s` after the call, a write to `s` before the call, or a
   second pass of `&mut s` is not `exposed`.

## Test Mapping

- `crates/ripr/tests/out_parameter_oracle.rs`.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/out_parameter.rs`:
  `assertion_out_parameter`.
- `crates/ripr/src/analysis/classify/reveal.rs`: `analyze_related_assertions`
  consumes it through `ReturnOracleAdmission::out_parameter_read`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
