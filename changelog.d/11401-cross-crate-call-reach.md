<!-- section: Fixed -->
- `ripr check` now follows a call that goes through another workspace crate. When
  `app::checkout` returns `wscore::discount(price)` and a test in `app` checks
  `checkout(100)` against an exact value, a changed boundary in `discount` is read
  as noticed instead of "no test path". The call must go through a normal
  `[dependencies]` entry named like the crate, to a unique `pub` function at the
  crate root. A dev-only dependency, a renamed dependency, a bare `use` of the
  function, a same-named function in another crate, and an intermediate function
  that changes the argument or the result keep the earlier answer. Bare
  `ripr repair` does not follow the call yet and never starts beside that test
  (#11401).
- A test in a workspace member's own `tests/` folder that imports its crate's
  function (`use wscore::checkout;`) is no longer read as an import from a foreign
  crate when the workspace root has no package of its own (#11401).
