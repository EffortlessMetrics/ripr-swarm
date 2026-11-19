<!-- section: Fixed -->
- Python `assert f(x) is True` / `is False` is graded as an exact identity pin (`exact_value` / strong) like `== True|False`, so a mutant that flips the returned bool is no longer reported as weakly exposed (#15250). `is None`, `is not ...` and truthiness forms are unchanged.
