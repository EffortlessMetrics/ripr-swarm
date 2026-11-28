# Fixture: ts_table_callback_comment_credited

Spec: RIPR-SPEC-0243

Refs #12494

## Given

Three TypeScript owners whose changed predicate `points > 1000` becomes
`points >= 1000`. Each has one `test.each` object-row table with rows 999 and
1000 and one `expect(owner(t.input)).toBe(t.out)` assertion in the callback.
They differ only in a comment: a `//` note before the assertion (`lineNote`),
a `/* */` note before it (`blockNote`), and a `//` note after it
(`trailingNote`).

## When

`ripr check --mode fast` runs on the diff.

## Then

All three read `exposed`: a comment is not code, so it neither hides the
single `expect(..)` statement nor changes which row values the assertion reads.
Row 1000 pins the boundary exactly as in `ts_table_row_expected_credit`.

## Must Not

A comment must not demote a row that reads `exposed` without it, and the
comment's words must not matter.
