<!-- section: Fixed -->
- A test that formats a value of a type whose `Display` or `Debug` impl
  changed (`format!("{:?}", err)`, `err.to_string()`, `write!(out, "{}", err)`)
  is now related to the changed `fmt` as a direct owner call, so the assertion
  on the formatted text is credited instead of "no test is seen calling fmt".
  The value must be typed as the impl's self type by a constructor head or a
  preceding `let`, the placeholder must match the impl's trait (`{}` for
  `Display`, `{:?}` for `Debug`), and exactly one indexed `fmt` may carry that
  trait and type; otherwise nothing is linked. Derived `Debug` is not covered
  (#8999).
