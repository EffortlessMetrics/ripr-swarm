# Fixture: match_arm_result_method_tail_not_credited

Spec: RIPR-SPEC-0229

## Given

The fallback arm of `parse_unit` now builds `UnitError::Blank` where it built
`UnitError::Unknown`, but the `match` is the receiver of
`.map_err(|_| UnitError::Unknown)`, which maps every error to one variant.
The only test passes an input that reaches that arm and pins the remapped
result:

```rust
assert_eq!(parse_unit("q"), Err(UnitError::Unknown));
```

## When

```bash
cargo xtask fixtures match_arm_result_method_tail_not_credited
```

## Then

The `match_arm` finding is not `exposed`. The assertion holds before and
after the change, because the method call on the `match` makes the arm's
result unobservable.

## Must Not

- Confirm a selected arm whose `match` value is consumed by a method call
  before it is the owner's return value.

Refs #12143.
