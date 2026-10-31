# Fixture: ts_trailing_whitespace_in_template (trailing spaces inside a multi-line template literal are string content)

Spec: RIPR-SPEC-0027

## Given

A TypeScript diff whose only change is trailing spaces on a line inside a multi-line template literal returned by `banner`. The whitespace is part of the returned string, so the edit changes behavior.

## When

`ripr check` analyzes the diff against the TypeScript preview adapter.

## Then

The guard does not apply: the line is inside a template literal that crosses a line break, so the edit keeps its probe (a finding is emitted).

## Must Not

- Treat whitespace inside a multi-line template literal as a no-op.
