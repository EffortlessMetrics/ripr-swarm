<!-- section: Fixed -->
- TypeScript and JavaScript: a class method that a field of the same name
  shadows (`total = (n) => n;` beside `total(n) { ... }`) no longer relates to
  the tests that call `total`, which run the field instead; a change in the
  method reads `no_static_path` and its guidance names the field (#16306). A
  field bound from the method itself (`total = this.total.bind(this)`) still
  runs the method and keeps the relation.
