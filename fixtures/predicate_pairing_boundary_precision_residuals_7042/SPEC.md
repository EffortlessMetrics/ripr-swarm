# Fixture: predicate_pairing_boundary_precision_residuals_7042

Spec: RIPR-SPEC-0186

Owner: analysis-fixtures

Issue: #7042 (items 1–3)

## Given

The diff changes `gate` from `>` to `>=`. Three tests each bind the
boundary call, then exercise a shape that previously voided pairing
fail-closed without reintroducing false credit:

- `after_assert`: exact oracle, then a post-assertion reassignment.
- `commented`: block comment whose interior looks like `got = true;`.
- `nested`: inner block shadows `got`, mutates the inner binding, then
  asserts on the outer `got`.

Reverting `>=` to `>` leaves each test passing without observing the
boundary side, so `exposed` remains deserved only when pairing survives.

## When

```bash
cargo xtask fixtures predicate_pairing_boundary_precision_residuals_7042
```

## Then

Each predicate probe reads `exposed` with a strong exact oracle on the
boundary binding in the same test.

The honesty corpus pins all three vectors independently.

## Must Not

- Credit pairing through a genuinely pre-assertion mutation (#7004).
- Credit newline-split reassignment void misses (#7042 item 4 regression).
- Claim runtime mutation outcomes or full scope/dataflow fidelity.
