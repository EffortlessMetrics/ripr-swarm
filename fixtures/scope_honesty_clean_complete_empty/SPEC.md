# Fixture: scope_honesty_clean_complete_empty

Spec: RIPR-SPEC-0108

## Given

A byte-pinned `ripr check --base HEAD --json` result has zero findings, an
explicit scope header, and `analysis_scope.completeness: "complete"`.

## When

```bash
cargo xtask check-evidence-promotion-honesty
```

## Then

The semantic corpus may treat the empty result as clean because the analyzed
scope is explicit and complete.

## Must Not

- Require findings for a complete empty result.
- Use this fixture to claim that limited or excluded scopes are clean.
