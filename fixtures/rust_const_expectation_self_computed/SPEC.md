# Fixture: rust_const_expectation_self_computed

Spec: RIPR-SPEC-0035

## Given

`quota(2)` is asserted equal to a module `const EXPECTED` that is itself `quota(2)` (#7260). Both sides move with the owner, and the recorded mutants of the corpus trap `trap-const-fn-self-expected` all pass.

## When

`ripr check` analyzes the changed return value (`n + 1` -> `n + 2`).

## Then

The finding is not `exposed`: the expectation is computed by the owner, so no independent discriminator exists.

## Must Not

Credit the equality as a strong oracle through the constant name.
