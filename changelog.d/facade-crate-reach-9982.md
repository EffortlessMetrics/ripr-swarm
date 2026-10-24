<!-- section: Fixed -->
- A workspace facade crate that re-exports the owner's crate
  (`pub use inner::*;`) and carries the tests no longer leaves a changed
  owner as a bare `no_static_path`: the finding names
  `rust_integration_public_api_path_unresolved` and points at the facade
  test, with no class change and no related test. A doctest of another
  module is no longer related to a change only because its name shares a
  token with the changed expression (#9982).
