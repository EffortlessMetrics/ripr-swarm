- A test in a private helper's own file that calls the helper's public
  wrapper now relates to the helper as `helper_owner_call` instead of
  `same_test_file` or `owner_named_test`, so reach no longer reads as
  file-level proximity (#6672).
- A predicate in a private helper reached only through a wrapper now pairs
  with the wrapper's exact assertion at the boundary input when the wrapper
  returns the helper's result directly or branches on it; a wrapper that
  drops, binds, or transforms the result keeps `same_test_pairing_missing`
  (#6694).
