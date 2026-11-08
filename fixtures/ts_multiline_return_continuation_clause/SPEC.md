# Fixture: ts_multiline_return_continuation_clause

Spec: RIPR-SPEC-0027

## Given

A TypeScript production module negates the operator of a `typeof` comparison
where the flipped clause sits on the continuation line of a multi-line boolean `return` (#12380):

```ts
  return flag &&
    typeof input === "function";
```

to:

```ts
  return flag &&
    typeof input !== "function";
```

and changes nothing else. One Jest-style test calls the owner once and asserts
the returned value exactly.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
cargo xtask fixtures ts_multiline_return_continuation_clause
```

## Then

The only test calls `f(42, false)`. With the flag false the clause is never decided by its `typeof` test, so the old and the edited code return the same value and the test cannot notice the edit. The finding must not read `exposed`; it reads `weakly_exposed`.

## Must Not

- Read a changed line of a multi-line expression as a stand-alone statement
  and credit an exact oracle that cannot tell the two versions apart.
- Read `exposed` for a clause the test input never reaches.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
