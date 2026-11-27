<!-- section: Fixed -->
- TypeScript and JavaScript: a class method that an assignment replaces
  (`this.total = (n) => n;` in the constructor, `Cart.prototype.total = ...` or
  a static `Cart.total = ...` in the same file) no longer relates to the tests
  that call `total`, which run the assigned value instead; a change in the
  method reads `no_static_path` and its guidance names the assignment. A
  binding or wrapper of the method itself (`this.total = this.total.bind(this)`)
  keeps the relation (#16314).
