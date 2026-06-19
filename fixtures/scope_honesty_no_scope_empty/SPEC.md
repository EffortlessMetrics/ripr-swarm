# Fixture: scope_honesty_no_scope_empty

Spec: RIPR-SPEC-0108

## Given

A byte-pinned `ripr check --json` result has zero findings and no explicit diff
scope, but it emits a `scope_disclosures` entry with `no_scope_provided`.

## When

```bash
cargo xtask check-evidence-promotion-honesty
```

## Then

The semantic corpus must treat the empty result as not clean because the result
discloses that no analysis scope was provided.

## Must Not

- Treat a no-scope empty result as a clean analyzed result.
- Claim that this fixture adds new analyzer reachability.
