# Fixture: ts_multiline_return_clause_taken

Spec: RIPR-SPEC-0027

## Given

A TypeScript production module negates the operator of a `typeof` comparison
where the same continuation-line flip, with a test input that takes the flipped clause (#12380):

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
cargo xtask fixtures ts_multiline_return_clause_taken
```

## Then

The test passes `true` for the flag, so the continuation clause is evaluated: under the old operator `typeof 42 === "function"` is false and under the edit it is true. The test expects `false`, so it fails under the edit and notices. The finding reads `exposed`.

## Must Not

- Read a changed line of a multi-line expression as a stand-alone statement
  and credit an exact oracle that cannot tell the two versions apart.
- Read `exposed` for a clause the test input never reaches.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
