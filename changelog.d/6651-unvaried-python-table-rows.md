<!-- section: Fixed -->
- Python `subTest`, `parametrize`, and `for` tables whose expected values come
  from rows that never vary a changed returned operand no longer credit those
  rows as strong exact oracles, and only when that constant matches the
  replacement return. The finding reports `weakly_exposed` with the reason
  `table rows do not vary <operand>` (#6651).
