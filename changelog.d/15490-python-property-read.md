<!-- section: Changed -->
- Python `ripr check` credits a change to a plain builtin `@property` getter's
  `return` as `exposed` when a related pytest test compares a direct read of
  the property with a literal (`assert Box([1, 2, 3]).total == 6`, or a local
  bound once to `Box(...)`). The test relates as `property_read` and the
  finding aligns `direct` with `strong_oracle_observes_property_read`. Any
  other shape (a same-name setter or deleter, a rebound or imported
  `property`, stacked decorators, a bare `assert box.total`, a read through a
  helper, a patch or mock) keeps `static_unknown` with
  `decorator_indirection` (#15490).
