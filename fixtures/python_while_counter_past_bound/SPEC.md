# Fixture: python_while_counter_past_bound

Spec: RIPR-SPEC-0028

## Given

A sole generator loop decrements `start` by one after yielding it. The diff
changes `start > 0` to `start >= 0`; its only test consumes `countdown(-2)`
and asserts the empty list. Native Python returns `[]` under both versions.

## When

```bash
ripr check --root fixtures/python_while_counter_past_bound/input --diff fixtures/python_while_counter_past_bound/diff.patch
```

## Then

The predicate is `weakly_exposed` with the missing `start == 0` boundary.
A unit-step walk reaches zero from three, but cannot reach it from minus two.

## Must Not

Credit the empty-list assertion as discriminating the changed loop bound, or
apply this walk to step-two, conditional, early-exit or pre-rebound bodies.
