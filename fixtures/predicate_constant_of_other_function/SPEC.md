# Fixture: predicate_constant_of_other_function

Spec: RIPR-SPEC-0094

## Given

`discount` changed its predicate from `total >= 100` to `total > 200`. The only related test calls `discount(50)` without asserting its result, and asserts `assert_eq!(label(0), "zero")`: a call of another pure function on a literal argument, which computes the same value on every run (#15476).

## When

`ripr check` analyzes the changed predicate.

## Then

The finding is `reachable_unrevealed` with observe and discriminate `no`, and the related test's miss is `assertion_not_observing`. The constant assertion is dropped like `assert!(true)` (#14968).

## Must Not

Credit observation or discrimination for the changed predicate from an assertion that observes nothing the owner does.
