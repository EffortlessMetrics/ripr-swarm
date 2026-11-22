<!-- section: Fixed -->
- TypeScript checks: a module function that a test reaches only through an
  exported entry constructing a module-private class wrapped in an immediately
  invoked function (`const Serializer = (function () { class Serializer { ...
  } return Serializer; })()`), or only as a comparator or callback passed by
  reference to a synchronous array method (`keys.sort(compareStrings)`), no
  longer reads `no_static_path`. The class and the functions its code calls
  are reached through the entry as the existing weaker-than-direct relation:
  `static_unknown`, never `exposed`, and no repair start. A stored, aliased or
  deferred reference (`setTimeout`, `then`, an event listener) and a write to
  the class outside its building function add no edge. On unjs/ohash, 34 of the
  36 findings of two `serialize` fixes move from `no_static_path` to
  `static_unknown` (#15928).
