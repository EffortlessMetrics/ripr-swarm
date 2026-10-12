<!-- section: Fixed -->
- Owner-return pins no longer refuse when the production type is declared in
  an enclosing non-root module: the owner's own module is its scope, not a
  test-local shadow, so the nested layout keeps its pin instead of reading
  `weakly_exposed` (#6957).
