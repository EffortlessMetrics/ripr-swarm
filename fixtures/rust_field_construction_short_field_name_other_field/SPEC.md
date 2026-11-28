# Fixture: rust_field_construction_short_field_name_other_field

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

`origin_shift` returns `Point { x: dx, y: 1 }`, and the edit changes the
initializer of the one-letter field `y` from `0` to `1`. The only test calls
the owner and pins the other field: `assert_eq!(origin_shift(3).x, 3)`. It
passes on the old code and on the new.

## When

`cargo xtask fixtures rust_field_construction_short_field_name_other_field` runs the public diff analyzer.

## Then

Both findings stay `weakly_exposed` with `observation_unconfirmed`. Only a
`.y` read on the owner's result confirms the changed field; a read of `.x`
does not.

## Must Not

- Promote either finding to `exposed` because the assertion calls the owner.
