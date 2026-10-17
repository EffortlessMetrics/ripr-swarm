<!-- section: Fixed -->
- A test that pins one element of a returned array no longer earns `exposed`
  for that element's field when the element type has a `Drop` impl whose body
  may exit the process while unwinding from a failed assertion; the finding
  stays `weakly_exposed` (#8084).
