# RIPR-SPEC-0443: Branch-local return-value grip needs a reaching call

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
- #13037 amendment (reached computed tail)

Amends RIPR-SPEC-0231 (Rust oracle kind and strength admission) for the
`return_value` seam kind only.

Support-tier impact:

- No tier change. The rule withholds credit; it never adds a class or a
  strength. A seam that loses credit moves from `strongly_gripped` to
  `weakly_gripped` or lower, which is a gap class, not a support promotion.
  Claim boundaries for this surface stay governed by the canonical ledger in
  [support tiers](../status/SUPPORT_TIERS.md); nothing here promotes a tier.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.d/RIPR-SPEC-0443.toml`.
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

Two more shapes are in the same class:

5. the seam expression also appears as a sibling `return` after the `if`, so
   the branch scan sees one match and cannot tell which occurrence the seam
   is; the call may reach the sibling, and withholding there is a false
   negative;
6. a parameter is mutated inside a loop after the `if`, so the call-argument
   fold is stale and can withhold a branch the call reaches on a later
   iteration.

## Behavior

The gate is `seam_branch_reached_by_call` in
`crates/ripr/src/analysis/test_grip_evidence/value_contradiction.rs`. It
answers `Some(true)`, `Some(false)` or `None` for one (seam, oracle) pair.

- The compared operands are resolved through their `let` initializers first
  (`assertion_operands`), so a call bound to a local is read. When the
  assertion's text occurs once in the test body, the binding read is the last
  plain `let` of the name before it whose block is still open there, so a
  rebind (`let got = f(1); let got = f(2);`) resolves to the second call and a
  `let` in a nested block that closed earlier is skipped (#14569). An
  initializer that reads the name itself (`let got = got.unwrap();`) is not
  resolved. Without that single anchor only a sole plain binding resolves.
- Exactly one operand must be a call to the owner whose arguments all fold to
  integers: the bare `owner(..)` form, `owner(..).await`, or a
  single-identifier wrapper (`wrapper(owner(..))`), matching the sibling
  invisibility path. A wrapper that changes the arguments is not modelled
  and answers `None`.
- For a same-file free call passing a literal one-field tuple struct, a
  comparison such as `a.0 > 100` can select the other arm of a tail `if`.
  The constructor and its primitive field type must be established, and both
  numbers must fit that type. A diff can also read a directly resolved
  `Add::add` this way, including `let a = Money(2); let b = Money(3);`
  followed by `assert_eq!(a + b, Money(0))`. Every related call must skip
  the edited arm before the diff loses credit. The assertion must compare
  like-typed values, with derived value equality for the returned newtype.
  A destructor that can run user code, a taken arm, a shadow, an unsupported
  field, or an unreadable call preserves the prior reading (#15770).
- The owner body is parsed. Every parameter must be a plain identifier and
  the argument count must match, or the answer is `None`.
- If the owner body rebinds a parameter (`let name = ..`), assigns one
  (`name = ..`, `name += ..`) or binds one in a `for` pattern, anywhere in
  the body including a loop body, the answer is `None`: the call-argument
  environment is stale. A `let` whose enclosing block neither holds the
  seam's `if` nor overlaps a condition on its path is out of scope for every
  folded condition and does not count (#14569); assignments and mutable
  borrows count wherever they are.
- The seam expression must be the tail or a statement of exactly one `if`
  branch, matched by the seam's own line. Both arms holding it, or more than
  one `if` holding it, answers `None`. A seam whose own line is not inside an
  `if` branch (a plain tail, or a sibling `return` after the `if`) answers
  `None` and keeps today's credit. The seam is credited only when the
  crediting call reaches that branch: a sibling duplicate `return` keeps the
  sibling seam's grip, not the branch-local one, and the branch-local seam
  answers `Some(false)` when the call takes the other branch.
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
- A parameter mutated inside a loop after the `if` keeps the reaching
  call's grip.
- A parameter bound by a `for` pattern keeps the reaching call's grip.
- Identical arms keep their grip.
- A sibling duplicate `return` after the `if` keeps the grip of the call
  that reaches it.
- A parameter rebound by a destructuring `let` keeps the reaching call's grip.
- A parameter mutably borrowed (`let slot = &mut amount; *slot = -*slot;`)
  keeps the reaching call's grip.
- A condition with a short-circuited `&&` followed by `||` folds the `||`
  comparison, so the reaching call keeps its grip.
- An operand that holds an owner call neither parser resolves keeps today's
  credit instead of withholding.
- A branch-missing `Strong` oracle cannot lift the stage to `Yes` above a
  reaching `Weak` one, and the stage summary names the branch refusal rather
  than reporting a kind mismatch.
- A wrapped call (`block_on(owner(..))`) from the else branch does not
  strongly grip the then-branch return.
- The committed two-test `fixtures/boundary_gap` control stays strongly
  gripped.
- The branch-miss case is registered in the evidence-promotion honesty
  corpus.

## Non-Goals

- No change to seam creation, seam kind or the `#14392` tail `if/else` form.
- No modelling of iteration or closures. A parameter mutated anywhere in the
  body, including a loop body, makes the fold refuse (`None`) rather than
  model the loop; a single-identifier wrapper is read through, a wrapper that
  changes the arguments is not.
