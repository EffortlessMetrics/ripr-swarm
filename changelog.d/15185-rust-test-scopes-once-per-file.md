<!-- section: Fixed -->
- Rust analysis reads a file once to find the name scopes of every test in it,
  instead of once per test: a one-line change beside a module of 4,000 tests
  took 38 s to check and now takes 5 s, with the same output (#15185).
