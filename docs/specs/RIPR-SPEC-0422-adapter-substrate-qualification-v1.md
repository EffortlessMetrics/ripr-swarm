# RIPR-SPEC-0422: Adapter Substrate Qualification Receipt V1

Status: accepted

Owner: provider_contract / language-expansion qualification

Created: 2026-11-02

Linked issues:

- [#6382](https://github.com/EffortlessMetrics/ripr-swarm/issues/6382)
- [#6369](https://github.com/EffortlessMetrics/ripr-swarm/issues/6369)

Linked authorities:

- [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531) semantic capability cells
- Per-ecosystem substrate ADRs (referenced, not duplicated)
- [#6512](https://github.com/EffortlessMetrics/ripr-swarm/issues/6512) safety harness receipts (referenced)

## Problem

Language adapters sit on heterogeneous parser, compiler, helper, packet, and
native substrates. Operational qualification needs one versioned receipt that
records how a substrate was accepted and which semantic claims it may support,
without selecting a parser or duplicating ADR proof.

## Behavior

### Schema

The contract lives in `crates/ripr/src/provider_contract/adapter_substrate/` and
exposes:

```text
AdapterSubstrateQualificationV1
AdapterSubstrateLayerV1
AdapterSubstrateDependencyRefV1
AdapterSubstrateSafetyRefV1
SubstrateClaimCeilingV1
SubstrateLimitationV1
```

`schema_version` is `ripr_adapter_substrate_qualification.v1`. Documents use
`deny_unknown_fields` and require explicit `validate()` after deserialization.

### Substrate classes

The closed class set is exactly:

```text
in_process_parser
generated_parser
compiler_index_service
external_helper
packet_producer_consumer
project_build_metadata_reader
native_library
hybrid
```

### Evidence law (validation)

- ADR and safety receipts are referenced by id/generation/currentness; stale
  required references fail.
- A syntax-only layer cannot inherit compiler-resolved claim ceilings.
- Packet producers cannot claim local compiler or syntax authority.
- External helpers require an explicit syntax-only `required_fallback`.
- Generated parsers require build-script and generated-code packaging disposition.
- Native libraries require explicit `platform_limits`.
- Hybrid substrates require typed limitations; one receipt cannot promote
  multiple languages.
- Missing tool/runtime state cannot deserialize as a clean result.
- TypeScript and JavaScript may share `shared_adapter_family_id` while retaining
  distinct `language` cells.

Portable semantic identity hashes canonical qualification content with layers
sorted by `layer_id`, excluding `semantic_digest` and checkout `locator:` suffixes
on limitation messages.

## Required Evidence

- Unit tests in `crates/ripr/src/provider_contract/adapter_substrate/tests.rs`
  cover substrate-class constructible shapes, shared TS/JS families, grammar
  generation splits, unknown-field rejection, semantic digest invariance, and
  precise ceiling failures.
- `fixtures/adapter-substrate-qualification-corpus/corpus.json` pins accept/reject
  schema fixtures for substrate classes, stale authorities, packaging disposition,
  and impossible ceilings.

## Known Limits

- This slice defines schema and validation only. No substrate inventory, ADR
  normalization (#6384), dependency gate (#6386), or support promotion is
  included.

## Non-Goals

- No parser selection or installation, helper execution, analyzer semantics,
  package build, support ruling, release, or publication.

## Acceptance Examples

1. A hybrid matrix lists one layer per substrate class and validates.
2. TypeScript and JavaScript receipts share a family id with distinct languages.
3. A packet producer claiming syntax facts: rejected with `ImpossibleClaimCeiling`.
4. Reordered layers and relocated checkout locators: identical semantic digest.

## Test Mapping

See `.ripr/traceability.toml` behavior `RIPR-SPEC-0422`.

## Implementation Mapping

- `crates/ripr/src/provider_contract/adapter_substrate/model.rs`
- `crates/ripr/src/provider_contract/adapter_substrate/validate.rs`
- `crates/ripr/src/provider_contract/adapter_substrate/digest.rs`
- `fixtures/adapter-substrate-qualification-corpus/corpus.json`

## Metrics

- `unit_test_pass_rate`
