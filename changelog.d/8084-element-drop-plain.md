<!-- section: Fixed -->
- A test that pins one element of a returned array no longer earns `exposed`
  for that element's field when the element type has a `Drop` impl with a
  non-empty body (a body that could exit the process while unwinding from a
  failed assertion cannot be ruled out, so any non-empty body is treated as
  non-plain); the finding stays `weakly_exposed` (#8084).
