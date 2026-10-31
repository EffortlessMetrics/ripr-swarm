# Fixture: typescript_typeof_kind_exposed

Spec: RIPR-SPEC-0027

## Given

The same flipped `typeof input` kind test as `typescript_typeof_kind_exposed`
(#11196). One test passes a number; another passes a function, `() => 42`,
and asserts the returned `42` exactly.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
cargo xtask fixtures typescript_typeof_kind_exposed
```

## Then

The test that passes a value written as a function at the position of
`input` discriminates the flipped kind test, so the finding reads `exposed`.

## Must Not

- Require the type-name string as the input.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
