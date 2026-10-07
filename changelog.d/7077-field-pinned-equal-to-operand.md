<!-- section: Fixed -->
- A field initializer such as `total_cents: subtotal + shipping` no longer
  reads `exposed` when every test that pins `total_cents` also pins a field
  bound to `subtotal` to the same value. For that input the total equals the
  subtotal, so dropping `shipping` passes. The finding now reads
  `weakly_exposed` with `field_pinned_equal_to_operand` and names the
  operand to vary (#7077).
