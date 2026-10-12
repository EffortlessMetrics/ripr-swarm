<!-- section: Fixed -->
- Owner-return pins no longer credit a production method when the test's own
  module declares a same-name type. A `#[cfg(test)] mod tests` with its own
  `struct Window` shadowed the production type, so the test-local clone was
  mistaken for the changed owner and the field read false `exposed`. The pin
  is now refused when the test's module scope declares the receiver name,
  and the finding stays `weakly_exposed` (#6905).
