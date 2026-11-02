# Fixture: typescript_typeof_negation_other_kind_exposed

Spec: RIPR-SPEC-0027

## Given

A TypeScript production module negates the operator of a `typeof` guard
(#12281), the shape of the citty `resolveValue` false start:

```ts
typeof input === "function"
```

to:

```ts
typeof input !== "function"
```

and changes nothing else. One Jest-style test calls the owner with a number
and asserts the returned `42` exactly. Under the edit `typeof 42 !== "function"`
holds, so the owner calls `(42)()` and the test fails: it already notices the
edit.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
cargo xtask fixtures typescript_typeof_negation_other_kind_exposed
```

## Then

Negating the operator sends every input down the other branch, so the
observed call with an input of another written kind witnesses the change and
the finding reads `exposed`. No missing check is named and no repair start is
offered.

The companion `typescript_typeof_kind_gap` changes the kind name instead; an
input of another kind takes the same branch there, and that finding stays
`weakly_exposed`.

## Must Not

- Ask for a function-valued input when a test already passes an input that
  the negated guard sends down the other branch.
- Offer a repair start for this shape.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
