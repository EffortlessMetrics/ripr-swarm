# Fixture: rust_lossy_post_operation_pin

Spec: RIPR-SPEC-0197

## Given

`scale` changes from `(x * 2) >> 4` to `(x * 3) >> 4`. The only test pins `assert_eq!(scale(4), 0)`. At 4 the old expression is `8 >> 4 == 0` and the new one is `12 >> 4 == 0` (#10737).

## When

`ripr check` analyzes the diff.

## Then

The `return_value` finding is not `exposed`: the pinned input cannot tell the two versions apart, while another input (`scale(6)`) does.

## Must Not

Credit an exact pin whose input the owner's lossy last operation maps to the same value for the old and the new expression.
