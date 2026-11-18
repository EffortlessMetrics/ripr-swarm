<!-- section: Fixed -->
- A registered `harness = false` inline trial whose expected value is bound
  only to an unfilled placeholder macro (`todo!()`, `unimplemented!()`) no
  longer earns exposure credit through an assertion macro or a terminal
  `return Err(..)` guard, matching ordinary `#[test]` functions (#15020).
  A name the callback can capture from the enclosing function with a real
  value keeps its reading, even when a nested closure binds the same name to a
  placeholder.
