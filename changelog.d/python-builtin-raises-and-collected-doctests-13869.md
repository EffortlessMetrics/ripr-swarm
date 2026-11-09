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
- Python: a FastAPI-style `TestClient` test over the owner's own app that
  pins the changed key of the handler's returned dict, with
  `assert response.json()["total"] == 190` or
  `self.assertEqual(response.json()["total"], 190)`, now reads `exposed`.
  Before, ripr reported a gap even though the test already checked the
  changed value. A `status_code` check, another key, another route or an
  unidentified app still never reads `exposed` (#13869).
