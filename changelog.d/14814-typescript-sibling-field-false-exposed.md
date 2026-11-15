<!-- section: Fixed -->
- A TypeScript object-literal field change read through a sibling field no
  longer reports `exposed`: `expect(f().verbose)` does not observe an edit to
  `retries`, so the finding is `weakly_exposed` with `propagation_unknown`,
  matching the Rust and Python sibling-field rule (#14814). A read that does
  name the changed field keeps `exposed`, including through a nested path
  (`f().retries.n`), an operator (`f().retries || f().verbose`) and optional
  chaining (`f().retries?.n`).
