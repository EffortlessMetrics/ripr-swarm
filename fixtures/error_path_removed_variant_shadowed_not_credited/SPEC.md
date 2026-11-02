# Fixture: error_path_removed_variant_shadowed_not_credited

Spec: RIPR-SPEC-0106

## Given

The fallback arm of `parse_unit` now returns `UnitError::Blank` where it
returned `UnitError::Unknown`. The only test shadows the owner with a local closure that always returns the removed variant:

```rust
let parse_unit = |_raw: &str| -> Result<u32, UnitError> { Err(UnitError::Unknown) };
assert_eq!(parse_unit("q"), Err(UnitError::Unknown));
```

## When

```bash
cargo xtask fixtures error_path_removed_variant_shadowed_not_credited
```

## Then

The `error_path` finding is not `exposed`: the assertion exercises the local closure, not `parse_unit`.

## Must Not

- Credit a pin whose called name is a test-local shadow of the owner.

Refs #12110.

This fixture claims only the `error_path` finding; the companion `match_arm` finding follows the existing arm-selection rules (see #12143).
