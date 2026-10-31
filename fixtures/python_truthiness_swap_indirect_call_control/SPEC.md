# Fixture: python_truthiness_swap_indirect_call_control

Spec: RIPR-SPEC-0028

## Given

The `x is None` to `not x` change of `python_truthiness_swap_no_exposed_credit`.
One test call is a readable `pick(None, 3)`, but the same test also hands the
owner to `map(pick, [0], [3])`, an indirect call whose input the call-row
reader never sees. That call passes the falsy value `0`.

## When

`cargo xtask fixtures python_truthiness_swap_indirect_call_control`

## Then

The finding stays `exposed`: the owner is mentioned other than as a direct
call, so the rows do not cover every strong input and the swap rule keeps the
existing verdict.

## Must Not

Demote the finding to `weakly_exposed` while a strong test reaches the owner
through a callback, alias or local helper the rule cannot read.
