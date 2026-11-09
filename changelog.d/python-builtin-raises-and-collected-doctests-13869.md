<!-- section: Fixed -->
- Python: `pytest.raises(TypeError)` (or the class raised before the edit)
  now reads `exposed` when a change swaps one builtin exception for another
  outside its hierarchy, such as `ValueError` to `TypeError`, and the test's
  literal input reaches the changed `raise`. Before, ripr reported a gap and
  asked for the assertion the test already had. Parent/child swaps such as
  `KeyError` to `LookupError` still read weak (#13869).
- Python: a doctest that pytest collects (`--doctest-modules`) but ripr
  cannot evaluate, such as one for a function with an `if`, now reads
  `static_unknown` instead of `no_static_path`. Before, ripr claimed no test
  reached code the doctest runs (#13869).
