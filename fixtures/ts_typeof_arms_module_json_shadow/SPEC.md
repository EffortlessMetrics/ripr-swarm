# Fixture: ts_typeof_arms_module_json_shadow

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

The module binds its own `JSON`, so the two arms no longer print the same value for `fmt(42)` and the test notices the edit. The finding must keep reading `exposed`: the converging-arms withdrawal folds only the built-in globals and refuses when the owner file rebinds either name.

## Must Not

- Credit a branch change whose two arms return the same value for the
  asserted input.
- Withhold credit from a test input that demonstrably takes a different branch
  after the edit.
- Offer a repair start for a finding that already reads `exposed`.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
