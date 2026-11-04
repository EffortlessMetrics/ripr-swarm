# RIPR-SPEC-0421: Cross-Language Edge Registry Loading and Currentness V1

Status: accepted

Owner: provider_contract / polyglot edges

Created: 2026-11-02

Linked issues:

- [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559)
- [#6484](https://github.com/EffortlessMetrics/ripr-swarm/issues/6484)
- [#6433](https://github.com/EffortlessMetrics/ripr-swarm/issues/6433)

Linked specs:

- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md)

## Problem

The registry schema (#6481 / RIPR-SPEC-0378) defines typed edges and static
validation, but producers and impact analysis still need deterministic loading,
producer registration binding, currentness evaluation, and base/head movement
facts compatible with the shared dependency-index model (#5182).

## Behavior

### Loading

`load_cross_language_edge_registry_from_json` deserializes with
`deny_unknown_fields`, runs `CrossLanguageEdgeRegistryV1::validate`, and binds
the portable `semantic_digest` for the loaded document.

### Currentness evaluation

`analyze_cross_language_edge_registry_currentness` emits:

- per-edge effective disposition and first unresolved leg;
- bounded anomalies (duplicate/conflicting/reversed/over-broad/dangling edges,
  artifact cross-bind, missing producer registration);
- bounded recovery guidance.

Positive relation availability requires a current registered producer fact. Stale
or missing producer facts remove the positive relation and name the first
unresolved leg. Generated artifact identities cannot cross-bind across distinct
project/configuration scopes with the same filename.

### Base/head movement

`compare_cross_language_edge_registries` classifies edges between loaded base and
head registries using semantic relationship keys and content digests:

```text
head_only_added
base_only_deleted
unchanged
moved
replaced
stale_on_head
invalid_on_head
unsupported_family_on_head
```

Reordering producers or edges does not change registry semantic identity.

## Required Evidence

- Unit tests in `crates/ripr/src/provider_contract/cross_language_edge/tests.rs`
  cover missing-producer effective downgrade, artifact cross-bind, producer/edge
  reorder invariance on load, and corpus-driven movement/currentness cases.
- `fixtures/cross-language-edge-registry-currentness-corpus/corpus.json` pins
  load accept/reject, effective stale downgrade, base/head movement, and
  artifact cross-bind anomalies.

## Known Limits

- No Bun profile migration, consumer cutover, public output, or support-tier
  changes (#6489 and later slices own producer projection and consumption).
- No generic dependency graph construction (#5182 remains the index authority).

## Non-Goals

- Bun shadow projection (#6489), runtime execution, GitHub lookup, codegen, or
  support promotion.

## Acceptance Examples

1. A valid registry loads with a bound semantic digest.
2. A positive edge whose producer registration is missing evaluates to
   `unresolved_leg` with `missing_producer_registration`.
3. The same generated filename in another project/config emits
   `artifact_cross_bind`.
4. Base-only deleted and head-only added edges are distinct movement records.
5. Reordered producers/edges produce identical semantic digests on load.

## Test Mapping

See `.ripr/traceability.toml` behavior `RIPR-SPEC-0421`.

## Implementation Mapping

- `crates/ripr/src/provider_contract/cross_language_edge/load.rs`
- `crates/ripr/src/provider_contract/cross_language_edge/currentness.rs`
- `crates/ripr/src/provider_contract/cross_language_edge/movement.rs`
- `crates/ripr/src/provider_contract/cross_language_edge/digest.rs`
- `fixtures/cross-language-edge-registry-currentness-corpus/corpus.json`

## Metrics

- `unit_test_pass_rate`
