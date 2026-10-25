# RIPR-SPEC-0360: A changed update of a zero-initialized loop accumulator the owner returns is a return-value change, credited only by a pin that walks the loop

Status: proposed

Owner: product / analysis

Created: 2026-10-25

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6485

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory; a changed accumulator
  update gets a typed probe and a bounded credit instead of `static_unknown`.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No output schema change. The `rust_pinned_shape_unresolved` limit
  (RIPR-SPEC-0240) gains one owner shape.
- Cache schema generations advance because probe production changes.

## Problem

```rust
pub fn total_weight(items: &[u32]) -> u32 {
    let mut sum = 0;
    for w in items {
        sum += w;                // edited from `sum = sum + w;`
    }
    sum
}

#[test]
fn three_parcels_weigh_twelve() { assert_eq!(total_weight(&[3, 4, 5]), 12); }
```

Deleting the statement, `-=` and `*=` all fail the exact pin in a real run.
ripr reported `static_unknown` ("Changed syntax is not mapped to a
high-confidence probe family"): the parser has no shape for an arithmetic
update of a local, and the lexical fallback treats it as an accumulator whose
call (if any) is consumed (RIPR-SPEC-0228). Abstaining is allowed by the
truth table, but it means ripr says nothing on every counter and sum, which
are the most common statement mutants. RIPR-SPEC-0228 left crediting an
accumulator through an exact owner-return assertion as future work because it
needs loop-count reasoning.

## Behavior

### The shape

An owner has the **accumulator shape** when all of these hold, read from the
parsed function body:

- its body is `let mut acc = 0;` (an integer zero, optionally suffixed, with
  no type annotation), then one `for x in P { .. }` (or `P.iter()`) where `P`
  is a parameter of a slice-like type (`&[T]`, `&mut [T]`, `Vec<T>`, `&Vec<T>`),
  then the bare tail `acc`;
- the loop body is one statement, the update: `acc += term`, `acc -= term`,
  `acc = acc + term` or `acc = acc - term`, on one line;
- `term` is built only from names, literals, field and index reads, `*`, `&`
  and `+ - *`: no call, method call, macro, closure, `?`, `.await`, block,
  branch or loop;
- `acc` is named exactly by its `let`, the update and the tail, and the body
  holds no other loop, `return`, `break`, `continue`, `?`, macro, closure,
  `if`, `match`, `async` or `unsafe`.

Any other owner keeps its existing reading.

### Probe and flow

A changed update in an owner with the accumulator shape is a `return_value`
probe, and its flow sink is the owner's returned tail `acc` on the update's
line. The propagation edge from the update to the tail is established by the
shape itself (`acc` flows to the return unchanged by anything else).

### Credit

An exact pin credits the update (`exposed`) only when all of these hold:

- the standard owner-return pin gates hold (a direct call, one plain
  `assert_eq!` whose other operand does not call the owner, no `should_panic`,
  eager execution);
- the pinned call passes the walked parameter a non-empty literal sequence, and
  the pinned value is a nonzero integer literal. For an assertion borrowed
  from a test-local check helper, some eager call of the helper must pass a
  non-empty literal sequence and a nonzero integer literal for the helper
  parameter the assertion compares.

Starting from zero, a nonzero total separates the update from its neighbours:
a flipped sign gives the opposite total, a product, quotient, remainder and
`&` give zero, and a deleted statement gives zero. `|=` and `^=` are not
separated for a one-element input; no mutation tool produces them from `+=`
or `-=`, and the residual is documented here rather than hidden.

### Not a gap when the pin cannot be read

A related direct-call test that pins the owner's whole result with an exact
`assert_eq!` on a non-degenerate input, where the credit gates above do not
hold (a bound input, a computed expectation, a value the helper computes),
withholds the gap: the finding is `static_unknown` with
`rust_pinned_shape_unresolved`, because ripr cannot read that the pinned input
walks the loop and totals to a nonzero value. An empty or zero pin is
degenerate and keeps its gap: it cannot separate the update. A test with no
exact pin (a smoke call, a relational check) keeps its gap.

## Required Evidence

- The probe, flow sink and established propagation edge for the shape.
- The nonzero literal pin and the non-empty literal input at the pinned call.
- The refusal evidence for every shape outside the list above.

## Known Limits

- A table-driven pin (`for (items, want) in cases { .. }`), a call in the term
  (`count += line.split_whitespace().count()`) and a nonzero start keep their
  `static_unknown` probe. Crediting them is future work.
- Only `+`/`-` updates qualify; `*=` from zero is constant zero.
- Unsigned subtraction that underflows panics in debug builds; ripr does not
  model overflow.
- Types with user `AddAssign` impls cannot appear: the zero start is an
  unsuffixed or built-in-suffixed integer literal.

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- Loop-count reasoning beyond a non-empty literal input.
- Accumulators into fields, derefs or indexes (RIPR-SPEC-0228).

## Acceptance Examples

1. `let mut sum = 0; for w in items { sum += w; } sum` pinned by
   `assert_eq!(total_weight(&[3, 4, 5]), 12)` is `exposed`, including the
   subtracting form with a negative pin, a `.iter()` walk, and a helper that
   pins a literal run.
2. The same owner pinned only on an empty input (`&[]`, `0`) or on all-zero
   inputs with a zero total is a gap: the pin cannot separate the update.
3. The same owner pinned exactly on a bound input (`let rolls = vec![..];
   total(&rolls)`) or against a computed expectation is `static_unknown`
   under `rust_pinned_shape_unresolved`, never a gap.
4. The same owner reached by a smoke call or a relational check
   (`>= before`) is a gap.
5. An accumulator whose term calls a function, or that is driven by a
   table-driven loop in the test, keeps its `static_unknown` verdict.

## Test Mapping

- `crates/ripr/tests/loop_accumulator_pin.rs`.
- `crates/ripr/src/analysis/syntax/loop_accumulator.rs` unit tests.
- Rust verdict corpus cases `grid-loop-*`, `loop-accumulator-*` and
  `trap-loop-accumulator-*`.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/loop_accumulator.rs`: the shape reader.
- `crates/ripr/src/analysis/syntax/ra/probe_shapes.rs`: the probe.
- `crates/ripr/src/analysis/classify/flow.rs`: the returned-local sink.
- `crates/ripr/src/analysis/classify/propagation_witness.rs`:
  `accumulates_into_returned_local`.
- `crates/ripr/src/analysis/classify/owner_pin.rs` and
  `owner_pin/closure_adaptor.rs`: `ReturnPathGate::LoopAccumulator`,
  `pinned_run_sums_to_nonzero`.
- `crates/ripr/src/analysis/classify/pinned_shape.rs`: the withheld gap.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
