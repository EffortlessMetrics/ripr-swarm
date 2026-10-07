# Fixture: predicate_pairing_compound_assigned_binding

Spec: RIPR-SPEC-0186

Owner: analysis-fixtures

Issue: #7004

## Given

The diff changes `bucket` from `>` to `>=`. `compounded` binds the
boundary call result (`let mut got = bucket(10)`), compound-assigns the
binding (`got += 1`), then asserts exactly on the mutated name
(`assert_eq!(got, 2)`).

This case is fail-closed by design: reverting `>=` to `>` fails the
test (`bucket(10)` returns 0, so `got` is 1, not 2), meaning the test
would discriminate at runtime. The static pairing rule has no value
analysis — it cannot tell a preserving shift (`+= 1`) from a
destroying one (`*= 0`) — so any compound assignment voids the binding
rather than risk a false `exposed`. Full dataflow stays a non-goal.

These assertions are intentional analyzed fixture input, governed by the
existing `fixtures/**` source-input policy.

The unmutated control (`let got = bucket(10); assert_eq!(got, 1)`) is
pinned by unit tests in `classify/boundary_pairing.rs` and still pairs.

## When

```bash
cargo xtask fixtures predicate_pairing_compound_assigned_binding
```

The public diff analysis examines the changed predicate on `src/lib.rs`.

## Then

The predicate reads below `exposed`. The post-`let` compound
assignment voids the `got` binding, so the exact oracle does not pair
with the boundary call. The discriminate summary names
`same_test_pairing_missing`.

The honesty corpus independently prohibits `exposed` even if a golden is
changed.

## Must Not

- Pair the assertion with the boundary call through the mutated name.
- Promote any finding's class.
- Change proximity-only oracle credit (#4486).
- Change bare-name method relation (#4760).
- Claim sink identity, population accuracy, or runtime adequacy.
