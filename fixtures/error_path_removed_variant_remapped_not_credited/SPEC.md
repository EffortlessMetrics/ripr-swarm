# Fixture: error_path_removed_variant_remapped_not_credited

Spec: RIPR-SPEC-0106

## Given

The fallback arm of `parse_unit` now builds `UnitError::Blank` where it built
`UnitError::Unknown`, but the `match` result is bound to a local and every
error is remapped to `UnitError::Unknown` afterwards. The only test pins the
removed variant on a call whose input reaches that arm:

```rust
assert_eq!(parse_unit("q"), Err(UnitError::Unknown));
```

## When

```bash
cargo xtask fixtures error_path_removed_variant_remapped_not_credited
```

## Then

The `error_path` finding is not `exposed`. The assertion holds before and
after the change, because the remap makes the arm's variant unobservable.

The companion `match_arm` finding reads `exposed` today through the arm
selector, which does not require a tail `match`. That predates this fixture
and is tracked in #12143; this fixture claims only the `error_path` finding.

## Must Not

- Credit a pin of the removed variant when the arm's result is not the
  owner's return value (the `match` is not the owner's tail expression).

Refs #12110.
