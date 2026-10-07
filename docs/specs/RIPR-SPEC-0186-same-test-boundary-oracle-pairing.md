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
  and named-constant pairing when an unrelated extra argument is compound.
- Golden drift is reviewed row by row: every downgrade names the missing
  same-test pairing, and no finding gains a class.
- An honesty-corpus case independently prohibits `exposed` on the split
  reproduction.

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

## Test Mapping

- `crates/ripr/src/analysis/classify/boundary_pairing.rs`
- `crates/ripr/src/analysis/classify/owner_pin/tests/helper_pins.rs::the_loan_maps_only_plain_parameters_and_lone_eager_calls`
- `fixtures/split_test_boundary_oracle`
- `fixtures/predicate_boundary_oracle_refused`
- `fixtures/predicate_boundary_oracle_admitted`
- `crates/ripr/tests/owner_pin_execution.rs::predicate_pairing_cannot_reuse_refused_boundary_equalities`

## Implementation Mapping

- `crates/ripr/src/analysis/classify/boundary_pairing.rs`: pairing authority,
  including `loan_pairs_boundary_call` for borrowed check-helper assertions.
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
