<!-- section: Fixed -->
- TypeScript and JavaScript: a getter read straight off a fresh instance
  (`expect(new Cart(50).total).toBe(45)`) now relates the test to a change in
  the `get total()` body, as a read through a named receiver already did.
  Before, the change read `no_static_path` (#16353).
