# Fixture: ts_noop_trailing_comment (a trailing `//` comment has no behavior delta)

Spec: RIPR-SPEC-0027

## Given

A TypeScript diff whose only change is a trailing `//` comment on the `return` line of `discount`. The statement is the same code and the related test is a strong exact-value oracle.

## When

`ripr check` analyzes the diff against the TypeScript preview adapter.

## Then

ripr emits **no behavior probe** (#10706): the line runs identically, so no gap id or discriminator can embed the comment text. The finding set is empty.

## Must Not

- Emit a finding, gap id or discriminator for a comment-only change to a line.
- Treat a changed token or a changed semicolon as a no-op.
- Run any TypeScript runtime; static preview evidence only.
