<!-- section: Fixed -->
- Python: a changed decorator line (`@retry(3)` → `@retry(2)`) now belongs to the decorated function instead of the module, so the function's decorator-indirection limit applies and the finding abstains rather than crediting a test that cannot observe the argument (#9412).
