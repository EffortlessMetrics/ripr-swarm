<!-- section: Fixed -->
- A test that pins a value returned by an owner no longer earns `exposed`
  for the constructed field when the returned type has a `Drop` impl whose
  body may exit the process while unwinding from a failed assertion; the
  finding stays `weakly_exposed` (#8480).
