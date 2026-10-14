# RIPR-SPEC-0260: Rust closures in iterator adaptors

Status: proposed

Owner: product / analysis

Created: 2026-10-14

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6502 (closure in an iterator adaptor reads as a gap)

Linked PRs:

- #7914

Support-tier impact:

- No tier change. The rules read one more owner shape on the existing
  Rust boundary, owner-return pin and propagation paths.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class or relation
  reason. The propagation witness gains one limitation label,
  `iterator_adaptor_owner_tail`, beside `constant_owner_tail`; no static
  limitation kind is added.
- Amends RIPR-SPEC-0231's owner-return pin (a closure in the tail no longer
  refuses it on this shape) and the RIPR-SPEC-0027 boundary reading of a
  closure binding.

## Problem

Three Rust verdict-corpus rows score `false_actionable`
(`grid-iter-exact`, `grid-iter-helper`, `grid-iter-table`). Each owner is
one closure in an iterator adaptor over a slice parameter, and real
mutants of the closure fail the tests:

```rust
pub fn count_positive(xs: &[i32]) -> usize {
    xs.iter().filter(|&&x| x > 0).count()
}

#[test]
fn counts_two_positive() {
    assert_eq!(count_positive(&[-1, 0, 3, 7]), 2);
}
```

ripr read the predicate `x > 0` as `infection_unknown` (the binding `x` is
"a local or computed value ripr cannot map to the related tests' inputs")
and the return value as `weakly_exposed`: the owner-return pin refuses any
tail with a closure, since a pin on some input may never run it, and the
propagation witness treats closure syntax as an opaque path.

The closure runs once per element of the parameter. When the test passes a
literal sequence, the elements are the values the binding takes, and a
non-empty sequence runs the closure.

## Behavior

### Scope: the admitted adaptor tail

An owner is in scope when its whole body is one tail expression

```text
P.iter().A(|PAT| E)            or            P.iter().A(|PAT| E).T()
```

and every condition below holds. Anything else keeps the pre-0260 reading.

- The body holds no statement, no `return`, no `?`, and no macro outside
  the non-returning list the owner-return pin already trusts. The owner is
  not `async` or `const`.
- `P` is a plain identifier parameter (no `ref`, `mut` or sub-pattern) of
  a slice-like type: `&[T]`, `&mut [T]`, `&'a [T]`, `[T; N]`, `&Vec<T>`
  or `Vec<T>`. Its `iter` is the slice's own, called with no arguments
  and no turbofish.
- `A` is `filter`, `map`, `any` or `all`, with no turbofish and exactly
  one argument, a closure.
- `A` and `T` form one of these pairs, each of which keeps every closure
  value in the owner's result, so a changed closure value changes what the
  owner returns:
  - `filter` + `count()`;
  - `filter` or `map` + `collect()` into a `Vec`: a `collect::<Vec<..>>()`
    turbofish, or no turbofish and an owner return type spelled `Vec<..>`;
  - `map` + `sum()` (a `sum::<u64>()` turbofish is allowed) over an
    arithmetic `E`: no comparison, `!`, `true`, `false` or `as` cast;
  - `any` or `all` with no `T`.

  Every other pair is refused (fail closed): `map` + `count` ignores the
  closure value; `filter` + `sum` cannot tell a kept additive identity
  (`x > 0` on `0`) from a dropped one; `collect` into anything not provably
  a `Vec` (a `HashSet`, a `BTreeSet`, an alias, `_`) may lose order and
  multiplicity; `map` + `sum` over a comparison or cast folds `bool`s into a
  count; a bare `filter` or `map` returns a lazy iterator; a terminal with
  arguments, or any other terminal, is not read.
- The closure has one untyped parameter whose pattern is `x`, `&x` or
  `&&x` (no `mut`, `ref`, or destructuring), at most as many `&` as the
  adaptor hands (`filter` gives `&&T`, the others `&T`), and is not
  `move`, `async` or `static` and declares no return type.
- `E` evaluates every part on every call: it is not a block and holds no
  closure, `||`, `&&`, branch or loop keyword, `Option`/`Result`
  combinator, `?`, macro, `return`, `.await`, `async`, `yield` or
  `unsafe`. A bitwise `|` or `&` is allowed: it evaluates both operands.
  Every path in `E` is the binding or an `UPPER_CASE` constant, so the
  closure captures nothing else.

The binding read as an element of `P` is the binding with at most as many
`*` as the adaptor's reference layers minus the pattern's `&` (`x` for
`|&&x|` under `filter`, `**x` for `|x|`).

