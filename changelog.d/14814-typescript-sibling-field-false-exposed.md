<!-- section: Fixed -->
- A TypeScript object-literal field change read through a sibling field no
  longer reports `exposed`: `expect(f().verbose)` does not observe an edit to
  `retries`, so the finding is `weakly_exposed` with `propagation_unknown`,
  matching the Rust and Python sibling-field rule (#14814).
