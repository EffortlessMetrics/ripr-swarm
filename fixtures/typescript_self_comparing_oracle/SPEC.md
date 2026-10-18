# Fixture: typescript_self_comparing_oracle

Spec: RIPR-SPEC-0243 rule 9 (#8713)
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

The related test reaches `price(...)` through a direct owner call, but its only
assertion is self-comparing — the expected side re-invokes the owner with the
same arguments:

```ts
expect(price(150)).toBe(price(150));
```

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

## When

```bash
cargo xtask fixtures typescript_self_comparing_oracle
```

or:

```bash
ripr check \
  --root fixtures/typescript_self_comparing_oracle/input \
  --diff fixtures/typescript_self_comparing_oracle/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- finds the `price` owner in `src/lib.ts`,
- finds the related test in `tests/lib.test.ts`,
- reads the self-comparing `toBe` as `relational_check` with weak strength
  (RIPR-SPEC-0243 rule 9: both sides run the changed code with the same
  inputs, so the assertion passes under both sides of the diff),
- keeps the finding preview-labeled and `weakly_exposed`.

## Must Not

- Treat a self-comparing exact matcher as `exact_value` evidence.
- Promote the finding to `exposed`.
- Claim runtime mutation confidence or TypeScript support-tier promotion.
