# Fixture: typescript_self_comparing_one_hop_local

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

The related test captures the owner call in a local and asserts the local
against itself — both sides re-read the same one-hop local:

```ts
const a = price(150);
expect(a).toBe(a);
```

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

## When

```bash
cargo xtask fixtures typescript_self_comparing_one_hop_local
```

or:

```bash
ripr check \
  --root fixtures/typescript_self_comparing_one_hop_local/input \
  --diff fixtures/typescript_self_comparing_one_hop_local/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- finds the `price` owner in `src/lib.ts`,
- finds the related test in `tests/lib.test.ts`,
- reads the self-comparing `toBe` as `relational_check` with weak strength
  (RIPR-SPEC-0243 rule 9: the one-hop alias credit observes the changed code
  on both sides alike, so the assertion passes under both sides of the diff),
- keeps the finding preview-labeled and `weakly_exposed`.

## Must Not

- Let the one-hop local alias credit turn a self-comparison into an
  `exact_value` observation.
- Promote the finding to `exposed`.
- Claim runtime mutation confidence or TypeScript support-tier promotion.
