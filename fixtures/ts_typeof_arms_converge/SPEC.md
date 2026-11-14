# Fixture: ts_typeof_arms_converge

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
cargo xtask fixtures ts_typeof_arms_converge
```

## Then

The only test calls `fmt(42)`. The edit sends `42` down the other arm, but `String(42)` and `JSON.stringify(42)` both print `42`, so the old and the edited code return the same value and the test cannot notice. The finding must not read `exposed`; it reads `weakly_exposed`.

## Must Not

- Credit a branch change whose two arms return the same value for the
  asserted input.
- Withhold credit from a test input that demonstrably takes a different branch
  after the edit.
- Offer a repair start for a finding that already reads `exposed`.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
