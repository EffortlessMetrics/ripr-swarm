# Fixture: rust_return_value_inline_tail_if_arm_repeated_value

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

Issue #15274 repro 4: the then-arm of a one-line tail `if` changes from
`Some(x)` to `None`, so both arms now read `None`. One test pins
`assert_eq!(find_even(4), None)`, which fails on the old code and passes on
the new.

## When

`cargo xtask fixtures rust_return_value_inline_tail_if_arm_repeated_value` runs the public diff analyzer.

## Then

The line carries one `return_value` finding on the changed arm, not an
`infection_unknown` finding on the unchanged condition. It stays
`weakly_exposed`: the line spells `None` twice, so the owner-return pin
cannot place the old line and fails closed.

## Must Not

- Report a `predicate` finding for the unchanged condition.
- Read `exposed` from an old line the pin had to guess.
