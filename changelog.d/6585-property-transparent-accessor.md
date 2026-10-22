<!-- section: Fixed -->
- A changed body line of a plain built-in `@property` accessor is no longer
  reported as `decorator_indirection`. Rebound `property` bindings and
  `@v.setter` / `@v.deleter` stay limited (#6585).
  A getter with no related test keeps the limit instead of reading
  `no_static_path`, since attribute-read reach is not modeled.
