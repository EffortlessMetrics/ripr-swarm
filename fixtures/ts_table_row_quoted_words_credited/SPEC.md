# Fixture: ts_table_row_quoted_words_credited

Spec: RIPR-SPEC-0243

## Given

Two TypeScript owners whose changed predicate `points > 1000` becomes
`points >= 1000` (issue #12253). Each has one related `test.each` object-row
table whose expected value is read as `t.out`:

- `quotedTitle`: the test title is `"tier works with this exact boundary"`.
  `with` and `this` are English words in a string, not code;
- `realThis`: the callback is a `function (t)` whose body reads `this`, a
  real hidden route that could rewrite what a row holds.

## When

```bash
ripr check \
  --root fixtures/ts_table_row_quoted_words_credited/input \
  --diff fixtures/ts_table_row_quoted_words_credited/diff.patch \
  --mode fast
```

## Then

`quotedTitle` is `exposed`: the table resolves, the row `1000 -> "gold"` is
the boundary witness, exactly as in `ts_table_row_expected_credit`.
`realThis` stays `weakly_exposed`: the table is not resolved because the code
reads `this`.

## Must Not

- Refuse a table because a title or string holds `eval`, `with`, `Function`,
  `arguments` or `this`.
- Resolve a table whose code uses one of those words as an identifier,
  property name, `this`, `new.target` or `with` statement.

Refs #12253.
