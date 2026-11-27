# Fixture: python_positional_only_marker_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`key` adds the positional-only marker: `def key(self, *args, **kwargs):`
becomes `def key(self, /, *args, **kwargs):`. The only test asserts
`key(None, 1, 2) == (1, 2)`. Every argument is passed by position, so the call
runs the same way on both headers and the assertion passes with either.

## When

`cargo xtask fixtures python_positional_only_marker_no_exposed_credit`

## Then

The finding is not `exposed`. Only a call that passes `self` by keyword runs
differently after the edit, and no related call does, so the finding reads
`weakly_exposed` and names the keyword call to add. This is the #16011 shape 1
false-exposed correction.

## Must Not

Credit a positional call as observing the moved marker. The RIPR-SPEC-0108
honesty corpus independently forbids promotion.
