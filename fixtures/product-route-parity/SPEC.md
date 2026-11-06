# Product route parity corpus (#3332)

Spec: RIPR-SPEC-0253

Corpus for `cargo xtask check-product-route-parity` (GitHub #3332), which
compares synthetic ordinary and external-tree projections built from the typed
#3330 product gate plan; neither live route is executed.

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
mutation controls. Applicability mismatches remain `not_proven`, and the
proof-equivalence verdict remains `not_proven` even if projected rows match.
The public/fork control inspects synthetic runner labels only.
The report separately inventories literal command lines and step-outcome
bindings in the checked-in ordinary Rust workflow. Unmapped summary steps are
left unclassified: command text does not prove execution, applicability,
complete workflow inventory, or product-proposition equivalence.
Composite owners are reported separately as not validated by the literal-line
audit. Their synthetic comparison includes event-specific modeled commands,
step owners, subject, and artifact edges; it does not validate actual shell
control flow, history-sync exceptions, artifact authenticity, or execution.

## Must Not

- Execute live reusable workflow proof (#3331) or emit portable product proof
  (#3329) from this corpus alone.
- Treat runner/cache/wall-clock deltas as semantic parity defects.
- Delete or reroute live required checks.
