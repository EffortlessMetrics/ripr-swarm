# Fixture: inherent_receiver_helper_scope_shadow

Spec: RIPR-SPEC-0005

## Given

A function type parameter shadows an existing helper module name. Its trait
associated function returns the expected value without executing the private
owner. The paired receiver control shadows the owner type name instead.

## When

Analyze both changed-owner probes and all four owner seams. Run native originals
and one-arm owner mutants. Compare the genuine-module positive.

## Then

Neither type-parameter shadow can supply high owner attribution. Both originals
and their owner mutants pass; the genuine-module positive rejects the mutant.

## Must Not

Resolve a type parameter to a same-spelled repository module or owner type.
No generic argument, factory return, parameter value or field flow is inferred.

## Regression Controls

The shared matrix and independent native axes are documented in
`fixtures/inherent_receiver_free_helper_identity/BOUNDARIES.md`. Every named
subject and both changed-owner probes remain present. The same-file namespace
and receiver authorities share the syntax-backed declaration refusal.
