<!-- section: Fixed -->
- An integration test of the root package that calls a tested crate-root
  wrapper through the package's library name with no `use`
  (`my_crate::order_discount(..)`) is now related to the wrapper's private
  helper like the imported spelling. A different qualifier, a longer path,
  a test that declares, renames, imports or binds the crate name, a unit
  test in `src/`, a workspace member, a binary or inline-module entry, and
  a package name that `[lib] name` replaces all stay unrelated (#8999).
