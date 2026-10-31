# Fixture: rust_closed_if_statement_tail

Spec: RIPR-SPEC-0108

## Given

`pick` assigns `r = 1` in both arms of a one-line `if`, then returns `r + 0`. The test pins `pick(5)`.

## When

The predicate changes `x > 5` to `x >= 5`. Both versions return 1 at `x = 5`.

## Then

No returned-value sink is attached to the predicate, so the finding is not `exposed`.

## Must Not

Read the line after a self-closed `if` statement as that branch's produced value and credit the pin (#10880).
