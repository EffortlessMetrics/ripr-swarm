# Fixture: ts_table_row_expected_refused

Spec: RIPR-SPEC-0243

## Given

Ten TypeScript owners whose changed predicate `points > 1000` becomes
`points >= 1000` (issue #11196), each with one related test whose expected
value is a `row.key` read from a table that is NOT provably a never-touched
`const` array of all-literal object rows:

- `pushedTable`: `cases.push(...)` adds a row after the declaration;
- `missingKey`: the boundary row has no `out` key;
- `identifierCell`: the boundary row's `out` is an identifier (`gold`);
- `letTable`: the table is declared with `let`;
- `derivedTable`: the loop iterates `cases.map(...)`, a derived table;
- `exportedTable`: the table is an exported module-level `const` another
  module could mutate;
- `mutatedContainer`: a container cell (`input: { points }`) is changed by
  `Object.assign(t.input, ...)` before it is asserted, so the row literal is
  not what the owner sees;
- `titleMutation`: the loop row is mutated in the registration title,
  outside the callback;
- `aliasedKeys`: a destructuring pattern repeats the `input` key, so two
  names share one cell;
- `offBoundary`: a clean all-literal table whose rows (`100`, `5000`) never
  reach the changed boundary.

## When

```bash
ripr check \
  --root fixtures/ts_table_row_expected_refused/input \
  --diff fixtures/ts_table_row_expected_refused/diff.patch \
  --mode fast
```

## Then

None is `exposed`. `pushedTable`, `missingKey`, `identifierCell`, `mutatedContainer`, `titleMutation` and `aliasedKeys` keep the
dynamic-assertion refusal ("compares against a variable or computed value,
not a literal"); `letTable`, `derivedTable` and `exportedTable` are loops
over data static analysis cannot enumerate, so their registrations stay
`static_unknown` with a conditional-registration limit (unchanged);
`offBoundary` resolves its literals but is only `weakly_exposed` because no
row reaches the boundary.

## Must Not

Report `exposed` for any of these: a wrong exposed result is worse than a
missed one.
