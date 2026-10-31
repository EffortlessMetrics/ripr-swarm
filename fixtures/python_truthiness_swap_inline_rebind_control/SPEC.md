# Fixture: python_truthiness_swap_inline_rebind_control

Spec: RIPR-SPEC-0028

## Given

The `x is None` to `not x` change of `python_truthiness_swap_no_exposed_credit`,
but the owner first runs `if flag: x = 0`, an inline body that rebinds the
parameter. The call `pick(None, 3, True)` therefore reaches the changed
predicate with `x == 0`, where the old and the new predicate disagree.

## When

`cargo xtask fixtures python_truthiness_swap_inline_rebind_control`

## Then

The finding stays `exposed`: the swap rule treats an assignment after a
condition's `:` as a rebinding and keeps the existing verdict.

## Must Not

Demote the finding to `weakly_exposed` from the call arguments alone when the
owner may rebind the parameter before the predicate.
