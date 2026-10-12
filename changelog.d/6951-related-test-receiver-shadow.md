<!-- section: Fixed -->
- Related-test reach no longer credits a bare method name when the test's own
  module declares the receiver type. A `#[cfg(test)] mod tests` with its own
  `struct Window` shadowed the production type, so the test-local `clone`
  was reported as direct production reach. The relation now stays name-only
  (`weak_token_substring`) and reach reads `weak` instead of `yes` when the
  receiver resolves to a test-local shadow, while the finding stays
  `weakly_exposed` with its missing discriminator (#6951).
