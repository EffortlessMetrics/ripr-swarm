# Fixture: rust_const_expectation_literal_control

Spec: RIPR-SPEC-0035

## Given

`quota(2)` is asserted equal to a module `const EXPECTED: u32 = 4`, an independent literal (#7260).

## When

`ripr check` analyzes the changed return value (`n + 1` -> `n + 2`).

## Then

The finding stays `exposed`: the expectation does not depend on the owner.

## Must Not

Demote a literal constant expectation because it is spelled as a constant.
