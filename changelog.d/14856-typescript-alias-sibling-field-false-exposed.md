<!-- section: Fixed -->
- A TypeScript object-literal field change read through a local alias of the
  owner call no longer reports `exposed` when the test asserts only a sibling
  field: `const cfg = buildConfig(); expect(cfg.verbose).toBe(false)` does not
  observe an edit to `retries`, so the finding is `weakly_exposed` with
  `propagation_unknown` (#14856). A read of the changed field through the same
  alias (`cfg.retries`) keeps `exposed`.
