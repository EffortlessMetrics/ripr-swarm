# Fixture: adapter-substrate-qualification-corpus

Spec: RIPR-SPEC-0379

This manifest-only fixture pins versioned adapter-substrate qualification
receipt documents for #6382 before substrate ADR normalization (#6384).

## Given

A language adapter substrate qualification receipt using schema version
`ripr_adapter_substrate_qualification.v1`, closed substrate classes, explicit
claim ceilings, ADR references (not copied proof), and load-bearing dependency
grammar generations.

## When

The corpus case is deserialized and passed through
`AdapterSubstrateQualificationV1::validate`.

## Then

Accept cases validate cleanly. Reject cases fail with the pinned error code for
stale authorities, impossible claim ceilings, contradictory layers, or missing
packaging disposition.

## Must Not

- Select, install, execute, enable, package, or support-promote a parser.
- Copy ADR or safety harness proof into a second proof system.
- Collapse TypeScript and JavaScript capability cells when sharing implementation.

Run:

```bash
cargo test -p ripr provider_contract::adapter_substrate::tests::corpus_cases_cover_substrate_acceptance_matrix -- --test-threads=1
```
