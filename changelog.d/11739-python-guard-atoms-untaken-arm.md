<!-- section: Fixed -->
- Python: a return-value change inside an `if`/`for` body guarded by `is None`, truthiness, `not`, `in <literal>`, `isinstance` or an empty-literal loop, which every strong test call provably misses, now reads `weakly_exposed` instead of `exposed` (#11739).
