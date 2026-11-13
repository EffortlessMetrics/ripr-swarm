<!-- section: Fixed -->
- `ripr pilot` and the repo-exposure grip path credit a test that imports the
  changed owner under an alias as a direct owner call when the import names the
  owner's module (crate root, nested file module, `mod.rs`, or an integration
  test's package-qualified import). A `crate::` import in an integration test
  names that test crate's own module, so it does not credit the library owner
  (#14508). The alias credits only when the resolved `crate::` chain file is
  the owner file; a missing declaration or a non-literal `#[path]` on any
  segment is withheld. Inline-module owners stay uncredited, tracked in
  #14558.
