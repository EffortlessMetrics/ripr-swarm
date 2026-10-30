# Fixture: boundary_pairing_sibling_element

Spec: RIPR-SPEC-0186

Owner: analysis-fixtures

Issue: #4828

## Given

The diff changes only the element predicate in `a::gate` from `>=` to `>`.
The test discards its boundary result, asserts its far-input result and
asserts the same-named imported `b::gate` at the element boundary.
Both original and wrong-operator runtime variants pass this one test.

These assertions are intentional analyzed fixture input, governed by the
existing `fixtures/**` source-input policy.

## When

```bash
cargo xtask fixtures boundary_pairing_sibling_element
```

The public diff analysis examines the predicate on `src/lib.rs` line 3.

## Then

The predicate is `weakly_exposed`, with `same_test_pairing_missing` and
confidence 0.92. The honesty corpus independently prohibits `exposed`.
The integration control importing `a::gate` remains `exposed` and its
runtime assertion detects the changed element result.

## Must Not

- Bypass call identity through the adaptor-element pairing path.
- Downgrade the established owner-boundary control.
- Claim runtime adequacy or full Rust name resolution.
