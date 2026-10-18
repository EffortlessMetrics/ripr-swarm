# Fixture: typescript_conditional_assertion_credit

Spec: RIPR-SPEC-0243
Spec: RIPR-SPEC-0108

## Given

A TypeScript production module changes its return value:

```ts
return amount - 10;
```

to:

```ts
return amount - 20;
```

The related test credits an assertion that sits under a condition static
analysis cannot evaluate (`ok` — no constant propagation):

```ts
if (ok) { expect(price(150)).toBe(140); }
```

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

## When

```bash
cargo xtask fixtures typescript_conditional_assertion_credit
```

or:

```bash
ripr check \
  --root fixtures/typescript_conditional_assertion_credit/input \
  --diff fixtures/typescript_conditional_assertion_credit/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- finds the `price` owner in `src/lib.ts`,
- finds the related test in `tests/lib.test.ts`,
- keeps the credited oracle and the finding classification unchanged
  (RIPR-SPEC-0243 example 11: `exposed` does not move),
- renders the `typescript_conditional_assertion_credit` analysis limitation
  naming the unevaluated condition `ok` and recording that the credited
  oracle may never execute (#8714).

## Must Not

- Move the finding class because of the conditional credit.
- Withhold the assertion at collection (the condition is not constant).
- Claim runtime knowledge of whether the branch executes.