A **literal sequence** is an argument `&[a, b]`, `[a, b]`, `vec![a, b]` or
`&vec![a, b]` whose every element is one unsuffixed integer or float
literal (optionally negated), `true`, `false`, or a char literal. `&[]`
is the empty literal sequence. A local (`&v`), a repeat (`vec![v; n]`), a
range, a call, a suffixed element (`3u32`), a string or a slice of a
literal (`&[1, 2][..]`) is not one.

A **run** of an owner call is the sequence the call passes `P`: the
argument itself when it is a literal sequence, or, when the argument is a
column of a RIPR-SPEC-0186 constant-row `for` table, one run per row whose
cell is a literal sequence. Any row that is not one makes the call
unreadable.

### Rule 1: the closure binding is an element of the parameter

A changed `predicate` probe whose expression is exactly `E` (whitespace
aside), with one operand the binding read as an element of `P`, reads the
other operand as the boundary. Its inputs are the elements of every run of
every related call:

- direct owner calls in related tests; or, when there is none, the calls
  of the resolved RIPR-SPEC-0159 helper chain's entry, where each hop
  passes its caller's own parameter for `P` unchanged (no macro hop, no
  rebinding, no shadowed entry call).

When some related call is unreadable, or none exists, the rule does not
apply and the pre-0260 reading stands: the binding is an operand ripr
cannot map, so infection stays unknown.

With every run read, the boundary's value must be visible (a literal or a
same-file constant declared once); otherwise infection is unresolved and
names the boundary. A run **reaches the boundary** when some element
equals it and, for `any` and `all`, the closure's current value on every
other element of the run is the non-deciding one (`false` for `any`,
`true` for `all`), so the boundary element decides the result. An element
the comparison cannot be evaluated on counts as deciding. When some run
reaches the boundary, `x == b` is an observed value and infection reads
yes. Otherwise the missing discriminator `x == b` names the observed
element values (or says every call passes an empty sequence, so the
closure never runs), and the finding stays a gap.

The RIPR-SPEC-0186 same-test pairing reads the same runs: an admitted
discriminating or owner-pinned assertion pairs with the boundary when its
owner call's argument for `P` has a run that reaches the boundary, read
directly, through a table column, or, for an assertion borrowed from a
test-local check helper, at each eager call of the helper in the test.
Pairing reads only a literal boundary: a constant boundary (`x > LIMIT`)
may be observed by activation but is never paired (fail closed).

### Rule 2: the owner-return pin through the adaptor

