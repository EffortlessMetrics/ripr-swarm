<!-- section: Fixed -->
- A TypeScript object-literal field change read through a local alias of the
  owner call is now affirmatively identified as not observing the changed
  field when the test asserts only a sibling field:
  `const cfg = buildConfig(); expect(cfg.verbose).toBe(false)` does not
  observe an edit to `retries`, so the related test is marked
  `assertion_not_observing` and the finding stays `weakly_exposed` with
  `propagation_unknown` (#14856). A read of the changed field through the same
  alias (`cfg.retries`) keeps `exposed`.
