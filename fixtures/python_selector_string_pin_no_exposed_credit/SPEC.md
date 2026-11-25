# Fixture: python_selector_string_pin_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`largest` loops over the whole list from `best = xs[0]` (`for x in xs:`) and
changes its selection comparison from `x > best` to `x >= best`. The only test
asserts `largest(['pear', 'apple', 'fig']) == 'pear'`; equal strings are
indistinguishable under `==`, so the assertion passes with both comparisons.

## When

`cargo xtask fixtures python_selector_string_pin_no_exposed_credit`

## Then

The finding is `infection_unknown`, not `exposed`: static evidence does not
place the loop's operands at the changed equality boundary. This extends the
#12954 false-exposed correction from integer pins to other literal pins.

## Must Not

Promote the finding to `exposed` from a string value pin alone. The
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
