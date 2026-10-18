<!-- section: Fixed -->
- An expected value held in an immutable `let` of an interior-mutable type
  (`Cell`, `RefCell`, `Mutex`, atomics) or in a `static mut` is no longer read
  as its initializer, so a changed assertion is not wrongly weakened as
  comparing the owner with itself (#8112).
