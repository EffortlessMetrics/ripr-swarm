# Fixture: typescript_typeof_negation_compound_gap

Spec: RIPR-SPEC-0027

## Given

A TypeScript production module negates the operator of a `typeof` guard that
is one clause of a compound condition (#12281 review):

```ts
if (typeof input === "function" && flag) return (input as any)();
```

to:

```ts
if (typeof input !== "function" && flag) return (input as any)();
```

One Jest-style test calls `pick(42, false)` and asserts the returned `42`
exactly. With `flag` false the condition is false under both versions, so
the test cannot notice the edit.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
cargo xtask fixtures typescript_typeof_negation_compound_gap
```

## Then

The changed behavior is the guard line and the missing check names
`typeof input == "function"`, the changed clause. Because the clause is joined
by `&&`, negating its operator does not send every input down the other
branch, so the number input is not credited and the finding stays
`weakly_exposed`. No repair start is offered here (the fixture has no verify
command); the point is that the finding is not read `exposed`.

The companion `typescript_typeof_negation_other_kind_exposed` negates a guard
that is the whole condition and reads `exposed`.

## Must Not

- Credit an input of another kind for a negated clause of a compound
  condition.
- Read this finding `exposed`.
- Use mutation-runtime outcome vocabulary reserved for real mutation
  execution.