A changed `return_value` probe whose expression is the whole admitted tail
establishes the owner-return pin's adaptor gate. A pinned assertion that
passes every other RIPR-SPEC-0231 pin gate is admitted only when the
pinned owner call evaluates the closure at least once: its argument for
`P` is read as runs (directly, through a constant-row table column, or,
for a borrowed helper assertion, at the helper's eager calls in the test),
every run is readable, and at least one run is non-empty. An empty literal,
or any argument ripr cannot read, refuses the pin.

### Rule 3: propagation through the adaptor

The admitted tail is the owner's whole body, so its value is what the
owner returns:

- a `return_value` probe that is the whole tail completes a
  `DirectReturn` witness on the tail's line, as a constant owner tail
  does, with limitation `iterator_adaptor_owner_tail`;
- a `predicate` probe that is `E` flows to a `ReturnValue` sink at the
  tail.

Whether a test's input runs the closure is rules 1 and 2's question, not
this edge's.

### Decisions

1. **Literal sequences only.** Adopted: an element is read only from a
   literal sequence spelled at the call or in a constant-row table.
   Rejected: following `let v = vec![..]` bindings, which needs the
   mutation and aliasing scans the table reader already performs for its
   own shape.
2. **Short circuits are evaluated, not assumed.** Adopted: for `any` and
   `all`, a boundary element counts only when every other element's
   current value cannot decide the result. Rejected: counting every
   listed element, which credits `any(|&d| d > 30)` tested with
   `&[31, 30]`, where the walk stops at `31` and `d >= 30` survives.
3. **Every related call must be readable.** Adopted: one unreadable call
   keeps the boundary unresolved, since it may sit on the boundary.
4. **Only consumers that keep every closure value.** Adopted: an explicit
   allow-list of adaptor/terminal pairs. Rejected: admitting any listed
   adaptor with any listed terminal, which credits `map(|&x| x > 0).count()`
   (the count ignores every closure value), `filter(|&&x| x > 0).sum()` (a
   kept or dropped `0` sums alike) and a `collect` into a set (`{false,
   true}` for both `>` and `>=`).

## Required Evidence

- Unit tests pin the scope, literal sequences, the pin's non-empty rule,
  the element boundary and the short-circuit rule.
- On the Rust verdict corpus, `grid-iter-exact`, `grid-iter-helper` and
  `grid-iter-table` move from `false_actionable` to `ideal`;
  `grid-iter-property` and `grid-iter-none` do not move; no row becomes
  `false_exposed`.

## Non-Goals

- Other adaptors and consumers: `fold`, `flat_map`, `filter_map`, `find`,
  `position`, `take_while`, `max_by_key` and the rest keep the pre-0260
  reading, as do the refused pairs (`map` + `count`, `filter` + `sum`,
  `map` + `sum` over a comparison or cast, and `collect` into anything not
  provably a `Vec`).
- Closures that capture another local or parameter, closures with a block
  or conditional body, and nested closures.
- Receivers other than an owner parameter (`self.items.iter()`,
  `xs.to_vec().iter()`), `into_iter`, `iter_mut`, and owners with any
  statement before the tail.
- Equivalence judgment: a run that reaches the boundary is an observed
  input, not a claim that every boundary mutant is killed.

## Acceptance Examples

1. `xs.iter().filter(|&&x| x > 0).count()` with
   `assert_eq!(count_positive(&[-1, 0, 3, 7]), 2)`: the return value reads
   `exposed`; the predicate observes `x == 0` and reads `exposed`.
2. `days.iter().any(|&d| d > 30)` checked through a test-local helper
   `check_overdue(days, want)` asserting `assert_eq!(any_overdue(days),
   want)`, called with `(&[31], true)` and `(&[30, 2], false)`: the return
   value and the predicate read `exposed` (`30` decides the second run).
3. `xs.iter().map(|&x| x * 2).collect()` tested by a constant-row table
   `[(&[], vec![]), (&[1], vec![2]), (&[3, 5], vec![6, 10])]`: the return
   value reads `exposed`.
4. As example 1 with only `assert_eq!(count_positive(&[3, 7]), 2)`: the
   return value reads `exposed`, the predicate `weakly_exposed` with the
   missing discriminator `x == 0` (observed `3, 7`).
5. As example 1 with only `assert_eq!(count_large(&[]), 0)` on
   `filter(|&&x| x > 100)`: the pin refuses the empty slice, so the return
   value reads `weakly_exposed`; the predicate reads `weakly_exposed` with
   the missing discriminator `x == 100` (the closure never runs).
6. `ages.iter().filter(|&&a| a >= 18).count()` tested only through
   `let ages = vec![40; n]` in a `for n in 1..10` loop: the rule does not
   apply; the predicate stays `infection_unknown`.

## Test Mapping

- `crates/ripr/src/analysis/classify/owner_pin/closure_adaptor.rs`: the
  scope, the admitted adaptor/terminal pairs, literal sequences,
  helper-call runs and the short-circuit run rule.
- `crates/ripr/src/analysis/classify/owner_pin/tests.rs`: rule 2 through a
  direct call and a constant-row table, and the owner-side refusals.
- `crates/ripr/src/analysis/classify/activation.rs` tests: rule 1's
  observed, missing, short-circuit and unreadable readings.
- `crates/ripr/src/analysis/classify/boundary_pairing.rs` tests: rule 1's
  same-test pairing, including the constant-boundary refusal.
- `crates/ripr/tests/iterator_adaptor_propagation.rs`: rule 3 through
  `check_workspace`, and the `map` + `count` refusal.
- `fixtures/rust-verdict-corpus`: examples 1 to 3 (`grid-iter-exact`,
  `grid-iter-helper`, `grid-iter-table`), 4 (`trap-iter-boundary-missing`),
  5 (`trap-iter-empty-slice`) and 6 (`grid-iter-property`).

## Implementation Mapping

- `crates/ripr/src/analysis/classify/owner_pin/closure_adaptor.rs`:
  `adaptor_tail_of`, `admitted_consumer`, `literal_sequence`, `argument_sequences`,
  `pinned_call_runs_closure`, `AdaptorTail::run_reaches_boundary`.
- `crates/ripr/src/analysis/classify/owner_pin.rs`:
  `ReturnPathGate::Adaptor` in `OwnerReturnPin::establish` and `admits`.
- `crates/ripr/src/analysis/classify/activation.rs`:
  `adaptor_element_view`, `element_boundary_discriminator`,
  `element_boundary_observations`.
- `crates/ripr/src/analysis/classify/boundary_pairing.rs`:
  `adaptor_element_boundary`, `loan_pairs_boundary_element`.
- `crates/ripr/src/analysis/classify/propagation_witness.rs`:
  `constant_tail_witness`; `crates/ripr/src/analysis/classify/flow.rs`:
  `predicate_flow_sinks`.

## Metrics

- `rust_iterator_adaptor_mismatch`: acceptance examples whose observed
  reading differs from this spec. Target: zero.
