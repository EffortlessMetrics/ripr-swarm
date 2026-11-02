# Fixture: ts_table_row_expected_credit

Spec: RIPR-SPEC-0243

## Given

Three TypeScript owners whose changed predicate `points > 1000` becomes
`points >= 1000` (issue #11196). Each has one related test whose expected
value is read from an object-row table, `row.key` (or a destructured key),
instead of being written at the assertion:

- `loopTable`: describe-local `const cases = [{ input: 999, out: "silver" },
  { input: 1000, out: "gold" }]` iterated by `for (const t of cases)` around
  one `test(...)` that asserts `toBe(t.out)`;
- `eachTable`: the same rows inline in `test.each([...])("..", (t) => ...)`;
- `eachPattern`: inline `test.each` rows read through
  `({ points, want }) => ...`.

Every table is a never-touched `const` (or inline) array of object literals,
every row holds a one-line literal at every key the callback reads, and the
row is read only as plain `row.key` values.

## When

```bash
ripr check \
  --root fixtures/ts_table_row_expected_credit/input \
  --diff fixtures/ts_table_row_expected_credit/diff.patch \
  --mode fast
```

## Then

Each finding is decided by the existing classifier from the row set: the
expected value is a concrete literal and the row `1000 -> "gold"` is the
boundary witness, so the finding is no longer refused as "compares against a
variable or computed value, not a literal".

## Must Not

Credit any shape in `ts_table_row_expected_refused`.
