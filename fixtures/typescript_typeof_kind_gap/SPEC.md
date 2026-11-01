# Fixture: typescript_typeof_kind_gap

Spec: RIPR-SPEC-0027

## Given

A TypeScript production module changes the kind a guard tests (#11196):

```ts
typeof input === "function"
```

to:

```ts
typeof input === "object"
```

The operator is unchanged, so this is not a negation (#12281): a number or a
string takes the same branch under both guards.

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

- names the missing discriminator `typeof input == "object"`: the kind the
  guard tests, never `input == "object"`, which would send a reader to pass
  the type-name string;
- does not credit either test, because neither passes a value written as an
  object at the position of `input`, and the changed kind name means an input
  of another kind is not sent down the other branch;
- classifies the changed line `weakly_exposed`; the repair packet stays
  incomplete here (no repair start is offered), and the assertion-shape wording
  that asks for a value of the kind is covered by unit tests rather than this
  golden.

The companion fixture `typescript_typeof_kind_exposed` adds a test that calls
the owner with `() => 42` and reads `exposed`. The companion
`typescript_typeof_negation_other_kind_exposed` negates the operator instead,
where `resolveValue(42)` alone already notices the edit and reads `exposed`.

## Must Not

- Name the type-name string as the input that discriminates a `typeof` test.
- Credit a call that passes the kind's name as a string, with or without a
  type assertion.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
