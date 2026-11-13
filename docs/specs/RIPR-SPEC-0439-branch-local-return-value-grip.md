# RIPR-SPEC-0439: Branch-local return-value grip needs a reaching call

Status: accepted

Owner: product / analysis

Created: 2026-11-13

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #14496 (a branch-local `return_value` seam is credited by a call that does
  not take its branch)

Linked PRs:

- #14534 (this contract)

Amends RIPR-SPEC-0231 (Rust oracle kind and strength admission) for the
`return_value` seam kind only.

Support-tier impact:

- No tier change. The rule withholds credit; it never adds a class or a
  strength. A seam that loses credit moves from `strongly_gripped` to
  `weakly_gripped` or lower, which is a gap class, not a support promotion.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.d/RIPR-SPEC-0439.toml`.
- No schema or JSON field change. The per-test record's `oracle_strength`
  moves with the stage, so the two records agree.

## Problem

A `return_value` seam records one expression the owner returns. When that
expression is the tail of one `if` branch, an exact-value assertion on the
owner's result is credited as strong grip for the seam even when the test's
call takes the other branch and observes a different value.

Example. The owner is

```rust
pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {
    if amount >= discount_threshold {
        return amount - 10;
    }
    amount
}
```

and the only test is `assert_eq!(discounted_total(50, 100), 50);`. The call
takes the `else` path and returns `amount`; it never evaluates
`amount - 10`. Before this spec the `return amount - 10` seam read
`strongly_gripped`, and its related-test record read `strong`.

The same over-credit appears in four shapes the first fix missed:

1. the assertion pins a local (`let got = discounted_total(50, 100);
   assert_eq!(got, 50);`), so the call is not in the assertion text;
2. the seam's `if` is nested inside an enclosing `if` the call does not take;
3. the owner rebinds or mutates a parameter before the `if`, so folding the
   call's literal arguments can withhold a branch the call does reach;
4. both arms hold the same expression, so the seam's own branch is not
   established by text.

## Behavior

The gate is `seam_branch_reached_by_call` in
`crates/ripr/src/analysis/test_grip_evidence/value_contradiction.rs`. It
answers `Some(true)`, `Some(false)` or `None` for one (seam, oracle) pair.

- The compared operands are resolved through their `let` initializers first
  (`assertion_operands`), so a call bound to a local is read.
- Exactly one operand must be a bare `owner(..)` call whose arguments all
  fold to integers. A `.await` or wrapper call is not modelled and answers
  `None`.
- The owner body is parsed. Every parameter must be a plain identifier and
  the argument count must match, or the answer is `None`.
- If the owner body rebinds a parameter (`let name = ..`), assigns one
  (`name = ..`, `name += ..`) or binds one in a `for` pattern, the answer is
  `None`: the call-argument environment is stale.
- The seam expression must be the tail or a statement of exactly one `if`
  branch. Both arms holding it, or more than one `if` holding it, answers
  `None`.
- The seam's own condition folds with the call's literal arguments. If the
  seam's branch is not the taken one, the answer is `Some(false)`.
- Every enclosing `if` on the path must also take the branch that holds the
  seam. An unreadable enclosing branch answers `None`; a taken-false one
  answers `Some(false)`.

`oracle_discriminates_seam_in_test` returns `false` on `Some(false)` and
keeps today's credit on `None`. `best_oracle` applies the same decision, so
the per-test `oracle_strength` cannot publish `strong` for a test the stage
refuses.

## Required Evidence

- The else-only test above does not strongly grip the `return amount - 10`
  seam; its related-test record is not `strong`.
- The let-bound form of the same test does not strongly grip it.
- The nested-guard form does not strongly grip the inner return.
- A reaching call through a rebound parameter keeps its grip.
- Identical arms keep their grip.
- The committed two-test `fixtures/boundary_gap` control stays strongly
  gripped.
- The branch-miss case is registered in the evidence-promotion honesty
  corpus.

## Non-Goals

- No change to seam creation, seam kind or the `#14392` tail `if/else` form.
- No modelling of `.await`, wrapper calls, loops or closures.
- No change to any seam kind other than `return_value`.
- No new JSON field or grip class.

## Acceptance Examples

1. `discounted_total(50, 100)` asserted at `50`: the `amount - 10` seam is
   not `Yes`.
2. `let got = discounted_total(50, 100); assert_eq!(got, 50);`: not `Yes`.
3. Nested `if discount_threshold > 0 { if amount >= discount_threshold { .. } }`
   asserted at `(50, 0)`: not `Yes`.
4. `let amount = -amount;` before the `if`, asserted at `(-50, 0) == 40`:
   `Yes`.
5. `if c { return n; } else { return n; }` asserted at `(50, 100) == 50`:
   `Yes`.
6. The two-test fixture (one call per branch): `Yes`.

## Test Mapping

- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_is_not_strongly_gripped_by_an_else_branch_test`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_is_not_gripped_by_a_let_bound_else_branch_call`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_behind_an_untaken_enclosing_guard_is_not_gripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_a_rebound_parameter_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_identical_arms_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_record_agrees_with_the_stage`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_is_strongly_gripped_by_a_reaching_test`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_in_the_taken_branch_stays_strong`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::committed_boundary_gap_fixture_stays_strongly_gripped`

## Implementation Mapping

- `crates/ripr/src/analysis/test_grip_evidence/value_contradiction.rs`:
  `seam_branch_reached_by_call`, `parameter_is_rebound_or_mutated`,
  `enclosing_branches_reached`.
- `crates/ripr/src/analysis/test_grip_evidence.rs`: `branch_gate_withholds`,
  its call in `oracle_discriminates_seam_in_test`, and the per-test
  downgrade in `best_oracle`.

## Metrics

- `branch_local_return_value_false_promotions`: seams credited by a call
  that does not reach their branch; must be zero.
