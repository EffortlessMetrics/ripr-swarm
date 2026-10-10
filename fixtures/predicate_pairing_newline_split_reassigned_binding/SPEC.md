# Fixture: predicate_pairing_newline_split_reassigned_binding

Spec: RIPR-SPEC-0186

Owner: analysis-fixtures

Issue: #7042

## Given

The diff changes `gate` from `>` to `>=`. `rebound` binds the boundary
call result (`let mut got = gate(10)`), reassigns the binding across a
newline (`got\n= true;`), then asserts exactly on the rebound name
(`assert_eq!(got, true)`).

Reverting `>=` to `>` leaves the test passing: the reassignment
overwrites the boundary result either way, so the oracle never observes
which side of the boundary the call returned.

A per-line mutation scan misses this shape because identifier and `=`
never share a `;` segment. Whole-body masking plus an ordered statement
scan voids it the same way a same-line `got = true` does.

These assertions are intentional analyzed fixture input, governed by the
existing `fixtures/**` source-input policy.

Unmutated controls (`let got = gate(10); assert_eq!(got, true)` and a
multiline assertion on an unmutated binding) are pinned by unit tests in
`classify/boundary_pairing.rs` and still pair.

## When

```bash
cargo xtask fixtures predicate_pairing_newline_split_reassigned_binding
```

The public diff analysis examines the changed predicate on `src/lib.rs`.

## Then

The predicate reads below `exposed`. The newline-split post-`let`
reassignment voids the `got` binding, so the exact oracle does not pair
with the boundary call. The discriminate summary names
`same_test_pairing_missing`.

The honesty corpus independently prohibits `exposed` even if a golden is
changed.

## Must Not

- Pair the assertion with the boundary call through the rebound name.
- Promote any finding's class.
- Change proximity-only oracle credit (#4486).
- Change bare-name method relation (#4760).
- Claim items 1-3 of #7042 (positional liveness, multiline interiors,
  nested-shadow scope).
- Claim sink identity, population accuracy, or runtime adequacy.
