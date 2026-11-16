<!-- section: Fixed -->
- `ripr pilot` and the repo-exposure grip path credit a test that imports the
  changed owner under an alias as a direct owner call when the import names the
  owner's module in the same compilation unit (crate root, nested file module,
  `mod.rs`, or an integration test's package-qualified import). A `crate::`
  import in an integration test or a binary target names that unit's own
  module, so it does not credit the library owner (#14508).
