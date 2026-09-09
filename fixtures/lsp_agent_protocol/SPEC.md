# Fixture: lsp_agent_protocol

Spec: RIPR-SPEC-0131

## Purpose

This fixture corpus pins the versioned wire contract for generic headless
clients. It tests vocabulary and envelope shape without pretending that any
reserved request handler exists.

## Given

- `capability.json` is the initialize capability projection.
- `requests/` contains one valid envelope for every reserved request.
- `errors/` contains one valid envelope for every reserved error kind.
- `success-envelope.json` keeps snapshot, input, profile, and budget identities
  distinct and includes explicit edit boundaries and non-claims. Its routes
  carry `legacy_string_only` readiness with null command specs: the examples
  own display strings only, not producer-owned typed CommandSpec values
  (#1617 slice 4, schema 0.2).
- `negative/` contains unsupported-version and unsupported-profile examples.

## When

A generic client validates the examples against the repository-owned schemas in
`schemas/ripr/`.

## Then

The valid examples are deterministic and closed over the schema-0.2 vocabulary
(major 0 unchanged, additive route fields included). The negative examples are
rejected visibly. No example advertises a supported request, source edit,
continuation, progress, cancellation, or autonomous repair.

## Must Not

- Treat a reserved request as implemented.
- Collapse snapshot, input, profile, and budget identities.
- Treat a limitation or non-claim as evidence of runtime adequacy.
- Infer a repair edit from a missing edit boundary.
- Credit a readiness value that the envelope's own fields do not carry.

## Non-claims

These fixtures do not prove transport behavior, request lifecycle behavior,
snapshot freshness, complete evidence retrieval, or repair usefulness.
