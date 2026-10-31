# Fixture: ts_noop_trailing_whitespace (trailing spaces have no behavior delta)

Spec: RIPR-SPEC-0027

## Given

A TypeScript diff whose only change is three trailing spaces on the `return` line of `discount`. The related test is a strong exact-value oracle.

## When

`ripr check` analyzes the diff against the TypeScript preview adapter.

## Then

ripr emits **no behavior probe** (#10706): the line runs identically. The finding set is empty.

## Must Not

- Emit a finding for a trailing-whitespace-only change to a line.
- Run any TypeScript runtime; static preview evidence only.
