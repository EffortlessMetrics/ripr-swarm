# Fixture: rust_adversarial_shared_error_variant_proximity (false-exposed guard: shared error variant pinned on another owner)

Spec: RIPR-SPEC-0108

Corpus case: `rust_shared_error_variant_proximity_other_owner` in
`fixtures/evidence-promotion-honesty-corpus/corpus.json` (issue #7063). The
rule it pins lives in RIPR-SPEC-0094.

## Given

The changed owner is `deposit_cap`; its error return is respelled as a
turbofish, `Err::<i64, PayError>(PayError::Limit)`. Two tests sit in the
same `mod tests`:

```rust
// reaches the owner, but only on the happy path
assert_eq!(deposit_cap(100), Ok(100));

// pins the same variant on a different function
assert!(matches!(refund(20_000), Err(PayError::Limit)));
```

`PayError::Limit` is a value every function returning `PayError` shares.
The second test never runs `deposit_cap`, so it cannot notice a change to
`deposit_cap`'s error.

## When

```bash
ripr check \
  --root fixtures/rust_adversarial_shared_error_variant_proximity/input \
  --diff fixtures/rust_adversarial_shared_error_variant_proximity/diff.patch
```

## Then

Both findings (`error_path`, `return_value`) read `weakly_exposed`. A test
related only by file or module cannot confirm an exact error variant while
another related test calls the owner (RIPR-SPEC-0094, #7063).

**This fixture must never read `exposed`.** With the guard removed, both
findings read `exposed`, borrowing the `refund` test's pin.

## Must Not

- Confirm the owner's error variant from a same-file pin on another
  function beside a test that reaches the owner.
- Withhold confirmation from a same-file test that may reach the owner, or
  from a non-variant return value (`rust_strong_error_oracle_control`
  stays `exposed`).
- Use mutation-runtime outcome vocabulary.
