# RIPR-SPEC-0377: Cross-Language Edge Registry V1

Status: accepted

Owner: provider_contract / polyglot edges

Created: 2026-11-02

Linked issues:

- [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559)
- [#6481](https://github.com/EffortlessMetrics/ripr-swarm/issues/6481)

Linked specs:

- [RIPR-SPEC-0062](RIPR-SPEC-0062-cross-language-oracle-graph.md)

## Problem

RIPR needs one versioned place to store producer-registered cross-language
relation edges across FFI, generated bindings, subprocess, schema, and related
boundary families. Names, paths, extensions, dependencies, or co-location must
not mint positive edges.

## Behavior

### Schema

The contract lives in `crates/ripr/src/provider_contract/cross_language_edge/`
and exposes:

```text
CrossLanguageEdgeRegistryV1
CrossLanguageEdgeV1
CrossLanguageSubjectV1
BoundaryFamilyV1
EdgeStrengthV1
EdgeObligationV1
EdgeLimitationV1
```

`schema_version` is `ripr_cross_language_edge_registry.v1`. Documents use
`deny_unknown_fields` and require explicit `validate()` after deserialization.

### Boundary families

The closed family set is exactly:

```text
ffi_binding
generated_binding
native_addon
wasm_boundary
subprocess_cli
schema_protocol
serialization_contract
generated_client
shared_resource
manual_registered_bridge
unknown_dynamic_boundary
```

### Evidence law (validation)

- Positive `positive_registered` edges require a registered `producer_id` and
  `registered_producer_fact` basis.
- Effective strength and propagation ceiling cannot exceed the weakest unresolved
  leg, family ceiling, obligation requirement, or stale/partial currentness.
- Process, schema, resource, and in-process call families remain distinct.
- `unknown_dynamic_boundary` cannot be positively registered.
- Subject relation is edge-owned: same display name across languages without a
  registered edge is unrelated.

Portable semantic identity hashes canonical registry content with edges sorted by
`edge_id`, excluding `semantic_digest` and subject `display_name` locators.

## Required Evidence

- Unit tests in `crates/ripr/src/provider_contract/cross_language_edge/tests.rs`
  cover unrelated same-name subjects, duplicate edge ids, family strength
  ceilings, unknown-field rejection, semantic digest invariance, and constructible
  shapes for every boundary family.
- `fixtures/cross-language-edge-registry-corpus/corpus.json` pins accept/reject
  schema fixtures for families, directions, dispositions, and precise error
  codes.

## Known Limits

- This slice defines schema and validation only. No Bun profile migration,
  producer loading, consumer cutover, or output/actionability behavior is
  included.

## Non-Goals

- No Bun migration, producer implementation, consumer cutover, output or
  actionability changes, runtime execution, or support promotion.

## Acceptance Examples

1. A registry whose matrix case lists one edge per boundary family validates.
2. Two subjects share a display name but no registered edge: unrelated.
3. A subprocess edge claiming `oracle_established` strength: rejected with
   `IncompatibleFamilyStrength`.
4. Reordered edges and changed checkout display paths: identical semantic digest.

## Test Mapping

See `.ripr/traceability.toml` behavior `RIPR-SPEC-0377`.

## Implementation Mapping

- `crates/ripr/src/provider_contract/cross_language_edge/model.rs`
- `crates/ripr/src/provider_contract/cross_language_edge/validate.rs`
- `crates/ripr/src/provider_contract/cross_language_edge/digest.rs`
- `fixtures/cross-language-edge-registry-corpus/corpus.json`

## Metrics

- `unit_test_pass_rate`
