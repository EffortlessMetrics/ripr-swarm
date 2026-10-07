# RIPR-SPEC-0186: Same-test pairing for boundary-class probes

Status: proposed

Owner: product / analysis

Created: 2026-09-29

Linked proposal:

Linked ADRs:

Linked plan:

Linked issues:

- #4828
- #5027 (shared execution admission before pairing)
- #6668 (argument must be the boundary literal, not merely contain it)
- #7004 (post-`let` mutation voids a bound boundary name)

Linked PRs:

Support-tier impact:

- Honesty fix only. Narrows a false `exposed` on predicate probes. No support-tier
  change. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- No schema version bump. The discriminate summary for the split case is text
  only (`same_test_pairing_missing`); not a new JSON field.

## Problem

A predicate probe can read `exposed` when no test discriminates the boundary.
One test calls the owner at the boundary without asserting. A different test
asserts exactly, but only far from the boundary:

```rust
pub fn gate(input: u32) -> bool { input >= 10 }

#[test] fn boundary() { let _ = gate(10); let _ = gate(9); }
#[test] fn far() { assert_eq!(gate(100), true); }
```

Infection credits "related test input at the changed boundary" from `boundary`.
Discrimination credits the strong oracle from `far`. Nothing requires those
signals to come from the same test, or from the same owner call. The mutant
`>=` → `>` still passes both tests.

#4486 covers a proximity-only test supplying the oracle. #4574 / #4715 cover
test-side helper credit. #4760 covers bare-name method relation. This spec is
only the split between a real boundary input and a real oracle on different
tests or different calls.

## Behavior

For a `predicate` probe, `exposed` requires one related test that both:

1. feeds a boundary input to the changed owner, and
2. holds an admitted discriminating oracle on that call's result.

For the bare equality subset in RIPR-SPEC-0197, pairing uses the same parser-backed
execution admission as reveal. A refused boundary equality cannot pair with an
admitted far-input oracle, within one test or across tests. Original assertion
cardinality is retained; a filtered clone cannot create a singleton fallback.

Otherwise the finding reads at most `weakly_exposed`. The discriminate stage
is `weak` and its summary names `same_test_pairing_missing`.

A control of the form `assert_eq!(gate(10), true)` stays `exposed`. A let-bound
owner call whose binding a later exact assertion observes (`let got = gate(10);
assert_eq!(got, true)`) also pairs. Pairing reuses activation's boundary `==`
facts, so a same-test exact oracle that already infected through a named
constant or helper hop stays paired. A same-test mix that calls the boundary
without asserting and asserts a far call (`let _ = gate(10); assert_eq!(gate(100),
true)`) does not pair.

