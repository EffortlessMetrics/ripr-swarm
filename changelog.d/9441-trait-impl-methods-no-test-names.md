<!-- section: Fixed -->
- A weak gap on a trait-impl method no test names (`Money(2) + Money(3)` for
  `Add::add`, a scope end for `Drop::drop`, `shapes[0].area()` through a
  trait object) is withheld as `static_unknown` under
  `rust_implicit_trait_call_unresolved` instead of reading `weakly_exposed`
  (#9441, RIPR-SPEC-0240). Nothing is credited.
