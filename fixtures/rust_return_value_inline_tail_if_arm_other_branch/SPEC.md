# Fixture: rust_return_value_inline_tail_if_arm_other_branch

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

The same edit as `rust_return_value_inline_tail_if_arm`: the then-arm of a
one-line tail `if` changes from `Some(x)` to `None`. The only test calls the
owner with an odd input, which takes the unchanged else-arm, and pins
`assert_eq!(find_even(3), Some(0))`. It passes on the old code and on the
new.

## When

`cargo xtask fixtures rust_return_value_inline_tail_if_arm_other_branch` runs the public diff analyzer.

## Then

The `return_value` finding on the changed arm stays `weakly_exposed`: the
pinned call never takes the changed arm.

## Must Not

- Promote the finding to `exposed` because the assertion pins the owner's
  result.
