<!-- section: Fixed -->
- Python `subTest`, `parametrize`, and `for` tables whose expected values come
  from rows that never vary a changed returned operand no longer credit those
  rows as strong exact oracles. The finding reports `weakly_exposed` with the
  reason `table rows do not vary <operand>` (#6651).
