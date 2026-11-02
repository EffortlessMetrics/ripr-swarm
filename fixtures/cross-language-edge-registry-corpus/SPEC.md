# Fixture: cross-language-edge-registry-corpus

Spec: RIPR-SPEC-0378

This manifest-only fixture pins versioned cross-language edge registry schema
documents for #6481 before Bun producers or consumers migrate.

## Given

A producer-owned cross-language edge registry document using schema version
`ripr_cross_language_edge_registry.v1`, closed boundary families, explicit edge
dispositions, and load-bearing project/configuration identities.

## When

The corpus case is deserialized and passed through
`CrossLanguageEdgeRegistryV1::validate`.

## Then

Accept cases validate cleanly. Reject cases fail with the pinned error code for
duplicate edge ids, incompatible family strength, or contradictory direction.

## Must Not

- Infer positive edges from names, paths, extensions, dependencies, or co-location.
- Treat same display name across languages as related without a registered edge.
- Change Bun bridge behavior, output, or actionability (schema-only slice).

Run:

```bash
cargo test -p ripr provider_contract::cross_language_edge::tests::corpus_cases_cover_registry_acceptance_matrix -- --test-threads=1
```
