<!-- section: Fixed -->
- Python: an assertion that cannot run or cannot fail no longer credits a
  change. Dead `if False:` and `while 0:` arms, a `try` body whose handler
  swallows `AssertionError`, an empty `parametrize` table, and a module-level
  `importorskip` / `skip(allow_module_level=True)` / skip `pytestmark` now
  read as no assertion or as an activation limit instead of `exposed`. The
  four false-credit trap rows clear and the trap ceiling drops to 0 (#9691).
