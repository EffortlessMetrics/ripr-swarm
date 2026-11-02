# Fixture: ts_table_loop_quoted_words_credited

Spec: RIPR-SPEC-0243

## Given

Two TypeScript owners whose changed predicate `points > 1000` becomes
`points >= 1000` (#12572). Each has one related `for...of` literal-row table:

- `quotedWithTitle`: the loop body ends with a comment `// works with spaces`.
  `with` is an English word in a comment, not code;
- `realEval`: the loop body calls `eval(...)`, a real hidden route.

## When

```bash
ripr check \
  --root fixtures/ts_table_loop_quoted_words_credited/input \
  --diff fixtures/ts_table_loop_quoted_words_credited/diff.patch \
  --mode fast
```

## Then

`quotedWithTitle` is `exposed`: the table resolves and the row `1000 -> "gold"`
is the boundary witness. `realEval` stays `weakly_exposed`: the table is not
resolved because the body calls `eval`.

## Must Not

- Refuse a `for...of` table because a comment, string or nested test title
  merely quotes `with` or `eval`.
- Resolve a table whose statement list or loop body uses a real `eval` or `with`
  statement.

Refs #12572.
