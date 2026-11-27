# Fixture: inserted_guard_gap

Spec: RIPR-SPEC-0263

## Given

Production code with a threshold predicate:

```rust
amount >= discount_threshold
```

Existing tests cover a value below the threshold and a value far above the
threshold. No test passes a negative amount.

## When

A 3-line guard is inserted before the unchanged threshold check, with no
existing line modified (a pure insertion, `before` empty on the changed line):

```rust
if amount < 0 {
    return 0;
}
```

## Then

`ripr` names the inserted guard's activating domain as the missing check:
`Missing check: if amount < 0`, with the RIPR-SPEC-0263 guard-entry reason
listing the observed `amount` values that stay outside the guard. The
inserted-line return value reads the same guard, so its infection stage drops
to weak with that reason. The equality point `amount == 0` is never
recommended: it passes on both sides of the insertion (the guard is strict,
so 0 falls through to the identical threshold check), so it discriminates
nothing. The producer gap id keeps minting from the changed expression
(`amount==0`); only the advice changes.

## Must Not

- Recommend `amount == 0` as a missing boundary check for the inserted guard.
- Claim a test reaches or activates the inserted line when every related
  test input leaves the guard false.
- Change the canonical gap id mint for the inserted line.
