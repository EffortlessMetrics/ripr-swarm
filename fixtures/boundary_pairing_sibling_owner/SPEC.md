# Fixture: boundary_pairing_sibling_owner

Spec: RIPR-SPEC-0186

Owner: analysis-fixtures

Issue: #4828

## Given

The diff changes only `a::gate` from `>` to `>=`. The test discards its
boundary result, asserts its far-input result, and asserts the same-named
`b::gate` at the boundary. Both original and wrong-operator runtime variants
pass this one test; the sibling assertion cannot observe the changed owner.

These assertions are intentional analyzed fixture input, governed by the
existing `fixtures/**` source-input policy.

## When

```bash
cargo xtask fixtures boundary_pairing_sibling_owner
```

The public diff analysis examines the predicate on `src/lib.rs` line 3.

## Then

The predicate is `weakly_exposed`, with `same_test_pairing_missing` and
confidence 0.92. Reach, infection, propagation, observation and the strong
far-input oracle remain valid. The honesty corpus independently forbids
`exposed`; the integration controls include direct, let-bound and imported
owner/sibling calls, plus executable original and wrong-operator variants.

## Must Not

- Combine the sibling's boundary input with the changed owner's far oracle.
- Downgrade an assertion on the changed owner's own boundary call.
- Claim runtime adequacy, full Rust name resolution or a support-tier change.
