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
- #9478

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

- its body is exactly, in this order, `let mut acc = 0;` (an integer zero,
  optionally suffixed, with no type annotation), one `for x in P { .. }` (or
  `for &x in P`, or `P.iter()`) and the bare tail `acc`. Nothing runs before
  the initialization, so nothing can shadow or rebind the walked parameter;
- `P` is a parameter whose type is a built-in integer slice (`&[T]`,
  `&mut [T]`, `Vec<T>`, `&Vec<T>`, with `T` one of `i8`..`i128`, `isize`,
  `u8`..`u128`, `usize`) and its name appears only in its declaration and the
  loop;
- the loop body is one statement, the update: `acc += term`, `acc -= term`,
  `acc = acc + term` or `acc = acc - term`, on one line;
- `term` is the loop variable, `*variable`, an integer literal, or a **call
  term** (below). The elements are a built-in integer type, so the update is
  the primitive operator. A field or a second name could carry a user
  `AddAssign`/`SubAssign` impl (`impl AddAssign<&W> for i32`), and an
  operator inside a plain term has mutants (`w * w` to `w + w`) that a pin of
  the total does not separate;
- `acc` is named exactly by its `let`, the update and the tail, and the body
  holds no other loop, `return`, `break`, `continue`, `?`, macro, closure,
  `if`, `match` or `unsafe`, and no `.await` except a call term's own. An
  `async` owner has the shape only through a call term.

Any other owner keeps its existing reading.

### The call term

A **call term** is `callee(x)` or `callee(*x)` on the loop variable, with an
optional `.await`, and optionally one `+`, `-`, `*`, `/` or `%` with a decimal
integer literal on its right (`double(*x).await * 1`). `callee` is one bare
name, not a path, method, closure or macro; it is neither the accumulator,
the walked parameter, the loop variable nor the owner itself. The call's
value is read by evaluating the callee, never assumed, so the credit below
applies only when ripr can read it:

- exactly one function of that name exists in the workspace, a free function
  in the owner's file (an `impl` method of the same name is not it), and the
  name appears in the owner once, as the call target (a parameter or binding
  of that name would shadow the function);
- it has no generics, is not `unsafe` or `const`, takes one plain identifier
  parameter (no `mut`, `ref` or `self`) of the same built-in integer type it
  returns, which is the owner's return type and at most 64 bits, and its body
  is one expression of that parameter, decimal integer literals, parentheses,
  unary `-` and `+ - * / %`, with every step in the type's range (a cast, a
  call, a statement or a second use of an unreadable name is not read).

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

For a call term the pinned run is folded through the callee instead, and the
update is credited only when all of these hold for some run: the elements and
the expected value are decimal integer literals (a table row or a
test-local-helper run is read the same way); the fold of the callee's values
through the written update gives the pinned nonzero value; and every
neighbouring mutant gives a different value or panics. The neighbours are
folded exactly: the opposite sign, `|=`, `^=` and overwriting (`acc = term`,
the last element), and, for a term with an operator, each other of
`+ - * / %` in its place. `*= /= %= &=` stay zero from the zero start, which
differs from a nonzero pin. A mutant that is the same function for every input
(`* 1` and `/ 1`, `+ 0` and `- 0`) is equivalent, not a gap. A run in which a
neighbour coincides with the written update (a one-element run of a `+=`,
where overwriting gives the same value; `id(x) * 2` and `id(x) + 2` at
`[2, 2]`; `2 | 4` beside `2 + 4`) and a fold that does not give the pinned
value keep their withheld verdict.

A call term is also never reported as a gap. Its value is a function ripr may
not read, so a related direct-call test that does not credit it (a smoke call,
a relational check, an unreadable pin) is withheld as `static_unknown` under
`rust_pinned_shape_unresolved`; ripr cannot say the update's neighbours are
distinguishable at all, as in `fn w(x: u32) -> u32 { 0 }`.

Starting from zero, a nonzero total separates the update from its neighbours:
a flipped sign gives the opposite total, a product, quotient, remainder and
`&` give zero, and a deleted statement gives zero. `|=` and `^=` are not
separated for a one-element input; no mutation tool produces them from `+=`
or `-=`, and the residual is documented here rather than hidden.

### Not a gap when the pin cannot be read

A related direct-call test that pins the owner's whole result with an exact
`assert_eq!` on a non-degenerate input (not an empty collection, and not
compared with a zero literal; a zero element among others is not degenerate), where the credit gates above do not
hold (a bound input, a computed expectation, a value the helper computes),
withholds the gap: the finding is `static_unknown` with
`rust_pinned_shape_unresolved`, because ripr cannot read that the pinned input
walks the loop and totals to a nonzero value. An empty pin, or a pin whose expected
value is a zero literal (an all-zero or cancelling run), is
degenerate and keeps its gap: it cannot separate the update. A test with no
exact pin (a smoke call, a relational check) keeps its gap.

