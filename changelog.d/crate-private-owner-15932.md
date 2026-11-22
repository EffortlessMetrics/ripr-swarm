<!-- section: Fixed -->
- `ripr check` no longer relates an integration test under `tests/` to a
  changed function its crate does not export (private, `pub(crate)`,
  `pub(super)`). A call of that name in the test runs another function
  (`tokio::time::sleep`, not the crate-private `runtime::jspi::sleep`), so
  the finding no longer lists those tests as direct callers. A `pub`
  function, an exported or attribute-rewritten one, and tests inside the
  crate are read as before (#15932).
