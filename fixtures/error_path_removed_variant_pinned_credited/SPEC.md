# Fixture: error_path_removed_variant_pinned_credited

Spec: RIPR-SPEC-0106

## Given

The fallback arm of `parse_unit` now returns `UnitError::Blank` where it
returned `UnitError::Unknown`. The only test pins the removed variant on an
owner call whose input reaches that arm:

```rust
assert_eq!(parse_unit("q"), Err(UnitError::Unknown));
```

## When

```bash
cargo xtask fixtures error_path_removed_variant_pinned_credited
```

## Then

The `error_path` finding is `exposed`: the assertion would fail once the arm
yields `Blank`, so it discriminates the change even though it names the
variant the change removed, not the one it added.

## Must Not

- Leave the finding `weakly_exposed` because the assertion does not spell the
  new variant.
- Credit a pin of the removed variant whose input does not reach the changed
  arm (see `error_path_removed_variant_other_arm_not_credited`).

Refs #12110.