## Required Evidence

- The probe, flow sink and established propagation edge for the shape.
- The nonzero literal pin and the non-empty literal input at the pinned call.
- The refusal evidence for every shape outside the list above.

## Known Limits

- A call outside the call term above (`count += line.split_whitespace().count()`,
  a method, a path, a second argument, an operator on both sides) and a
  nonzero start keep their `static_unknown` probe. Crediting them is future
  work.
- A call term's mutants inside the callee (`x * 2` to `x + 2` in `double`) are
  changes to the callee's own line, not to the update. The update's neighbours
  are the ones listed above; ripr does not model mutants that replace the call
  with a constant.
- A table-driven pin is read only through a constant-row table
  (`for (items, want) in [(&[3, 4], 7), ..]`, inline or a typed `let`) whose
  loop body cannot leave a row early. A table of all-empty or all-zero rows,
  or one ripr cannot read (a computed or short row, a rebound name, a skip
  guard), is withheld as `rust_pinned_shape_unresolved`; ripr does not claim
  the gap through a loop.
- Panic counts as a kill, as for the plain accumulator: the fold assumes
  overflow checks, so a profile with `overflow-checks = false` can let a
  wrapping mutant survive a pin.
- A release build can wrap an unsigned `-=` back to the pinned value for a
  single large element; debug builds panic. ripr does not model overflow.
- Only `+`/`-` updates qualify; `*=` from zero is constant zero.
- Unsigned subtraction that underflows panics in debug builds; ripr does not
  model overflow.
- Types with user `AddAssign` impls cannot appear: the elements are a
  built-in integer type and the term is the loop variable, `*variable` or an
  integer literal.
- A term edit that coincides with the old term on the pinned input (`w * w`
  to `w + w` at `[2]`) is outside the shape: compound terms keep their
  `static_unknown` probe.

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
5. An accumulator whose term has an operator (`w * w`, which equals `w + w`
   at 2) or calls a function keeps its `static_unknown` verdict.
6. The same owner pinned through a constant-row table loop is `exposed` when
   some row pairs a non-empty literal input with a nonzero integer, even
   beside an empty or zero row. A table whose rows are all empty or zero, or
   that ripr cannot read, is `static_unknown` under
   `rust_pinned_shape_unresolved`, never a gap.

7. `async fn sum_doubled(xs: &[u32]) -> u32 { let mut t = 0; for x in xs {
   t += double(*x).await * 1; } t }` with `async fn double(x: u32) -> u32 { x
   * 2 }` pinned by `assert_eq!(block_on(sum_doubled(&[1, 2, 3])), 12)` is
   `exposed`: the callee folds the run to 12 and `+ 1`, overwriting, `|=`,
   `^=` and every other neighbour give a different value.
8. The same owner pinned on a run the callee does not fold to the expected
   value, on one element, or on a run where two neighbours coincide, with a
   callee that has a statement body, a call, a cast, a different return type,
   generics, a `mut` parameter or a second definition, or a callee name that a
   parameter shadows, is `static_unknown` (`rust_pinned_shape_unresolved`; an
   `async` owner keeps its executor-helper limit), never `exposed` and never a
   gap. A smoke call of a call term is `static_unknown`, never a gap.
9. A call term outside the list above (a method, a path, a second argument, a
   computed argument, a non-literal or hexadecimal operand, a second operator)
   keeps its `static_unknown` probe.

## Test Mapping

- `crates/ripr/tests/loop_accumulator_pin.rs`.
- `crates/ripr/tests/loop_accumulator_call_term.rs`.
- `crates/ripr/src/analysis/syntax/loop_accumulator.rs` unit tests.
- Rust verdict corpus cases `grid-loop-*`, `loop-accumulator-*`,
  `shape-async-await-accumulate` and `trap-loop-accumulator-*`, including the table pair
  `loop-accumulator-table-pin` and `trap-loop-accumulator-table-zero-rows`.

## Implementation Mapping

- `crates/ripr/src/analysis/syntax/loop_accumulator.rs`: the shape reader.
- `crates/ripr/src/analysis/syntax/ra/probe_shapes.rs`: the probe.
- `crates/ripr/src/analysis/classify/flow.rs`: the returned-local sink.
- `crates/ripr/src/analysis/classify/propagation_witness.rs`:
  `accumulates_into_returned_local`.
- `crates/ripr/src/analysis/classify/owner_pin.rs` and
  `owner_pin/closure_adaptor.rs`: `ReturnPathGate::LoopAccumulator`,
  `pinned_run_sums_to_nonzero`, `pinned_run_accepts`.
- `crates/ripr/src/analysis/classify/owner_pin/call_term_fold.rs`: the call
  term's callee evaluation and neighbour folds.
- `crates/ripr/src/analysis/classify/pinned_shape.rs`: the withheld gap (and
  the call term's no-gap rule).

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
