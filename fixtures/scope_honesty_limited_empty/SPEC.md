# Fixture: scope_honesty_limited_empty

Spec: RIPR-SPEC-0108

## Given

A byte-pinned `ripr check --json` result has zero findings, an explicit scope
header, `analysis_scope.completeness: "limited"`, and a limitation entry.

## When

```bash
cargo xtask check-evidence-promotion-honesty
```

## Then

The semantic corpus must treat the empty result as not clean because the
analysis was limited.

## Must Not

- Treat a limited empty result as clean.
- Claim that this fixture proves any specific analyzer limitation taxonomy.
