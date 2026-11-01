# Fixture: error_path_removed_variant_diagnostic_not_credited

Spec: RIPR-SPEC-0106

## Given

The fallback arm of `parse_unit` now returns `UnitError::Blank` where it
returned `UnitError::Unknown`. The only test expects an unrelated third variant and names the removed one only in its failure message:

```rust
assert_eq!(parse_unit("q"), Err(UnitError::Other), "expected UnitError::Unknown");
```

## When

```bash
cargo xtask fixtures error_path_removed_variant_diagnostic_not_credited
```

## Then

The `error_path` finding is not `exposed`: the message text is not a pin of `UnitError::Unknown`.

## Must Not

- Treat a variant named only in a diagnostic argument as a pin of the removed variant.

Refs #12110.

This fixture claims only the `error_path` finding; the companion `match_arm` finding follows the existing arm-selection rules (see #12143).