An owner-call argument is a boundary input only when it is the literal itself
(including a type suffix such as `10u32`), a named local bound to that literal,
or a form infection already recorded as `==` the boundary. An expression that
merely contains the literal does not pair, including `gate(if false { 10 } else
{ 50 })`, `gate(std::cmp::max(10, 50))`, and a bool-owner `assert!(gate(..))`
pin of either shape (#6668). Those cases fall back to `same_test_pairing_missing`.

A `let`-bound boundary name stays paired only while the binding still holds
the call's result. A post-`let` reassignment (`got = true`), compound
assignment (`got += 1`), or `&mut` borrow (`&mut got`) voids the binding
fail-closed, so a later exact assertion on the name does not pair (#7004).
`let mut` alone does not void, and a mutation before the boundary `let`
does not void the fresh binding. Re-`let` shadowing is unchanged.

A name bound by the pattern of a `for` over a constant-row table (RIPR-SPEC-0197,
#5328) is also a boundary input, one value per row: `for (amount, want) in
[(99, 99), (100, 90)] { assert_eq!(gate(amount), want); }` pairs when some row
holds the boundary. The name's only binding in the test must be a plain
identifier that is the whole pattern of an unlabeled `for` or one field of a
flat tuple pattern whose fields are identifiers or `_`, every row must be a
tuple of that arity, and every cell of the column must be one whole numeric or
boolean literal (a string, char, constructor or call yields nothing);
otherwise the column yields nothing. A `break`, `continue` or `return`
anywhere in the loop body voids the column, since a row after it may never
reach the call; so does an `if`, `match`, `while`, `loop`, nested `for` or
`|` (a closure) anywhere in the body, since it can run the call for some rows
alone (`if let Some(want) = want { .. }`), and so does any macro argument that names the column beside
`|`, `let`, `for`, `fn` or `=>`, since the parser cannot see a rebinding there.
Cells of one row stay together: a call with two table-bound arguments is one
input row per table row, so `[(100, 99, ..), (99, 100, ..)]` never feeds
`amount == threshold`. A literal or `let`-bound argument holds for every row
and joins each one. An rstest `#[case]` column still contributes only its
first value to the call's input row: it drops the cases it cannot read, so its
slots do not line up with another column's.

Helper-call transfer, proximity-only oracle credit, and bare-name method
relation are out of scope.

## Required Evidence

- A fixture matching the reproduction reads below `exposed`, with
  `same_test_pairing_missing` in the discriminate summary.
- A control where one test does both (`assert_eq!(gate(10), true)`, and
  `fixtures/strong_boundary_oracle`) stays `exposed`.
- Unit tests cover split tests, same-call pairing, same-test split calls,
  same-line split calls, unused-argument literals, shadowed bindings,
  let-bound pairing including short names, buried-literal if-expression and
  `std::cmp::max` arguments (including `assert!`), typed literals, locals
  bound to the boundary, named-constant pairing through infection `==`,
  named-constant pairing when an unrelated extra argument is compound,
  post-`let` reassignment, compound assignment, and `&mut` borrows (each
  voiding the binding), and the unmutated `let mut` control that still pairs.
- Golden drift is reviewed row by row: every downgrade names the missing
  same-test pairing, and no finding gains a class.
- An honesty-corpus case independently prohibits `exposed` on the split
  reproduction, and one case per post-`let` mutation variant
  (reassignment, compound assignment, mutable borrow) does the same.

## Non-Goals

- No helper-call assertion credit (#4574, #4715).
- No change to proximity-only oracle credit (#4486).
- No change to bare-name method relation (#4760).
- No runtime mutation vocabulary.
- No schema bump.

## Acceptance Examples

- Given `boundary` calling `gate(10)` with no oracle and `far` asserting
  `gate(100) == true`, when the predicate `input >= 10` is classified, then
  the finding is at most `weakly_exposed` and names `same_test_pairing_missing`.
- Given `assert_eq!(gate(10), true)` in one test, when the same predicate is
  classified, then the finding stays `exposed`. The `strong_boundary_oracle`
  fixture remains `exposed`.
- Given `let _ = gate(10); assert_eq!(gate(100), true)` in one test, when the
  predicate is classified, then it does not pair.
- Given `assert_eq!(gate(if false { 10 } else { 50 }), true)` or
  `assert_eq!(gate(std::cmp::max(10, 50)), true)`, when the predicate is
  classified, then it does not pair: the evaluated argument is 50.
- Given `let threshold = 10; assert_eq!(gate(threshold), true)`, or
  `assert_eq!(gate(10u32), true)`, or `assert_eq!(gate(LIMIT), true)` with an
  infection `==` fact, when the predicate is classified, then it pairs.
  `assert_eq!(gate(LIMIT, make_context()), true)` with that same `==` fact
  also pairs: the extra compound argument is not the compared parameter.
  `assert_eq!(bulk_rate(parcels::BULK_ITEMS), 90)` with `items == BULK_ITEMS`
  also pairs: a path-qualified constant is still the named boundary.
  Given `let amount = raw; amount >= threshold` and
  `assert_eq!(gate(if false { 10 } else { 50 }, 10), true)`, pairing does
  not treat the aliased input as a boundary just because `threshold` is 10.
- Given `let mut got = gate(10); got = true; assert_eq!(got, true)`, or
  `got += 1` / `&mut got` in place of the reassignment, when the predicate
  is classified, then it does not pair: the binding no longer holds the
  boundary call's result. Given `let mut got = gate(10);` with no later
  mutation, then `assert_eq!(got, true)` still pairs.

## Test Mapping

- `crates/ripr/src/analysis/classify/boundary_pairing.rs`
- `fixtures/split_test_boundary_oracle`
- `fixtures/predicate_boundary_oracle_refused`
- `fixtures/predicate_boundary_oracle_admitted`
- `fixtures/predicate_pairing_reassigned_binding`
- `fixtures/predicate_pairing_compound_assigned_binding`
- `fixtures/predicate_pairing_mutably_borrowed_binding`
- `crates/ripr/tests/owner_pin_execution.rs::predicate_pairing_cannot_reuse_refused_boundary_equalities`

## Implementation Mapping

- `crates/ripr/src/analysis/classify/boundary_pairing.rs`: pairing authority.
- `crates/ripr/src/analysis/classifier/evidence.rs`: apply the pairing gate
  before `exposed`.
- `crates/ripr/src/analysis/classify/decision.rs`: missing evidence names the
  split.

## Metrics

- `same_test_boundary_oracle_pairing`: a predicate does not read `exposed`
  unless one test both feeds a boundary input to the owner and holds a
  discriminating oracle on that call's result.
