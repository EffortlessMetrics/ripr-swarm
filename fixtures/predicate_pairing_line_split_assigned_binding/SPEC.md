# Fixture: predicate_pairing_line_split_assigned_binding

Spec: RIPR-SPEC-0186

Owner: analysis-fixtures

Issue: #7004

## Given

The diff changes `gate` from `>` to `>=`. `line_split` binds the boundary
call result (`let mut got = gate(10)`), then reassigns the binding with
the name and the operator split across the newline (`got` on one line,
`= true;` on the next), then asserts exactly on the rebound name
(`assert_eq!(got, true)`).

Reverting `>=` to `>` leaves the test passing: the reassignment
overwrites the boundary result either way, so the oracle never observes
which side of the boundary the call returned.

These assertions are intentional analyzed fixture input, governed by the
existing `fixtures/**` source-input policy.

The unmutated control (`let got = gate(10); assert_eq!(got, true)`) is
pinned by unit tests in `classify/boundary_pairing.rs` and still pairs;
a same-line reassignment is pinned by
`fixtures/predicate_pairing_reassigned_binding`.

## When

```bash
cargo xtask fixtures predicate_pairing_line_split_assigned_binding
```

The public diff analysis examines the changed predicate on `src/lib.rs`.

## Then

The predicate reads below `exposed`. The newline-split reassignment
voids the `got` binding exactly like the same-line spelling, so the
exact oracle does not pair with the boundary call. The discriminate
summary names `same_test_pairing_missing`.

The honesty corpus independently prohibits `exposed` even if a golden is
changed.

## Must Not

- Pair the assertion with the boundary call through the rebound name.
- Treat the line split as a detector blind spot that keeps the stale
  pairing (the confirmed over-credit this fixture pins).
- Promote any finding's class.
- Change proximity-only oracle credit (#4486).
- Change bare-name method relation (#4760).
- Claim sink identity, population accuracy, or runtime adequacy.
