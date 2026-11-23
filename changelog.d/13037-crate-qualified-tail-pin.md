<!-- section: Fixed -->
- `ripr check` no longer reads a changed computed return as `weakly_exposed` when an integration test pins the exact value through the package's own library path with no `use` (`assert_eq!(pkg::owner(100, true), 800)`); the bare imported spelling was already credited. A rebound crate name, a longer or wrong path and a module child of the test crate stay uncredited (#13037)
