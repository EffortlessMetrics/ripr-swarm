<!-- section: Fixed -->
- Python: a star import (direct or through a package `__init__`) no longer
  credits a public function when its module rewrites the name dynamically
  (`globals()`, `exec`, `vars()`, `setattr`) or star-imports another module
  that may rebind it; those cases stay `weakly_exposed` (#9415).
