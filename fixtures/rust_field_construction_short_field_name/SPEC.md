# Fixture: rust_field_construction_short_field_name

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

`origin_shift` returns `Point { x: dx, y: 1 }`, and the edit changes the
initializer of the one-letter field `y` from `0` to `1`. One test calls the
owner and pins that field: `assert_eq!(origin_shift(3).y, 1)`. It fails on
the old code and passes on the new.

## When

`cargo xtask fixtures rust_field_construction_short_field_name` runs the public diff analyzer.

## Then

The `field_construction` finding for `y: 1` and the `return_value` finding
for the changed literal both read `exposed`. The `.y` read on the owner's
own result observes the changed field, as a read of a longer field name
(`.retries`) already does.

## Must Not

- Leave either finding at `weakly_exposed` with `observation_unconfirmed`
  because the field's name is too short to be a token.