- No modelling of `match` arms. `enclosing_branches_reached` walks `if`
  ancestors only, so a seam inside a `match` arm the call does not take keeps
  today's credit. That is a known limit of this contract, reported for a
  follow-up issue rather than fixed here.
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
7. `assert_eq!(block_on(discounted_total(50, 100)), 50)`: the `amount - 10`
   seam is not `Yes`.
8. `if amount >= discount_threshold { return amount - 10; } return amount -
   10;` asserted at `(50, 100) == 40`: the sibling seam keeps its grip
   (`Yes`), and the then-branch seam on line 3 is not `Yes`.
9. `loop { if amount >= threshold { return amount - 10; } amount += 10; }`
   asserted at `(50, 100) == 90`: `Yes`.
10. `let (amount,) = (-amount,);` before the `if`, asserted at `(-50, 0) ==
    40`: `Yes`.
11. `let slot = &mut amount; *slot = -*slot;` before the `if`, asserted at
    `(-50, 0) == 40`: `Yes`.
12. `if amount < 0 && amount > 100 || amount >= discount_threshold { return
    amount - 10; }` asserted at `(50, 0) == 40`: `Yes` — the `||` comparison
    after the short-circuited `&&` is still folded.
13. `assert_eq!(discounted_total(50, 100), discounted_total(110, 100) / 2)`:
    `Yes` — an operand call neither parser resolves keeps today's credit.
14. A test that pins the else branch exactly (branch withheld, `Strong`
    before the gate) beside a `Weak` test that reaches the branch: the
    discriminate stage is not `Yes`, and its summary reads "kind matches; the
    call does not reach the seam's branch".
