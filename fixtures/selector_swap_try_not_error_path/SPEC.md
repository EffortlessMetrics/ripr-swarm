# Fixture: selector_swap_try_not_error_path

Spec: RIPR-SPEC-0107

## Given

A one-line selector swap (`max_by_key` to `min_by_key`) on a line that ends in
`?`. The `?` only propagates `None` when the queue is empty; the changed
behavior is which element is selected (#9866, split from #8361).

## When

```bash
cargo xtask fixtures selector_swap_try_not_error_path
```

## Then

The line is not an `error_path` finding and carries no "execute the changed
error arm" guidance. It abstains as `static_unknown` with an inspect-first next
step.

## Must Not

- Read an unchanged trailing `?` as a changed error arm when the edit swapped a
  selector.
- Promote the line to a repair-ready finding.
- Weaken a real changed `?` error arm (see `selector_swap_try_error_arm_control`).
