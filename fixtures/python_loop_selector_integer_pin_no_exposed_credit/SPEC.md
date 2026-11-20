# Fixture: python_loop_selector_integer_pin_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`largest` changes its selection comparison from `x > best` to `x >= best`. The
only test asserts `largest([1, 5, 3]) == 5`; equal integers are
indistinguishable, so the assertion passes with both comparisons.

## When

`cargo xtask fixtures python_loop_selector_integer_pin_no_exposed_credit`

## Then

The finding is `infection_unknown`, not `exposed`: static evidence does not
place the loop's operands at the changed equality boundary. This is the #12954
false-exposed correction.

## Must Not

Promote the finding to `exposed` from an integer value pin alone. The
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
