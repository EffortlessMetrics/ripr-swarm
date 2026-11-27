<!-- section: Fixed -->
- TypeScript checks: a module function that a test reaches only through an
  exported entry constructing a module-private class wrapped in an immediately
  invoked function (`const Serializer = (function () { class Serializer { ...
  } return Serializer; })()`), or only as a comparator or callback passed by
  reference to a synchronous array method (`keys.sort(compareStrings)`), no
  longer reads `no_static_path`. The class and the functions its code calls
  are reached through the entry as the existing weaker-than-direct relation:
  `static_unknown` when the entry's test pins its result exactly,
  `weakly_exposed` when that test has only a weak assertion, never `exposed`,
  and no repair start. A stored, aliased or
  deferred reference (`setTimeout`, `then`, an event listener) and a write to
  the class outside its building function add no edge. On unjs/ohash, 34 of the
  36 findings of two `serialize` fixes move from `no_static_path` to
  `static_unknown` (#15928).
- TypeScript checks: a method of a module-private class that an exported entry
  constructs and calls in one expression (`new Serializer().write(input)`), a
  sibling it calls on `this`, and a module function only such a method calls
  no longer read `no_static_path` when a test calls the entry. They are
  reached as the same weaker-than-direct relation: `static_unknown` under an
  exact assertion, `weakly_exposed` under a weak one, never `exposed`. A method the entry's call path does not run keeps
  `no_static_path`, and so does every method of a class that is aliased,
  subclassed, re-exported, used as a type, or whose prototype or member of
  that name is assigned anywhere in the module. An instance kept in a
  variable before the call is not followed (#15928).
