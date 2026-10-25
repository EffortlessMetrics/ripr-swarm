<!-- section: Fixed -->
- Python: an exact assertion on `obj.attr` is credited as observing a changed
  `obj.attr = ...` write when `obj` is a module-scope object bound once to a
  call result (`response = SimpleNamespace(...)`), the write is a direct
  statement of a module-level function, and the test imports `obj` from the
  owner's module and reads the attribute after calling the owner. Such a
  change was reported `weakly_exposed` because the assertion names neither the
  owner nor a changed token (#9877).
