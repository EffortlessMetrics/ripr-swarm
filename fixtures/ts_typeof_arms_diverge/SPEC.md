# Fixture: ts_typeof_arms_diverge

Spec: RIPR-SPEC-0027

## Given

A TypeScript production module edits a `typeof` guard of a returned
conditional (#12296):

```ts
return typeof input === "object" ? JSON.stringify(input) : String(input);
```

to:

```ts
return typeof input !== "object" ? JSON.stringify(input) : String(input);
```

and changes nothing else. One Jest-style test calls the owner once with a
literal input and asserts the returned value exactly.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
cargo xtask fixtures ts_typeof_arms_diverge
```

## Then

The test calls `fmt("a")`. The edit sends the string down the other arm, where `JSON.stringify("a")` prints `"a"` with quotes while the old arm printed `a`. The test expects `a`, so it fails under the edit and notices. The finding reads `exposed`.

## Must Not

- Credit a branch change whose two arms return the same value for the
  asserted input.
- Withhold credit from a test input that demonstrably takes a different branch
  after the edit.
- Offer a repair start for a finding that already reads `exposed`.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
