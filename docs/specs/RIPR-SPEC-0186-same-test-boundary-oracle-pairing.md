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
- #6482 (an assertion borrowed from a test-local check helper pairs through
  the helper's call)
- #7004 (post-`let` mutation voids a bound boundary name)
- #7042 item 4 (newline-split reassignment still voids)

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

A private helper reached only through a wrapper (RIPR-SPEC-0159 chain,
#6694 / #6672) pairs on the wrapper call: an admitted discriminating
assertion whose subject is one call of the chain's entry, that names no
second entry call and no owner call, and whose entry arguments are each a
plain identifier, a whole scalar literal or a path (a scalar buried in a
compound argument such as `entry(std::cmp::max(10, 50))` or `entry(10 * 2)`
does not pair, since activation binds its first scalar), pairs when activation recorded a
boundary `==` row bound down the chain from that assertion's line. This
holds only when every hop hands its call's result to its caller's return:
the caller body has no `return` or `?`, it does not rebind or assign a
parameter it forwards to the hop, and its tail is the hop call itself or
`if <call> { A } else { B }` (or `if !<call>`) where `A` and `B` are
literals of distinct values (`05` and `5` are equal; an escaped string or
char literal never counts as distinct). A discarded, let-bound, transformed, branch-guarded or
computed-branch result, or a rebound forwarded parameter, keeps the
pairing missing (and RIPR-SPEC-0159 makes propagation unknown). A match
guard that only reads a forwarded parameter (`n if n > qty =>`) is not a
rebinding; a pattern that binds it (`Some(qty) =>`) is. The row must come
from the entry call on the assertion's own line, in the assertion's own
test: activation is recomputed from that test alone, because a row carries
no source test and another test in another file can share the line. That
line holds one entry call and no direct owner call. A computed hop argument
already stops the row transfer (RIPR-SPEC-0159), so no `==` row exists to
pair, and so does an entry call shadowed by a test-local closure or nested
fn of the entry's name. The owner-return pin (RIPR-SPEC-0197) judges the owner's own call and
never admits a wrapper assertion here.

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
reach the call; so does an `if`, `match`, `while`, `loop`, nested `for`,
`|` (a closure), `&&` or `||` anywhere in the body, since it can run the call
for some rows alone (`if let Some(want) = want { .. }`), and so does any macro
argument that names the column beside `|`, `let`, `for`, `fn` or `=>`, since
the parser cannot see a rebinding there.
Cells of one row stay together: a call with two table-bound arguments is one
input row per table row, so `[(100, 99, ..), (99, 100, ..)]` never feeds
`amount == threshold`. A literal or `let`-bound argument holds for every row
and joins each one. An rstest `#[case]` column still contributes only its
first value to the call's input row: it drops the cases it cannot read, so its
slots do not line up with another column's.

An assertion RIPR-SPEC-0197 rule 7 borrows from a test-local check helper
(#6482) names the helper's parameters, not the test's inputs:

```rust
fn check_pass(score: u32, want: bool) { assert_eq!(passes(score), want); }
#[test] fn pass_mark() { check_pass(50, true); check_pass(49, false); }
```

It pairs when activation's `==` fact sits on the line of one of the test's
calls to that helper, and every link from that call to the compared value is
fixed:

- the call is one the admission found on the test's eager path, and it is
  the only call expression on its line, so the fact describes that call;
- the helper's parameters are plain identifier patterns its body never
  rebinds (no `mut`, `ref`, sub-pattern, or same-named `let`, closure
  parameter or nested item);
- the helper calls the owner exactly once, and that call is a whole compared
  operand whose arguments are the helper's parameters passed unchanged
  (`passes(score)`, not `passes(score + 1)` or `{ let score = 1;
  passes(score) }`);
- the call passes only scalar literals (`check_pass(50, true)`, not
  `check_pass(n, true)`), with no comment or string before it on its line,
  and the activation fact's `parameter == value` equals the literal this call
  feeds that owner parameter through the helper's parameter slot.
  Activation's facts are pooled across related tests without file identity,
  so a fact from another file's same-line call through a different helper
  cannot stand in for this call's input.

A borrowed assertion pairs only this way: its operands name the helper's
parameters, so the direct owner-call and bound-name paths, which read the
test's own bindings, never see it. The same holds for any assertion outside
the test's own lines, including a harness-registry callback's assertions
credited to a trial: they no longer pair through the test's bindings, which
can only withhold pairing.

The same owner-name defeats as an inline oracle apply. Anything else keeps
`same_test_pairing_missing`.

Other helper-call transfer, proximity-only oracle credit, and bare-name method
relation are out of scope.

## Required Evidence

- A fixture matching the reproduction reads below `exposed`, with
  `same_test_pairing_missing` in the discriminate summary.
- A control where one test does both (`assert_eq!(gate(10), true)`, and
  `fixtures/strong_boundary_oracle`) stays `exposed`.
- Unit tests cover a borrowed check-helper assertion pairing through its
  call, with a control that the same assertion without the loan does not,
  and refusals for a fact on another line, a call outside the admission's
  eager lone-line calls, unmappable parameters, a second owner call in the
  helper, a computed, partial or block operand, another function's call
  on the line, and a helper call passing a local or computed input, which
  another test's identical same-line call could bind differently in the
  pooled activation facts (#6482).
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

- No helper-call assertion credit beyond rule 7's borrowed assertions
  (#4574, #4715, #6482).
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
- Given private `fn is_bulk(qty: u32) -> bool { 10 <= qty }` reached only
  through `pub fn order_discount(qty: u32) -> u32 { if is_bulk(qty) { 5 } else
  { 0 } }` and `assert_eq!(order_discount(10), 5)`, when the predicate is
  classified, then it pairs. With `let _ = is_bulk(qty); 5` as the wrapper
  body, or with the tests calling only `order_discount(12)` and
  `order_discount(3)`, it does not.
- Given `let mut got = gate(10); got = true; assert_eq!(got, true)`, or
  `got += 1` / `&mut got` in place of the reassignment, when the predicate
  is classified, then it does not pair: the binding no longer holds the
  boundary call's result. Given `got` then `= true;` on the next line, it
  also does not pair (#7042 item 4). Given `let mut got = gate(10);` with
  no later mutation, then `assert_eq!(got, true)` still pairs, including
  when the assertion is wrapped across lines.

## Test Mapping

- `crates/ripr/src/analysis/classify/boundary_pairing.rs`
- `crates/ripr/src/analysis/classify/owner_pin/tests/helper_pins.rs::the_loan_maps_only_plain_parameters_and_lone_eager_calls`
- `fixtures/split_test_boundary_oracle`
- `fixtures/predicate_boundary_oracle_refused`
- `fixtures/predicate_boundary_oracle_admitted`
- `fixtures/predicate_pairing_reassigned_binding`
- `fixtures/predicate_pairing_compound_assigned_binding`
- `fixtures/predicate_pairing_mutably_borrowed_binding`
- `fixtures/predicate_pairing_newline_split_reassigned_binding`
- `crates/ripr/tests/owner_pin_execution.rs::predicate_pairing_cannot_reuse_refused_boundary_equalities`
- `crates/ripr/tests/helper_wrapper_reach.rs`

## Implementation Mapping

- `crates/ripr/src/analysis/classify/boundary_pairing.rs`: pairing authority,
  including `loan_pairs_boundary_call` for borrowed check-helper assertions.
- `crates/ripr/src/analysis/classify/helper_transfer.rs`: the hop forwarding
  and parameter-rebinding checks the wrapper-entry pairing uses.
- `crates/ripr/src/analysis/syntax/owner_pin.rs`: the `HelperLoan` facts
  (helper, plain parameters, eager lone-line call lines) pairing reads.
- `crates/ripr/src/analysis/classifier/evidence.rs`: apply the pairing gate
  before `exposed`.
- `crates/ripr/src/analysis/classify/decision.rs`: missing evidence names the
  split.

## Metrics

- `same_test_boundary_oracle_pairing`: a predicate does not read `exposed`
  unless one test both feeds a boundary input to the owner and holds a
  discriminating oracle on that call's result.