15. `let got = discounted_total(50, 100); let got = discounted_total(100,
    50); assert_eq!(got, 90);` on `if .. { return amount - 10; } else {
    return amount; }`: the `return amount` seam is not `Yes` and the
    `return amount - 10` seam is `Yes` (#14569).
16. The same assertion reading an outer `let got = discounted_total(100, 50);`
    past a closed `{ let got = discounted_total(50, 100); }`: the outer call
    decides (#14569).
17. An owner `{ let amount = 0; }` block that closes before the `if`, asserted
    at `(50, 100) == 50`: the `return amount - 10` seam is not `Yes`. The same
    `let` in a block holding the `if` keeps today's credit (#14569).

## Amendment (#13037): a reached computed tail is observed by its exact pin

`ripr check` read a changed computed return (`base * 2` under `if express`)
as `weakly_exposed` even when a test pinned the exact value that branch
returns: observation of a `return_value` probe was confirmed only by an
assertion that repeats a token of the changed expression, and `800` names no
token of `base * 2`. The repo seam view `ripr repair` reads already credited
the same pin, so the two commands disagreed.

An exact pin now also confirms observation when
`return_seam_tail_reached_by_call` (same file as the gate above) holds. It is
strict where the gate above keeps today's credit: every unreadable shape
answers no credit.

- The assertion is an equality (`assert_eq!`, not `assert_ne!`, and not a
  `debug_` form a release test run compiles out) and exactly one compared
  operand is a bare owner call (no wrapper, no local argument) whose
  arguments are `true`/`false` or integer expressions that fold with no
  bindings (`100`, `50 + 50`).
- An integration test may spell that owner call through the package's own
  library path with no `use` (`shipping::shipping_cost(100, true)`,
  `shipping::rates::shipping_cost(..)`). The path is read as the bare owner
  call only when the relation's own-crate authority
  (`own_crate_qualified_entry_path`, #8999/#9638) establishes it: the test
  is a test-crate root under the root package's `tests/`, the owner is a
  module-level item of that package's library, the path is exactly
  `<library name>::<module path>::<owner>`, and nothing in the file or the
  test body declares, imports, renames or globs the library name. Any other
  qualified spelling (a rebound crate name, a longer or wrong path, a module
  child of the test crate, another package) stays unevaluated.
- The other compared operand is a value the test fixes itself: `true`,
  `false` or an integer expression that folds with no bindings, read through
  a `let` when it names one. An expected value computed by calling code
  (`let expected = helper(..)`) may call the owner and move with it.
- The test body does not shadow the owner's name (`let shipping_cost =
  |..| 800;`), so the asserted call is the owner.
- The owner's name is unique in a complete workspace index (so the bare
  call cannot reach a same-named free function, test stub or associated
  function), and its plain, non-`mut` parameters are typed `bool` or a
  recognized integer type and are never rebound or mutated. The owner body
  carries no attribute (a `#[cfg]` could swap a statement the fold reads).
- The seam is the whole tail of exactly one branch of the `if` chain that is
  itself the owner body's tail, so the branch value is the returned value.
- Every statement before that tail is either `let NAME = EXPR;` whose
  initializer folds (it calls nothing, so it cannot exit or panic), which
  binds `NAME` for later conditions, or an `if COND { .. }` without `else`
  whose condition folds false.
- Every condition on the path folds, reading the parameters, those bindings
  and, for a top-level owner, `const` items declared once at the top level
  of the owner's file with a literal initializer and a known integer type,
  named in no `use`, in a file with no top-level glob `use`. A `static` can
  change before the owner reads it, so it does not resolve. The fold
  selects the seam's branch.
- Every integer the fold produces must fit `i32` (an unconstrained
  literal's type), every integer width the owner names (parameters, return
  type, annotations, literal suffixes) and each read constant's declared
  width, since with overflow checks off the runtime wraps where the fold
  would not. An owner naming `usize`, `isize`, `u128` or `i128`, or spelling
  any non-primitive type (an alias such as `type Byte = u8;` could hide a
  narrower width), is not read. Nor is an owner that is not at its file's
  top level, or whose file could give a primitive name another meaning: a
  `type` item named like a primitive (`pub type i32 = u8;`), a `use` naming
  a primitive (`use core::primitive::u8 as i32;`), a top-level glob `use`,
  or a macro that could expand to such an item (`macro_rules!`, or a
  top-level item macro invocation). A raw identifier (`r#i32`) counts as
  the primitive it spells. The scan is deliberately coarse: a `use` that
  only names a primitive without rebinding it (`use core::primitive::u8 as
  Byte;`) also refuses, which only withholds credit.
- A remainder by `-1` does not fold: `MIN % -1` panics in the operand's own
  width (in release too), so a call with those arguments never reaches the
  tail, though the fold's wider arithmetic would read 0.
- A test binding that the test writes again after the call (`got %= ..`, or
  any compound assignment) is not a bare pin of the call's value.

The admission is the bool field pin's: the test is not `should_panic`, the
assertion macros are the standard ones, and no `use` rebinds the owner's
name. The import and cross-package owner defeats apply as for every other
owner pin. The rule only confirms observation; it adds no oracle kind or strength.

Acceptance:

1. `assert_eq!(shipping_cost(100, 10, true), 800)` on `base * 2`:
   `exposed`.
2. `assert_eq!(shipping_cost(100, 10, false), 400)` only: not `exposed`.
3. `assert_ne!(shipping_cost(100, 10, true), 1200)`: not `exposed`.
4. An earlier `if order_cents >= FREE_OVER_CENTS { return 0; }` the call
   takes (`(100, 9000, true)`): not `exposed`; `(100, 10, true)`: `exposed`.
5. A computed constant (`5 * 1_000`), a constant named in a `use` or
   declared only inside a nested module, a `let` that calls a function, an
   attributed statement, a same-named function elsewhere, a rebound
   parameter, a computed operand (`f(..) / 2`) or a local argument: not
   `exposed`.
6. A narrow local that wraps (`let n: u8 = 200; let total = n + 100;`, or
   the same through `type Byte = u8;` or a primitive-named
   `pub type i32 = u8;`), a
   `static` threshold, a closure that shadows the owner in the test, and an
   expected value computed by a helper that calls the owner: not `exposed`.
   `debug_assert_eq!` and a top-level glob import are refused here too, as a
   second guard behind refusals that already hold upstream.

Fixtures `fixtures/rust_computed_return_tail_reached` (promoted),
`fixtures/rust_computed_return_tail_other_branch` (not promoted) and
`fixtures/rust_computed_return_tail_wrapping_local` (not promoted: a `u8`
local wraps, so the call takes the other branch) and
`fixtures/rust_computed_return_tail_primitive_alias` (not promoted: the
same wrap through `pub type i32 = u8;`) pin both sides in the
evidence-promotion honesty corpus.
`fixtures/rust_computed_return_tail_crate_qualified` (promoted: the pin is
spelled through the package's own library path) and
`fixtures/rust_computed_return_tail_crate_rebound` (not promoted: the test
file declares a module with the library's name) pin the own-crate spelling
there too.

## Test Mapping

- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_is_not_strongly_gripped_by_an_else_branch_test`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_is_not_gripped_by_a_let_bound_else_branch_call`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_behind_an_untaken_enclosing_guard_is_not_gripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_a_rebound_parameter_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_a_loop_carried_rebind_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_a_for_pattern_rebind_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_identical_arms_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_a_sibling_duplicate_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_a_destructured_rebind_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_a_mutable_borrow_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_behind_a_short_circuited_or_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_with_an_unresolved_operand_call_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_summary_names_the_branch_refusal`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_stage_is_not_stronger_than_a_missing_branch`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_is_not_gripped_by_a_wrapped_else_branch_call`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_record_agrees_with_the_stage`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_is_strongly_gripped_by_a_reaching_test`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::branch_local_return_value_seam_in_the_taken_branch_stays_strong`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::committed_boundary_gap_fixture_stays_strongly_gripped`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::a_rebound_let_resolves_to_the_binding_the_assertion_reads`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::a_single_let_bound_reaching_call_keeps_its_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::a_let_in_a_closed_test_block_does_not_bind_the_asserted_value`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::a_brace_in_a_char_literal_or_comment_does_not_reopen_a_closed_block`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::a_let_in_a_comment_or_string_is_not_the_binding_read`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::an_owner_let_in_a_closed_block_does_not_keep_an_unreached_seams_grip`
- `crates/ripr/src/analysis/test_grip_evidence/tests.rs::an_owner_let_in_a_block_holding_the_seam_keeps_its_grip`
- `crates/ripr/tests/computed_return_tail_13037.rs::an_exact_pin_from_a_call_that_reaches_the_branch_is_exposed`
- `crates/ripr/tests/computed_return_tail_13037.rs::a_pin_from_the_other_branch_only_is_not_credited`
- `crates/ripr/tests/computed_return_tail_13037.rs::an_inequality_pins_no_value`
- `crates/ripr/tests/computed_return_tail_13037.rs::an_earlier_return_the_call_takes_is_not_credited`
- `crates/ripr/tests/computed_return_tail_13037.rs::shapes_the_fold_cannot_read_keep_todays_reading`
- `crates/ripr/tests/computed_return_tail_13037.rs::a_constant_declared_only_inside_the_test_module_does_not_resolve`
- `crates/ripr/tests/computed_return_tail_13037.rs::a_wrapped_or_computed_operand_is_not_a_bare_pin`
- `crates/ripr/tests/computed_return_tail_13037.rs::a_wrapping_local_a_static_or_a_shadowing_closure_is_not_credited`
- `crates/ripr/tests/computed_return_tail_13037.rs::an_expected_value_the_test_computes_by_calling_code_is_not_credited`
- `crates/ripr/tests/computed_return_tail_13037.rs::a_release_elided_assertion_or_a_glob_import_is_not_credited`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::a_crate_qualified_exact_pin_that_reaches_the_branch_is_exposed`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::the_bare_imported_spelling_keeps_its_credit`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::a_crate_qualified_pin_through_a_library_module_is_exposed`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::the_library_target_name_is_the_spelling_that_counts`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::a_qualified_pin_from_the_other_branch_only_is_not_credited`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::a_rebound_crate_name_is_not_credited`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::another_path_ending_in_the_owner_is_not_credited`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::a_transformed_or_wrapped_qualified_call_is_not_a_pin`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::a_test_outside_the_test_crate_root_is_not_credited`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::an_explicit_library_name_is_the_spelling_that_counts`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::an_expected_name_bound_twice_resolves_to_the_binding_in_scope`
- `crates/ripr/tests/computed_return_tail_crate_qualified.rs::a_let_bound_qualified_call_reads_like_the_bare_let_bound_call`

## Implementation Mapping

- `crates/ripr/src/analysis/test_grip_evidence/value_contradiction.rs`:
  `seam_branch_reached_by_call`, `parameter_is_rebound_or_mutated`,
  `enclosing_branches_reached`.
- `crates/ripr/src/analysis/test_grip_evidence.rs`: `branch_gate_withholds`,
  its call in `oracle_discriminates_seam_in_test`, and the per-test
  downgrade in `best_oracle`.
- `crates/ripr/src/analysis/test_grip_evidence/value_contradiction.rs`:
  `return_seam_tail_reached_by_call` (#13037).
- `crates/ripr/src/analysis/classify/related_tests.rs`:
  `own_crate_qualified_owner_path`, the own-crate spelling the admission
  establishes; `return_seam_tail_reached_by_call` reads the assertion and
  the test body with that path spelled bare (`spell_path_bare`), so a `let`
  the assertion names is still selected by the assertion's position.
- `crates/ripr/src/analysis/classifier/evidence.rs`: the `reached_tail_pin`
  admission; `crates/ripr/src/analysis/classify/reveal.rs`: its use in
  observation confirmation.

## Metrics

- `branch_local_return_value_false_promotions`: seams credited by a call
  that does not reach their branch; must be zero.
