# Fixture: error_path_removed_variant_other_arm_not_credited

Spec: RIPR-SPEC-0106

## Given

The fallback arm of `parse_unit` now returns `UnitError::Blank` where it
returned `UnitError::Unknown`. The only test pins `UnitError::Unknown`, but
through an input that selects a different arm, which still returns it:

```rust
assert_eq!(parse_unit(""), Err(UnitError::Unknown));
```

## When

```bash
cargo xtask fixtures error_path_removed_variant_other_arm_not_credited
```

## Then

The `error_path` finding stays below `exposed` (`reachable_unrevealed`, no
related assertion). The assertion holds before and after the change, so it
observes nothing the change moved.

## Must Not

- Credit a pin of the removed variant because the owner is called; the input
  must select the changed arm.

Refs #12110.
