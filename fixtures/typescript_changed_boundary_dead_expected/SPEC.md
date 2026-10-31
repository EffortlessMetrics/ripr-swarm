# Fixture: typescript_changed_boundary_dead_expected

Spec: RIPR-SPEC-0027

This fixture is also registered as the
`typescript_changed_boundary_dead_expected` vector in the
evidence-promotion-honesty corpus (`fixtures/evidence-promotion-honesty-corpus`),
whose independent invariant contract is RIPR-SPEC-0108.

## Given

A compound predicate contains a grouped comparison. An exact assertion passes
its equality-boundary input but expects 999; both possible return values are
90 or 100, so the expected value cannot reveal the changed behavior.

## When

`cargo xtask fixtures typescript_changed_boundary_dead_expected` analyzes
`total > 100` changing to `total >= 100` through the real CLI.

## Then

Exactly one predicate finding remains `weakly_exposed`, names `total == 100`,
and retains the strong exact-value oracle without crediting its dead expectation.

## Must Not

- Lose whole-line source provenance when selecting one comparison span.
- Promote a strong assertion whose expected value matches neither behavior.
- Infer runtime mutation adequacy from this static finding.
