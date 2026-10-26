<!-- section: Fixed -->
- Python: a module-object field write is no longer withheld as `static_unknown` when a test in another file reads the object live, even if a sibling test file reloads or replaces the module's names. The limit still holds when no test credits the write or the credit comes from a rebinding test (#10354).
