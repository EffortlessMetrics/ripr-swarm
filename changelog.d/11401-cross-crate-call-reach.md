<!-- section: Fixed -->
- `ripr check` now follows a call that goes through another workspace crate. When
  `app::checkout` returns `wscore::discount(price)` and a test in `app` checks
  `checkout(100)` against an exact value, a changed boundary in `discount` is read
  as noticed instead of "no test path". The call must go through a normal
  `[dependencies]` entry named like the crate, to a unique `pub` function at the
  crate root. A dev-only dependency, a same-named function in another crate,
  and an intermediate function
  that changes the argument or the result keep the earlier answer (#11401).
- Bare `ripr repair` now says "already noticed" for a changed spot that
  `ripr check` reads as noticed by a test, instead of "could not establish which
  test runs this code" or "repair route not ready". It never starts a repair
  beside that test, including when the changed function's own package also holds
  a test that reaches it away from the boundary (#11401).
- A test in a workspace member's own `tests/` folder that imports its crate's
  function (`use wscore::checkout;`) is no longer read as an import from a foreign
  crate when the workspace root has no package of its own (#11401).
- The check run that backs this reading is bounded: it happens at most once per
  `ripr repair` or `ripr continue` start suggestion, and only for a changed
  top-level `pub fn` of a package's `src/lib.rs` when there is a `Cargo.toml`
  besides that package's own (#11401).
