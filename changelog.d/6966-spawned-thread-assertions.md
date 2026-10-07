<!-- section: Fixed -->
- An exact assertion inside a spawned thread now earns exact credit when the
  thread's panic reaches the test: `std::thread::spawn(..).join().unwrap()`
  (or `.expect(..)`), and `s.spawn(..)` inside `std::thread::scope`. Before,
  these tests read `weakly_exposed` (from the `.unwrap()` alone) or as a gap.
  Detached threads and joins whose result is dropped stay uncredited (#6966).
