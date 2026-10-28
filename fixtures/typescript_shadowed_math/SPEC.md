# Fixture: typescript_shadowed_math

Spec: RIPR-SPEC-0247

## Given

A module-local `Math.abs` returns 1 regardless of its argument. The test
asserts that `cmToIn(1)` is close to 1.

## When

The return changes from `Math.abs(cm)` to `Math.abs(cm + 1)`.

## Then

The test passes against both expressions; the approximate assertion does not
credit the change. The independent honesty corpus requires a nonempty finding
and refuses promotion.

## Must Not

Treat the shadowed module binding as the built-in `Math` object.
