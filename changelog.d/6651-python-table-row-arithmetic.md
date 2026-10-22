<!-- section: Fixed -->
- Python `subTest`, `parametrize`, and `for` tables no longer credit a strong
  exact oracle when every row's bound integer literals make a changed simple
  arithmetic expression evaluate the same, such as loguru-style `year % 100`
  vs `year % 1000` over only 20xx years. The finding reports `weakly_exposed`
  with the reason `table rows do not vary <operand>` (#6651).
