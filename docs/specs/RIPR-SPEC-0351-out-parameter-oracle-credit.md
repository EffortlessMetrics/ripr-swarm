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

1. The changed method has a receiver and a `&mut` parameter that its body
   uses only through the allowlist in Known Limits (top-level additive writes,
   a leading `clear()`, read-only calls) and never rebinds or shadows.
2. One operand of the assertion is a bare local. The other operand does not
   mention it.
3. In the same test body, that local is bound exactly once by
   `let mut local = <recognized empty constructor>;` (`String::new()`,
   `Vec::new()`, `default()`/`with_capacity(..)` of those and the std maps
   and sets; no `let ... else`), and no statement between that binding and
   the call mentions it.
4. The next mention is a top-level statement that calls the method, possibly
   through `?`, `.unwrap()` or `.expect(..)`, passing exactly `&mut local` at
   an out-parameter position, with the local named once in the statement.
5. The call and the assertion are top-level statements of the test, in that
   order, and the call's receiver resolves to the owner's impl type by the
   existing relation-layer receiver rules. No new receiver type inference is
   added.
6. When the changed expression is itself an unconditional additive write
   through that parameter (the same allowlist as item 1), the assertion also
   confirms observation;
   otherwise it is only a candidate oracle.

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

- Credit needs a clearly non-empty expected operand: a non-empty string or
  byte-string literal, a non-zero integer literal, such a literal with
  `.to_string()`/`.to_owned()`/`.into()`/`.to_vec()`, `String::from(lit)`, or
  a non-empty `vec![..]`. Every empty spelling, every local and every other
  expression is refused, as is an operand equal to the local's initializer.
  Other expected values are not evaluated for correctness: like every
  exact-value oracle, the helper decides that a discriminator exists, not
  that the test passes.
- The owner is accepted by an allowlist, not a blocklist. Every use of the
  out-parameter in the owner body must be a top-level additive write
  statement (`push`, `push_str`, `insert`, `extend`, `append`, `write_*`, or
  a std `write!`/`writeln!`), a `clear()` before the first write, or a
  read-only method call (`len`, `is_empty`, ...) on the bare identifier. Any
  other use (passing it onward, reborrowing, aliasing, a deref or
  parenthesized receiver, closure capture, an unknown method such as
  `clone_from` or `split_off`, any other macro) refuses, as does a write with
  only empty arguments.
- The writes must be straight-line top-level statements with no `return`,
  `break`, `continue` or `?` before the last one. A write inside an
  `if`/`match` arm, loop or closure refuses, because the test input may not
  execute it. The changed expression must itself be such a write with no
  branch, loop, closure or early exit inside it, so a changed token on an
  unexercised conditional side is not credited.
- `write!`/`writeln!` resolve by exact std path; a workspace file that
  defines a macro whose name starts with `write` refuses every owner that
  mentions `write`. Macros imported from other crates are not inspected.
- A write after an early `return` in a callee branch the owner scan cannot
  see (a helper the owner calls) is outside this check, and two same-named
  callees under `#[cfg(test)]` and `#[cfg(not(test))]` are not distinguished.
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
- `fixtures/out_parameter_unrelated_write_no_promote` and
  `fixtures/out_parameter_empty_expected_no_promote`, pinned as
  RIPR-SPEC-0108 non-promotion cases in
  `fixtures/evidence-promotion-honesty-corpus/corpus.json`.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/out_parameter.rs`:
  `assertion_out_parameter`.
- `crates/ripr/src/analysis/classify/reveal.rs`: `analyze_related_assertions`
  consumes it through `ReturnOracleAdmission::out_parameter_read`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
