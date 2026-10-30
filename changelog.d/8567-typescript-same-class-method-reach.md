<!-- section: Fixed -->
- TypeScript: an instance method reached only through other methods of the
  same class (`this._collect(...)` called from a public `add`) no longer
  reads `no_static_path` when a test calls the public method on an
  instance it built. The test relates through a bounded `this.method(...)`
  call chain (at most 3 calls), reads `weakly_exposed` at most, and the
  missing-evidence line names the method the test calls. A caller with a
  nested `function` or `class`, a name defined twice, a method rebound on
  `this`, and static methods record no reach (#8567).
