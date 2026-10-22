<!-- section: Fixed -->
- Python: a dunder method (`__add__`, `__eq__`, `__exit__`, ...) related only
  through `dunder_protocol`, and a module-level statement (constant, decorator
  argument, conditional definition) related only by a filename-stem heuristic
  while a test imports the module, now read `static_unknown` with a stop
  reason instead of an actionable `weakly_exposed` gap (#9421, #9423, #9412).
