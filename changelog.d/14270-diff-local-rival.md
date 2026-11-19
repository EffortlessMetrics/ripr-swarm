<!-- section: Fixed -->
- `ripr check`: stop crediting a test that reaches a changed free function
  only through a test-local rival module (`mod local` plus a bare or glob
  `use`) as a direct call of the production owner; the relation now demotes
  to `weak_token_substring` with no reach, matching the repo-exposure grip
  path (#14270). The demotion keeps the production relation when the test
  body globs the production module (which shadows the module-level rival
  import) and when a rival import carries a `#[cfg(..)]` that is disabled or
  not decidable from the source; `#[cfg(test)]` still binds the rival.
