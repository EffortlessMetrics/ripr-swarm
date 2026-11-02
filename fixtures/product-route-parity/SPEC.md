# Product route parity corpus (#3332)

Spec: RIPR-SPEC-0167

Corpus for `cargo xtask check-product-route-parity`, which compares normalized
gate propositions from the ordinary swarm route and the reusable external-tree
route projection built from the typed #3330 product gate plan.

## Given

Committed `corpus.json` subjects with expected parity or classified mismatch
owners, plus provenance digests for every corpus byte.

## Layout

- `corpus.json` — representative subjects, expected parity or classified
  mismatch owners, and public/fork trust controls.
- `provenance.json` — SHA-256 bindings for `check-fixture-contracts`.

## When

```bash
cargo xtask check-product-route-parity
```

## Then

The check emits `ripr.product_route_parity.v1`, keeps failure and unavailable
rows in the denominator, and refuses omitted gates or pass-marked skips in unit
mutation controls.

## Must Not

- Execute live reusable workflow proof (#3331) or emit portable product proof
  (#3329) from this corpus alone.
- Treat runner/cache/wall-clock deltas as semantic parity defects.
- Delete or reroute live required checks.
