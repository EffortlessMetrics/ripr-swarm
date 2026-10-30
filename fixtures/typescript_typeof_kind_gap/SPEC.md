# Fixture: typescript_typeof_kind_gap

Spec: RIPR-SPEC-0027

## Given

A TypeScript production module flips the kind test of a guard (#11196):

```ts
typeof input === "function"
```

to:

```ts
typeof input !== "function"
```

Two Jest-style tests call the owner with strong exact assertions. One passes
a number. The other passes the string `"function"` behind a type assertion
(`"function" as any`), which is the type-name string, not a function.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
cargo xtask fixtures typescript_typeof_kind_gap
```

## Then

The TypeScript preview adapter:

- names the missing discriminator `typeof input == "function"`: the kind the
  guard tests, never `input == "function"`, which would send a reader to pass
  the type-name string;
- does not credit either test, because neither passes a value written as a
  function at the position of `input`;
- classifies the changed line `weakly_exposed` and keeps the repair packet
  delegatable, with an assertion shape that asks for a function value and
  never reuses the observed number.

The companion fixture `typescript_typeof_kind_exposed` adds a test that calls
the owner with `() => 42` and reads `exposed`.

## Must Not

- Name the type-name string as the input that discriminates a `typeof` test.
- Credit a call that passes the kind's name as a string, with or without a
  type assertion.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
