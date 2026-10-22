<!-- section: Fixed -->
- Python: a test that does `from module import *` now relates its assertions to
  the module's public owners, so a strong exact assertion through a star import
  reads `exposed` instead of `weakly_exposed` (#9415). Underscore-prefixed names
  stay unresolved.
