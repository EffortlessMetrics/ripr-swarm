# Fixture: python_guarded_statement_unreached_arm_no_exposed_credit

Spec: RIPR-SPEC-0260

## Given

`humanize` returns `"just now"` when `diff < 10` and enters its second arm
only when `months >= 1 and diff < 1000`. The diff changes a local assignment
inside that second arm from `m = months` to `m = 2 * months`. The only test
asserts `humanize(1, 0) == "just now"`, which takes the first arm and never
runs the changed line.

## When

`cargo xtask fixtures python_guarded_statement_unreached_arm_no_exposed_credit`

## Then

The finding is `weakly_exposed` and names the guard no observed input
satisfies. This is the #15970 false-exposed correction: the guarded-arm gate
reads an assignment in an unentered arm the way it reads a `return` there.

## Must Not

Promote the finding to `exposed` from an exact-value assertion on another
arm. The RIPR-SPEC-0108 honesty corpus independently forbids promotion.
