# Fixture: typescript_negated_identity_pin

Spec: RIPR-SPEC-0246
Spec: RIPR-SPEC-0108

## Given

A TypeScript production module replaces a returned copy with the original
(a copy-for-identity change, RIPR-SPEC-0246 rule 1):

```ts
return { ...cfg };
```

to:

```ts
return cfg;
```

The related test pairs a negated identity matcher — which asserts
NON-identity and earned only capped weak credit under RIPR-SPEC-0243 rule 2
(#8712) — with a structural matcher:

```ts
expect(ready(cfg)).not.toBe(other);
expect(ready(cfg)).toEqual(cfg);
```

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

## When

```bash
cargo xtask fixtures typescript_negated_identity_pin
```

or:

```bash
ripr check \
  --root fixtures/typescript_negated_identity_pin/input \
  --diff fixtures/typescript_negated_identity_pin/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- finds the `ready` owner in `src/settings.ts`,
- finds the related test in `tests/settings.test.ts`,
- reads the negated `.not.toBe` as capped `relational_check` / weak and
  carries NO reference-identity property on it,
- demotes the copy-for-identity change to `weakly_exposed` because no
  unconditional identity assertion pins the returned object's identity.

## Must Not

- Let the negated identity matcher pin identity for rule 1.
- Promote the finding to `exposed` on the structural matcher alone.
- Claim runtime mutation confidence or TypeScript support-tier promotion.
