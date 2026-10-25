<!-- section: Fixed -->
- A changed private helper that tests reach only through a public inherent
  method (`req.matches(&version)` where `VersionReq::matches` calls
  `eval::matches_req(..)`, which calls the changed `matches_caret`) is now
  related as `helper_owner_call` instead of "no test is seen calling the
  helper". The call into the helper layer may be spelled through an own-crate
  module path (`eval::`, `crate::eval::`, `self::`, `super::`) when it resolves
  to the one callee's module, and the receiver must be bound to the method's
  type (a struct literal, a constructor, or a `let` annotated with the type); a
  factory without an annotation, a `str::matches`, a second method of the same
  name, a renamed import and an imported module name stay unrelated. The finding
  reads `infection_unknown`, never `exposed` (#8999).
