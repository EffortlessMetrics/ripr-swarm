# Fixture: observation_unverified_call_deletion

Spec: RIPR-SPEC-0094

## Given

Production code changes a `store_result` function to use `"result_key"` as the
cache key (instead of `"old_key"`):

```rust
pub fn store_result(cache: &Cache, result: i32) {
    cache.insert("result_key", result);   // ← was cache.insert("old_key", result)
}
```

The only related test calls `store_result` with a trivial assertion that does
not reference `"result_key"` or any other token from the changed call
expression:

```rust
#[test]
fn store_result_runs_without_panic() {
    let cache = Cache;
    store_result(&cache, 42);
    assert!(true);
}
```

The assertion text `assert!(true)` contains no token from the `CallDeletion`
probe expression `cache.insert("result_key", result)` that is specific to this
call site. There is no `token_match` beyond the generic "assert" prefix.

## When

```bash
cargo xtask fixtures observation_unverified_call_deletion
```

or:

```bash
ripr check --root fixtures/observation_unverified_call_deletion/input \
           --diff fixtures/observation_unverified_call_deletion/diff.patch --mode fast
```

## Then

`ripr` must emit `reachable_unrevealed` (not `exposed`) for the `CallDeletion`
probe. The test reaches `store_result`, but `assert!(true)` compares only a
literal and reads no value, so it matches no probe and supplies no oracle
(#14968). Before #14968 this fixture read `weakly_exposed` with
`observation_unverified`; an assertion that reads a value but names no token of
the changed call still reads that way (`observation_unverified_side_effect`).

This fixture is the **before/escape-hatch proof** for the CallDeletion family
in [RIPR-SPEC-0094](../../docs/specs/RIPR-SPEC-0094-observation-unverified-guard-generalization.md):
before this fix, `oracle_matches_family(CallDeletion, ...)` fired on any
assertion text containing "assert", so `assert!(true)` was credited as
discriminating the specific `"result_key"` change — a false-actionable claim.

## Must Not

- Report `exposed` when no assertion text references any token from the changed cache.insert call.
- Use mutation-runtime outcome vocabulary.
- Credit `assert!(true)` with any oracle strength: it observes nothing.
