# Fixture: observation_verified_call_deletion

Spec: RIPR-SPEC-0094

## Given

Production code changes a `store_result` function to use `"result_key"` as the
cache key:

```rust
pub fn store_result(cache: &mut Cache, result: i32) {
    cache.insert("result_key", result);   // ← was cache.insert("old_key", result)
}
```

The related test exercises `store_result` and then asserts on the cache
contents, directly referencing `"result_key"`:

```rust
#[test]
fn store_result_inserts_result_key() {
    let mut cache = Cache::new();
    store_result(&mut cache, 42);
    assert!(cache.inserted.iter().any(|entry| entry.contains("result_key")));
}
```

The assertion text contains `"result_key"`, which is a token extracted from
the `CallDeletion` probe expression `cache.insert("result_key", result)`.
Therefore `token_match` fires and `observation_unverified` is NOT set.

## When

```bash
cargo xtask fixtures observation_verified_call_deletion
```

or:

```bash
ripr check --root fixtures/observation_verified_call_deletion/input \
           --diff fixtures/observation_verified_call_deletion/diff.patch --mode fast
```

## Then

`ripr` must NOT emit `observation_unverified` for the `CallDeletion` probe.
The discriminate summary must NOT contain `observation_unverified` — the
weakness (if any) must come from oracle strength, not the token-match guard.

This fixture is the **anti-over-correction proof** for the CallDeletion family
in [RIPR-SPEC-0094](../../docs/specs/RIPR-SPEC-0094-observation-unverified-guard-generalization.md):
a CallDeletion probe with a token-matching assertion must NOT be downgraded by
`observation_unverified`.

## Must Not

- Emit `observation_unverified` when the assertion references the specific changed call argument.
- Use mutation-runtime outcome vocabulary.
