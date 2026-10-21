<!-- section: Fixed -->
- An integration test that calls a tested wrapper through the package's own
  library name with no `use` (`my_crate::order_discount(..)`) is now related
  to the wrapper's private helper like the imported spelling. Only the root
  package's crate-root library items qualify; any other qualifier, a rebound
  crate name, a shadowed entry or a non-library entry stays unrelated (#8999).
